//! The layout of an enum of the `Rust` representation, found by experiments in const evaluation on
//! a sample of each variant.
//!
//! Rust does not specify the layout of an enum with fields, of the `Rust` representation. Stable
//! Rust names neither where its tag is nor its values. `offset_of!` on variants,
//! [`mem::variant_count`] and [`discriminant_value`] are unstable, and nothing tells a niche. But
//! const evaluation runs code on values of the enum. `#[derive(Pun)]` makes a *sample* of each
//! variant ([`sample`]), with every scalar of each field at the least of its values and the padding
//! uninitialized. It finds the offsets of the fields by matching on the sample. [`discover`] finds
//! the rest by experiments on copies of the samples, which read the variant with
//! [`mem::discriminant`] and read bytes. Every conclusion is either observed, or a hypothesis that
//! an experiment verifies. When a hypothesis is wrong, the experiment gives another answer, and the
//! enum is refused with a message ("no tag found", "unsupported layout", and so on), or it is an
//! error of const evaluation, and the build fails. It never finds a wrong layout. Most experiments
//! copy a sample, change some of its bytes, and read the variant of the copy. The others read bytes
//! of a sample. The number of experiments grows with the number of variants and of the scalars that
//! may hold a niche, not with the size of the fields.
//!
//! # Cases
//!
//! - **No variant has values.** The enum has no tag and no value.
//! - **One variant has values**, and either no other variant is laid out, or the fields of that
//!   variant start at 0. `rustc` lays out a variant without values only if one of its fields has a
//!   size. Fields at 0 mean a niche in its own fields, or none. The enum is then a struct of the
//!   fields of that variant. Its gaps are filled with zeros, then with ones, and it must stay that
//!   variant.
//! - **A direct tag** is used when the fields of no variant start at 0. `rustc` puts the tag before
//!   the fields, widened to all the bytes before the first field of any variant. For example, `enum
//!   { A(u16), B(u32) }` has a tag of 2 bytes, in which the high byte of each value is 0. Those
//!   bytes are *read* in every sample. A byte that is not part of the tag would be padding,
//!   uninitialized, and reading it fails the build ([Const evaluation fails on what is not a
//!   value][fails], [A fresh value has uninitialized padding][fresh]). So the tag is exact on
//!   either byte order. No claim is made of a byte that may be padding, and the tag is an integer
//!   of native byte order wherever its low byte is.
//! - **A niche** is used when some field starts at 0. The candidates are the scalars of the fields
//!   of each variant that the type restricts, and that the fields of no other variant cover. The
//!   scalars that the type restricts are `bool`, `char`, `NonZero`, enums without fields, and tags
//!   of enums, but not a `valid(..)`, which safe code can break. They are tried for the largest
//!   variant first, the *dataful* one. The bytes of the scalar are moved from the sample of each
//!   variant to the sample of a variant that does not hold them, which must become that variant. A
//!   wrong scalar changes nothing there, and is dropped without failing. Then the tag of each other
//!   variant is read there. It is initialized, so it is the tag ([fresh]). The dataful variant has
//!   the values of that scalar, which the tags of the other variants must not be.
//!
//! In both cases, the gaps of each variant that has one tag, outside the tag, are filled with zeros
//! and then with ones, which must leave it that variant, since a byte of a tag outside would give
//! two values to one variant. All the gaps of the dataful variant of a niche are filled the same
//! way, since its scalar must tell it alone, and a layout in which another place takes part, such
//! as an encoding in two levels, is refused. The tags must differ, and what is found is checked
//! against the size and the alignment of the enum (`Layout::enumeration`).
//!
//! Such an enum is then what an enum of a specified layout is. It is proven variant by variant, and
//! checked by its tag and its variant, with the same precision. A variant whose fields have no
//! value is in the layout, with no tag. The lists of fields and variants are the declared ones. A
//! field of a variant without values is not in the layout, and its entry in the list says so (its
//! `FIELD` is `None`). The sample of a variant cannot hold a scalar that a pun never makes, because
//! `puns` cannot make a value of one ([`sample`]). `Option<T>` and `Result<T, E>` are found the
//! same way (`std_enums.rs`).
//!
//! # What the experiments rely on
//!
//! The experiments rely on these facts of the compiler, and on one fact of this crate.
//!
//! ## The layout is that of the target
//!
//! The layout of a type is the same in const evaluation as at run time, for a given target. Const
//! evaluation computes it for that target, with the same code. This is a property of how `rustc` is
//! built.
//!
//! ## One scalar tells the variant
//!
//! The variant of a value is told by one scalar at a fixed place, its *tag*. Either it is a direct
//! tag before the fields of every variant, with one value for each variant. Or it is a *niche*, a
//! scalar in the fields of one variant, the dataful one, that no field of another variant covers.
//! The values of the niche that are valid for its field are that variant, and each other value is
//! one other variant, or no value. This is how `rustc` lays out enums, since 2017 for one variant
//! with fields and since Rust 1.65 for several. It is not a promise of the language. The
//! experiments verify what they rely on of it. The tag decides, and nothing outside the tag does. A
//! layout of another shape is refused.
//!
//! ## Const evaluation fails on what is not a value
//!
//! Const evaluation reports an error when it reads an uninitialized byte, and when it reads the
//! variant of a value whose tag is not the tag of a variant that has values. This is a behavior of
//! the compiler, which the Reference does not promise for undefined behavior in `const`. The
//! compile-fail tests `discover_reads_padding` and `discover_invalid_tag` check this behavior.
//!
//! ## The variant is read from the tag
//!
//! When const evaluation reads the variant of a value whose tag is the tag of a variant that has
//! values, it reads the tag alone, and neither reads nor checks the other bytes of the value. Yet
//! `mem::discriminant` takes a reference to the whole value, and the Reference requires a reference
//! to point to a valid value. [`try_tag`] relies on this behavior of the compiler: the bytes of the
//! tag of one variant, moved into the sample of another, make a value whose tag is valid and whose
//! other bytes may be no valid fields of the variant that the tag tells. The model check compares
//! the layouts that the experiments find with oracles that do not use them, for its enums of the
//! `Rust` representation.
//!
//! ## The discriminant is the index
//!
//! `mem::Discriminant<E>` of an enum of the `Rust` representation holds its discriminant, which is
//! the index of the variant, as an `isize`. No explicit discriminant is allowed on such an enum
//! with fields. The size of `Discriminant<E>` is asserted, and the discriminant of every sample is
//! checked to be its own index.
//!
//! ## A fresh value has uninitialized padding
//!
//! A value made in const evaluation (`E::A(x)`) has its padding uninitialized, because making a
//! variant writes its fields and its tag and nothing else. So a byte of a fresh sample outside its
//! fields that can be read is a byte of the tag, which the compile-fail test
//! `discover_reads_padding` checks.
//!
//! ## The valid values of a layout are valid for `rustc`
//!
//! The valid values of a type, as `Pun::LAYOUT` gives them, are valid for `rustc`, as `Pun`
//! promises. So the values that `rustc` gives to the other variants of a niche, which are invalid
//! for the field that holds the niche, are not values of that field in its layout. "Each other
//! value" is then the dataful variant once its fields are valid.
//!
//! # What each experiment holds
//!
//! - Every sample is its own variant. This verifies [The discriminant is the index][index], and
//!   that the samples are values.
//! - The bytes of the tag, moved from the sample of each variant to the sample of another
//!   ([`try_tag`]), make it that variant. This verifies [One scalar tells the variant][one]: the
//!   tag is there. A wrong candidate changes nothing, and is dropped. A byte that is moved and is
//!   padding stays padding. The experiment relies on [the variant is read from the tag][tag].
//! - The gaps of each variant that has one tag, outside the tag, filled with zeros and then with
//!   ones, leave it that variant ([`decided_by`]). This verifies [one]: nothing else tells the
//!   variant, since a byte of a tag outside would take two values for one variant. The same is
//!   verified of every gap of the dataful variant of a niche, and of the only variant of an enum
//!   laid out as a struct: its fields alone tell it.
//! - The tag of each variant but the dataful one is read in its sample, and must be initialized. By
//!   [A fresh value has uninitialized padding][fresh], it is then a byte of the tag, and by [Const
//!   evaluation fails on what is not a value][fails], the build fails if it is padding. For a
//!   direct tag, every byte before the first field of any variant is read so, since `rustc` widens
//!   the tag to these bytes.
//! - The tags differ, and are not values of the scalar of a niche. This verifies [one].
//! - A layout that is not of the shape of [one] is refused. An assertion of the size of
//!   `mem::Discriminant<E>`, and the check of every sample, verify [index].
//!
//! [one]: #one-scalar-tells-the-variant
//! [fails]: #const-evaluation-fails-on-what-is-not-a-value
//! [tag]: #the-variant-is-read-from-the-tag
//! [index]: #the-discriminant-is-the-index
//! [fresh]: #a-fresh-value-has-uninitialized-padding
//! [`mem::variant_count`]: https://doc.rust-lang.org/core/mem/fn.variant_count.html
//! [`discriminant_value`]: https://doc.rust-lang.org/core/intrinsics/fn.discriminant_value.html
//! [`mem::discriminant`]: https://doc.rust-lang.org/core/mem/fn.discriminant.html

use core::mem::{self, MaybeUninit};

use crate::layout::{Endian, Field, Kind, Layout, Spec, Valid, load, max_value};

/// Returns a `T` that is valid in the sense of the Reference, whose rules may not hold, in which
/// every scalar is the least of its values and padding is uninitialized.
///
/// Of an enum, the sample is its first variant that has values.
///
/// # Panics
///
/// Panics if `T` has no value, or if its sample holds a scalar that a pun never makes, since no
/// value of one is made of nothing.
pub const fn sample<T: crate::Pun>() -> MaybeUninit<T> {
  assert!(T::LAYOUT.is_inhabited(), "a type without values has no sample");
  let mut value = MaybeUninit::<T>::uninit();
  // SAFETY: `value` is a local `MaybeUninit<T>`, valid for writes of `size_of::<T>()` bytes,
  // which is the size of `T::LAYOUT` (`Pun`, section Safety). This is what `write` requires.
  unsafe { write(&T::LAYOUT, value.as_mut_ptr().cast()) };
  value
}

/// Writes the least value of `layout` at `ptr`, and leaves its padding as it is.
///
/// # Panics
///
/// Panics if the value that it writes holds a scalar that a pun never makes, a slice or a pattern.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `ptr` is valid for writes of the size of `layout`.
const unsafe fn write(layout: &Layout, ptr: *mut u8) {
  // SAFETY: `ptr` is valid for writes of `layout.size` bytes, by contract on the caller. Every
  // write is within them, as the layout tells:
  // - A scalar is `spec.size` bytes, the size of the layout, as `store` requires.
  // - An array is `len` elements of `elem.size` bytes. `write` of the first is within it. Each
  //   copy reads `count * elem.size` bytes from `ptr`, within the `done` elements written, since
  //   `count <= done`, and writes as many from element `done`, within the `len` elements, since
  //   `count <= len - done`. The two regions do not overlap, since the first ends before element
  //   `done` and the second starts there. Both are of `u8`s, aligned at any address. These are
  //   the requirements of `copy_nonoverlapping`, which copies bytes whether they are initialized
  //   or not.
  // - A field of a struct is within the size of the struct, which `Layout::structure` asserts,
  //   so `add` stays within the value, and `write` of the field is within it.
  // - The tag of an enum, and each of its variants, are within the size of the enum, which
  //   `Layout::enumeration` asserts. So `store` of the tag and `write` of the variant are within
  //   the value.
  unsafe {
    match layout.kind {
      Kind::Scalar(spec) => match spec.valid {
        Valid::Uninit => {}
        Valid::Any => store(ptr, spec.size, spec.endian, 0),
        Valid::In(ranges) => store(ptr, spec.size, spec.endian, ranges[0].0),
        Valid::Never => panic!("a scalar that a pun never makes has no sample value"),
      },
      Kind::Array(elem, len) => {
        if len == 0 || elem.size == 0 {
          return;
        }
        write(elem, ptr);
        // Each copy doubles the elements written, so there is no step per element.
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
      Kind::Slice(_) | Kind::Pattern(_) => panic!("a field of an enum is sized"),
    }
  }
}

/// Writes the integer `value` as `size` bytes at `ptr`, in `endian` byte order.
///
/// Bytes beyond the 16 of a `u128`, in a scalar wider than that, are 0.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `ptr` is valid for writes of `size` bytes.
const unsafe fn store(ptr: *mut u8, size: usize, endian: Endian, value: u128) {
  let mut i = 0;
  while i < size {
    let digit = if endian.is_little() { i } else { size - 1 - i };
    // SAFETY: `i < size`, and `ptr` is valid for writes of `size` bytes, by contract on the
    // caller. So `add(i)` stays within them, and the byte there is valid for writes. A `u8` is
    // aligned at any address. These are the requirements of `add` and of `write`.
    unsafe { ptr.add(i).write(if digit < 16 { (value >> (8 * digit)) as u8 } else { 0 }) };
    i += 1;
  }
}

/// Returns the index of the variant of the `E` at `ptr`.
///
/// In const evaluation, it fails the build if the tag is not the tag of a variant that has values
/// ([const evaluation fails on what is not a
/// value](crate::discover#const-evaluation-fails-on-what-is-not-a-value)), and otherwise reads the
/// variant from the tag alone, whatever the other bytes hold ([the variant is read from the
/// tag](crate::discover#the-variant-is-read-from-the-tag)).
///
/// # Panics
///
/// Panics if `mem::Discriminant<E>` is not the size of an `isize` ([the discriminant is the
/// index](crate::discover#the-discriminant-is-the-index)).
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `ptr` is not null, is aligned for `E`, and is valid for reads of `size_of::<E>()` bytes.
/// - Unless the call is evaluated at compile time, `ptr` points to a valid `E`.
const unsafe fn variant<E>(ptr: *const E) -> usize {
  // SAFETY: The reference made from `ptr` must meet the requirements of a reference [1]:
  // - It is not null and aligned for `E`, by contract on the caller.
  // - It is not dangling, because `ptr` is valid for reads of an `E`, by contract on the caller.
  // - It points to a valid `E` at run time, by contract on the caller. At compile time, this is not
  //   established: the bytes at `ptr` may be no `E`, which is valid only if its tag is the tag of a
  //   variant and the fields of that variant are valid [2]. The call then relies on two behaviors
  //   of `rustc` that the Reference does not promise, and that the crate documentation discloses
  //   (section Soundness). `rustc` reports an error if the tag is uninitialized or not the tag of a
  //   variant that has values, and otherwise reads the variant from the tag alone, without reading
  //   the fields (`discover`, module documentation).
  //
  // [1] Per https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.reference-box:
  //
  //     A reference or `Box<T>` must be aligned and non-null, it cannot be dangling, and it must
  //     point to a valid value
  //
  // [2] Per https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.enum:
  //
  //     An enum must have a valid discriminant, and all fields of the variant indicated by that
  //     discriminant must be valid at their respective type.
  let discriminant = mem::discriminant(unsafe { &*ptr });
  // "The discriminant is the index", above: an `isize` that is the index of the variant. Its size
  // is asserted here, and its value is checked on every sample.
  assert!(
    size_of::<mem::Discriminant<E>>() == size_of::<isize>(),
    "the discriminant of an enum of the `Rust` representation is not an `isize`"
  );
  // SAFETY: `read_unaligned` requires the pointer to be valid for reads, and to point to an
  // initialized `isize`, at any alignment. `discriminant` is a local of the size of an `isize`,
  // which is asserted above, so it is valid for reads. The initialization of its bytes is what
  // `rustc` makes of a `Discriminant<E>`, which holds an `isize` (`discover`, module
  // documentation). Rust does not document it, and the assertion checks only the size. In const
  // evaluation, where `discover` runs, `rustc` reports a read of an uninitialized byte as an
  // error.
  let index = unsafe { core::ptr::from_ref(&discriminant).cast::<isize>().read_unaligned() };
  index.cast_unsigned()
}

/// A copy of a sample, to experiment on.
struct Buf<E>(MaybeUninit<E>);

impl<E> Buf<E> {
  /// Returns a copy of the bytes of `sample`, initialized or not.
  const fn of(sample: &MaybeUninit<E>) -> Self {
    let mut copy = MaybeUninit::<E>::uninit();
    // SAFETY: This satisfies the safety contract of `copy_nonoverlapping`. `sample` is a
    // reference to a `MaybeUninit<E>`, valid for reads of an `E` and aligned. `copy` is a local
    // `MaybeUninit<E>`, valid for writes of an `E` and aligned, which `sample` cannot overlap.
    // The copy is of bytes, initialized or not.
    unsafe { core::ptr::copy_nonoverlapping(sample.as_ptr(), copy.as_mut_ptr(), 1) };
    Self(copy)
  }

  /// Copies the `len` bytes at `offset` of `from`, initialized or not, over the same bytes.
  ///
  /// # Panics
  ///
  /// Panics if the bytes are not within an `E`.
  const fn transplant(&mut self, from: &MaybeUninit<E>, offset: usize, len: usize) {
    assert!(offset + len <= size_of::<E>());
    // SAFETY: This satisfies the safety contract of `copy_nonoverlapping`. `offset + len` is at
    // most the size of an `E`, which is asserted above, so both `add(offset)` stay within their
    // `E`, and the `len` bytes from there are valid for reads in `from` and for writes in
    // `self`. A `u8` is aligned at any address. `self` is an exclusive borrow and `from` a shared
    // one, so the two do not overlap.
    unsafe {
      core::ptr::copy_nonoverlapping(
        from.as_ptr().cast::<u8>().add(offset),
        self.0.as_mut_ptr().cast::<u8>().add(offset),
        len,
      );
    }
  }

  /// Writes `byte` over the `len` bytes at `offset`.
  ///
  /// # Panics
  ///
  /// Panics if the bytes are not within an `E`.
  const fn fill(&mut self, offset: usize, len: usize, byte: u8) {
    assert!(offset + len <= size_of::<E>());
    // SAFETY: `offset + len` is at most the size of an `E`, which is asserted above. So
    // `add(offset)` stays within `self`, and the `len` bytes from there are valid for writes. A
    // `u8` is aligned at any address. These are the requirements of `add` and of `write_bytes`.
    unsafe { core::ptr::write_bytes(self.0.as_mut_ptr().cast::<u8>().add(offset), byte, len) };
  }

  /// Returns the index of the variant of the value.
  ///
  /// # Safety
  ///
  /// The call must be evaluated at compile time, in a `const` item or block. The tag of the value
  /// may be uninitialized, or not the tag of a variant, and reading the variant of such a value is
  /// undefined behavior at run time. Const evaluation reports an error instead ([const evaluation
  /// fails on what is not a value](crate::discover#const-evaluation-fails-on-what-is-not-a-value)).
  const unsafe fn variant(&self) -> usize {
    // SAFETY: `as_ptr` of the `MaybeUninit<E>` of the buffer is not null, is aligned for `E`, and
    // is valid for reads of one, since it is made from a reference to the buffer. This call is
    // evaluated at compile time, by contract on the caller, so the bytes of the buffer need not
    // be a valid `E`. These are the requirements of `variant`.
    unsafe { variant(self.0.as_ptr()) }
  }
}

/// What the experiments find of the layout of an enum with `V` variants.
#[doc(hidden)]
#[derive(Copy, Clone, Debug)]
pub struct Desc<const V: usize> {
  /// The offset and the size of the tag, or `(0, 0)` if there is none.
  ///
  /// There is none for a single variant laid out as a struct, or if no variant has values.
  tag: (usize, usize),
  /// The variant that holds the tag in a niche, or `V` if none does.
  dataful: usize,
  /// Whether each variant has values.
  inhabited: [bool; V],
  /// The tag of each variant that has values, but the dataful one, as a range of one value.
  tags: [(u128, u128); V],
  /// The tags of the dataful variant, every value but the tags of the others, and their number.
  dataful_tags: ([(u128, u128); V], usize),
  /// The values of the niche scalar, in the field of the dataful variant that holds it, if they are
  /// known as a tag is read.
  ///
  /// These are the tags of the dataful variant, more exactly than `dataful_tags`. The tags of the
  /// other variants are none of them, by [one scalar tells the
  /// variant](crate::discover#one-scalar-tells-the-variant), and because the valid values of a
  /// layout are a subset of those of `rustc`.
  dataful_known: Option<&'static [(u128, u128)]>,
  /// The valid values of the tag, sorted, and their number.
  valid: ([(u128, u128); V], usize),
}

impl<const V: usize> Desc<V> {
  /// Returns a description in which no variant has values, from which the experiments start.
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

  /// Returns the offset of the tag.
  #[must_use]
  pub const fn tag_offset(&self) -> usize {
    self.tag.0
  }

  /// Returns whether the `variant`th variant holds the tag in a niche.
  #[must_use]
  pub const fn holds_tag(&self, variant: usize) -> bool {
    variant == self.dataful
  }

  /// Returns whether the `variant`th variant has values.
  ///
  /// # Panics
  ///
  /// Panics if `variant` is not less than `V`.
  #[must_use]
  pub const fn inhabited(&self, variant: usize) -> bool {
    self.inhabited[variant]
  }

  /// Returns the tags of the `variant`th variant, sorted, or none if it has no value.
  ///
  /// # Panics
  ///
  /// Panics if `variant` is not less than `V`.
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

  /// Returns the layout of the tag: a scalar of native byte order whose valid values are the tags.
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

/// Returns whether a field of `fields` covers a byte of `offset..offset + len`.
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

/// Writes `byte` over the bytes of `buf` that no field of `fields` covers, but `skip..skip + len`.
///
/// It makes one write for each gap.
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

/// Returns whether nothing but `offset..offset + len` tells the variant `v` from the others.
///
/// It is so if the gaps of `v` outside these bytes, filled with zeros and then with ones, leave it
/// the same variant. A byte of a tag that tells `v` has one value in `v`, not two.
///
/// # Safety
///
/// The call must be evaluated at compile time, in a `const` item or block. A fill may make the tag
/// invalid, and reading the variant of such a value is undefined behavior at run time. Const
/// evaluation reports an error instead ([const evaluation fails on what is not a
/// value](crate::discover#const-evaluation-fails-on-what-is-not-a-value)).
const unsafe fn decided_by<E>(
  sample: &MaybeUninit<E>,
  fields: &[Field],
  v: usize,
  offset: usize,
  len: usize,
) -> bool {
  let (mut zeros, mut ones) = (Buf::of(sample), Buf::of(sample));
  fill_gaps(&mut zeros, fields, offset, len, 0);
  fill_gaps(&mut ones, fields, offset, len, 0xff);
  // SAFETY: `decided_by` is evaluated at compile time, by contract on the caller, so these calls
  // are too.
  unsafe { zeros.variant() == v && ones.variant() == v }
}

/// Returns the integer of `len` bytes at `offset` of `sample`, in native byte order.
///
/// In const evaluation, it fails the build if one of the bytes is not initialized ([const
/// evaluation fails on what is not a
/// value](crate::discover#const-evaluation-fails-on-what-is-not-a-value)). So a byte that is read
/// outside the fields of the variant of the sample is a byte of the tag ([a fresh value has
/// uninitialized padding](crate::discover#a-fresh-value-has-uninitialized-padding)).
///
/// # Panics
///
/// Panics if the bytes are not within an `E`, or if `len` is more than 16.
///
/// # Safety
///
/// The call must be evaluated at compile time, in a `const` item or block. The bytes may be
/// uninitialized, and reading them is undefined behavior at run time. Const evaluation reports an
/// error instead ([const evaluation fails on what is not a
/// value](crate::discover#const-evaluation-fails-on-what-is-not-a-value)).
pub const unsafe fn read<E>(sample: &MaybeUninit<E>, offset: usize, len: usize) -> u128 {
  assert!(offset + len <= size_of::<E>() && len <= 16);
  // SAFETY: `offset + len` is at most the size of an `E`, and `len` at most 16, which is asserted
  // above. So `add(offset)` stays within `sample`, and the `len` bytes from there are valid for
  // reads. `load` also requires them to be initialized, which is not established. By contract on
  // the caller, the call is evaluated at compile time, where `rustc` reports an error instead of
  // reading an uninitialized byte. No documentation promises this behavior, which the crate
  // documentation discloses (section Soundness).
  unsafe { load(sample.as_ptr().cast::<u8>().add(offset), len, Endian::NATIVE) }
}

/// Returns the layout of the enum `E`, found from a sample of each variant and the fields of each.
///
/// `samples[v]` is a sample of the `v`th variant in the order of declarations, or anything if
/// that variant has no value. `fields[v]` are its fields, at the offsets observed on its sample.
///
/// # Panics
///
/// Panics, with the reason, if the layout is not of a shape that `rustc` gives ([one scalar tells
/// the variant](crate::discover#one-scalar-tells-the-variant)), or if a sample is not its own
/// variant ([the discriminant is the index](crate::discover#the-discriminant-is-the-index)).
///
/// # Safety
///
/// The call must be evaluated at compile time, in a `const` item or block. The experiments read
/// bytes that may be uninitialized, and the variants of values whose tags may be invalid, which is
/// undefined behavior at run time. Const evaluation reports an error instead ([const evaluation
/// fails on what is not a value](crate::discover#const-evaluation-fails-on-what-is-not-a-value)).
#[expect(
  clippy::many_single_char_names,
  reason = "`v`, `u` and `d` are variants, `i` and `c` fields and candidates, by their index"
)]
pub const unsafe fn discover<E, const V: usize>(
  samples: &[MaybeUninit<E>; V],
  fields: &[&[Field]; V],
) -> Desc<V> {
  let (size, mut desc) = (size_of::<E>(), Desc::none());
  // Find which variants have values and where their fields are, and check that each sample is its
  // own variant ("The discriminant is the index", above).
  let (mut count, mut first, mut sized, mut start, mut v) = (0, V, false, size, 0);
  while v < V {
    let (these, mut i) = (fields[v], 0);
    let mut inhabited = true;
    while i < these.len() {
      let field = &these[i];
      inhabited &= field.layout.is_inhabited();
      i += 1;
    }
    if inhabited {
      // SAFETY: `as_ptr` of a `MaybeUninit<E>` in a slice is not null, is aligned for `E`, and is
      // valid for reads of one. By contract on the caller of `discover`, this call is evaluated
      // at compile time, so the sample need not be a valid `E`. These are the requirements of
      // `variant`.
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
    // One variant has values, and either no other variant is laid out, so the enum is a struct, or
    // its fields start at 0, where a direct tag would be. The enum is then a struct, or a niche in
    // its fields all of whose valid values are this variant. Either way, its fields alone tell it,
    // which is checked.
    let these = fields[first];
    assert!(
      // SAFETY: `discover` is evaluated at compile time, by contract on the caller, so this call
      // is too.
      unsafe { decided_by(&samples[first], these, first, 0, 0) },
      "the only variant that has values is told by bytes that are not its fields"
    );
    desc.tags[first] = (0, 0);
    desc.valid = ([(0, 0); V], 1);
    return desc;
  }
  if start > 0 {
    // A direct tag is before the fields of every variant. `rustc` widens it to all the bytes before
    // the first field, which are then read in every sample. A byte that is not of the tag is
    // padding, uninitialized, and fails the build ("Const evaluation fails on what is not a value"
    // and "A fresh value has uninitialized padding", above).
    assert!(
      matches!(start, 1 | 2 | 4 | 8 | 16),
      "the bytes before the fields of an enum of the `Rust` representation are not an integer: unsupported layout"
    );
    // SAFETY: `discover` is evaluated at compile time, by contract on the caller, so this call is
    // too.
    unsafe { try_tag(&mut desc, samples, fields, (0, start, None), V) };
    assert!(desc.tag.1 > 0, "the tag of the enum does not tell its variants: unsupported layout");
    return desc;
  }
  // A niche is in the fields of one variant, and not covered by the fields of any other. `rustc`
  // puts it in the largest variant, so its scalars are tried first. The order only changes the
  // cost.
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
        // SAFETY: `discover` is evaluated at compile time, by contract on the caller, so this
        // call is too.
        if free && unsafe { try_tag(&mut desc, samples, fields, (offset, len, known), d) } {
          return desc;
        }
      }
      i += 1;
    }
  }
  panic!("no tag found in the enum: unsupported layout")
}

/// Returns whether the bytes `offset..offset + len` of `window` are the tag, and fills `desc` if
/// they are.
///
/// The tag is in the fields of `dataful`, a niche, or in the fields of none if `dataful` is `V`,
/// a direct tag. The last element of `window` is the values of the scalar of a niche, if they are
/// known. The bytes are the tag if all of the following hold:
///
/// - Moving them from the sample of each variant to the sample of another variant makes it that
///   variant. This does not fail the build, since a byte of a niche is moved from a variant to
///   one where it is not a field.
/// - Nothing else tells a variant from the others.
/// - Each variant but the dataful one has one value there, all of whose bytes are initialized ([a
///   fresh value has uninitialized
///   padding](crate::discover#a-fresh-value-has-uninitialized-padding)).
///
/// # Panics
///
/// Panics if no variant that has values is not `dataful`, if two variants have the same tag, or
/// if the tag of a variant is a valid value of the scalar of the niche.
///
/// # Safety
///
/// The call must be evaluated at compile time, in a `const` item or block. The experiments read
/// bytes that may be uninitialized, and the variants of values whose tags may be invalid, which is
/// undefined behavior at run time. Const evaluation reports an error instead ([const evaluation
/// fails on what is not a value](crate::discover#const-evaluation-fails-on-what-is-not-a-value)).
const unsafe fn try_tag<E, const V: usize>(
  desc: &mut Desc<V>,
  samples: &[MaybeUninit<E>; V],
  fields: &[&[Field]; V],
  window: (usize, usize, Option<&'static [(u128, u128)]>),
  dataful: usize,
) -> bool {
  let (offset, len, known) = window;
  // A variant that has values and is not the dataful one. The tag is not one of its fields.
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
      // SAFETY: `try_tag` is evaluated at compile time, by contract on the caller, so this call
      // is too.
      if unsafe { buf.variant() } != r {
        return false;
      }
    }
    r += 1;
  }
  // Nothing else tells the variants that have one tag from the others.
  let mut v = 0;
  while v < V {
    if desc.inhabited[v] && v != dataful {
      if covers(fields[v], offset, len) {
        return false;
      }
      // SAFETY: `try_tag` is evaluated at compile time, by contract on the caller, so this call
      // is too.
      if !unsafe { decided_by(&samples[v], fields[v], v, offset, len) } {
        return false;
      }
    }
    v += 1;
  }
  // Nothing outside its fields tells the dataful variant either: its gaps, filled with zeros and
  // then with ones, leave it that variant. "One scalar tells the variant", above, says that a niche
  // is told by its scalar alone, so a layout in which another place takes part, such as an encoding
  // in two levels, is refused here.
  //
  // SAFETY: `try_tag` is evaluated at compile time, by contract on the caller, so this call is
  // too, as `decided_by` requires.
  if dataful < V && !unsafe { decided_by(&samples[dataful], fields[dataful], dataful, 0, 0) } {
    return false;
  }
  // Their tags are initialized, so they are bytes of the tag ("A fresh value has uninitialized
  // padding", above), and they must differ.
  let (mut values, mut single, mut v) = ([0; V], [false; V], 0);
  while v < V {
    if desc.inhabited[v] && v != dataful {
      // SAFETY: `try_tag` is evaluated at compile time, by contract on the caller, so this call
      // is too.
      let value = unsafe { read(&samples[v], offset, len) };
      let mut u = 0;
      while u < v {
        assert!(!single[u] || values[u] != value, "two variants have the same tag");
        u += 1;
      }
      (values[v], single[v], desc.tags[v]) = (value, true, (value, value));
    }
    v += 1;
  }
  // The variants without a single tag take that of `base`, which merges with it.
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
    // The dataful variant takes every other value of the scalar. Its field rejects those that are
    // not valid for it.
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
    // The tags of the other variants must not be values of the scalar.
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

/// The scalars of a field that may hold a niche.
///
/// These are the scalars whose values the type of the field restricts, and the tags of enums. A
/// `valid(..)` that safe code can break is not one. Each comes with its values, if they are known
/// as a tag is read, in native byte order.
struct Candidates {
  /// The candidates, of which the first `len` are used.
  list: [Candidate; 16],
  /// The number of candidates.
  len: usize,
}

/// The offset and the size of a scalar, and its values if they are known as a tag is read.
type Candidate = (usize, usize, Option<&'static [(u128, u128)]>);

impl Candidates {
  /// No candidate.
  const NEW: Self = Self { list: [(0, 0, None); 16], len: 0 };

  /// Adds the scalar `spec` at `offset`, if there is room and it is 1 to 16 bytes.
  ///
  /// Its values are known as a tag is read if it is one byte, or of native byte order.
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

  /// Adds the candidates of `layout`, at `base`.
  ///
  /// These are its scalars whose values are restricted, the first element of an array, the
  /// fields of a struct that `valid(..)` does not restrict, and the tags of enums.
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
      Kind::Slice(_) | Kind::Pattern(_) => {}
    }
  }
}
