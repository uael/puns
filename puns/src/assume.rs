//! The layout that `puns` assumes `rustc` gives an enum of the `Rust` representation, checked at
//! compile time against what the experiments find.
//!
//! Rust does not specify the layout of an enum of the `Rust` representation, but `rustc` lays it
//! out by one algorithm, from the layouts of the fields of its variants, and that algorithm has not
//! changed since Rust 1.65. Without the `unstable` feature, `puns` assumes that layout where the
//! algorithm gives the same bytes in every layout that Rust allows for the fields, and nothing
//! depends on an order that `-Zrandomize-layout` draws. Where the assumption applies, the layout
//! that it gives is computed here, from the layouts of the fields and the alignments of the
//! integers of the target, and compared with the layout that [`discover`](crate::discover) finds
//! by experiments. If they differ, the build fails: a compiler that lays the enum out otherwise
//! does not change a verdict, it refuses to compile. Elsewhere the layout stays one that Rust
//! leaves to the compiler ([`Layout::unspecified`]), proven only by the same type.
//!
//! `Option<T>` and `Result<T, E>` are such enums, and go through the same code
//! ([`std_enums`](crate::std_enums)). An enum without fields is a scalar, whose assumption is
//! [`fieldless`].
//!
//! # The assumption
//!
//! A variant is *absent* if its fields have no values and each of them has size 0 and alignment 1:
//! `rustc` does not lay it out, and it takes no value of the tag. Every other variant is *present*.
//!
//! - **One present variant.** The enum is a struct of its fields, from offset 0, with no tag.
//! - **A tag.** The tag is at offset 0. Its width is the alignment of the first field of a present
//!   variant that is not of size 0 and alignment 1, the least such alignment over the variants, if
//!   an unsigned integer of that size has that alignment on the target. It is at least the width
//!   of the least integer that holds the index of every variant. Each variant is its index there.
//!   The fields of a variant start at the first offset after the tag that their alignment allows.
//! - **A niche.** The *largest* variant is the present variant whose fields take the most bytes,
//!   the last one if several do. If it has exactly one scalar whose values Rust restricts (the
//!   *niche*, [`Niche`]), and that scalar has enough values outside the *valid range* of `rustc`
//!   for every other present variant from the first to the last, these variants take consecutive
//!   values there ([`reserve`]). Each other variant then starts at 0 if it ends before the niche,
//!   or else at the first offset after the niche that its alignment allows, and the niche layout
//!   exists only if every variant fits within the size of the largest one.
//! - **Which one.** The niche layout is used if it is smaller than the tagged one, or of the same
//!   size with more values left outside the valid range of its niche than the tag of the tagged
//!   one has. Otherwise the tag is.
//!
//! The *valid range* of a scalar is the shortest range of values, wrapping past the greatest value
//! to 0, that holds every value that Rust allows for it ([`arc`]). For a `char`, it is 0 to
//! `0x10FFFF`, surrogates included, and for an enum with the tags 0 and 255, it is 255 to 0.
//!
//! # Where it applies
//!
//! - Every variant has at most one field, or several fields of the same alignment, fixed at any
//!   depth, whose every order gives the same bytes: all fields bytes of any value with no rules, or
//!   all the same layout (as [`Layout::rust`] requires of a struct). Such a variant has no field of
//!   size 0. `-Zrandomize-layout` draws the order of the fields of such a variant, but
//!   neither its size nor where it starts, and its layout records that its bytes are not ordered.
//! - No present variant has a field without values that has a size, which `rustc` lays out with
//!   no value of the tag.
//! - The niche of the largest variant is known ([`Niche`]): its scalar is one whose values Rust
//!   restricts and that `puns` knows (`bool`, `char`, `NonZero`, an enum without fields, the tag
//!   of an enum of a specified layout or of an enum this assumption gives), at the same place in
//!   every layout that Rust allows, and it has more free values than any other such scalar of the
//!   variant. Where two have as many, `rustc` chooses one by where it is, which is not assumed. So
//!   a variant of several fields that hold such a scalar is not assumed either.
//! - The tagged layout needs a tag of the width of the first fields, as above. A tag narrower than
//!   the alignment of the fields after it would leave padding before them, which the experiments
//!   refuse already.
//!
//! The layouts measured to build this rule, under `-Zrandomize-layout` with many seeds and on
//! 32-bit and big-endian targets, varied only where a variant has several fields, and there only
//! as this section allows: the order of the fields, and the tag between `u8` and `u16` when the
//! fields have different alignments, which is out of the scope. The value that a niche takes for
//! one variant is the one exception to a single rule that the measurements show, and it is
//! written as such in [`reserve`].

use crate::{
  discover::Desc,
  layout::{Endian, Field, Kind, Layout, Uniform, Valid, Variant, max_value, round_up},
};

/// What is known of the niche of a layout: the scalar in it whose values Rust restricts with the
/// most values left free, which `rustc` takes as the niche of the layout.
#[derive(Copy, Clone)]
enum Niche {
  /// No scalar of the layout has values that Rust restricts, in any layout that Rust allows.
  None,
  /// One scalar: its offset, its size, and its valid range `(start, end)`, which wraps if `start`
  /// is greater than `end`. Its values outside the range are free.
  One(usize, usize, (u128, u128)),
  /// Not known: two such scalars with as many free values, of which `rustc` chooses one by where
  /// they are, a scalar whose values for Rust `puns` does not know, or a scalar whose place differs
  /// between the layouts that Rust allows.
  Unknown,
}

/// Returns the niche of `layout` ([`Niche`]).
///
/// A scalar that accepts every initialized value has none, since every value that it accepts is
/// one that Rust allows. Otherwise its values are known to be those of Rust only if its `Spec` says
/// so (`Spec::validity`), which `bool`, `char`, `NonZero` and the enums without fields that derive
/// `Pun` do, and a scalar made by hand does not, since its values may be kept by its code rather
/// than by Rust. The tag of an enum is one, whose valid range is that of
/// the tags of its variants, and for a niche, of the values of its scalar too. An array has the
/// niche of its first element. A part that is not `fixed` has none known, unless it has no such
/// scalar at all, which does not depend on where its fields are.
const fn niche(layout: &Layout) -> Niche {
  let found = match layout.kind {
    Kind::Scalar(spec) => match (spec.valid, spec.known) {
      // Every initialized value is accepted, so Rust allows it: no niche. So it is for a field
      // restricted by `valid(..)` whose type accepts every value.
      (Valid::Any | Valid::Uninit, _) | (Valid::In(_), Valid::Any) => Niche::None,
      // The values of the type, which Rust allows, in the byte order of the target.
      (Valid::In(_), Valid::In(ranges))
        if spec.validity && (spec.size == 1 || spec.endian.equals(Endian::NATIVE)) =>
      {
        match arc(Set::List(ranges), spec.size) {
          Some(range) => Niche::One(0, spec.size, range),
          None => Niche::None,
        }
      }
      // Values that the code of the type keeps, which Rust may not restrict, or bytes that may be
      // uninitialized, or a scalar that a pun never makes.
      _ => Niche::Unknown,
    },
    Kind::Array(elem, len) => {
      if len == 0 || elem.size == 0 {
        Niche::None
      } else {
        niche(elem)
      }
    }
    Kind::Struct(fields, _) => {
      // The niche with the most free values. `rustc` chooses among equal ones by where they are.
      let (mut found, mut most, mut tied, mut i) = (Niche::None, 0, false, 0);
      while i < fields.len() {
        match niche(fields[i].layout) {
          Niche::None => {}
          Niche::One(offset, size, range) => {
            let free = free(size, range);
            if free > most {
              (found, most, tied) =
                (Niche::One(fields[i].offset + offset, size, range), free, false);
            } else if free == most {
              tied = true;
            }
          }
          Niche::Unknown => return Niche::Unknown,
        }
        i += 1;
      }
      if tied { Niche::Unknown } else { found }
    }
    Kind::Enum(e) => {
      if e.union {
        // `rustc` gives a union no niche.
        Niche::None
      } else {
        let Kind::Scalar(tag) = e.tag.layout.kind else { panic!("the tag of an enum is a scalar") };
        tag_niche(e.variants, e.tag.offset, tag.size)
      }
    }
    Kind::Slice(_) | Kind::Pattern(_) => Niche::Unknown,
  };
  if !layout.stable.fixed && !matches!(found, Niche::None) { Niche::Unknown } else { found }
}

/// Returns the number of values of a scalar of `size` bytes outside the valid range `range`.
const fn free(size: usize, range: (u128, u128)) -> u128 {
  let max = max_value(size);
  max - (range.1.wrapping_sub(range.0) & max)
}

/// Returns the niche of an enum whose tag is `size` bytes at `offset`, of the `variants`.
const fn tag_niche(variants: &'static [Variant], offset: usize, size: usize) -> Niche {
  if size == 0 {
    // No tag: the one variant that has values, if there is one, is the enum.
    let mut i = 0;
    while i < variants.len() {
      if variants[i].is_present() {
        return niche(variants[i].layout);
      }
      i += 1;
    }
    return Niche::None;
  }
  let (mut dataful, mut i) = (variants.len(), 0);
  while i < variants.len() {
    if variants[i].holds_tag {
      dataful = i;
    }
    i += 1;
  }
  let extra = if dataful < variants.len() {
    // The values of the scalar of the niche, which is in the fields of `dataful`, at the tag.
    match niche(variants[dataful].layout) {
      Niche::One(at, width, range) if at == offset && width == size => Some(range),
      _ => return Niche::Unknown,
    }
  } else {
    None
  };
  match arc(Set::Tags(variants, dataful, extra), size) {
    Some(range) => Niche::One(offset, size, range),
    None => Niche::None,
  }
}

/// A set of values of a scalar, as ranges that may overlap, in no order.
#[derive(Copy, Clone)]
enum Set {
  /// The ranges.
  List(&'static [(u128, u128)]),
  /// The tags of the present variants but the `usize`th, and the valid range of a niche, which
  /// wraps if its start is greater than its end.
  Tags(&'static [Variant], usize, Option<(u128, u128)>),
}

impl Set {
  /// Returns the `index`th range of the variant `variant`, or of the list, or of the range of the
  /// niche after the variants (split in two where it wraps), or `None` past the last.
  ///
  /// The ranges are visited by `variant` from 0, and `index` from 0 in each.
  const fn get(&self, variant: usize, index: usize, size: usize) -> Option<(u128, u128)> {
    match *self {
      Self::List(ranges) => {
        if variant == 0 && index < ranges.len() {
          Some(ranges[index])
        } else {
          None
        }
      }
      Self::Tags(variants, skip, extra) => {
        if variant < variants.len() {
          let tags = variants[variant].tags();
          if variant == skip || !variants[variant].is_present() || index >= tags.len() {
            None
          } else {
            Some(tags.get(index))
          }
        } else if variant == variants.len() {
          match extra {
            Some((start, end)) if start <= end => {
              if index == 0 {
                Some((start, end))
              } else {
                None
              }
            }
            Some((start, end)) => match index {
              0 => Some((0, end)),
              1 => Some((start, max_value(size))),
              _ => None,
            },
            None => None,
          }
        } else {
          None
        }
      }
    }
  }

  /// Returns the number of lists of ranges that [`Set::get`] visits.
  const fn lists(&self) -> usize {
    match *self {
      Self::List(_) => 1,
      Self::Tags(variants, ..) => variants.len() + 1,
    }
  }
}

/// Returns the least value of `set` that is at least `from`, if there is one.
const fn least_from(set: Set, from: u128, size: usize) -> Option<u128> {
  let (mut least, mut v) = (None, 0);
  while v < set.lists() {
    let mut i = 0;
    while let Some((lo, hi)) = set.get(v, i, size) {
      let at = if lo >= from {
        Some(lo)
      } else if hi >= from {
        Some(from)
      } else {
        None
      };
      if let Some(at) = at {
        least = match least {
          Some(least) if least <= at => Some(least),
          _ => Some(at),
        };
      }
      i += 1;
    }
    v += 1;
  }
  least
}

/// Returns the last value of the run of consecutive values of `set` that starts at `start`, a value
/// of the set.
const fn run_end(set: Set, start: u128, size: usize) -> u128 {
  let (max, mut end) = (max_value(size), start);
  loop {
    let (mut grown, mut v) = (false, 0);
    while v < set.lists() {
      let mut i = 0;
      while let Some((lo, hi)) = set.get(v, i, size) {
        // A range that starts within the run or right after it, and ends after it.
        if end < max && lo <= end + 1 && hi > end {
          end = hi;
          grown = true;
        }
        i += 1;
      }
      v += 1;
    }
    if !grown {
      return end;
    }
  }
}

/// Returns the valid range of `rustc` for the values of `set`, of a scalar of `size` bytes: the
/// shortest range, wrapping past the greatest value to 0, that holds them all.
///
/// It leaves out the largest gap between the values. The gap that wraps from the greatest value
/// to the least is looked at first, then the others in order, and a later gap must be larger to be
/// taken. This is what `rustc` does where gaps are equal: the values 0 and 128 of a byte give the
/// range 0 to 128, and the values 0, 100, 200 and 255 give the range 100 to 0.
///
/// Returns `None` if the values are every value of the scalar, which leaves no value free, or if
/// `set` is empty, which only an enum without values has, and `rustc` gives such an enum no niche.
const fn arc(set: Set, size: usize) -> Option<(u128, u128)> {
  let max = max_value(size);
  let Some(least) = least_from(set, 0, size) else { return None };
  let mut end = run_end(set, least, size);
  // The first of the largest gaps between two runs, and the range that leaves it out.
  let mut inner: Option<(u128, (u128, u128))> = None;
  while end < max {
    let Some(next) = least_from(set, end + 1, size) else { break };
    let gap = next - end - 1;
    match inner {
      Some((largest, _)) if largest >= gap => {}
      _ => inner = Some((gap, (next, end))),
    }
    end = run_end(set, next, size);
  }
  // The gap that wraps, from the greatest value to the least.
  let mut best = (max - (end - least), (least, end));
  if let Some((gap, range)) = inner
    && gap > best.0
  {
    best = (gap, range);
  }
  if best.0 == 0 { None } else { Some(best.1) }
}

/// Returns the distance to zero of the value `v` of a scalar whose greatest value is `max`, read
/// as a signed integer.
const fn distance_to_zero(v: u128, max: u128) -> u128 {
  if v <= max / 2 { v } else { max - v + 1 }
}

/// Returns the value of the first of `count` variants that take free values of a niche of `size`
/// bytes whose valid range is `range`, or `None` if there are fewer free values.
///
/// For more than one variant, it is as `rustc` computes it: the values after the end of the range,
/// unless the range does not wrap and its start leaves room for them before it, closer to zero, or
/// the values after its end would wrap past zero into it. For one variant, the measurements show
/// the free value next to the range that is closest to zero as a signed integer, the one before
/// the start where both are, with one exception: the range 0 to 1, of a `bool`, takes 2.
const fn reserve(range: (u128, u128), size: usize, count: u128) -> Option<u128> {
  let (start, end, max) = (range.0, range.1, max_value(size));
  if count == 0 || count > free(size, range) {
    return None;
  }
  let before = start.wrapping_sub(count) & max;
  let after = end.wrapping_add(1) & max;
  if count == 1 {
    if start == 0 && end == 1 {
      return Some(2);
    }
    let closer = distance_to_zero(before, max) <= distance_to_zero(after, max);
    return Some(if closer { before } else { after });
  }
  let to_zero = max - end;
  Some(if start > end {
    after
  } else if start <= to_zero {
    if count <= start { before } else { after }
  } else {
    let last = end.wrapping_add(count) & max;
    if last >= 1 && last <= end { before } else { after }
  })
}

/// What the assumption takes from the fields of a variant.
#[derive(Copy, Clone)]
struct Shape {
  /// Whether `rustc` lays the variant out: it is not absent.
  present: bool,
  /// The bytes that its fields take, one right after another.
  size: usize,
  /// The alignment of its fields, 1 if it has none.
  align: usize,
  /// Whether it has a field that is not of size 0 and alignment 1, which is then its first.
  first: bool,
  /// The niche of its fields.
  niche: Niche,
}

impl Shape {
  /// A variant without fields.
  const UNIT: Self = Self { present: true, size: 0, align: 1, first: false, niche: Niche::None };
}

/// Returns what the assumption takes from the `fields` of a variant, or `None` where it does not
/// apply ("Where it applies", above).
const fn shape(fields: &[Field]) -> Option<Shape> {
  let (mut inhabited, mut unit, mut i) = (true, true, 0);
  while i < fields.len() {
    let layout = fields[i].layout;
    inhabited &= layout.is_inhabited();
    unit &= layout.size == 0 && layout.align == 1;
    i += 1;
  }
  if !inhabited {
    // Absent, or laid out with no value of the tag, which is not assumed.
    return if unit { Some(Shape { present: false, ..Shape::UNIT }) } else { None };
  }
  match fields.len() {
    0 => Some(Shape::UNIT),
    1 => {
      let layout = fields[0].layout;
      Some(Shape {
        present: true,
        size: layout.size,
        align: layout.align,
        first: !unit,
        niche: niche(layout),
      })
    }
    _ => {
      // Several fields with a size and the same alignment, whose every order gives the same bytes.
      let first = fields[0].layout;
      let (mut any, mut same, mut size, mut i) = (true, true, 0, 0);
      while i < fields.len() {
        let layout = fields[i].layout;
        // A field whose layout Rust leaves to the compiler may have its bytes or its size differ
        // between layouts, so whether the fields give the same bytes in every order would too.
        if layout.size == 0 || layout.align != first.align || !layout.stable.deep {
          return None;
        }
        any &= matches!(layout.uniform, Uniform::Any) && !layout.has_rules;
        same &= layout.equals(first);
        size += layout.size;
        i += 1;
      }
      if !any && !same {
        return None;
      }
      // Every field holds the same scalars: if one of them restricts its values, `rustc` chooses
      // which field gives the niche.
      let niche = match niche(first) {
        Niche::None => Niche::None,
        _ => Niche::Unknown,
      };
      Some(Shape { present: true, size, align: first.align, first: true, niche })
    }
  }
}

/// Returns the alignment of the unsigned integer of `size` bytes on the target, or 0 if there is
/// no such integer.
const fn int_align(size: usize) -> usize {
  match size {
    1 => align_of::<u8>(),
    2 => align_of::<u16>(),
    4 => align_of::<u32>(),
    8 => align_of::<u64>(),
    16 => align_of::<u128>(),
    _ => 0,
  }
}

/// Returns the greater of `a` and `b`.
const fn larger(a: usize, b: usize) -> usize {
  if a > b { a } else { b }
}

/// The layout that the assumption gives an enum of `V` variants.
struct Assumed<const V: usize> {
  /// The offset and the size of the tag, `(0, 0)` if there is none.
  tag: (usize, usize),
  /// The variant whose fields hold the tag, as a niche, or `V`.
  dataful: usize,
  /// The tag of each present variant but `dataful`.
  tags: [u128; V],
  /// The offset at which the fields of each present variant start.
  starts: [usize; V],
  /// The size and the alignment of the enum.
  size: (usize, usize),
}

/// Returns the layout that the assumption gives an enum of variants of the `fields`, or `None`
/// where it does not apply. Only the layouts of the fields are read, not their offsets.
const fn assume<const V: usize>(fields: &[&[Field]; V]) -> Option<Assumed<V>> {
  let mut shapes = [Shape::UNIT; V];
  let (mut present, mut last, mut v) = (0, 0, 0);
  while v < V {
    let Some(shape) = shape(fields[v]) else { return None };
    shapes[v] = shape;
    if shape.present {
      present += 1;
      last = v;
    }
    v += 1;
  }
  let mut out = Assumed { tag: (0, 0), dataful: V, tags: [0; V], starts: [0; V], size: (0, 1) };
  if present == 0 {
    return None;
  }
  if present == 1 {
    let only = shapes[last];
    let Some(size) = round_up(only.size, only.align) else { return None };
    out.size = (size, only.align);
    return Some(out);
  }
  // The tagged layout.
  let (mut start_align, mut align, mut first, mut v) = (0, 1, V, 0);
  while v < V {
    let shape = shapes[v];
    if shape.present {
      if first == V {
        first = v;
      }
      if shape.first && (start_align == 0 || shape.align < start_align) {
        start_align = shape.align;
      }
      align = larger(align, shape.align);
    }
    v += 1;
  }
  let narrowest = if last < 0x100 {
    1
  } else if last < 0x1_0000 {
    2
  } else {
    4
  };
  let width = if start_align > 0 && int_align(start_align) == start_align {
    larger(start_align, narrowest)
  } else {
    narrowest
  };
  if start_align > width {
    // Padding between the tag and every field, which the experiments refuse.
    return None;
  }
  let tag_align = int_align(width);
  let (mut tagged, mut end, mut v) = (Assumed { tag: (0, width), ..out }, width, 0);
  while v < V {
    let shape = shapes[v];
    if shape.present {
      let Some(start) = round_up(width, shape.align) else { return None };
      (tagged.starts[v], tagged.tags[v]) = (start, v as u128);
      end = larger(end, start + shape.size);
    }
    v += 1;
  }
  let Some(size) = round_up(end, larger(align, tag_align)) else { return None };
  tagged.size = (size, larger(align, tag_align));
  // The values of the tag that no variant takes, outside the range of the present variants.
  let tag_free = max_value(width) - (last - first) as u128;
  // The niche layout, in the largest variant, the last of the largest ones.
  let (mut dataful, mut v) = (first, 0);
  while v < V {
    if shapes[v].present && shapes[v].size >= shapes[dataful].size {
      dataful = v;
    }
    v += 1;
  }
  let (offset, bytes, range) = match shapes[dataful].niche {
    Niche::None => return Some(tagged),
    Niche::Unknown => return None,
    Niche::One(offset, bytes, range) => (offset, bytes, range),
  };
  let (mut low, mut high, mut v) = (V, 0, 0);
  while v < V {
    if shapes[v].present && v != dataful {
      if low == V {
        low = v;
      }
      high = v;
    }
    v += 1;
  }
  let count = (high - low + 1) as u128;
  let Some(niche_start) = reserve(range, bytes, count) else { return Some(tagged) };
  let Some(size) = round_up(shapes[dataful].size, align) else { return None };
  let mut niched = Assumed { tag: (offset, bytes), dataful, size: (size, align), ..out };
  let mut v = 0;
  while v < V {
    let shape = shapes[v];
    if shape.present && v != dataful {
      if shape.size > offset {
        // After the niche.
        let Some(start) = round_up(offset + bytes, shape.align) else { return None };
        if start + shape.size > size {
          return Some(tagged);
        }
        niched.starts[v] = start;
      }
      niched.tags[v] = niche_start.wrapping_add((v - low) as u128) & max_value(bytes);
    }
    v += 1;
  }
  let niche_free = free(bytes, range) - count;
  Some(if size < tagged.size.0 || (size == tagged.size.0 && niche_free > tag_free) {
    niched
  } else {
    tagged
  })
}

/// Returns whether the assumption applies to the enum `E` of variants of the `fields`, at the
/// offsets observed on its samples, that the experiments found as `desc`.
///
/// # Panics
///
/// Panics if the assumption applies and `rustc` laid the enum out otherwise.
pub const fn holds<E, const V: usize>(desc: &Desc<V>, fields: &[&[Field]; V]) -> bool {
  let Some(assumed) = assume(fields) else { return false };
  let mut agrees = size_of::<E>() == assumed.size.0
    && align_of::<E>() == assumed.size.1
    && desc.tag_offset() == assumed.tag.0
    && desc.tag_size() == assumed.tag.1;
  let mut v = 0;
  while v < V {
    let these = fields[v];
    let shape = shape(these);
    let present = matches!(shape, Some(Shape { present: true, .. }));
    agrees &= desc.inhabited(v) == present && desc.holds_tag(v) == (v == assumed.dataful);
    if present && assumed.tag.1 > 0 && v != assumed.dataful {
      agrees &= desc.tag_of(v) == assumed.tags[v];
    }
    let (start, mut i) = (assumed.starts[v], 0);
    while present && i < these.len() {
      let field = these[i];
      agrees &= if these.len() == 1 {
        field.offset == start
      } else {
        let Some(Shape { size, .. }) = shape else { panic!() };
        field.offset >= start && field.offset + field.layout.size <= start + size
      };
      i += 1;
    }
    v += 1;
  }
  assert!(
    agrees,
    "an enum of the `Rust` representation is not laid out as `puns` assumes (see \"What `puns` \
     assumes\" in the crate documentation): `rustc` has changed how it lays out enums; enable the \
     `unstable` feature, which uses the layout that `rustc` chose"
  );
  true
}

/// Returns the least integer size, 1, 2, 4, 8 or 16 bytes, that holds every value of
/// `discriminants`, as unsigned integers if none is negative and signed otherwise.
const fn fit(discriminants: &[i128]) -> usize {
  let (mut least, mut most, mut i) = (0, 0, 0);
  while i < discriminants.len() {
    if discriminants[i] < least {
      least = discriminants[i];
    }
    if discriminants[i] > most {
      most = discriminants[i];
    }
    i += 1;
  }
  let mut size = 1;
  while size < 16 {
    let bits = 8 * size as u32;
    let fits = if least >= 0 {
      most < 1 << bits
    } else {
      least >= -(1 << (bits - 1)) && most < 1 << (bits - 1)
    };
    if fits {
      return size;
    }
    size *= 2;
  }
  16
}

/// Returns whether the assumption holds for the enum without fields `E` of the `Rust`
/// representation, whose variants have the `discriminants` and are stored as the bytes `bits`, read
/// in the byte order of the target.
///
/// `rustc` stores such an enum as its discriminant, in the least integer that holds every
/// discriminant ([`fit`]), signed if one is negative, and with no byte if it has one variant or
/// none.
///
/// # Panics
///
/// Panics if `rustc` laid the enum out otherwise.
pub const fn fieldless<E>(discriminants: &[i128], bits: &[u128]) -> bool {
  let size = if discriminants.len() <= 1 { 0 } else { fit(discriminants) };
  let mut agrees = size_of::<E>() == size && discriminants.len() == bits.len();
  let mut i = 0;
  while agrees && i < bits.len() {
    agrees &= discriminants[i].cast_unsigned() & max_value(size) == bits[i];
    i += 1;
  }
  assert!(
    agrees,
    "an enum without fields of the `Rust` representation is not laid out as `puns` assumes (see \
     \"What `puns` assumes\" in the crate documentation): `rustc` has changed how it lays out \
     enums; enable the `unstable` feature, which uses the layout that `rustc` chose"
  );
  true
}

#[cfg(test)]
mod tests {
  use super::{Set, arc, reserve};

  /// Two values of a byte that the variants of an enum without fields take, the number of
  /// variants without fields of an enum that holds it in its other variant, and the first value of
  /// the niche that `rustc` gave them, as `#[rustc_dump_layout(debug)]` shows on 1.100.0 and on
  /// nightly-2026-11-17.
  const MEASURED: [(u128, u128, u128, u128); 115] = [
    (0, 1, 1, 2),
    (0, 1, 2, 2),
    (0, 1, 3, 2),
    (0, 2, 1, 255),
    (0, 2, 2, 3),
    (0, 2, 3, 3),
    (0, 3, 1, 255),
    (0, 3, 2, 4),
    (0, 3, 3, 4),
    (0, 100, 1, 255),
    (0, 100, 2, 101),
    (0, 100, 3, 101),
    (0, 126, 1, 255),
    (0, 127, 1, 255),
    (0, 127, 2, 128),
    (0, 127, 3, 128),
    (0, 128, 1, 255),
    (0, 128, 2, 129),
    (0, 128, 3, 129),
    (0, 129, 1, 1),
    (0, 129, 2, 1),
    (0, 129, 3, 1),
    (0, 200, 1, 1),
    (0, 200, 2, 1),
    (0, 200, 3, 1),
    (0, 252, 1, 1),
    (0, 252, 2, 1),
    (0, 252, 3, 1),
    (0, 253, 1, 1),
    (0, 253, 2, 1),
    (0, 253, 3, 1),
    (0, 254, 1, 1),
    (0, 254, 2, 1),
    (0, 254, 3, 1),
    (0, 255, 1, 1),
    (0, 255, 2, 1),
    (0, 255, 3, 1),
    (1, 2, 1, 0),
    (1, 2, 2, 3),
    (1, 2, 3, 3),
    (1, 5, 1, 0),
    (1, 128, 1, 0),
    (1, 128, 2, 129),
    (1, 128, 3, 129),
    (1, 129, 1, 0),
    (1, 130, 1, 2),
    (1, 200, 1, 2),
    (1, 200, 2, 2),
    (1, 200, 3, 2),
    (1, 250, 1, 2),
    (1, 250, 2, 2),
    (1, 250, 3, 2),
    (1, 251, 1, 2),
    (1, 253, 1, 2),
    (1, 253, 2, 2),
    (1, 253, 3, 2),
    (1, 254, 1, 2),
    (1, 254, 2, 2),
    (1, 254, 3, 2),
    (1, 255, 1, 254),
    (1, 255, 2, 2),
    (1, 255, 3, 2),
    (2, 3, 1, 1),
    (2, 3, 2, 0),
    (2, 3, 3, 4),
    (2, 254, 1, 253),
    (2, 255, 1, 254),
    (2, 255, 2, 3),
    (2, 255, 3, 3),
    (3, 5, 1, 2),
    (3, 5, 2, 1),
    (3, 5, 3, 0),
    (5, 10, 1, 4),
    (5, 10, 2, 3),
    (5, 10, 3, 2),
    (5, 251, 1, 250),
    (5, 251, 2, 6),
    (5, 251, 3, 6),
    (5, 255, 1, 254),
    (100, 156, 1, 99),
    (100, 156, 2, 157),
    (100, 156, 3, 157),
    (100, 200, 1, 201),
    (100, 200, 2, 201),
    (100, 200, 3, 201),
    (100, 255, 1, 254),
    (125, 254, 1, 253),
    (125, 255, 1, 254),
    (126, 127, 1, 125),
    (126, 127, 2, 124),
    (126, 127, 3, 123),
    (126, 128, 1, 125),
    (126, 255, 1, 254),
    (126, 255, 2, 127),
    (126, 255, 3, 127),
    (127, 128, 1, 126),
    (127, 128, 2, 125),
    (127, 128, 3, 124),
    (127, 129, 1, 126),
    (127, 129, 2, 130),
    (127, 129, 3, 130),
    (128, 129, 1, 130),
    (128, 129, 2, 130),
    (128, 129, 3, 130),
    (128, 255, 1, 0),
    (128, 255, 2, 126),
    (128, 255, 3, 125),
    (200, 255, 1, 0),
    (200, 255, 2, 198),
    (200, 255, 3, 197),
    (252, 255, 1, 0),
    (252, 255, 2, 250),
    (252, 255, 3, 249),
    (253, 255, 1, 0),
    (254, 255, 1, 0),
  ];

  #[test]
  fn niches_take_the_values_that_rustc_gives() {
    for (a, b, count, start) in MEASURED {
      let ranges: &'static [(u128, u128)] = if b == a + 1 {
        Box::leak(Box::new([(a, b)]))
      } else {
        Box::leak(Box::new([(a, a), (b, b)]))
      };
      let Some(range) = arc(Set::List(ranges), 1) else { panic!("no range for {a} and {b}") };
      assert_eq!(reserve(range, 1, count), Some(start), "{a} and {b}, {count} variants");
    }
  }

  #[test]
  fn valid_ranges_are_the_shortest() {
    let arc_of = |ranges: &'static [(u128, u128)], size| arc(Set::List(ranges), size);
    // A `char`: the surrogates are inside.
    assert_eq!(arc_of(&[(0, 0xD7FF), (0xE000, 0x10_FFFF)], 4), Some((0, 0x10_FFFF)));
    // Equal gaps: the one that wraps is left in, else the first is left out.
    assert_eq!(arc_of(&[(0, 0), (128, 128)], 1), Some((0, 128)));
    assert_eq!(arc_of(&[(0, 0), (100, 100), (200, 200), (255, 255)], 1), Some((100, 0)));
    assert_eq!(arc_of(&[(10, 10), (110, 110), (210, 210)], 1), Some((110, 10)));
    assert_eq!(arc_of(&[(0, 0), (129, 129)], 1), Some((129, 0)));
    assert_eq!(arc_of(&[(0, 255)], 1), None);
  }

  // With the `unstable` feature, nothing is assumed: the layout is the one that `rustc` chose.
  #[test]
  #[cfg(not(feature = "unstable"))]
  fn options_and_results_are_assumed_where_the_rule_applies() {
    use core::{convert::Infallible, mem::MaybeUninit, num::NonZero};

    use crate::Pun;

    /// Returns whether the layout of `T` is the same in every layout that Rust allows: for the
    /// types below, whether `puns` assumes it.
    const fn fixed<T: Pun>() -> bool {
      T::LAYOUT.stable.fixed
    }

    // A tag before the field.
    assert!(fixed::<Option<u8>>() && fixed::<Option<u16>>() && fixed::<Option<u128>>());
    assert!(fixed::<Option<[u32; 0]>>() && fixed::<Option<()>>() && fixed::<Option<[u16; 3]>>());
    assert!(fixed::<Option<MaybeUninit<u32>>>() && fixed::<Option<(u8, u16)>>());
    // A niche.
    assert!(fixed::<Option<bool>>() && fixed::<Option<char>>() && fixed::<Option<[bool; 2]>>());
    assert!(fixed::<Option<Option<bool>>>() && fixed::<Option<Option<Option<u16>>>>());
    assert!(fixed::<Option<NonZero<u8>>>() && fixed::<Option<Option<NonZero<u8>>>>());
    // One variant.
    assert!(fixed::<Option<Infallible>>() && fixed::<Result<u32, Infallible>>());
    // Two variants with fields.
    assert!(
      fixed::<Result<u8, u32>>() && fixed::<Result<u32, ()>>() && fixed::<Result<(), bool>>()
    );
    assert!(
      fixed::<Result<bool, u8>>() && fixed::<Result<u8, bool>>() && fixed::<Result<char, ()>>()
    );
    // Not assumed: a niche in a tuple, whose field may be anywhere, two niches with as many free
    // values, and a part whose size differs between layouts.
    assert!(!fixed::<Option<(bool, u16)>>() && !fixed::<Option<(bool, bool)>>());
    assert!(!fixed::<Option<(u8, u16, u8)>>() && !fixed::<Result<(bool, u8), ()>>());
  }
}
