//! The layout of an enum whose representation `rustc` chose (no `repr`), found by experiments in
//! const evaluation on a sample of each variant.
//!
//! Stable Rust names neither the place of the tag of such an enum nor its values, but const
//! evaluation runs code on values of it: the offsets of the fields of a sample (a `match`), its
//! variant (`mem::discriminant`), its bytes. Everything that is concluded is either observed, or
//! a hypothesis that an experiment verifies. When a hypothesis is wrong, the experiment gives
//! another answer (the enum is then refused, with a message) or is an error of const evaluation
//! (the build fails): never a wrong layout. What this rests on, of the compiler:
//!
//! - **R0** the layout of a type is the same in const evaluation and at run time, for a target
//!   (const evaluation computes it for that target, with the same code);
//! - **R1** the variant of a value is told by one scalar at a fixed place, its *tag*: either a
//!   direct tag, before the fields of every variant, of one value per variant, or a *niche*: a
//!   scalar in the fields of one variant (the *dataful* one) that no field of another variant
//!   covers, whose values that are valid for that field are that variant, and whose other values
//!   are each one other variant (or no value). This is how `rustc` lays out enums, stable since
//!   2017 for one variant with fields, since 1.64 for several;
//! - **R2** const evaluation reports as an error reading an uninitialized byte, and reading the
//!   variant of a value whose tag is not the one of a variant that has values;
//! - **R3** `mem::Discriminant<E>` of an enum without `repr` holds its discriminant, which is the
//!   index of the variant, as an `isize` (asserted for its size, and checked on every sample);
//! - **R4** a value made in const evaluation (`E::A(x)`) has its padding uninitialized: making a
//!   variant writes its fields and its tag, nothing else. So a byte of a fresh sample outside its
//!   fields that can be read is a byte of the tag.
//!
//! R2 and R4 are held by compile-fail tests (`discover_reads_padding`, `discover_invalid_tag`),
//! R1 by the experiments themselves (a layout that is not of that shape is refused), R3 by an
//! assertion on every sample. Each experiment, and what it holds:
//!
//! - every sample reads as its own variant: R3 (and that the samples are values);
//! - the bytes of the tag, moved from the sample of each variant to the one of another (`try_tag`),
//!   make it that variant: R1, the tag is there (a wrong candidate changes nothing, and is
//!   dropped; a byte moved that is padding stays padding);
//! - the gaps of each variant that has one tag, outside the tag, filled with zeros then with ones,
//!   leave it that variant (`decided_by`): R1, nothing else tells it (a byte of a tag outside would
//!   take two values for one variant); and the same of every gap of the dataful variant of a niche,
//!   and of the only variant of an enum laid out as a struct: R1, its fields tell it alone;
//! - the tag of each variant but the dataful one is read in its sample, and must be initialized:
//!   R4 makes it a byte of the tag, R2 fails the build if it is padding; for a direct tag, every
//!   byte before the first field of any variant is read so (which `rustc` widens the tag to);
//! - the tags differ, and are not values of the scalar of a niche: R1.
//!
//! The library's own validity of a type is a subset of the one of `rustc` (what `Pun::LAYOUT`
//! promises): the values that `rustc` gives to the other variants of a niche are invalid for the
//! field that holds it, so "every other value" is the dataful variant once its fields are valid.

use core::mem::{self, MaybeUninit};

use crate::layout::{Endian, Field, Kind, Layout, Spec, Valid, load, max_value};

/// A valid `T`: every scalar the least of its values, padding left uninitialized.
///
/// # Panics
/// If `T` has no value, or holds a self-relative pointer (no value of one is made of nothing).
pub const fn sample<T: crate::Pun>() -> MaybeUninit<T> {
  assert!(T::LAYOUT.is_inhabited(), "a type without values has no sample");
  let mut value = MaybeUninit::<T>::uninit();
  // SAFETY: the bytes of the value, as its layout tells.
  unsafe { write(&T::LAYOUT, value.as_mut_ptr().cast()) };
  value
}

/// Write a value of `layout` at `ptr`.
///
/// # Safety
/// `ptr` must be valid for writes of the size of `layout`.
const unsafe fn write(layout: &Layout, ptr: *mut u8) {
  // SAFETY: in the bounds of the value, as the layout tells.
  unsafe {
    match layout.kind {
      Kind::Scalar(spec) => match spec.valid {
        Valid::Uninit => {}
        Valid::Any => store(ptr, spec.size, spec.endian, 0),
        Valid::In(ranges) => store(ptr, spec.size, spec.endian, ranges[0].0),
      },
      Kind::Array(elem, len) => {
        if len == 0 || elem.size == 0 {
          return;
        }
        write(elem, ptr);
        // As many copies as the elements, doubled each time: no step per element.
        let mut done = 1;
        while done < len {
          let count = if done < len - done { done } else { len - done };
          core::ptr::copy_nonoverlapping(ptr, ptr.add(done * elem.size), count * elem.size);
          done += count;
        }
      }
      Kind::Struct(fields, _) => {
        let mut i = 0;
        while i < fields.len() {
          write(fields[i].layout, ptr.add(fields[i].offset));
          i += 1;
        }
      }
      Kind::Enum(layout) => {
        let mut i = 0;
        while !layout.variants[i].is_present() {
          i += 1;
        }
        let (variant, tag) = (&layout.variants[i], &layout.tag);
        if !variant.holds_tag
          && let Kind::Scalar(spec) = tag.layout.kind
        {
          store(ptr.add(tag.offset), spec.size, spec.endian, variant.tags().get(0).0);
        }
        // The variant that holds a niche: its fields are a value of it.
        write(variant.layout, ptr);
      }
      Kind::Pointer(_) => panic!("a self-relative pointer is not made of nothing"),
      Kind::Slice(_) | Kind::Pattern(_) => panic!("a field of an enum is sized"),
    }
  }
}

/// Write the integer `value` of `size` bytes at `ptr`, in `endian` byte order.
///
/// # Safety
/// `ptr` must be valid for writes of `size` bytes.
const unsafe fn store(ptr: *mut u8, size: usize, endian: Endian, value: u128) {
  let mut i = 0;
  while i < size {
    let digit = if endian.is_little() { i } else { size - 1 - i };
    // SAFETY: `i < size`.
    unsafe { ptr.add(i).write((value >> (8 * digit)) as u8) };
    i += 1;
  }
}

/// The variant (its index) of the `E` at `ptr`.
///
/// A failure of const evaluation if its tag is not the one of a variant that has values (R2).
///
/// # Safety
/// `ptr` must point to an `E` whose tag is initialized, and whose fields are valid for the variant
/// its tag tells, if any.
const unsafe fn variant<E>(ptr: *const E) -> usize {
  // SAFETY: guaranteed by the caller.
  let discriminant = mem::discriminant(unsafe { &*ptr });
  // R3: an `isize`, the index of the variant. Its size here, its value on every sample.
  assert!(
    size_of::<mem::Discriminant<E>>() == size_of::<isize>(),
    "the discriminant of an enum without `repr` is not an `isize`"
  );
  // SAFETY: an `isize`, as asserted.
  let index = unsafe { core::ptr::from_ref(&discriminant).cast::<isize>().read() };
  index.cast_unsigned()
}

/// A copy of a sample, to experiment on.
struct Buf<E>(MaybeUninit<E>);

impl<E> Buf<E> {
  const fn of(sample: &MaybeUninit<E>) -> Self {
    let mut copy = MaybeUninit::<E>::uninit();
    // SAFETY: an untyped copy: what is uninitialized stays so.
    unsafe { core::ptr::copy_nonoverlapping(sample.as_ptr(), copy.as_mut_ptr(), 1) };
    Self(copy)
  }

  /// Copy the `len` bytes at `offset` of `from` (initialized or not) over the same bytes.
  const fn transplant(&mut self, from: &MaybeUninit<E>, offset: usize, len: usize) {
    assert!(offset + len <= size_of::<E>());
    // SAFETY: in bounds of both, which do not overlap.
    unsafe {
      core::ptr::copy_nonoverlapping(
        from.as_ptr().cast::<u8>().add(offset),
        self.0.as_mut_ptr().cast::<u8>().add(offset),
        len,
      );
    }
  }

  const fn fill(&mut self, offset: usize, len: usize, byte: u8) {
    assert!(offset + len <= size_of::<E>());
    // SAFETY: in bounds.
    unsafe { core::ptr::write_bytes(self.0.as_mut_ptr().cast::<u8>().add(offset), byte, len) };
  }

  /// The variant of the value: a failure of const evaluation if its tag is not valid.
  const fn variant(&self) -> usize {
    // SAFETY: experiments only write over what is not a field of the variant of the sample (or
    // a tag that is copied whole): its fields are valid, and its tag is initialized (or the
    // build fails, R2).
    unsafe { variant(self.0.as_ptr()) }
  }
}

/// What is known of the layout of an enum of `V` variants.
#[doc(hidden)]
#[derive(Copy, Clone, Debug)]
pub struct Desc<const V: usize> {
  /// The offset and the size of the tag: `(0, 0)` if there is none (a single variant laid out as
  /// a struct, or no variant that has values).
  tag: (usize, usize),
  /// The variant that holds the tag (a niche), `V` if none.
  dataful: usize,
  /// The variants that have values.
  inhabited: [bool; V],
  /// The tag of each variant that has values but the dataful one, as a range of one value.
  tags: [(u128, u128); V],
  /// The tags of the dataful variant: every value but the tags of the others.
  dataful_tags: ([(u128, u128); V], usize),
  /// Or better, the values of the scalar of its field that holds the niche, if they are known as
  /// tags are read: the others are none of them (R1, and the library's validity is in the one of
  /// `rustc`).
  dataful_known: Option<&'static [(u128, u128)]>,
  /// The valid values of the tag, sorted.
  valid: ([(u128, u128); V], usize),
}

impl<const V: usize> Desc<V> {
  /// No variant that has values: what an enum whose layout is given otherwise is found to be
  /// (`Option` of a self-relative pointer, see `Pun::OPTION`), so that nothing is tried of it.
  #[must_use]
  pub const fn none() -> Self {
    Self {
      tag: (0, 0),
      dataful: V,
      inhabited: [false; V],
      tags: [(0, 0); V],
      dataful_tags: ([(0, 0); V], 0),
      dataful_known: None,
      valid: ([(0, 0); V], 0),
    }
  }

  /// The offset of the tag.
  #[must_use]
  pub const fn tag_offset(&self) -> usize {
    self.tag.0
  }

  /// Whatever the `variant`th variant holds the tag (a niche).
  #[must_use]
  pub const fn holds_tag(&self, variant: usize) -> bool {
    variant == self.dataful
  }

  /// Whatever the `variant`th variant has values.
  #[must_use]
  pub const fn inhabited(&self, variant: usize) -> bool {
    self.inhabited[variant]
  }

  /// The tags of the `variant`th variant, sorted: none if it has no value.
  #[must_use]
  pub const fn variant_ranges(&'static self, variant: usize) -> &'static [(u128, u128)] {
    if !self.inhabited[variant] {
      &[]
    } else if variant == self.dataful {
      match self.dataful_known {
        Some(ranges) => ranges,
        None => self.dataful_tags.0.split_at(self.dataful_tags.1).0,
      }
    } else {
      self.tags.split_at(variant).1.split_at(1).0
    }
  }

  /// The layout of the tag: a scalar of native byte order whose values are the tags.
  #[must_use]
  pub const fn tag(&'static self) -> Layout {
    let (size, ranges) = (self.tag.1, self.valid.0.split_at(self.valid.1).0);
    let full = ranges.len() == 1 && ranges[0].0 == 0 && ranges[0].1 == max_value(size);
    let spec = if size == 0 || ranges.is_empty() || full {
      Spec::of_size(size)
    } else {
      Spec::of_size(size).with_ranges(ranges)
    };
    Layout::tag(spec.not_a_number())
  }
}

/// Whatever some field of `fields` covers a byte of `offset..offset + len`.
const fn covers(fields: &[Field], offset: usize, len: usize) -> bool {
  let mut i = 0;
  while i < fields.len() {
    let field = &fields[i];
    if field.layout.size > 0
      && field.offset < offset + len
      && offset < field.offset + field.layout.size
    {
      return true;
    }
    i += 1;
  }
  false
}

/// Fill the bytes of `buf` that no field of `fields` covers, but `skip..skip + len`, with `byte`:
/// one write for each gap.
const fn fill_gaps<E>(buf: &mut Buf<E>, fields: &[Field], skip: usize, len: usize, byte: u8) {
  let (size, mut pos) = (size_of::<E>(), 0);
  while pos < size {
    // The field that `pos` is in, else the first one after it.
    let (mut next, mut i, mut inside) = (size, 0, false);
    while i < fields.len() {
      let field = &fields[i];
      let end = field.offset + field.layout.size;
      if field.layout.size > 0 && field.offset <= pos && pos < end {
        (pos, inside) = (end, true);
        break;
      }
      if field.layout.size > 0 && field.offset > pos && field.offset < next {
        next = field.offset;
      }
      i += 1;
    }
    if inside {
      continue;
    }
    // The gap `pos..next`, but the bytes skipped.
    let (stop, resume) = (skip, skip + len);
    if next <= stop || pos >= resume || len == 0 {
      buf.fill(pos, next - pos, byte);
    } else {
      if pos < stop {
        buf.fill(pos, stop - pos, byte);
      }
      if next > resume {
        buf.fill(resume, next - resume, byte);
      }
    }
    pos = next;
  }
}

/// Nothing but `offset..offset + len` tells the variant `v` from the others: its gaps outside of
/// these bytes, filled with zeros then with ones, leave it the same variant. A byte of a tag that
/// tells `v` has one value in `v`, not two.
const fn decided_by<E>(
  sample: &MaybeUninit<E>,
  fields: &[Field],
  v: usize,
  offset: usize,
  len: usize,
) -> bool {
  let (mut zeros, mut ones) = (Buf::of(sample), Buf::of(sample));
  fill_gaps(&mut zeros, fields, offset, len, 0);
  fill_gaps(&mut ones, fields, offset, len, 0xff);
  zeros.variant() == v && ones.variant() == v
}

/// The integer of `len` bytes at `offset` of `sample`, in native byte order: a failure of const
/// evaluation if one of them is not initialized (R2), which, out of the fields of the variant of
/// the sample, makes it a byte of the tag (R4).
const fn read<E>(sample: &MaybeUninit<E>, offset: usize, len: usize) -> u128 {
  assert!(offset + len <= size_of::<E>() && len <= 16);
  // SAFETY: in bounds; initialized, or the build fails.
  unsafe { load(sample.as_ptr().cast::<u8>().add(offset), len, Endian::NATIVE) }
}

/// Find the layout of the enum `E` from a sample of each variant that has values (`samples[v]`
/// is the `v`th variant in the order of declarations, anything if it has no value) and the
/// fields of each, observed on its sample.
///
/// Evaluated in constants only (`Discover::DESC`, and the ones of `Option` and `Result`): an
/// experiment that reads what it must not fails the build there (R2), where at run time it would
/// be undefined behavior.
///
/// # Panics
/// If the layout is not of a shape that `rustc` gives (R1), with why; or a failure of const
/// evaluation (R2) if an experiment reads what it must not.
#[expect(
  clippy::many_single_char_names,
  reason = "`v`, `u` and `d` are variants, `i` and `c` fields and candidates, by their index"
)]
pub const fn discover<E, const V: usize>(
  samples: &[MaybeUninit<E>; V],
  fields: &[&[Field]; V],
) -> Desc<V> {
  let (size, mut desc) = (size_of::<E>(), Desc::none());
  // Field: what has values, where the fields are; each sample is its own variant (R3).
  let (mut count, mut first, mut sized, mut start, mut v) = (0, V, false, size, 0);
  while v < V {
    let (these, mut i) = (fields[v], 0);
    let mut inhabited = true;
    while i < these.len() {
      let field = &these[i];
      assert!(
        !field.layout.has_pointers(),
        "an enum without `repr` holds no self-relative pointer"
      );
      inhabited &= field.layout.is_inhabited();
      i += 1;
    }
    if inhabited {
      // SAFETY: the sample of a variant that has values is one.
      assert!(unsafe { variant(samples[v].as_ptr()) } == v, "the discriminant is not the index");
      let mut i = 0;
      while i < these.len() {
        let field = &these[i];
        assert!(field.offset + field.layout.size <= size, "a field is out of the enum");
        assert!(!covers(these.split_at(i).0, field.offset, field.layout.size), "fields overlap");
        if field.layout.size > 0 && field.offset < start {
          start = field.offset;
        }
        i += 1;
      }
      desc.inhabited[v] = true;
      if first == V {
        first = v;
      }
      count += 1;
    } else {
      // A variant without values that `rustc` lays out all the same: one with a field that has
      // a size.
      let mut i = 0;
      while i < these.len() {
        sized |= these[i].layout.size > 0;
        i += 1;
      }
    }
    v += 1;
  }
  if count == 0 {
    // No value: no tag, no variant.
    return desc;
  }
  if count == 1 && (!sized || start == 0) {
    // One variant that has values, and no other laid out (it is then a struct), or one whose
    // fields start at 0, where a direct tag would be: a struct, or a niche in its fields, all of
    // whose valid values are this variant. Either way its fields tell it, nothing else: checked.
    let these = fields[first];
    assert!(
      decided_by(&samples[first], these, first, 0, 0),
      "the only variant that has values is told by bytes that are not its fields"
    );
    desc.tags[first] = (0, 0);
    desc.valid = ([(0, 0); V], 1);
    return desc;
  }
  if start > 0 {
    // A direct tag: before the fields of every variant. `rustc` widens it to all the bytes before
    // the first field, which are then read in every sample: a byte that is not of the tag is
    // padding, uninitialized, and fails the build (R2, R4).
    assert!(
      matches!(start, 1 | 2 | 4 | 8 | 16),
      "the bytes before the fields of an enum without `repr` are not an integer: unsupported layout"
    );
    try_tag(&mut desc, samples, fields, (0, start, None), V);
    assert!(desc.tag.1 > 0, "the tag of the enum does not tell its variants: unsupported layout");
    return desc;
  }
  // A niche: in the fields of one variant, not covered by the fields of any other. `rustc` puts it
  // in the largest variant: its scalars are tried first (the order only changes the cost).
  let (mut largest, mut most, mut v) = (first, 0, 0);
  while v < V {
    if desc.inhabited[v] {
      let (these, mut bytes, mut i) = (fields[v], 0, 0);
      while i < these.len() {
        bytes += these[i].layout.size;
        i += 1;
      }
      if bytes > most {
        (largest, most) = (v, bytes);
      }
    }
    v += 1;
  }
  let mut round = 0;
  while round <= V {
    let d = if round == 0 { largest } else { round - 1 };
    round += 1;
    if !desc.inhabited[d] || (round > 1 && d == largest) {
      continue;
    }
    let (these, mut i) = (fields[d], 0);
    while i < these.len() {
      let mut candidates = Candidates::NEW;
      candidates.collect(these[i].layout, these[i].offset);
      let mut c = 0;
      while c < candidates.len {
        let (offset, len, known) = candidates.list[c];
        c += 1;
        let mut free = true;
        let mut u = 0;
        while u < V {
          if u != d && desc.inhabited[u] && covers(fields[u], offset, len) {
            free = false;
          }
          u += 1;
        }
        if free && try_tag(&mut desc, samples, fields, (offset, len, known), d) {
          return desc;
        }
      }
      i += 1;
    }
  }
  panic!("no tag found in the enum: unsupported layout")
}

/// Try the bytes `offset..offset + len` of `window` as the tag, in the fields of `dataful` (a
/// niche, whose values are the last of `window` if they are known) or of none (`V`, a direct
/// tag): `desc` is filled if they are.
///
/// They are if moving them from the sample of each variant to the one of another variant makes
/// it that variant (without failing: a byte of a niche is moved from a variant to one where it is
/// not a field), if nothing else tells a variant from the others, and if each variant but the
/// dataful one has one value there, all of whose bytes are initialized (R4).
const fn try_tag<E, const V: usize>(
  desc: &mut Desc<V>,
  samples: &[MaybeUninit<E>; V],
  fields: &[&[Field]; V],
  window: (usize, usize, Option<&'static [(u128, u128)]>),
  dataful: usize,
) -> bool {
  let (offset, len, known) = window;
  // A variant that has values, not the dataful one: the tag is not one of its fields.
  let mut base = 0;
  while base < V && (!desc.inhabited[base] || base == dataful) {
    base += 1;
  }
  assert!(base < V, "no variant without the tag");
  let mut r = 0;
  while r < V {
    if desc.inhabited[r] && r != base {
      let mut buf = Buf::of(&samples[base]);
      buf.transplant(&samples[r], offset, len);
      if buf.variant() != r {
        return false;
      }
    }
    r += 1;
  }
  // Nothing else tells the variants that have one tag from the others.
  let mut v = 0;
  while v < V {
    if desc.inhabited[v]
      && v != dataful
      && (covers(fields[v], offset, len) || !decided_by(&samples[v], fields[v], v, offset, len))
    {
      return false;
    }
    v += 1;
  }
  // Nor anything outside its fields the dataful variant: its gaps filled with zeros, then with
  // ones, leave it that variant (R1 says that a niche is told by its scalar alone; a layout in
  // which another place takes part, a two-level encoding say, is refused here).
  if dataful < V && !decided_by(&samples[dataful], fields[dataful], dataful, 0, 0) {
    return false;
  }
  // Their tags: initialized, so bytes of the tag (R4), and different.
  let (mut values, mut single, mut v) = ([0; V], [false; V], 0);
  while v < V {
    if desc.inhabited[v] && v != dataful {
      let value = read(&samples[v], offset, len);
      let mut u = 0;
      while u < v {
        assert!(!single[u] || values[u] != value, "two variants have the same tag");
        u += 1;
      }
      (values[v], single[v], desc.tags[v]) = (value, true, (value, value));
    }
    v += 1;
  }
  // The variants without a single tag take the one of `base`, which merges with it.
  let mut v = 0;
  while v < V {
    if !single[v] {
      values[v] = values[base];
    }
    v += 1;
  }
  let (singles, n) = crate::layout::sorted_ranges(values);
  if dataful == V {
    let mut i = 0;
    while i < n {
      desc.valid.0[i] = singles[i];
      i += 1;
    }
    desc.valid.1 = n;
  } else {
    // Every other value of the scalar, of which its field rejects the ones that are not.
    let (max, mut next, mut i, mut out) = (max_value(len), 0u128, 0, 0);
    let mut done = false;
    while i < n && !done {
      let (lo, hi) = singles[i];
      if lo > next {
        desc.dataful_tags.0[out] = (next, lo - 1);
        out += 1;
      }
      if hi == max {
        done = true;
      } else {
        next = hi + 1;
      }
      i += 1;
    }
    if !done {
      desc.dataful_tags.0[out] = (next, max);
      out += 1;
    }
    desc.dataful_tags.1 = out;
    // The values of the scalar, which the tags of the others are not.
    if let Some(ranges) = known {
      let mut v = 0;
      while v < V {
        assert!(
          !single[v] || !crate::layout::contains(ranges, values[v]),
          "the tag of a variant is a value of the scalar that holds it: unsupported layout"
        );
        v += 1;
      }
      desc.dataful_known = Some(ranges);
    }
    desc.valid = ([(0, 0); V], 1);
    desc.valid.0[0] = (0, max);
  }
  (desc.tag, desc.dataful) = ((offset, len), dataful);
  true
}

/// The scalars of a field that may hold a niche: those whose values the type of the field
/// restricts (not a `valid(..)` that safe code can break), and tags of enums.
///
/// Each with its values, if they are known as a tag is read (in native byte order).
struct Candidates {
  list: [Candidate; 16],
  len: usize,
}

/// The offset and the size of a scalar, and its values if they are known as a tag is read.
type Candidate = (usize, usize, Option<&'static [(u128, u128)]>);

impl Candidates {
  const NEW: Self = Self { list: [(0, 0, None); 16], len: 0 };

  const fn push(&mut self, offset: usize, spec: Spec) {
    if self.len < self.list.len() && spec.size > 0 && spec.size <= 16 {
      let native = spec.size == 1 || spec.endian.equals(Endian::NATIVE);
      let known = match spec.known {
        Valid::In(ranges) if native => Some(ranges),
        _ => None,
      };
      self.list[self.len] = (offset, spec.size, known);
      self.len += 1;
    }
  }

  const fn collect(&mut self, layout: &'static Layout, base: usize) {
    match layout.kind {
      Kind::Scalar(spec) => {
        if matches!(spec.known, Valid::In(_)) {
          self.push(base, spec);
        }
      }
      // `rustc` takes the niche of the first element.
      Kind::Array(elem, len) => {
        if len > 0 {
          self.collect(elem, base);
        }
      }
      Kind::Struct(fields, _) => {
        let mut i = 0;
        while i < fields.len() {
          if !fields[i].restricted {
            self.collect(fields[i].layout, base + fields[i].offset);
          }
          i += 1;
        }
      }
      Kind::Enum(layout) => {
        if let Kind::Scalar(spec) = layout.tag.layout.kind {
          self.push(base + layout.tag.offset, spec);
        }
      }
      Kind::Slice(_) | Kind::Pattern(_) | Kind::Pointer(_) => {}
    }
  }
}
