//! Layouts of types as values, and the checksums that rules compute.
//!
//! Every [`Pun`](crate::Pun) type has a layout, a constant of type [`Layout`]. A layout is what
//! `puns` knows about a type: its size, its alignment, where its fields are, and which values are
//! [valid]. It is a tree, whose leaves are scalars ([`Spec`]) and tails of bytes that an automaton
//! accepts ([`Dfa`]), and whose nodes are arrays, slices, structs ([`Field`], with their
//! [`Rule`]s), and enums with fields and unions ([`Enum`], [`Variant`]). A byte of a struct that no
//! field covers is padding.
//!
//! The analysis of `puns` is ordinary `const fn` code that compares two layouts and decides at
//! compile time whether a pun is proven, checked or refused. At run time, the validation checks
//! what the analysis could not prove, as the layouts describe it.
//!
//! `#[derive(Pun)]` and the implementations of [`Scalar`](crate::Scalar) make layouts with the
//! functions of this module. A layout stands for a type only through an implementation of `Pun`,
//! which is an `unsafe trait`.
//!
//! The functions of this module are `const fn`s, meant to be evaluated in constants, where a panic
//! is a compile error.
//!
//! The module also holds what the rules of `#[derive(Pun)]` compute: the checksums [`crc32`],
//! [`sum16`], [`Crc`] and [`Checksum`], and the variable-length integers of [`Varint`].
//!
//! # Nodes
//!
//! `const fn`s build a layout from what the compiler says about the type, such as `size_of`, the
//! offsets of the fields, and `size_of_val_raw`. A node is one of the following:
//!
//! - A **scalar** ([`Spec`]) is a number of bytes in a byte order, with a sorted list of valid
//!   ranges. A scalar that restricts its values is 1 to 16 bytes, while one that accepts any bytes
//!   may have any size. `bool` is `0..=1`, `char` is two ranges, and `NonZero` is `1..`. An enum
//!   without fields is the values of its variants, read from the compiler whatever its `repr`. A
//!   scalar has two sets of values, the values that it *accepts* and the values that it is *known*
//!   to hold, which [`Spec`] defines.
//! - A **struct** has its fields ([`Field`]) at their offsets, which come from the compiler for
//!   any `repr`. The bytes that no field covers are padding. A struct may have rules ([`Rule`]).
//! - An **enum** with fields ([`Enum`]) has a tag, a scalar at a fixed place, and each [`Variant`]
//!   has its tags and a struct of its fields. The representations that Rust specifies (`repr(C)`,
//!   `repr(u8)` and the other primitive representations, [RFC 2195]) are made of `repr(C)` structs,
//!   so the derive computes their layouts from the sizes and alignments of the fields, as `repr(C)`
//!   lays out a struct, and the result is checked against the size and alignment of the enum. For
//!   an enum of the `Rust` representation, experiments in const evaluation find the layout that
//!   `rustc` chose. That layout has either a direct tag or a *niche* in the fields of one variant.
//!   With a niche, that variant has several tags, the values of the scalar that holds the niche,
//!   and it holds the tag in its fields. A variant that holds a type without values has no tag,
//!   since no value of it exists.
//! - A **union** (`repr(C)`) is an enum without a tag whose variants are its fields, each at offset
//!   0. A value of it must be one of its fields ([Unions](crate#unions)).
//! - A type **without values** is an enum without variants ([`Layout::uninhabited`]), such as
//!   `Infallible`. No value has its layout ([`Layout::is_inhabited`]), so as a source, a variant of
//!   an enum without values needs no proof where the variants of two enums are related.
//! - `MaybeUninit<T>` is padding with the size of `T`.
//! - An **array** is the layout of an element and a count, and a **slice** is the layout of an
//!   element alone, since its length is not part of its type.
//! - A **pattern** is a tail of bytes that an automaton ([`Dfa`]) accepts, and `str` is the 8-state
//!   automaton of UTF-8. An automaton is known by its states and edges. Its `id`, a field that safe
//!   code can write, only serves to tell two automata apart, and two patterns are the same layout
//!   only if their automata are the same, state by state. A value is checked with `str::from_utf8`
//!   only where the automaton is the UTF-8 one, which is decided at compile time.
//! - An **opaque scalar** ([`Spec::opaque`]), such as the offset of a self-relative pointer, is a
//!   scalar with its valid values, which a pun never makes as a target, but in a
//!   [`Deferred<T>`](crate::Deferred). Its values are what Rust requires of its bytes. What makes a
//!   value of it correct at its place is not in the layout.
//!
//! A layout is an automaton written as a tree, where a position in it is a state. The analysis
//! walks the layouts of the source and the target together, position by position.
//!
//! [RFC 2195]: https://rust-lang.github.io/rfcs/2195-really-tagged-unions.html
//! [valid]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity

use core::mem;

#[doc(hidden)]
pub use crate::discover::Desc;
use crate::{
  narrow::{self, How, fit},
  ptr,
};

// -------------------------------------------------------------------------------------------------
// `Endian`

/// The byte order of a scalar.
///
/// # Examples
///
/// ```
/// use puns::layout::Endian;
///
/// let native = if cfg!(target_endian = "little") { Endian::Little } else { Endian::Big };
/// assert_eq!(Endian::NATIVE, native);
/// ```
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Endian {
  /// The least significant byte first.
  Little,
  /// The most significant byte first.
  Big,
}

impl Endian {
  /// The byte order of the target.
  pub const NATIVE: Endian =
    if cfg!(target_endian = "little") { Endian::Little } else { Endian::Big };

  /// Returns whether the byte order is [`Endian::Little`].
  #[inline]
  pub(crate) const fn is_little(self) -> bool {
    matches!(self, Endian::Little)
  }

  /// Returns whether `self` and `other` are the same byte order, in `const`.
  #[inline]
  pub(crate) const fn equals(self, other: Endian) -> bool {
    self.is_little() == other.is_little()
  }
}

// -------------------------------------------------------------------------------------------------
// `Spec`

/// A set of values of a scalar.
#[derive(Copy, Clone, Debug)]
pub(crate) enum Valid {
  /// Any bytes, uninitialized bytes included.
  Uninit,
  /// Any initialized bytes.
  Any,
  /// The values of these inclusive ranges, read as an unsigned integer in the byte order of the
  /// scalar.
  ///
  /// The ranges are sorted, disjoint and not adjacent. They are never empty, and never the single
  /// range of every value, which is [`Valid::Any`].
  In(&'static [(u128, u128)]),
}

/// The size, the byte order and the valid values of a scalar.
///
/// A `Spec` is the [`SPEC`](crate::Scalar::SPEC) of a [`Scalar`](crate::Scalar) type, from which
/// [`Layout::scalar`] makes the layout of that type. The value of a scalar is its bytes, read as an
/// unsigned integer in its byte order, and a scalar that restricts its values is 1 to 16 bytes.
///
/// A `Spec` holds two sets of values. The values it *accepts* are what a target requires, so a pun
/// makes a value of the scalar only from one of these. The values it is *known* to hold are what
/// every value of the type guarantees, whatever safe code does, so a source proves these values,
/// and no others. Both sets are the same, with three exceptions. A field restricted by
/// `#[pun(valid(..))]` ([`Layout::restricted`]) accepts fewer values than it is known to hold
/// ([Guarantees and requirements](crate#guarantees-and-requirements)). A scalar that may be
/// uninitialized ([`Spec::maybe_uninit`]) accepts some values, but is known to hold anything,
/// padding included.
///
/// An [opaque](Spec::opaque) scalar has both sets, as any other, but is never a target: a pun reads
/// it, and never makes it, but in a [`Deferred<T>`](crate::Deferred).
///
/// # Examples
///
/// ```
/// use puns::Scalar;
///
/// assert!(bool::SPEC.contains(1) && !bool::SPEC.contains(2));
/// // A surrogate is not a `char`.
/// assert!(char::SPEC.contains(0x41) && !char::SPEC.contains(0xd800));
/// ```
#[derive(Copy, Clone, Debug)]
pub struct Spec {
  /// The size of the scalar in bytes.
  pub(crate) size: usize,
  /// The byte order in which the bytes of the scalar are its value.
  pub(crate) endian: Endian,
  /// The values that a target requires.
  pub(crate) valid: Valid,
  /// The values that every value of the type holds, whatever safe code does, and which a source
  /// therefore guarantees.
  ///
  /// This is `valid`, except for a field restricted by `#[pun(valid(..))]`, of which nothing is
  /// known beyond what its type guarantees (safe code may write any value of that type to the
  /// field), and for a scalar that may be uninitialized.
  pub(crate) known: Valid,
  /// Whether the value of the scalar is the value of an unsigned integer, which rules can read. It
  /// is not for a signed or floating-point number, a `bool`, a `char` or an enum.
  pub(crate) number: bool,
  /// Whether the scalar is opaque ([`Spec::opaque`]): never a target.
  pub(crate) opaque: bool,
}

/// Returns the greatest value of an unsigned integer of `size` bytes, `u128::MAX` from 16 bytes.
#[inline]
pub(crate) const fn max_value(size: usize) -> u128 {
  if size >= 16 { u128::MAX } else { (1u128 << (8 * size)) - 1 }
}

impl Spec {
  /// Returns the scalar of the size of `T` that accepts any initialized bytes.
  ///
  /// The scalar is in the byte order of the target, and rules read it as an unsigned integer unless
  /// [`Spec::not_a_number`] says otherwise.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::layout::Spec;
  ///
  /// let spec = Spec::any::<u16>();
  /// assert!(spec.contains(0) && spec.contains(0xffff));
  /// // Not a value of 2 bytes.
  /// assert!(!spec.contains(0x1_0000));
  /// ```
  #[inline]
  #[must_use]
  pub const fn any<T>() -> Self {
    let size = size_of::<T>();
    Self {
      size,
      endian: Endian::NATIVE,
      valid: Valid::Any,
      known: Valid::Any,
      number: true,
      opaque: false,
    }
  }

  /// Returns the same scalar, made [opaque](crate#opaque-scalars): a pun reads it, and never makes
  /// it.
  ///
  /// A self-relative pointer is such a scalar: its bytes are an offset, and whether it is correct
  /// depends on where it is and on what it aims at. The values of `self` are those that Rust
  /// requires of its bytes, such as "not zero" for an offset that is a `NonZero`. As a source, an
  /// opaque scalar proves what these values prove. As a target, nothing proves it, not even a value
  /// of the same type: a pun or a check that would make one does not compile, and a check never
  /// accepts a variant of an enum that holds one. Only the crate that declares the scalar makes a
  /// value of it, and proves it at its place: a [`Deferred<T>`](crate::Deferred) of a `T` that holds
  /// one is checked in everything else, the values of the scalar included, and its
  /// [`Validate`](crate::Validate) implementation proves the place.
  ///
  /// A rule reads an opaque scalar that is a number, and `#[pun(valid(..))]` may restrict one. An
  /// `Option` of an opaque scalar is opaque too, if it has the size of the scalar ([Opaque
  /// scalars](crate#opaque-scalars)).
  ///
  /// # Panics
  ///
  /// Panics if the scalar has a size of 0, since a pun would then make one from no bytes at all, or
  /// if its bytes may be uninitialized ([`Spec::uninit`], [`Spec::maybe_uninit`]), since a source
  /// would then know nothing of it. In the `SPEC` of a [`Scalar`](crate::Scalar), this is an error
  /// at compile time.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::layout::{Endian, Layout, Spec};
  ///
  /// // An offset of 4 bytes, never zero.
  /// let spec = Spec::ranges::<u32>(Endian::NATIVE, &[(1, u32::MAX as u128)]).opaque();
  /// // Its values are those of its bytes.
  /// assert!(!spec.contains(0) && spec.contains(8));
  /// // Its bytes are initialized: a value that holds one is plain.
  /// assert!(Layout::scalar::<u32>(spec).is_plain());
  /// // Nothing proves it as a target.
  /// assert!(Layout::scalar::<u32>(spec).holds_opaque());
  /// ```
  ///
  /// The scalar of a type is declared with [`Scalar`](crate::Scalar), as in the example of
  /// [`Validate`](crate::Validate).
  #[inline]
  #[must_use]
  pub const fn opaque(self) -> Self {
    assert!(self.size > 0, "an opaque scalar has at least one byte");
    assert!(
      !matches!(self.valid, Valid::Uninit) && !matches!(self.known, Valid::Uninit),
      "an opaque scalar is initialized"
    );
    Self { opaque: true, ..self }
  }

  /// Returns the scalar of `size` bytes, in the byte order of the target, that accepts any
  /// initialized bytes.
  ///
  /// This is the tag of an enum of the `Rust` representation found by `discover.rs`, which has no
  /// type of its own.
  ///
  /// # Panics
  ///
  /// Panics if `size` is greater than 16.
  pub(crate) const fn of_size(size: usize) -> Self {
    assert!(size <= 16, "a scalar is at most 16 bytes wide");
    Self {
      size,
      endian: Endian::NATIVE,
      valid: Valid::Any,
      known: Valid::Any,
      number: true,
      opaque: false,
    }
  }

  /// Returns the scalar of the size of `T` that accepts any bytes, uninitialized bytes included.
  ///
  /// Nothing is known about what a source holds there, not even that it is initialized, so the
  /// scalar is padding.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::layout::{Layout, Spec};
  ///
  /// // Its bytes may be uninitialized: it is not plain data, as one of `Spec::any` is.
  /// assert!(!Layout::scalar::<u16>(Spec::uninit::<u16>()).is_plain());
  /// assert!(Layout::scalar::<u16>(Spec::any::<u16>()).is_plain());
  /// ```
  #[inline]
  #[must_use]
  pub const fn uninit<T>() -> Self {
    let (size, endian) = (size_of::<T>(), Endian::NATIVE);
    Self { size, endian, valid: Valid::Uninit, known: Valid::Uninit, number: true, opaque: false }
  }

  /// Returns the scalar of the size of `T` whose value, read as an unsigned integer in the byte
  /// order `endian`, is in one of `ranges`.
  ///
  /// The ranges are inclusive, and must be sorted, disjoint and not adjacent. [`sorted_ranges`]
  /// makes them from a list of values. They are both what the scalar accepts and what every value
  /// of it is known to hold, since the type guarantees them. A single range that covers every value
  /// accepts any initialized bytes, as [`Spec::any`] does.
  ///
  /// # Panics
  ///
  /// Panics if `T` is not 1 to 16 bytes, or if `ranges` are not as [`Spec::with_ranges`] requires.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::layout::{Endian, Spec};
  ///
  /// let spec = Spec::ranges::<u8>(Endian::NATIVE, &[(1, 3), (7, 7)]);
  /// assert!(spec.contains(1) && spec.contains(3) && spec.contains(7));
  /// assert!(!spec.contains(0) && !spec.contains(5));
  /// ```
  #[must_use]
  pub const fn ranges<T>(endian: Endian, ranges: &'static [(u128, u128)]) -> Self {
    let size = size_of::<T>();
    Self { size, endian, valid: Valid::Any, known: Valid::Any, number: true, opaque: false }
      .with_ranges(ranges)
  }

  /// Returns the same scalar, whose values are those of `ranges`.
  ///
  /// The ranges are inclusive. They are both what the scalar accepts and what every value of it is
  /// known to hold, since the type guarantees them. A single range that covers every value accepts
  /// any initialized bytes, as [`Spec::any`] does. To make a field that requires values which its
  /// type does not guarantee, use [`Layout::restricted`].
  ///
  /// # Panics
  ///
  /// Panics if the scalar is not 1 to 16 bytes, if `ranges` is empty, or if a range is empty or
  /// holds a value too large for the scalar. Also panics if the ranges are not sorted, or if two of
  /// them overlap or are adjacent.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::layout::Spec;
  ///
  /// let digit = Spec::any::<u8>().with_ranges(&[(b'0' as u128, b'9' as u128)]);
  /// assert!(digit.contains(b'7'.into()) && !digit.contains(b'a'.into()));
  /// ```
  ///
  /// Two adjacent ranges are one range, and must be written as one:
  ///
  /// ```should_panic
  /// use puns::layout::Spec;
  ///
  /// let _ = Spec::any::<u8>().with_ranges(&[(0, 4), (5, 9)]);
  /// ```
  #[must_use]
  pub const fn with_ranges(self, ranges: &'static [(u128, u128)]) -> Self {
    let size = self.size;
    assert!(size >= 1 && size <= 16, "a constrained scalar must be 1 to 16 bytes wide");
    assert!(!ranges.is_empty(), "a scalar must have at least one valid value");
    let max = max_value(size);
    let mut i = 0;
    while i < ranges.len() {
      let (lo, hi) = ranges[i];
      assert!(lo <= hi && hi <= max, "scalar range is empty or out of bounds");
      if i > 0 {
        let prev = ranges[i - 1].1;
        assert!(prev < lo && lo - prev > 1, "scalar ranges must be sorted and non-adjacent");
      }
      i += 1;
    }
    let full = ranges.len() == 1 && ranges[0].0 == 0 && ranges[0].1 == max;
    let valid = if full { Valid::Any } else { Valid::In(ranges) };
    Self { valid, known: valid, ..self }
  }

  /// Returns the same scalar, in which a source may hold uninitialized bytes.
  ///
  /// A target still requires what `self` accepts, initialized bytes whose value is one of its
  /// values. Nothing is known about what a source holds there, not even that it is initialized.
  ///
  /// This describes bytes that are either a scalar or padding, with no way to tell which. Requiring
  /// the values of the scalar is sound in either case, and so is knowing nothing.
  ///
  /// # Panics
  ///
  /// Panics if `self` does not restrict its values: if it accepts any initialized bytes, or
  /// uninitialized bytes.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::layout::{Endian, Layout, Spec};
  ///
  /// let tag = Spec::ranges::<u8>(Endian::NATIVE, &[(0, 2)]);
  /// // Required: the same values.
  /// assert!(tag.maybe_uninit().contains(2) && !tag.maybe_uninit().contains(3));
  /// // Known: nothing, not even that its bytes are initialized.
  /// assert!(Layout::scalar::<u8>(tag).is_plain());
  /// assert!(!Layout::scalar::<u8>(tag.maybe_uninit()).is_plain());
  /// ```
  #[must_use]
  pub const fn maybe_uninit(self) -> Self {
    assert!(
      matches!(self.valid, Valid::In(_)),
      "only a scalar that restricts its values may also be uninitialized"
    );
    Self { known: Valid::Uninit, ..self }
  }

  /// Returns whether the scalar is opaque ([`Spec::opaque`]): never a target.
  #[inline]
  pub(crate) const fn is_opaque(&self) -> bool {
    self.opaque
  }

  /// Returns the same scalar, not opaque: the scalar of its values, as a target that is a
  /// [`Deferred<T>`](crate::Deferred) takes it.
  #[inline]
  pub(crate) const fn decided(self) -> Self {
    Self { opaque: false, ..self }
  }

  /// Returns whether a source may hold uninitialized bytes in this scalar.
  #[inline]
  pub(crate) const fn may_be_uninit(&self) -> bool {
    matches!(self.known, Valid::Uninit)
  }

  /// Returns the same scalar, whose bytes are not the value of an unsigned integer.
  ///
  /// This is the scalar of a signed or floating-point number, a `bool`, a `char` or an enum. The
  /// expressions of `#[pun(..)]` compute on unsigned integers, and do not read such a scalar.
  ///
  /// # Examples
  ///
  /// The `Spec` of `i16` is `Spec::any::<i16>().not_a_number()`. A type of two bytes that are a
  /// signed number is declared the same way, and a rule then does not read it:
  ///
  /// ```should_panic
  /// use puns::{
  ///   Scalar,
  ///   layout::{Endian, Op, Spec},
  /// };
  ///
  /// /// A signed number of two bytes, the most significant first.
  /// #[derive(Clone, Copy)]
  /// #[repr(transparent)]
  /// struct I16be([u8; 2]);
  ///
  /// // SAFETY: `I16be` is `repr(transparent)` over `[u8; 2]`, so it has the layout of a `[u8; 2]`
  /// // [1], and `SPEC` has its size. Every value is two initialized bytes, which `Spec::any`
  /// // accepts, any two initialized bytes are a valid `I16be`, and it holds no `UnsafeCell`.
  /// //
  /// // [1] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.transparent.layout-abi:
  /// //
  /// //     Structs and enums with this representation have the same layout and ABI as the only
  /// //     non-size 0 non-alignment 1 field, if present, or unit otherwise.
  /// unsafe impl Scalar for I16be {
  ///   const SPEC: Spec = Spec::any::<I16be>().with_endian(Endian::Big).not_a_number();
  /// }
  ///
  /// // A rule reads a `u16`, but neither an `i16` nor an `I16be`.
  /// let _ = Op::load::<u16>(0);
  /// let _ = Op::load::<I16be>(0);
  /// ```
  #[must_use]
  pub const fn not_a_number(self) -> Self {
    Self { number: false, ..self }
  }

  /// Returns whether `T` has a size of 0, for `#[derive(Pun)]`.
  #[doc(hidden)]
  #[must_use]
  pub const fn is_zero_sized<T>() -> bool {
    size_of::<T>() == 0
  }

  /// Returns [`Spec::ranges`] in the byte order of the target, of a scalar that is not a number
  /// ([`Spec::not_a_number`]), for `#[derive(Pun)]`.
  ///
  /// This is the scalar of an enum without fields, whose values are its discriminants.
  ///
  /// # Panics
  ///
  /// Panics as [`Spec::ranges`] does.
  #[doc(hidden)]
  #[must_use]
  pub const fn native_ranges<T>(ranges: &'static [(u128, u128)]) -> Self {
    Self::ranges::<T>(Endian::NATIVE, ranges).not_a_number()
  }

  /// Returns [`sorted_ranges`] of `values`, for `#[derive(Pun)]`.
  #[doc(hidden)]
  #[must_use]
  pub const fn sorted_ranges<const N: usize>(values: [u128; N]) -> ([(u128, u128); N], usize) {
    sorted_ranges(values)
  }

  /// Returns [`range`] of these bounds, for `#[derive(Pun)]`.
  ///
  /// # Panics
  ///
  /// Panics if the range is empty.
  #[doc(hidden)]
  #[must_use]
  pub const fn range<T>(start: Option<u128>, end: Option<u128>, inclusive: bool) -> (u128, u128) {
    range::<T>(start, end, inclusive)
  }

  /// Returns [`merge_ranges`] of `ranges`, for `#[derive(Pun)]`.
  #[doc(hidden)]
  #[must_use]
  pub const fn merge_ranges<const N: usize>(
    ranges: [(u128, u128); N],
  ) -> ([(u128, u128); N], usize) {
    merge_ranges(ranges)
  }

  /// Returns [`bits_of`] of `value`, for `#[derive(Pun)]`.
  ///
  /// # Panics
  ///
  /// Panics if `T` is larger than 16 bytes.
  ///
  /// # Safety
  ///
  /// The caller must ensure that:
  ///
  /// - Every byte of `value` is initialized.
  #[doc(hidden)]
  #[must_use]
  pub const unsafe fn bits_of<T>(value: T) -> u128 {
    // SAFETY: Every byte of `value` is initialized, by contract on the caller. This is the
    // requirement of `bits_of`.
    unsafe { bits_of(value) }
  }

  /// Returns whether the scalar requires values that its type does not guarantee, as a field
  /// restricted by `#[pun(valid(..))]` does.
  ///
  /// As a target, such a scalar is proven by what the source requires at its place, and not only by
  /// what the source guarantees, because a requirement proves a requirement.
  #[inline]
  pub(crate) const fn is_soft(&self) -> bool {
    matches!(self.known, Valid::Any) && matches!(self.valid, Valid::In(_))
  }

  /// Returns the same scalar, whose value is its bytes in the byte order `endian`.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::{
  ///   Scalar,
  ///   layout::{Endian, Op, Spec},
  /// };
  ///
  /// /// Two bytes, the most significant first.
  /// #[derive(Clone, Copy)]
  /// #[repr(transparent)]
  /// struct Be16([u8; 2]);
  ///
  /// // SAFETY: `Be16` is `repr(transparent)` over `[u8; 2]`, so it has the layout of a `[u8; 2]`
  /// // [1], and `SPEC` has its size. Every value is two initialized bytes, which `Spec::any`
  /// // accepts, any two initialized bytes are a valid `Be16`, and it holds no `UnsafeCell`.
  /// //
  /// // [1] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.transparent.layout-abi:
  /// //
  /// //     Structs and enums with this representation have the same layout and ABI as the only
  /// //     non-size 0 non-alignment 1 field, if present, or unit otherwise.
  /// unsafe impl Scalar for Be16 {
  ///   const SPEC: Spec = Spec::any::<Be16>().with_endian(Endian::Big);
  /// }
  ///
  /// // What a rule reads of it.
  /// assert_eq!(Op::load::<Be16>(0), Op::Load { offset: 0, size: 2, big: true });
  /// ```
  #[inline]
  #[must_use]
  pub const fn with_endian(self, endian: Endian) -> Self {
    Self { endian, ..self }
  }

  /// Returns whether the scalar is at most 16 bytes, and initialized in every value. [`Spec::load`]
  /// can then read it.
  #[doc(hidden)]
  #[must_use]
  pub const fn is_integer(&self) -> bool {
    self.size <= 16 && !matches!(self.known, Valid::Uninit) && !matches!(self.valid, Valid::Uninit)
  }

  /// Returns the scalar at `ptr`, read as an unsigned integer in its byte order.
  ///
  /// # Safety
  ///
  /// The caller must ensure that:
  ///
  /// - [`Spec::is_integer`] is `true` of `self`.
  /// - `ptr` is valid for reads of `self.size` bytes, which are initialized. It need not be
  ///   aligned.
  #[doc(hidden)]
  #[inline(always)]
  #[must_use]
  pub const unsafe fn load(&self, ptr: *const u8) -> u128 {
    // SAFETY: `self.is_integer()` is `true`, by contract on the caller, so `self.size` is at most
    // 16. `ptr` is valid for reads of `self.size` initialized bytes, by contract on the caller.
    // These are the requirements of `load`.
    unsafe { load(ptr, self.size, self.endian) }
  }

  /// Returns whether the scalar accepts only some initialized values, or is opaque, which no
  /// source proves as a target.
  #[inline]
  pub(crate) const fn is_constrained(&self) -> bool {
    matches!(self.valid, Valid::In(_)) || self.opaque
  }

  /// Returns whether the scalar accepts `value`, the value of its bytes as an unsigned integer.
  ///
  /// A scalar that accepts any initialized bytes accepts every value that fits in its size.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::layout::{Endian, Spec};
  ///
  /// let spec = Spec::ranges::<u8>(Endian::NATIVE, &[(1, 9)]);
  /// assert!(spec.contains(1) && spec.contains(9));
  /// assert!(!spec.contains(0) && !spec.contains(10));
  /// ```
  #[must_use]
  pub const fn contains(&self, value: u128) -> bool {
    match self.valid {
      Valid::Uninit | Valid::Any => self.size >= 16 || value <= max_value(self.size),
      Valid::In(ranges) => contains(ranges, value),
    }
  }

  /// Returns whether the value of the bytes at `ptr` is one of the values of the scalar.
  ///
  /// A scalar that accepts any initialized bytes, or uninitialized ones, reads nothing. An opaque
  /// scalar is compared with its values like any other: whether it may be a target is not decided
  /// here, but by the analysis and by the validation (`validate::accepts`).
  ///
  /// # Safety
  ///
  /// The caller must ensure that:
  ///
  /// - If the scalar accepts some values but not all (`Spec::ranges`), `ptr` is valid for reads of
  ///   `self.size` bytes, which are initialized. It need not be aligned.
  #[inline(always)]
  pub(crate) const unsafe fn matches(&self, ptr: *const u8) -> bool {
    match self.valid {
      Valid::Uninit | Valid::Any => true,
      // SAFETY: The scalar accepts some values but not all, so `ptr` is valid for reads of
      // `self.size` initialized bytes, by contract on the caller. Only `Spec::with_ranges` makes a
      // `Valid::In`, and it asserts that the size is 1 to 16. These are the requirements of `load`.
      Valid::In(ranges) => contains(ranges, unsafe { load(ptr, self.size, self.endian) }),
    }
  }
}

/// Returns whether `value` is in one of the inclusive `ranges`.
///
/// Every range must have its low bound at most its high bound.
#[inline(always)]
pub(crate) const fn contains(ranges: &[(u128, u128)], value: u128) -> bool {
  let mut i = 0;
  // A few ranges: a comparison each, without a branch, so that the validation of a slice can be
  // vectorized (see `validate::validate_elems`).
  if ranges.len() <= 8 {
    let mut found = false;
    while i < ranges.len() {
      found |= value.wrapping_sub(ranges[i].0) <= ranges[i].1 - ranges[i].0;
      i += 1;
    }
    return found;
  }
  while i < ranges.len() {
    if ranges[i].0 <= value && value <= ranges[i].1 {
      return true;
    }
    i += 1;
  }
  false
}

/// Returns the unsigned integer of `size` bytes at `ptr`, in the byte order `endian`.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `size` is at most 16.
/// - `ptr` is valid for reads of `size` bytes, which are initialized. It need not be aligned.
#[inline(always)]
pub(crate) const unsafe fn load(ptr: *const u8, size: usize, endian: Endian) -> u128 {
  let mut buf = [0u8; 16];
  // SAFETY: This satisfies the safety contract of `copy_nonoverlapping` [1]:
  // - `ptr` is valid for reads of `size` bytes, by contract on the caller.
  // - `buf` is 16 bytes, and `size <= 16`, by contract on the caller. So the `size` bytes at
  //   `buf` or at `buf + 16 - size` are valid for writes, and `add(16 - size)` stays within `buf`.
  // - A `u8` is aligned at any address.
  // - `buf` is a local of this function, which no pointer from the caller reaches, so the two
  //   regions do not overlap.
  // The bytes of `buf` are then initialized, those that are copied by contract on the caller.
  //
  // [1] Per https://doc.rust-lang.org/1.99.0/core/ptr/fn.copy_nonoverlapping.html:
  //
  //     Behavior is undefined if any of the following conditions are violated:
  //
  //     - `src` must be valid for reads of `count * size_of::<T>()` bytes or that number must be
  //       0.
  //     - `dst` must be valid for writes of `count * size_of::<T>()` bytes or that number must
  //       be 0.
  //     - Both `src` and `dst` must be properly aligned.
  //     - The region of memory beginning at `src` with a size of `count * size_of::<T>()` bytes
  //       must not overlap with the region of memory beginning at `dst` with the same size.
  unsafe {
    match endian {
      Endian::Little => {
        core::ptr::copy_nonoverlapping(ptr, buf.as_mut_ptr(), size);
        u128::from_le_bytes(buf)
      }
      Endian::Big => {
        core::ptr::copy_nonoverlapping(ptr, buf.as_mut_ptr().add(16 - size), size);
        u128::from_be_bytes(buf)
      }
    }
  }
}

/// Returns the inclusive ranges of `values`, sorted, disjoint and not adjacent, and their count.
///
/// The first `count` elements of the array are the ranges, as [`Spec::ranges`] takes them, and the
/// others are `(0, 0)`. Repeated values count as one value.
///
/// # Examples
///
/// ```
/// use puns::layout::sorted_ranges;
///
/// assert_eq!(sorted_ranges([7, 1, 0, 2]), ([(0, 2), (7, 7), (0, 0), (0, 0)], 2));
/// ```
#[must_use]
pub const fn sorted_ranges<const N: usize>(mut values: [u128; N]) -> ([(u128, u128); N], usize) {
  // Insertion sort.
  let mut i = 1;
  while i < N {
    let mut j = i;
    while j > 0 && values[j - 1] > values[j] {
      values.swap(j - 1, j);
      j -= 1;
    }
    i += 1;
  }
  // Merge duplicates and neighbors.
  let (mut ranges, mut len, mut i) = ([(0, 0); N], 0, 0);
  while i < N {
    if len > 0 && values[i] - ranges[len - 1].1 <= 1 {
      ranges[len - 1].1 = values[i];
    } else {
      ranges[len] = (values[i], values[i]);
      len += 1;
    }
    i += 1;
  }
  (ranges, len)
}

// -------------------------------------------------------------------------------------------------
// `Layout`

/// What every byte of a layout is, if every byte is the same thing.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Uniform {
  /// The bytes are not all the same thing, so the layout must be looked into.
  No,
  /// Every byte is padding.
  Pad,
  /// Every byte is initialized, and may hold any value.
  Any,
}

/// What a layout has in common with every layout that Rust allows for its type.
///
/// Rust leaves part of some layouts to the compiler, which may choose differently in its next
/// version, or at random under `-Zrandomize-layout`, as [The `unstable`
/// feature](crate#the-unstable-feature) lists. A `Stable` says which facts about a layout hold
/// under every choice. It is computed from the layouts of its parts.
///
/// Without the `unstable` feature, the analysis does not look into a layout that is not `fixed`
/// ([`Layout::is_closed`]). As a source, such a layout is what every choice of it is (initialized
/// bytes if `init`, and otherwise bytes that may be padding). As a target, it is proven by the same
/// type at the same place (`Id`). A struct whose size is the same in every layout, and that has
/// no rule and no opaque scalar, is also proven field by field, at every offset each field may
/// have.
#[derive(Copy, Clone, Debug)]
#[expect(clippy::struct_excessive_bools, reason = "five facts of a layout, each asked alone")]
pub(crate) struct Stable {
  /// The offsets of the parts and the size are the same in every layout that Rust allows, given the
  /// sizes of the parts, or every layout of the type is the same bytes (`Layout::rust`). The
  /// analysis may look into such a layout.
  pub(crate) fixed: bool,
  /// Each byte of a value belongs to the same part in every layout that Rust allows.
  ///
  /// This is `fixed`, except for a struct whose field orders all make the same bytes, such as `(u8,
  /// u8)`. Its bytes are the same in every order, but which field each byte holds is not, so what a
  /// check would read from a value is not the same either.
  pub(crate) ordered: bool,
  /// The size is the same in every layout that Rust allows.
  pub(crate) size: bool,
  /// Every part, at any depth, is `fixed`.
  pub(crate) deep: bool,
  /// For a layout that is not `fixed`, every byte is initialized in every layout that Rust allows.
  /// Unused if the layout is `fixed`.
  pub(crate) init: bool,
}

impl Stable {
  /// The facts of a layout that Rust specifies, of parts that it specifies.
  pub(crate) const FIXED: Self = Self::computed(true, true);

  /// Returns the facts of a layout for which Rust specifies at most the size: the size if `size`,
  /// and that every byte is initialized if both `size` and `init`.
  const fn unordered(size: bool, init: bool) -> Self {
    Self { fixed: false, ordered: false, size, deep: false, init: size && init }
  }

  /// Returns the facts of a node whose offsets Rust computes from the sizes and alignments of its
  /// parts, as for a `repr(C)` struct, given whether the size of every part is the same in every
  /// layout (`size`), and whether every part is `deep`.
  const fn computed(size: bool, deep: bool) -> Self {
    Self { fixed: size, ordered: size, size, deep: size && deep, init: false }
  }

  /// Returns the facts of a node of the `parts`, as [`Stable::computed`] says.
  const fn of_parts(parts: &[Field]) -> Self {
    let (mut size, mut deep, mut i) = (true, true, 0);
    while i < parts.len() {
      size &= parts[i].layout.stable.size;
      deep &= parts[i].layout.stable.deep;
      i += 1;
    }
    Self::computed(size, deep)
  }

  /// Returns the facts of an enum of a representation that Rust specifies, or of a union, as
  /// [`Stable::of_parts`] does, since its variants are `repr(C)` structs.
  const fn of_variants(variants: &[Variant]) -> Self {
    let (mut size, mut deep, mut i) = (true, true, 0);
    while i < variants.len() {
      size &= variants[i].layout.stable.size;
      deep &= variants[i].layout.stable.deep;
      i += 1;
    }
    Self::computed(size, deep)
  }

  /// Returns the facts of a node of one repeated part, the element of an array or a slice, as
  /// [`Stable::computed`] says.
  const fn of_part(part: &Layout) -> Self {
    Self::computed(part.stable.size, part.stable.deep)
  }
}

/// The identity of a type, which tells two types apart where their layouts are the same.
///
/// A type has one layout in a program, but two types, however similar, may be laid out differently
/// where Rust leaves the layout to the compiler (see "Layout is subject to change" in the
/// documentation of `core::mem::offset_of!`). `Wrapper<u8, u8>` and `Wrapper<u8, i8>`, two
/// instances of a generic struct of the `Rust` representation, may have their fields in different
/// orders, as `-Zrandomize-layout` shows. Without the `unstable` feature, a layout that Rust leaves
/// to the compiler proves only the same type, at the same place.
///
/// Stable Rust cannot compare types in `const`, so an identity is a name, together with the
/// identities of the parameters that make up the type. `__site!` names a type that derives `Pun` by
/// where it is declared, and this crate names the types of `core` that it supports. Two identities
/// are equal only if both are known, with the same name, length and parameters.
///
/// [`Id::NONE`] is the identity of a type that has none (a generic type that derives `Pun`,
/// `PhantomData`, or a `Scalar` implemented by hand). It is equal to no identity, not even itself,
/// and neither is an identity with a parameter that has none. Where two types have the same
/// identity, the analysis still requires their layouts to be the same, so an identity can only
/// refuse.
#[doc(hidden)]
#[derive(Copy, Clone, Debug)]
pub struct Id {
  name: &'static str,
  args: &'static [Id],
  len: usize,
}

impl Id {
  /// The identity of a type that has none, equal to no identity.
  pub const NONE: Self = Self { name: "", args: &[], len: 0 };

  /// Returns the identity of the type `name`, whose parameters have the identities `args`.
  #[doc(hidden)]
  #[must_use]
  pub const fn new(name: &'static str, args: &'static [Id]) -> Self {
    Self { name, args, len: 0 }
  }

  /// Returns the identity of the array type `name` of length `len`, whose parameters have the
  /// identities `args`.
  #[doc(hidden)]
  #[must_use]
  pub const fn sized(name: &'static str, args: &'static [Id], len: usize) -> Self {
    Self { name, args, len }
  }

  /// Returns whether the identity is known. It is known if the type and every parameter, at any
  /// depth, have a name.
  const fn known(&self) -> bool {
    if self.name.is_empty() {
      return false;
    }
    let mut i = 0;
    while i < self.args.len() {
      if !self.args[i].known() {
        return false;
      }
      i += 1;
    }
    true
  }

  /// Returns whether `self` and `other` are the identity of the same type (both are known, with the
  /// same name, length and parameters).
  pub(crate) const fn equals(&self, other: &Id) -> bool {
    if !self.known() || !other.known() || self.len != other.len {
      return false;
    }
    let (a, b) = (self.name.as_bytes(), other.name.as_bytes());
    if a.len() != b.len() || self.args.len() != other.args.len() {
      return false;
    }
    let mut i = 0;
    while i < a.len() {
      if a[i] != b[i] {
        return false;
      }
      i += 1;
    }
    let mut i = 0;
    while i < self.args.len() {
      if !self.args[i].equals(&other.args[i]) {
        return false;
      }
      i += 1;
    }
    true
  }
}

/// What a layout is made of.
#[derive(Copy, Clone, Debug)]
pub(crate) enum Kind {
  /// A scalar, which is also the layout of an enum without fields.
  Scalar(Spec),
  /// An array of this many elements of this layout.
  Array(&'static Layout, usize),
  /// A slice of elements of this layout.
  Slice(&'static Layout),
  /// A struct: its fields, sorted by offset, and its rules. This is also the layout of a variant,
  /// of padding, and of `MaybeUninit`.
  Struct(&'static [Field], &'static [Rule]),
  /// A tail of bytes that a [`Dfa`] accepts.
  Pattern(&'static Dfa),
  /// An enum with fields, a union, or a type without values: a tag, and the fields of the variant
  /// that the tag selects.
  Enum(&'static Enum),
}

/// The tag and the variants of an enum with fields, or the fields of a union.
///
/// [`Layout::enumeration`] makes the layout of an enum from it. `#[derive(Pun)]` also makes the
/// layout of a union from one, with functions of its own. The tag of an enum is a scalar at a fixed
/// place, whose value tells the variant. Where Rust specifies the representation of the enum, the
/// tag is where the representation puts it. With the `Rust` representation, it is where `rustc` put
/// it, as `#[derive(Pun)]` finds it, and it may then be a niche, a scalar among the fields of one
/// variant, whose valid values are that variant and whose other values are the other variants.
///
/// # Examples
///
/// The layout of `#[repr(u8)] enum Shape { Dot, Square(u16) }`, as `#[derive(Pun)]` makes it:
///
/// ```
/// use core::mem::offset_of;
///
/// use puns::layout::{Endian, Enum, Field, Layout, Spec, Variant};
///
/// #[derive(Clone, Copy)]
/// #[repr(u8)]
/// enum Shape {
///   Dot,
///   Square(u16),
/// }
///
/// // The `repr(C)` structs of the variants that `repr(u8)` specifies: a tag, then fields.
/// #[repr(C)]
/// struct Dot(u8);
/// #[repr(C)]
/// struct Square(u8, u16);
///
/// // The tags are 0 and 1.
/// const TAG: Layout = Layout::scalar::<u8>(Spec::ranges::<u8>(Endian::NATIVE, &[(0, 1)]));
/// const SIDE: Layout = Layout::scalar::<u16>(Spec::any::<u16>());
/// // The fields of each variant, from the start of the enum.
/// const SQUARE_FIELDS: [Field; 1] =
///   Layout::sort_fields([Field::new(offset_of!(Square, 1), &SIDE)]);
/// const DOT: Layout = Layout::variant::<Shape>(&[]);
/// const SQUARE: Layout = Layout::variant::<Shape>(&SQUARE_FIELDS);
/// const VARIANTS: [Variant; 2] = [Variant::new(0, &DOT), Variant::new(1, &SQUARE)];
/// const SHADOWS: [(usize, usize); 2] =
///   [(size_of::<Dot>(), align_of::<Dot>()), (size_of::<Square>(), align_of::<Square>())];
/// const SHAPE: Enum = Enum::new(&TAG, false, &SHADOWS, 0, &VARIANTS);
/// const LAYOUT: Layout = Layout::enumeration::<Shape>(&SHAPE);
///
/// assert_eq!((LAYOUT.size(), LAYOUT.align()), (size_of::<Shape>(), align_of::<Shape>()));
/// ```
#[derive(Debug)]
pub struct Enum {
  /// The tag, a scalar whose valid values are the tags of the variants, at its offset in the enum.
  /// For a union and for a type without values, it is a scalar of size 0.
  pub(crate) tag: Field,
  /// The variants, in the order of their declarations.
  pub(crate) variants: &'static [Variant],
  /// The size and alignment that the representation of the enum gives, before any `align(N)`.
  /// Unused for a union.
  pub(crate) specified: (usize, usize),
  /// Whether this is a union. A union has no tag, its variants are its fields, and bytes are a
  /// valid value if they are a valid value of one of them.
  pub(crate) union: bool,
}

impl Enum {
  /// Returns the enum that `repr(C)`, `repr(u8)` or another integer type, or both, specify, laid
  /// out as [RFC 2195] describes.
  ///
  /// The tag is the scalar `tag`, whose valid values are the tags of the `variants`. Each variant
  /// is a `repr(C)` struct of its fields, called its *shadow*, and `shadows` holds the size and
  /// alignment of the shadow of each variant.
  ///
  /// With `repr(C)`, the tag is `outside` the shadows. The shadows then start at the offset
  /// `fields`, which must be [`Enum::fields`] of `tag` and `shadows`, and a field is at that offset
  /// plus its offset in its shadow. With an integer type alone, the tag is the first field of each
  /// shadow, and `fields` must be 0. In both cases, the offsets of the fields of `variants` are
  /// counted from the start of the enum.
  ///
  /// [RFC 2195]: https://rust-lang.github.io/rfcs/2195-really-tagged-unions.html
  ///
  /// # Panics
  ///
  /// Panics if `fields` is not where the shadows start, or if the size of the enum overflows a
  /// `usize`.
  ///
  /// # Examples
  ///
  /// With the types of the example of [`Enum`]:
  ///
  /// ```
  /// # use core::mem::offset_of;
  /// #
  /// # use puns::layout::{Endian, Enum, Field, Layout, Spec, Variant};
  /// #
  /// # #[derive(Clone, Copy)]
  /// # #[repr(u8)]
  /// # enum Shape {
  /// #   Dot,
  /// #   Square(u16),
  /// # }
  /// #
  /// # // The `repr(C)` structs of the variants that `repr(u8)` specifies: a tag, then fields.
  /// # #[repr(C)]
  /// # struct Dot(u8);
  /// # #[repr(C)]
  /// # struct Square(u8, u16);
  /// #
  /// # // The tags are 0 and 1.
  /// # const TAG: Layout = Layout::scalar::<u8>(Spec::ranges::<u8>(Endian::NATIVE, &[(0, 1)]));
  /// # const SIDE: Layout = Layout::scalar::<u16>(Spec::any::<u16>());
  /// # // The fields of each variant, from the start of the enum.
  /// # const SQUARE_FIELDS: [Field; 1] =
  /// #   Layout::sort_fields([Field::new(offset_of!(Square, 1), &SIDE)]);
  /// # const DOT: Layout = Layout::variant::<Shape>(&[]);
  /// # const SQUARE: Layout = Layout::variant::<Shape>(&SQUARE_FIELDS);
  /// # const VARIANTS: [Variant; 2] = [Variant::new(0, &DOT), Variant::new(1, &SQUARE)];
  /// const SHADOWS: [(usize, usize); 2] =
  ///   [(size_of::<Dot>(), align_of::<Dot>()), (size_of::<Square>(), align_of::<Square>())];
  /// const SHAPE: Enum = Enum::new(&TAG, false, &SHADOWS, 0, &VARIANTS);
  /// const LAYOUT: Layout = Layout::enumeration::<Shape>(&SHAPE);
  ///
  /// // Each variant has a tag of its own: which bytes are padding depends on it.
  /// assert!(!LAYOUT.is_plain());
  /// ```
  #[must_use]
  pub const fn new(
    tag: &'static Layout,
    outside: bool,
    shadows: &[(usize, usize)],
    fields: usize,
    variants: &'static [Variant],
  ) -> Self {
    let (mut size, mut align, mut i) = (0, 1, 0);
    while i < shadows.len() {
      if shadows[i].0 > size {
        size = shadows[i].0;
      }
      if shadows[i].1 > align {
        align = shadows[i].1;
      }
      i += 1;
    }
    let specified = if outside {
      assert!(fields == Self::fields(tag, shadows), "not where the fields of the variants are");
      let align = if tag.align > align { tag.align } else { align };
      (tail_offset(fields + size, align, usize::MAX), align)
    } else {
      assert!(fields == 0, "not where the fields of the variants are");
      (tail_offset(size, align, usize::MAX), align)
    };
    Self { tag: Field::new(0, tag), variants, specified, union: false }
  }

  /// Returns the enum of the `Rust` representation that `rustc` laid out, as `discover.rs` found
  /// it, for `#[derive(Pun)]`.
  ///
  /// The tag is the scalar `tag` at `offset`. The `variants` are in declaration order, as
  /// `Variant::discovered` makes them, and `specified` is the size and alignment of the enum.
  /// [`Layout::enumeration`] checks the result against the enum.
  #[doc(hidden)]
  #[must_use]
  pub const fn discovered(
    tag: &'static Layout,
    offset: usize,
    variants: &'static [Variant],
    specified: (usize, usize),
  ) -> Self {
    Self { tag: Field::new(offset, tag), variants, specified, union: false }
  }

  /// Returns the `repr(C)` union of `fields`, for `#[derive(Pun)]`.
  ///
  /// Each field is a variant with one field at offset 0, in declaration order. `Layout::union`
  /// checks the result against the union.
  #[doc(hidden)]
  #[must_use]
  pub const fn union(fields: &'static [Variant]) -> Self {
    Self { tag: Field::new(0, &NO_TAG), variants: fields, specified: (0, 1), union: true }
  }

  /// The enum without variants, which has no value.
  const NEVER: Self =
    Self { tag: Field::new(0, &NO_TAG), variants: &[], specified: (0, 1), union: false };

  /// Returns the offset of the variant fields of a `repr(C)` enum, whose tag is outside the
  /// `shadows` ([`Enum::new`]).
  ///
  /// This is the size of `tag`, rounded up to the greatest alignment of a shadow.
  ///
  /// # Panics
  ///
  /// Panics if the offset overflows a `usize`.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::layout::{Enum, Layout, Spec};
  ///
  /// // `#[repr(C, u8)] enum E { A(u8), B(u16) }`: the tag, then the fields of a variant, aligned
  /// // for the most aligned of the variants.
  /// const TAG: Layout = Layout::scalar::<u8>(Spec::any::<u8>());
  /// let shadows = [(size_of::<u8>(), align_of::<u8>()), (size_of::<u16>(), align_of::<u16>())];
  /// assert_eq!(Enum::fields(&TAG, &shadows), align_of::<u16>());
  /// ```
  #[must_use]
  pub const fn fields(tag: &Layout, shadows: &[(usize, usize)]) -> usize {
    let (mut align, mut i) = (1, 0);
    while i < shadows.len() {
      if shadows[i].1 > align {
        align = shadows[i].1;
      }
      i += 1;
    }
    tail_offset(tag.size, align, usize::MAX)
  }
}

/// A variant of an enum with fields, or a field of a union.
///
/// A variant holds its tags, and the layout of the enum when it is this variant, a struct of its
/// fields at their offsets from the start of the enum ([`Layout::variant`]).
///
/// # Examples
///
/// With the types of the example of [`Enum`]:
///
/// ```
/// # use core::mem::offset_of;
/// #
/// # use puns::layout::{Endian, Enum, Field, Layout, Spec, Variant};
/// #
/// # #[derive(Clone, Copy)]
/// # #[repr(u8)]
/// # enum Shape {
/// #   Dot,
/// #   Square(u16),
/// # }
/// #
/// # // The `repr(C)` structs of the variants that `repr(u8)` specifies: a tag, then fields.
/// # #[repr(C)]
/// # struct Dot(u8);
/// # #[repr(C)]
/// # struct Square(u8, u16);
/// #
/// # // The tags are 0 and 1.
/// # const TAG: Layout = Layout::scalar::<u8>(Spec::ranges::<u8>(Endian::NATIVE, &[(0, 1)]));
/// # const SIDE: Layout = Layout::scalar::<u16>(Spec::any::<u16>());
/// # // The fields of each variant, from the start of the enum.
/// # const SQUARE_FIELDS: [Field; 1] =
/// #   Layout::sort_fields([Field::new(offset_of!(Square, 1), &SIDE)]);
/// # const DOT: Layout = Layout::variant::<Shape>(&[]);
/// # const SQUARE: Layout = Layout::variant::<Shape>(&SQUARE_FIELDS);
/// const VARIANTS: [Variant; 2] = [Variant::new(0, &DOT), Variant::new(1, &SQUARE)];
/// const SHADOWS: [(usize, usize); 2] =
///   [(size_of::<Dot>(), align_of::<Dot>()), (size_of::<Square>(), align_of::<Square>())];
/// const SHAPE: Enum = Enum::new(&TAG, false, &SHADOWS, 0, &VARIANTS);
/// const LAYOUT: Layout = Layout::enumeration::<Shape>(&SHAPE);
///
/// // A value of it exists: one of its variants has one.
/// assert!(LAYOUT.is_inhabited());
/// ```
#[derive(Copy, Clone, Debug)]
pub struct Variant {
  /// The value of the tag for this variant, if `ranges` is `None`. Otherwise, the index of the
  /// variant, which tells it apart from the others in the type-level lists of variants.
  pub(crate) tag: u128,
  /// The values of the tag for this variant, as sorted inclusive ranges, if they are not `tag`
  /// alone. The variant of a niche has several, and a variant that no value can be, since it holds
  /// a type without values, has none.
  pub(crate) ranges: Option<&'static [(u128, u128)]>,
  /// Whether the tag is in the fields of this variant, as a niche.
  pub(crate) holds_tag: bool,
  /// The enum when it is this variant, a struct of its fields in which the tag is padding, except
  /// in the variant that holds it.
  pub(crate) layout: &'static Layout,
}

/// A set of values of a tag, as sorted inclusive ranges, read one range at a time.
#[derive(Copy, Clone)]
pub(crate) struct Tags {
  /// The one range, if `ranges` is `None`.
  one: (u128, u128),
  /// The ranges, sorted, if there is more than one.
  ranges: Option<&'static [(u128, u128)]>,
}

impl Tags {
  /// Returns the set of the values `lo..=hi`.
  pub(crate) const fn range(lo: u128, hi: u128) -> Self {
    Self { one: (lo, hi), ranges: None }
  }

  /// Returns the set of the values of `ranges`, which must be sorted.
  pub(crate) const fn of(ranges: &'static [(u128, u128)]) -> Self {
    Self { one: (0, 0), ranges: Some(ranges) }
  }

  /// Returns the number of ranges.
  pub(crate) const fn len(&self) -> usize {
    match self.ranges {
      Some(ranges) => ranges.len(),
      None => 1,
    }
  }

  /// Returns the `index`th range.
  ///
  /// # Panics
  ///
  /// Panics if `index` is not less than [`Tags::len`], unless the set is one range.
  pub(crate) const fn get(&self, index: usize) -> (u128, u128) {
    match self.ranges {
      Some(ranges) => ranges[index],
      None => self.one,
    }
  }

  /// Returns whether `value` is in the set.
  pub(crate) const fn has(&self, value: u128) -> bool {
    let mut i = 0;
    while i < self.len() {
      let (lo, hi) = self.get(i);
      if lo <= value && value <= hi {
        return true;
      }
      i += 1;
    }
    false
  }

  /// Returns whether a value is in both `self` and `other`.
  pub(crate) const fn meets(&self, other: &Tags) -> bool {
    let (mut mine, mut theirs) = (0, 0);
    while mine < self.len() && theirs < other.len() {
      let ((lo, hi), (other_lo, other_hi)) = (self.get(mine), other.get(theirs));
      if lo <= other_hi && other_lo <= hi {
        return true;
      }
      if hi < other_hi {
        mine += 1;
      } else {
        theirs += 1;
      }
    }
    false
  }

  /// Returns whether every value of `self` is in `other`.
  pub(crate) const fn within(&self, other: &Tags) -> bool {
    let mut i = 0;
    while i < self.len() {
      let ((lo, hi), mut j, mut inside) = (self.get(i), 0, false);
      while j < other.len() {
        let (a, b) = other.get(j);
        inside |= a <= lo && hi <= b;
        j += 1;
      }
      if !inside {
        return false;
      }
      i += 1;
    }
    true
  }

  /// Returns whether `self` and `other` have the same ranges.
  const fn equals(&self, other: &Tags) -> bool {
    if self.len() != other.len() {
      return false;
    }
    let mut index = 0;
    while index < self.len() {
      let (mine, theirs) = (self.get(index), other.get(index));
      if mine.0 != theirs.0 || mine.1 != theirs.1 {
        return false;
      }
      index += 1;
    }
    true
  }
}

impl Variant {
  /// Returns the variant whose tag is `tag`, and whose fields are `layout`.
  ///
  /// `tag` is the value of the enum's tag for this variant, read as an unsigned integer in the byte
  /// order of the tag, and `layout` is the [`Layout::variant`] of its fields.
  /// [`Layout::enumeration`] checks that `tag` is a valid value of the tag, and that no other
  /// variant has it.
  ///
  /// # Examples
  ///
  /// With the types of the example of [`Enum`]:
  ///
  /// ```
  /// # use core::mem::offset_of;
  /// #
  /// # use puns::layout::{Endian, Enum, Field, Layout, Spec, Variant};
  /// #
  /// # #[derive(Clone, Copy)]
  /// # #[repr(u8)]
  /// # enum Shape {
  /// #   Dot,
  /// #   Square(u16),
  /// # }
  /// #
  /// # // The `repr(C)` structs of the variants that `repr(u8)` specifies: a tag, then fields.
  /// # #[repr(C)]
  /// # struct Dot(u8);
  /// # #[repr(C)]
  /// # struct Square(u8, u16);
  /// #
  /// # // The tags are 0 and 1.
  /// # const TAG: Layout = Layout::scalar::<u8>(Spec::ranges::<u8>(Endian::NATIVE, &[(0, 1)]));
  /// # const SIDE: Layout = Layout::scalar::<u16>(Spec::any::<u16>());
  /// # // The fields of each variant, from the start of the enum.
  /// # const SQUARE_FIELDS: [Field; 1] =
  /// #   Layout::sort_fields([Field::new(offset_of!(Square, 1), &SIDE)]);
  /// # const DOT: Layout = Layout::variant::<Shape>(&[]);
  /// # const SQUARE: Layout = Layout::variant::<Shape>(&SQUARE_FIELDS);
  /// const VARIANTS: [Variant; 2] = [Variant::new(0, &DOT), Variant::new(1, &SQUARE)];
  /// const SHADOWS: [(usize, usize); 2] =
  ///   [(size_of::<Dot>(), align_of::<Dot>()), (size_of::<Square>(), align_of::<Square>())];
  /// const SHAPE: Enum = Enum::new(&TAG, false, &SHADOWS, 0, &VARIANTS);
  /// const LAYOUT: Layout = Layout::enumeration::<Shape>(&SHAPE);
  ///
  /// // `Layout::enumeration` accepts the variants, since each tag is a value of the tag of the
  /// // enum and no two variants share one, and the layout has the size of the enum.
  /// assert_eq!(LAYOUT.size(), size_of::<Shape>());
  /// ```
  #[must_use]
  pub const fn new(tag: u128, layout: &'static Layout) -> Self {
    Self { tag, ranges: None, holds_tag: false, layout }
  }

  /// Returns the tags of the variants of the enum `E`, with fields and a primitive or `C`
  /// representation, for `#[derive(Pun)]`, each read as an unsigned integer in the byte order of
  /// the target.
  ///
  /// `tags` are the variants of `T`, the enum without fields that the derive declares with the
  /// representation and the discriminant expressions of `E`, `Self` replaced by `E`. They are the
  /// tags of `E` only if each expression evaluates to the same value in both enums. But `T` is
  /// declared elsewhere, where a name that a macro writes may mean another item. So each tag is
  /// compared with the one in `E`:
  ///
  /// - If `samples[i]` is a value of the variant `i`, it starts with its tag ([RFC 2195]), which
  ///   must have the bytes of `tags[i]`.
  /// - Otherwise, `declared[i]` tells whether the tag is known from the declaration: `Some(true)`
  ///   for an expression that holds no macro and no attribute and that the derive parsed in full,
  ///   which has the same value in both enums, `Some(false)` for another expression, and `None` for
  ///   no discriminant, which is the tag before it plus one (0 for the first), so known if that one
  ///   is.
  ///
  /// A variant without a sample of which `exists` says that a value may exist in a source holds a
  /// union none of whose fields has values. Its tag must be known. That of a variant of which no value may exist need not be: no value has it, and it is
  /// not the tag of another variant, since the tags of `T` are distinct, and those of the variants
  /// of which a value may exist are those of `E`.
  ///
  /// [RFC 2195]: https://rust-lang.github.io/rfcs/2195-really-tagged-unions.html
  ///
  /// # Panics
  ///
  /// Panics if `T` is larger than 16 bytes, or larger than `E`, if a sample does not start with the
  /// bytes of its variant of `T`, or if a variant of which a value may exist has no sample and a
  /// tag that is not known.
  ///
  /// # Safety
  ///
  /// The caller must ensure that:
  ///
  /// - Every byte of each of `tags` is initialized.
  /// - The first `size_of::<T>()` bytes of each sample are initialized.
  #[doc(hidden)]
  #[must_use]
  pub const unsafe fn declared_tags<E, T, const N: usize>(
    tags: &[T; N],
    samples: &Samples<E, N>,
    declared: [Option<bool>; N],
    exists: [bool; N],
  ) -> [u128; N] {
    let size = size_of::<T>();
    assert!(size <= 16, "an enum without fields cannot be larger than 16 bytes");
    assert!(size <= size_of::<E>(), "the tag is larger than the enum");
    let (mut bits, mut known, mut i) = ([0; N], true, 0);
    while i < N {
      // SAFETY: `size <= 16`, which is asserted above. `tags[i]` is a `T` in `tags`, a reference,
      // so it is valid for reads of `size` bytes, which are initialized, by contract on the caller.
      // These are the requirements of `load`.
      bits[i] = unsafe { load((&raw const tags[i]).cast(), size, Endian::NATIVE) };
      known = match &samples[i] {
        Some(sample) => {
          // SAFETY: `size <= 16`, and `size <= size_of::<E>()`, which are asserted above.
          // `sample` is a `MaybeUninit<E>` in `samples`, a reference, so it is valid for reads of
          // its first `size` bytes, which are initialized, by contract on the caller. These are
          // the requirements of `load`.
          let tag = unsafe { load(sample.as_ptr().cast(), size, Endian::NATIVE) };
          assert!(tag == bits[i], "a discriminant has another value where the derive declares it");
          true
        }
        None => match declared[i] {
          Some(literal) => literal,
          None => known,
        },
      };
      assert!(
        known || !exists[i],
        "the tag of a variant without a sample is not known: its discriminant must hold no macro \
         and no attribute, or it must have none after a variant whose tag is known"
      );
      i += 1;
    }
    bits
  }

  /// Returns the `index`th variant of an enum of the `Rust` representation, as `discover.rs` found
  /// it, for `#[derive(Pun)]`.
  ///
  /// The values of the tag for this variant are `ranges`, sorted and disjoint, and there are none
  /// if no value can be this variant. If `holds_tag`, the tag is in the fields of the variant, as a
  /// niche. `layout` is the [`Layout::variant`] of its fields.
  #[doc(hidden)]
  #[must_use]
  pub const fn discovered(
    index: usize,
    ranges: &'static [(u128, u128)],
    holds_tag: bool,
    layout: &'static Layout,
  ) -> Self {
    Self { tag: index as u128, ranges: Some(ranges), holds_tag, layout }
  }

  /// Returns the values of the tag for this variant.
  pub(crate) const fn tags(&self) -> Tags {
    Tags { one: (self.tag, self.tag), ranges: self.ranges }
  }

  /// Returns whether a value can be this variant (some value of the tag selects it, and its fields
  /// have values).
  pub(crate) const fn is_present(&self) -> bool {
    !matches!(self.ranges, Some(ranges) if ranges.is_empty()) && self.layout.is_inhabited()
  }

  /// Returns whether a value of a source may be this variant (some value of the tag selects it,
  /// and a value of its fields may exist, [`Layout::may_exist`]).
  pub(crate) const fn may_exist(&self) -> bool {
    !matches!(self.ranges, Some(ranges) if ranges.is_empty()) && self.layout.may_exist()
  }

  /// Returns the fields of the variant, sorted by offset from the start of the enum.
  ///
  /// # Panics
  ///
  /// Panics if the layout of the variant is not a struct.
  pub(crate) const fn fields(&self) -> &'static [Field] {
    let Kind::Struct(fields, _) = self.layout.kind else { panic!("not a variant") };
    fields
  }
}

/// A field of a struct or of a variant: its offset and its layout.
///
/// [`Layout::sort_fields`] numbers the fields of a struct and sorts them by offset, and
/// [`Layout::structure`] makes the layout of the struct from them.
///
/// # Examples
///
/// ```
/// use core::mem::offset_of;
///
/// use puns::layout::{Field, Layout, Spec};
///
/// #[repr(C)]
/// struct Pair {
///   low: u8,
///   high: u16,
/// }
///
/// const LOW: Layout = Layout::scalar::<u8>(Spec::any::<u8>());
/// const HIGH: Layout = Layout::scalar::<u16>(Spec::any::<u16>());
/// const FIELDS: [Field; 2] = Layout::sort_fields([
///   Field::new(offset_of!(Pair, low), &LOW),
///   Field::new(offset_of!(Pair, high), &HIGH),
/// ]);
/// // SAFETY: `Pair` is `Sized`, and `Layout::structure` requires nothing of a `Sized` type.
/// const PAIR: Layout = unsafe { Layout::structure::<Pair>(core::ptr::null(), &FIELDS, &[]) };
///
/// assert_eq!((PAIR.size(), PAIR.align()), (size_of::<Pair>(), align_of::<Pair>()));
/// ```
#[derive(Copy, Clone, Debug)]
pub struct Field {
  /// The offset of the field in its struct, or in its enum for the field of a variant.
  pub(crate) offset: usize,
  /// The layout of the field.
  pub(crate) layout: &'static Layout,
  /// The index of the field in declaration order.
  pub(crate) index: usize,
  /// The number of leaves ([`Layout::leaves`]) of the fields declared before this one.
  pub(crate) leaf: usize,
  /// Whether the layout is a scalar that requires more than the type of the field guarantees
  /// ([`Layout::restricted`]).
  pub(crate) restricted: bool,
}

impl Field {
  /// Returns the field at `offset` in its struct, of the layout `layout`.
  ///
  /// The offset of the field of a variant is counted from the start of its enum.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::layout::{Field, Layout, Spec};
  ///
  /// #[repr(C)]
  /// struct Wrapper(u32);
  ///
  /// const VALUE: Layout = Layout::scalar::<u32>(Spec::any::<u32>());
  /// const FIELDS: [Field; 1] = [Field::new(0, &VALUE)];
  /// // SAFETY: `Wrapper` is `Sized`, and `Layout::structure` requires nothing of a `Sized` type.
  /// const WRAPPER: Layout =
  ///   unsafe { Layout::structure::<Wrapper>(core::ptr::null(), &FIELDS, &[]) };
  ///
  /// // Every byte is the field's: plain data, with nothing to check.
  /// assert!(WRAPPER.is_plain() && WRAPPER.is_free());
  /// ```
  #[inline]
  #[must_use]
  pub const fn new(offset: usize, layout: &'static Layout) -> Self {
    Self { offset, layout, index: 0, leaf: 0, restricted: false }
  }

  /// Returns the field at `offset`, with the scalar `layout`, which requires more than the type of
  /// the field guarantees.
  ///
  /// This is what `#[pun(valid(..))]` makes of a field, with a `layout` made by
  /// [`Layout::restricted`]. Since safe code may write any value of its type to the field, the
  /// field is required to hold the values of `layout`, but is known to hold only what its type
  /// guarantees.
  ///
  /// # Panics
  ///
  /// Panics if `layout` is not a scalar.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::layout::{Field, Layout, Spec};
  ///
  /// #[repr(C)]
  /// struct Version(u8);
  ///
  /// const BYTE: Layout = Layout::scalar::<u8>(Spec::any::<u8>());
  /// const DIGIT: Layout = BYTE.restricted(&[(0, 9)]);
  /// const FIELDS: [Field; 1] = [Field::restricted(0, &DIGIT)];
  /// // SAFETY: `Version` is `Sized`, and `Layout::structure` requires nothing of a `Sized` type.
  /// const VERSION: Layout =
  ///   unsafe { Layout::structure::<Version>(core::ptr::null(), &FIELDS, &[]) };
  ///
  /// // Its byte is checked where a `Version` is made.
  /// assert!(!VERSION.is_free());
  /// ```
  #[inline]
  #[must_use]
  pub const fn restricted(offset: usize, layout: &'static Layout) -> Self {
    assert!(matches!(layout.kind, Kind::Scalar(_)), "only scalar fields can be restricted");
    Self { offset, layout, index: 0, leaf: 0, restricted: true }
  }

  /// Returns the scalar of the field if it is restricted ([`Field::restricted`]), and `None`
  /// otherwise.
  #[doc(hidden)]
  #[must_use]
  pub const fn restriction(&self) -> Option<Spec> {
    match self.layout.kind {
      Kind::Scalar(spec) if self.restricted => Some(spec),
      _ => None,
    }
  }

  /// Returns the offset of the field in its struct, or in its enum for the field of a variant.
  #[doc(hidden)]
  #[must_use]
  pub const fn offset(&self) -> usize {
    self.offset
  }

  /// Returns the number of leaves of the fields declared before this one, which is where the leaves
  /// of this field start among those of its struct.
  #[doc(hidden)]
  #[must_use]
  pub const fn leaf(&self) -> usize {
    self.leaf
  }
}

/// The layout of a type: its size, its alignment, its parts, and its valid values.
///
/// Every [`Pun`](crate::Pun) type has a layout, which `#[derive(Pun)]` makes with the functions
/// below, and the analysis compares layouts at compile time.
///
/// The layout of a type is the one that `rustc` chose for the build, since the size, the alignment
/// and the offsets come from the compiler. Each layout also records which parts of it are the same
/// in every layout that Rust allows for the type, and the identity of the type. Without the
/// `unstable` feature ([`UNSTABLE`]), the analysis relies only on those.
///
/// A layout makes no value of a type. It stands for a type only through an implementation of `Pun`,
/// an `unsafe trait` that guarantees that the layout describes the type.
///
/// # Examples
///
/// ```
/// use puns::layout::{Layout, Spec};
///
/// const BYTE: Layout = Layout::scalar::<u8>(Spec::any::<u8>());
/// const WORD: Layout = Layout::array(&BYTE, 4);
///
/// assert_eq!((WORD.size(), WORD.align()), (4, 1));
/// // Any 4 bytes are one.
/// assert!(WORD.is_plain() && WORD.is_free());
/// ```
#[derive(Copy, Clone, Debug)]
pub struct Layout {
  /// What the layout is made of.
  pub(crate) kind: Kind,
  /// The size of the type if it is `Sized`, and otherwise the offset of the first element of its
  /// tail slice.
  pub(crate) size: usize,
  /// The alignment of the type.
  pub(crate) align: usize,
  /// The size of an element of the tail slice, or 0 if the type is `Sized`.
  pub(crate) tail: usize,
  /// Whether the type is unsized, ending with a slice or a pattern.
  pub(crate) unsized_: bool,
  /// The number of leaves, the parts that the validation may have to check: scalars that accept
  /// only some values (or none), patterns, enums with fields, unions, types without values, and
  /// structs with rules. The element of an array or a slice counts once.
  pub(crate) leaves: usize,
  /// What every byte is, if every byte is the same thing.
  pub(crate) uniform: Uniform,
  /// Whether a struct in the layout, at any depth, has rules. The fields of the variants of an enum
  /// do not count, since the enum is checked as a whole.
  pub(crate) has_rules: bool,
  /// Whether this is the layout of [`Deferred<T>`](crate::Deferred): the layout of `T`, of which a
  /// target takes the opaque scalars by their values. Only the root of a target is read.
  pub(crate) deferred: bool,
  /// What the layout has in common with every layout that Rust allows for its type.
  pub(crate) stable: Stable,
  /// The identity of the type.
  pub(crate) id: Id,
}

/// The kind of a layout, without its parts. It is what [`validate`](crate::validate::validate) has
/// to do for a type.
#[derive(Copy, Clone)]
pub(crate) enum Shape {
  Scalar(Spec),
  Array(usize),
  Slice,
  Struct,
  Pattern(&'static Dfa),
  Enum(&'static Enum),
}

/// Returns `value` rounded up to a multiple of `align`, or `None` if that overflows a `usize`.
///
/// `align` must be a power of two.
#[inline]
pub(crate) const fn round_up(value: usize, align: usize) -> Option<usize> {
  match value.checked_add(align - 1) {
    Some(v) => Some(v & !(align - 1)),
    None => None,
  }
}

/// Returns the offset of a field of a `repr(C)` struct, as the representation specifies it.
///
/// `end` is the offset where the previous field ends, or 0 for the first field. `align` is the
/// alignment of the type of the field, a power of two. `pack` is the `N` of `repr(packed(N))`, or
/// `usize::MAX` for a struct that is not packed. The field is at `end`, rounded up to the lesser of
/// `align` and `pack`.
///
/// # Panics
///
/// Panics if the offset overflows a `usize`.
#[doc(hidden)]
#[must_use]
pub const fn tail_offset(end: usize, align: usize, pack: usize) -> usize {
  match round_up(end, if align < pack { align } else { pack }) {
    Some(offset) => offset,
    None => panic!("struct is too large"),
  }
}

/// The tag of a union and of an enum without variants, a scalar of size 0.
const NO_TAG: Layout = Layout::scalar::<()>(Spec::any::<()>());

/// Does nothing. `Layout::PIN` holds instances of it, whose types name the types it pins.
fn pin<T>(_: T) {}

/// Whether the `unstable` feature is enabled.
///
/// Rust leaves part of some layouts to the compiler, which may choose differently in its next
/// version, or at random under `-Zrandomize-layout`, as [The `unstable`
/// feature](crate#the-unstable-feature) lists.
///
/// Without the `unstable` feature, nothing that `puns` does depends on a layout that Rust leaves to
/// the compiler, so whether a pun compiles, what it checks, and which values it accepts are the
/// same in every layout that Rust allows. Only the message of a refusal may differ.
///
/// With the `unstable` feature, `puns` uses the layout that `rustc` chose for the build, as it is,
/// and a pun is still sound for the build that compiles it. Everything that compiles without the
/// feature also compiles with it and does the same, and more compiles. See [The `unstable`
/// feature](crate#the-unstable-feature).
pub const UNSTABLE: bool = cfg!(feature = "unstable");

impl Layout {
  /// The types whose functions the code of `#[derive(Pun)]` calls to make a layout, pinned to the
  /// types of this crate.
  ///
  /// The generated code names these types by their path in the crate that `#[pun(crate = ..)]`
  /// names, which safe code can make name something else. It also writes this constant with the
  /// same paths, as a tuple of `fn` types, so a path that names another type does not compile,
  /// because a `fn` type is not a path, and nothing can replace it. The functions that make a layout
  /// are associated functions of these types, and methods of the values that they return. The other
  /// types that this code names by a path are fixed by the types of these functions: the
  /// description of an enum of the `Rust` representation is the type that [`Layout::discover`]
  /// returns. The rest of what the generated code names through the path is fixed by the traits
  /// that it implements, checked against the layout, or makes no layout ("What the derive trusts" in
  /// the documentation of `#[derive(Pun)]`).
  #[doc(hidden)]
  #[expect(
    clippy::type_complexity,
    reason = "the derive writes this tuple of `fn` types out, with the paths it is told"
  )]
  pub const PIN: (
    fn(Layout),
    fn(Spec),
    fn(Id),
    fn(Rule),
    fn(Op),
    fn(Field),
    fn(Enum),
    fn(Variant),
    fn(Crc),
    fn(Checksum),
    fn(Varint),
  ) = (
    pin::<Layout>,
    pin::<Spec>,
    pin::<Id>,
    pin::<Rule>,
    pin::<Op>,
    pin::<Field>,
    pin::<Enum>,
    pin::<Variant>,
    pin::<Crc>,
    pin::<Checksum>,
    pin::<Varint>,
  );

  /// Returns an uninitialized `T`, for `#[derive(Pun)]`, which finds the offsets of the fields of
  /// `T` by projecting a pointer to it.
  #[doc(hidden)]
  #[must_use]
  pub const fn uninit<T>() -> mem::MaybeUninit<T> {
    mem::MaybeUninit::uninit()
  }

  /// Returns a pointer to a `T` in memory of const evaluation, whose slice tail, if any, has no
  /// element, for `#[derive(Pun)]`, which measures the offsets of the fields of `T` by projecting
  /// it, or a null pointer if such a `T` is larger than the memory (4096 bytes).
  ///
  /// # Safety
  ///
  /// The caller must ensure that either `T` is `Sized`, or `T` is a struct whose last field is a
  /// slice, `str`, or a struct whose last field is one of these, at any depth, and `probe` is
  /// `<[E] as Pun>::PROBE` cast to `*const T` with `as` casts only.
  #[doc(hidden)]
  #[must_use]
  pub const unsafe fn measure_base<T: ?Sized>(probe: *const T) -> *const T {
    // SAFETY: By contract on the caller, `T` is `Sized`, or it is slice-tailed and `probe` is
    // `slice_from_raw_parts(null(), PROBE_LEN)` cast with `as` casts only, which are the
    // requirements of `verify`, and so of `in_buffer`.
    unsafe { ptr::in_buffer(probe) }
  }

  /// Returns what the experiments find of the layout of the enum `E` of the `Rust`
  /// representation, for `#[derive(Pun)]`, as the module documentation of `discover` explains.
  ///
  /// `samples` holds a sample of each of the `V` variants, in the order of declarations, or
  /// anything for a variant that has no value. `fields` holds the fields of each variant, at the
  /// offsets observed on its sample.
  ///
  /// # Panics
  ///
  /// Panics, with the reason, if the experiments find no layout of a shape that `rustc` gives, or
  /// if a sample is not its own variant.
  ///
  /// # Safety
  ///
  /// The call must be evaluated at compile time, in a `const` item or block. The experiments read
  /// bytes that may be uninitialized, which is undefined behavior at run time, and an error of
  /// const evaluation at compile time.
  #[doc(hidden)]
  #[must_use]
  pub const unsafe fn discover<E, const V: usize>(
    samples: &[mem::MaybeUninit<E>; V],
    fields: &[&[Field]; V],
  ) -> Desc<V> {
    // SAFETY: The call is evaluated at compile time, by contract on the caller, which is what
    // `discover` requires.
    unsafe { crate::discover::discover(samples, fields) }
  }

  /// Returns a pointer to the sample of the variant `index` in `samples`, or a null pointer if the
  /// variant has none, for `#[derive(Pun)]`.
  #[doc(hidden)]
  #[must_use]
  pub const fn sample_of<E, const N: usize>(samples: &Samples<E, N>, index: usize) -> *const E {
    match &samples[index] {
      Some(sample) => sample.as_ptr(),
      None => core::ptr::null(),
    }
  }

  /// Returns a copy of the sample at `sample` whose byte at `at` is `byte`, or an unchanged copy if
  /// `at` is not within an `E`, for `#[derive(Pun)]`, which reads the variant of the copy to check
  /// that a byte past the tag that it read is not a byte of the tag of the enum as compiled.
  ///
  /// # Safety
  ///
  /// The caller must ensure that `sample` is not null, is aligned for `E`, and is valid for reads
  /// of `size_of::<E>()` bytes, which need not be initialized.
  #[doc(hidden)]
  #[must_use]
  pub const unsafe fn with_byte<E>(sample: *const E, at: usize, byte: u8) -> mem::MaybeUninit<E> {
    // SAFETY: `sample` is valid for reads of the bytes of an `E`, and aligned, by contract on the
    // caller, and so is the `MaybeUninit<E>` cast from it, which has the size and alignment of an
    // `E`. A `MaybeUninit<E>` may hold bytes that are no `E`, uninitialized ones included [1], so
    // the copy that `read` makes is valid. These are the requirements of `read`.
    //
    // [1] Per https://doc.rust-lang.org/1.99.0/core/mem/union.MaybeUninit.html:
    //
    //     `MaybeUninit<T>` serves to enable unsafe code to deal with uninitialized data. It is a
    //     signal to the compiler indicating that the data here might *not* be initialized
    let mut copy = unsafe { core::ptr::read(sample.cast::<mem::MaybeUninit<E>>()) };
    if at < size_of::<E>() {
      // SAFETY: `at` is less than the size of an `E`, so `add(at)` stays within `copy`, and the
      // byte there is valid for writes. A `u8` is aligned at any address. These are the
      // requirements of `add` and of `write`.
      unsafe { copy.as_mut_ptr().cast::<u8>().add(at).write(byte) };
    }
    copy
  }

  /// Panics unless `read`, the variant of a sample of the variant `index` whose byte past the tag
  /// that the derive read was changed ([`Layout::with_byte`]), is `index`, for `#[derive(Pun)]`.
  ///
  /// # Panics
  ///
  /// Panics if `read` is not `index`: the tag of the enum as compiled is wider than the one that
  /// the derive read.
  #[doc(hidden)]
  pub const fn same_variant(read: usize, index: usize) {
    assert!(
      read == index,
      "the tag of the enum is wider than the one of the representation that the derive read"
    );
  }

  /// Returns a `T` that is valid in the sense of the Reference, whose rules may not hold, made from
  /// its layout alone, for the samples of the variants of an enum of the `Rust` representation
  /// (`Layout::discover`).
  ///
  /// Every scalar holds the least of its values, and padding is uninitialized.
  ///
  /// # Panics
  ///
  /// Panics if `T` has no value, or if its sample holds a slice or a pattern, which have no size of
  /// their own. An opaque scalar holds the least of its values, as any scalar: the sample is never
  /// a target, and is used only to measure the layout.
  #[doc(hidden)]
  #[must_use]
  pub const fn sample<T: crate::Pun>() -> mem::MaybeUninit<T> {
    crate::discover::sample::<T>()
  }

  /// Returns `value` as a sample, for `Layout::discover`.
  #[doc(hidden)]
  #[must_use]
  pub const fn wrap<T>(value: T) -> mem::MaybeUninit<T> {
    mem::MaybeUninit::new(value)
  }

  /// Returns whether every layout of `fields` has a value, so that a value made of these fields
  /// exists.
  #[doc(hidden)]
  #[must_use]
  pub const fn inhabited_all(fields: &[&'static Layout]) -> bool {
    let mut i = 0;
    while i < fields.len() {
      if !fields[i].is_inhabited() {
        return false;
      }
      i += 1;
    }
    true
  }

  /// Returns whether a value of the type of each layout of `fields` may exist in a source
  /// ([`Layout::may_exist`]), so that a value made of these fields may exist.
  #[doc(hidden)]
  #[must_use]
  pub const fn may_exist_all(fields: &[&'static Layout]) -> bool {
    let mut i = 0;
    while i < fields.len() {
      if !fields[i].may_exist() {
        return false;
      }
      i += 1;
    }
    true
  }

  /// Returns whether [`Layout::sample`] makes a value of the type of each layout of `fields`, so
  /// that a value made of these fields can be made in const evaluation.
  #[doc(hidden)]
  #[must_use]
  pub const fn sampled_all(fields: &[&'static Layout]) -> bool {
    let mut i = 0;
    while i < fields.len() {
      if !crate::discover::has_sample(fields[i]) {
        return false;
      }
      i += 1;
    }
    true
  }

  /// Returns the offsets of `fields` in a `repr(C)` struct, and the size and alignment of the
  /// struct, for `#[derive(Pun)]`.
  ///
  /// Each field is given as its size and alignment. The fields follow what `start` says comes
  /// before them (the offset where it ends, and its alignment). This is how [RFC 2195] specifies
  /// the variants of an enum with fields.
  ///
  /// [RFC 2195]: https://rust-lang.github.io/rfcs/2195-really-tagged-unions.html
  ///
  /// # Panics
  ///
  /// Panics if an offset or the size overflows a `usize`.
  #[doc(hidden)]
  #[must_use]
  pub const fn repr_c<const N: usize>(
    start: (usize, usize),
    fields: [(usize, usize); N],
  ) -> ([usize; N], (usize, usize)) {
    let (mut offsets, (mut end, mut align), mut i) = ([0; N], start, 0);
    while i < N {
      offsets[i] = tail_offset(end, fields[i].1, usize::MAX);
      end = offsets[i] + fields[i].0;
      if fields[i].1 > align {
        align = fields[i].1;
      }
      i += 1;
    }
    (offsets, (tail_offset(end, align, usize::MAX), align))
  }

  /// Returns the size and the alignment of `T`, for `#[derive(Pun)]`.
  #[doc(hidden)]
  #[must_use]
  pub const fn size_align<T>() -> (usize, usize) {
    (size_of::<T>(), align_of::<T>())
  }

  /// Returns [`tail_offset`] in a struct that is not packed, for `#[derive(Pun)]`.
  ///
  /// # Panics
  ///
  /// Panics if the offset overflows a `usize`.
  #[doc(hidden)]
  #[must_use]
  pub const fn tail_offset(end: usize, align: usize) -> usize {
    tail_offset(end, align, usize::MAX)
  }

  /// Returns [`tail_offset`] in a `repr(packed(pack))` struct, for `#[derive(Pun)]`.
  ///
  /// # Panics
  ///
  /// Panics if the offset overflows a `usize`.
  #[doc(hidden)]
  #[must_use]
  pub const fn tail_offset_packed(end: usize, align: usize, pack: usize) -> usize {
    tail_offset(end, align, pack)
  }

  /// Returns the layout of the scalar type `T`, whose values `spec` describes.
  ///
  /// The size is that of `spec`, and the alignment that of `T`.
  ///
  /// # Panics
  ///
  /// Panics if `spec` does not have the size of `T`.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::layout::{Endian, Layout, Spec};
  ///
  /// const HALF: Layout =
  ///   Layout::scalar::<u16>(Spec::ranges::<u16>(Endian::NATIVE, &[(0, 0x7fff)]));
  /// assert_eq!((HALF.size(), HALF.align()), (size_of::<u16>(), align_of::<u16>()));
  /// // Some values of a `u16` are not values of `HALF`, so a check is needed.
  /// assert!(!HALF.is_free());
  /// ```
  #[must_use]
  pub const fn scalar<T>(spec: Spec) -> Self {
    assert!(spec.size == size_of::<T>(), "`Spec` size mismatch");
    Self::of_scalar(spec, align_of::<T>())
  }

  /// Returns the tag layout of an enum of the `Rust` representation, as `discover.rs` finds it.
  ///
  /// The tag has no type of its own, so its alignment is 1.
  pub(crate) const fn tag(spec: Spec) -> Self {
    Self::of_scalar(spec, 1)
  }

  /// Returns the layout of the scalar `spec`, of alignment `align`.
  const fn of_scalar(spec: Spec, align: usize) -> Self {
    Self {
      kind: Kind::Scalar(spec),
      size: spec.size,
      align,
      tail: 0,
      unsized_: false,
      leaves: spec.is_constrained() as usize,
      uniform: match spec.valid {
        // An opaque scalar is not "any bytes" as a target, whatever its values.
        _ if spec.opaque => Uniform::No,
        Valid::Uninit => Uniform::Pad,
        Valid::Any => Uniform::Any,
        Valid::In(_) => Uniform::No,
      },
      deferred: false,
      has_rules: false,
      stable: Stable::FIXED,
      id: Id::NONE,
    }
  }

  /// Returns the same scalar, which requires its value to be in one of `ranges`, as a field with
  /// `#[pun(valid(..))]` does.
  ///
  /// This is a requirement, not a guarantee, which means that a target requires the values of
  /// `ranges`, but a source is known to hold only what the type of the scalar guarantees, since
  /// safe code may write any value of the type. [`Field::restricted`] makes the field. The ranges
  /// are as [`Spec::with_ranges`] takes them, and must all be values of the scalar.
  ///
  /// # Panics
  ///
  /// Panics if `self` is not a scalar, is a scalar that accepts uninitialized bytes
  /// ([`Spec::uninit`]), or is not 1 to 16 bytes. Also panics if `ranges` are not as
  /// [`Spec::with_ranges`] requires, or hold a value that the scalar does not accept.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::layout::{Layout, Spec};
  ///
  /// const BYTE: Layout = Layout::scalar::<u8>(Spec::any::<u8>());
  /// assert!(BYTE.is_free());
  /// assert!(!BYTE.restricted(&[(1, 9)]).is_free());
  /// ```
  ///
  /// ```should_panic
  /// use puns::layout::{Endian, Layout, Spec};
  ///
  /// // Not values of the scalar: it is 0 or 1.
  /// let bit = Layout::scalar::<u8>(Spec::ranges::<u8>(Endian::NATIVE, &[(0, 1)]));
  /// let _ = bit.restricted(&[(1, 2)]);
  /// ```
  #[must_use]
  pub const fn restricted(&self, ranges: &'static [(u128, u128)]) -> Self {
    let Kind::Scalar(spec) = self.kind else { panic!("only scalars can be restricted") };
    assert!(!matches!(spec.valid, Valid::Uninit), "only initialized scalars can be restricted");
    // The restriction is not something that the type of the field enforces.
    let new = Spec { known: spec.known, ..spec.with_ranges(ranges) };
    let mut i = 0;
    while i < ranges.len() {
      assert!(
        spec.contains(ranges[i].0) && spec.contains(ranges[i].1),
        "restricted to values that are not valid for the type"
      );
      if let Valid::In(old) = spec.valid {
        assert!(
          matches!(crate::analysis::relate_ranges(ranges, old), crate::analysis::Rel::Subset),
          "restricted to values that are not valid for the type"
        );
      }
      i += 1;
    }
    Self {
      kind: Kind::Scalar(new),
      leaves: new.is_constrained() as usize,
      uniform: if new.is_constrained() { Uniform::No } else { self.uniform },
      ..*self
    }
  }

  /// Returns the layout of the unsized type `T`, whose values are the bytes that `dfa` accepts, as
  /// `str` is.
  ///
  /// `T` must be a slice of bytes, or a type with the layout of one, such as a `repr(transparent)`
  /// struct of a `[u8]`. The size of the layout is 0, the offset of its bytes, and its alignment is
  /// 1.
  ///
  /// # Panics
  ///
  /// Panics if pointers to `T` are not represented as pointers to slices, or if a value of `T` is
  /// not its bytes alone. Also panics if `dfa` has no state or more than 64, or if an edge of a
  /// state is empty, leads to no state, or does not follow the edge before it.
  ///
  /// # Safety
  ///
  /// The caller must ensure that:
  ///
  /// - `T` is a slice, `str`, or a struct whose last field is one of these, at any depth.
  /// - `probe` is `<[u8] as Pun>::PROBE`, a null `*const [u8]` whose length is 3, cast to `*const
  ///   T` with `as` casts only.
  /// - The size of a value of `T` whose tail has the length of the tail of `probe` is at most
  ///   `isize::MAX`.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::{
  ///   Pun,
  ///   layout::{Dfa, Layout},
  /// };
  ///
  /// /// Decimal digits.
  /// #[repr(transparent)]
  /// struct Digits([u8]);
  ///
  /// // SAFETY: `Digits` is a struct whose last field is a slice. The probe is that of a slice
  /// // of bytes, cast to `*const Digits` with an `as` cast, and has 3 bytes, a size that fits in
  /// // an `isize`.
  /// const DIGITS: Layout =
  ///   unsafe { Layout::pattern::<Digits>(<[u8] as Pun>::PROBE as *const Digits, Dfa::DIGITS) };
  ///
  /// // Unsized: its size is where its bytes start.
  /// assert_eq!((DIGITS.size(), DIGITS.align()), (0, 1));
  /// assert!(!DIGITS.is_free());
  /// ```
  pub const unsafe fn pattern<T: ?Sized>(probe: *const T, dfa: &'static Dfa) -> Self {
    // SAFETY: By contract on the caller, `T` is a slice, `str`, or a struct whose last field is
    // one of these, and `probe` is `<[u8] as Pun>::PROBE` cast to `*const T` with `as` casts
    // only. `<[u8] as Pun>::PROBE` is `slice_from_raw_parts(null(), PROBE_LEN)`. These are the
    // requirements of `verify`.
    unsafe { ptr::verify::<T>(probe) };
    // SAFETY: This satisfies the safety contract of `size_of_val_raw` [1]. The unsized tail of `T`
    // is a slice or a `str`, by contract on the caller. The size of the value that `probe` points
    // to fits in an `isize`, by contract on the caller.
    //
    // [1] Per https://doc.rust-lang.org/1.99.0/core/mem/fn.size_of_val_raw.html:
    //
    //     If the unsized tail of `T` is: a slice `[U]`, `str`, or a trait object `dyn Trait`, then
    //     the size of the entire value (dynamic tail length + statically sized prefix) must fit in
    //     `isize`.
    assert!(unsafe { mem::size_of_val_raw(probe) } == ptr::PROBE_LEN, "not a byte string");
    dfa.check();
    Self {
      kind: Kind::Pattern(dfa),
      size: 0,
      align: 1,
      tail: 1,
      unsized_: true,
      leaves: 1,
      uniform: Uniform::No,
      deferred: false,
      has_rules: false,
      stable: Stable::FIXED,
      id: Id::NONE,
    }
  }

  /// Returns the layout of a type with the size and alignment of `T`, whose bytes are all padding.
  ///
  /// This is the layout of `MaybeUninit<T>`. As a target, it accepts any bytes. As a source, it
  /// proves nothing except a target that accepts any bytes.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::layout::Layout;
  ///
  /// const PADDING: Layout = Layout::padding::<u32>();
  /// assert_eq!(PADDING.size(), size_of::<u32>());
  /// // Nothing to check, and nothing that is known to be initialized.
  /// assert!(PADDING.is_free() && !PADDING.is_plain());
  /// ```
  #[must_use]
  pub const fn padding<T>() -> Self {
    Self {
      kind: Kind::Struct(&[], &[]),
      size: size_of::<T>(),
      align: align_of::<T>(),
      tail: 0,
      unsized_: false,
      leaves: 0,
      uniform: Uniform::Pad,
      deferred: false,
      has_rules: false,
      stable: Stable::FIXED,
      id: Id::NONE,
    }
  }

  /// Returns whether the validation has nothing to check of a value of the layout.
  ///
  /// This is so if the layout has no scalar that accepts only some values (or none), no pattern, no
  /// enum with fields, no union, no type without values, and no rule. Any bytes that are
  /// initialized wherever the layout is not padding are then a valid value, and a check of such a
  /// value reads nothing.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::layout::{Endian, Layout, Spec};
  ///
  /// const BYTE: Layout = Layout::scalar::<u8>(Spec::any::<u8>());
  /// const BIT: Layout = Layout::scalar::<u8>(Spec::ranges::<u8>(Endian::NATIVE, &[(0, 1)]));
  ///
  /// assert!(BYTE.is_free() && Layout::array(&BYTE, 3).is_free());
  /// assert!(!BIT.is_free() && !Layout::array(&BIT, 3).is_free());
  /// ```
  #[must_use]
  pub const fn is_free(&self) -> bool {
    self.leaves == 0
  }

  /// Returns whether every byte of every value of the layout is initialized.
  ///
  /// Such a value can be read as bytes. An enum with fields is never plain, since which of its
  /// bytes are padding depends on its tag, and neither is a union, a type without values, or an
  /// unsized type.
  ///
  /// # Examples
  ///
  /// ```
  /// use core::mem::offset_of;
  ///
  /// use puns::layout::{Field, Layout, Spec};
  ///
  /// #[repr(C)]
  /// struct Padded {
  ///   byte: u8,
  ///   word: u32,
  /// }
  ///
  /// const BYTE: Layout = Layout::scalar::<u8>(Spec::any::<u8>());
  /// const WORD: Layout = Layout::scalar::<u32>(Spec::any::<u32>());
  /// const FIELDS: [Field; 2] = Layout::sort_fields([
  ///   Field::new(offset_of!(Padded, byte), &BYTE),
  ///   Field::new(offset_of!(Padded, word), &WORD),
  /// ]);
  /// // SAFETY: `Padded` is `Sized`, and `Layout::structure` requires nothing of a `Sized` type.
  /// const PADDED: Layout =
  ///   unsafe { Layout::structure::<Padded>(core::ptr::null(), &FIELDS, &[]) };
  ///
  /// assert!(WORD.is_plain());
  /// // Its bytes between the fields are padding, where `u32` is aligned to more than 1.
  /// assert_eq!(PADDED.is_plain(), align_of::<u32>() == 1);
  /// ```
  #[must_use]
  pub const fn is_plain(&self) -> bool {
    match self.kind {
      Kind::Scalar(spec) => !spec.may_be_uninit(),
      Kind::Array(elem, _) => elem.is_plain(),
      Kind::Struct(fields, _) => {
        let (mut i, mut covered) = (0, 0);
        while i < fields.len() {
          // The fields are sorted by offset, so a field that does not start where the bytes covered
          // so far end leaves a hole.
          if fields[i].offset != covered || !fields[i].layout.is_plain() {
            return false;
          }
          covered += fields[i].layout.size;
          i += 1;
        }
        !self.unsized_ && covered == self.size
      }
      Kind::Slice(_) | Kind::Pattern(_) | Kind::Enum(_) => false,
    }
  }

  /// Returns the layout of an array of `len` elements of the layout `elem`.
  ///
  /// The elements are one after another, the size of `elem` apart.
  ///
  /// # Panics
  ///
  /// Panics if `elem` is unsized, or if the size of the array overflows a `usize`.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::layout::{Layout, Spec};
  ///
  /// const WORD: Layout = Layout::scalar::<u32>(Spec::any::<u32>());
  /// const WORDS: Layout = Layout::array(&WORD, 3);
  /// assert_eq!((WORDS.size(), WORDS.align()), (size_of::<[u32; 3]>(), align_of::<u32>()));
  /// ```
  #[must_use]
  pub const fn array(elem: &'static Layout, len: usize) -> Self {
    assert!(!elem.unsized_);
    Self {
      kind: Kind::Array(elem, len),
      size: match elem.size.checked_mul(len) {
        Some(size) => size,
        None => panic!("array is too large"),
      },
      align: elem.align,
      tail: 0,
      unsized_: false,
      leaves: elem.leaves,
      uniform: elem.uniform,
      deferred: false,
      has_rules: elem.has_rules,
      // Its elements are at multiples of their size.
      stable: Stable::of_part(elem),
      id: Id::NONE,
    }
  }

  /// Returns the layout of the slice `[T]`, whose elements have the layout `elem`.
  ///
  /// The size of the layout is 0, the offset of the first element.
  ///
  /// # Panics
  ///
  /// Panics if `elem` is unsized or does not have the size of `T`, or if pointers to `[T]` are not
  /// represented as `probe` says.
  ///
  /// # Safety
  ///
  /// The caller must ensure that:
  ///
  /// - `probe` is `<[T] as Pun>::PROBE`, a null `*const [T]` whose length is 3.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::{
  ///   Pun,
  ///   layout::{Layout, Spec},
  /// };
  ///
  /// const WORD: Layout = Layout::scalar::<u16>(Spec::any::<u16>());
  /// // SAFETY: The probe is `<[u16] as Pun>::PROBE`, as `Layout::slice` requires.
  /// const WORDS: Layout = unsafe { Layout::slice::<u16>(<[u16] as Pun>::PROBE, &WORD) };
  ///
  /// // Unsized: its size is where its elements start.
  /// assert_eq!((WORDS.size(), WORDS.align()), (0, align_of::<u16>()));
  /// assert!(!WORDS.is_plain());
  /// ```
  pub const unsafe fn slice<T>(probe: *const [T], elem: &'static Layout) -> Self {
    assert!(!elem.unsized_ && elem.size == size_of::<T>());
    // SAFETY: `[T]` is a slice, and `probe` is `<[T] as Pun>::PROBE` by contract on the caller,
    // which is `slice_from_raw_parts(null(), PROBE_LEN)`. These are the requirements of `verify`.
    unsafe { ptr::verify::<[T]>(probe) };
    Self {
      kind: Kind::Slice(elem),
      size: 0,
      align: elem.align,
      tail: elem.size,
      unsized_: true,
      leaves: elem.leaves,
      uniform: elem.uniform,
      deferred: false,
      has_rules: elem.has_rules,
      stable: Stable::of_part(elem),
      id: Id::NONE,
    }
  }

  /// Returns the layout of the struct `T`, of the fields `fields` and the rules `rules`.
  ///
  /// `fields` are the fields of `T`, as [`Layout::sort_fields`] returns them, and `rules` are what
  /// a value of `T` requires beyond the validity of its fields. A rule is a requirement, never a
  /// guarantee, because safe code may write the fields of a value so that its rules do not hold.
  ///
  /// The size and alignment of `T` come from the compiler. If the last field is unsized, so is `T`,
  /// and the size of a value of `T` whose tail has up to 8 elements is compared with the size that
  /// the layout computes (the offset of the tail, plus its elements, rounded up to the alignment of
  /// `T`).
  ///
  /// # Panics
  ///
  /// Panics in any of the following cases.
  ///
  /// - A field other than the last is unsized, two fields overlap or are not sorted by offset, or a
  ///   sized field ends past the size of `T`.
  /// - A rule is not well formed, as `#[derive(Pun)]` writes it: it reads a field past the sized
  ///   fields, has too many operations or variables, nests its walks too deeply, needs too deep a
  ///   stack, or is a size or a length that reads the size of the value.
  /// - `T` is sized, has a `len` rule, and its last field is not an array.
  /// - The size of `T` is not the one that the layout computes, as for a `packed` struct that ends
  ///   with an aligned unsized field.
  /// - `T` is unsized, and pointers to `T` are not represented as `probe` says.
  ///
  /// # Safety
  ///
  /// The caller must ensure that:
  ///
  /// - If `T` is unsized, it is a struct whose last field is a slice, `str`, or a struct whose last
  ///   field is one of these, at any depth.
  /// - If `T` is unsized, `probe` is `<[E] as Pun>::PROBE`, a null `*const [E]` whose length is 3,
  ///   for the element type `E` of its tail (`u8` for `str`), cast to `*const T` with `as` casts
  ///   only. Otherwise `probe` may be any pointer, which is not read.
  /// - If `T` is unsized, the size of a value of `T` whose tail has 8 elements is at most
  ///   `isize::MAX`.
  ///
  /// # Examples
  ///
  /// ```
  /// use core::mem::offset_of;
  ///
  /// use puns::layout::{Field, Layout, Op, Rule, Spec};
  ///
  /// #[repr(C)]
  /// struct Range {
  ///   start: u8,
  ///   end: u8,
  /// }
  ///
  /// const BYTE: Layout = Layout::scalar::<u8>(Spec::any::<u8>());
  /// const FIELDS: [Field; 2] = Layout::sort_fields([
  ///   Field::new(offset_of!(Range, start), &BYTE),
  ///   Field::new(offset_of!(Range, end), &BYTE),
  /// ]);
  /// // `start < end`.
  /// const OPS: [Op; 3] = [
  ///   Op::load::<u8>(offset_of!(Range, start)),
  ///   Op::load::<u8>(offset_of!(Range, end)),
  ///   Op::Lt,
  /// ];
  /// const RULES: [Rule; 1] = [Rule::Assert(&OPS)];
  /// // SAFETY: `Range` is `Sized`, and `Layout::structure` requires nothing of a `Sized` type.
  /// const RANGE: Layout =
  ///   unsafe { Layout::structure::<Range>(core::ptr::null(), &FIELDS, &RULES) };
  ///
  /// assert_eq!(RANGE.size(), size_of::<Range>());
  /// // Any bytes are valid fields, but they may break the rule, so a check is needed.
  /// assert!(!RANGE.is_free());
  /// ```
  pub const unsafe fn structure<T: ?Sized>(
    probe: *const T,
    fields: &'static [Field],
    rules: &'static [Rule],
  ) -> Self {
    let (mut i, mut end, mut leaves) = (0, 0, (!rules.is_empty()) as usize);
    let mut has_rules = !rules.is_empty();
    let (mut all_any, mut all_pad) = (true, true);
    let mut tail: Option<&Field> = None;
    while i < fields.len() {
      let field = &fields[i];
      assert!(tail.is_none(), "only the last field of a struct can be unsized");
      leaves += field.layout.leaves;
      has_rules |= field.layout.has_rules;
      if field.layout.unsized_ {
        assert!(field.offset >= end, "fields must not overlap");
        tail = Some(field);
      } else if field.layout.size > 0 {
        assert!(field.offset >= end, "fields must be sorted and must not overlap");
        all_any &= field.offset == end && matches!(field.layout.uniform, Uniform::Any);
        all_pad &= matches!(field.layout.uniform, Uniform::Pad);
        end = field.offset + field.layout.size;
      }
      i += 1;
    }
    match tail {
      None => {
        // SAFETY: This satisfies the safety contracts of `size_of_val_raw` and `align_of_val_raw`
        // [1], which are the same. If `T` is `Sized`, both are always safe to call. Otherwise, by
        // contract on the caller, the tail of `T` is a slice or a `str`, and `probe` is a null
        // pointer to a `T` whose tail has `PROBE_LEN` (3) elements. A value of `T` whose tail has 8
        // elements fits in an `isize`, by contract on the caller, and so does one of 3 elements.
        //
        // [1] Per https://doc.rust-lang.org/1.99.0/core/mem/fn.size_of_val_raw.html:
        //
        //     If `T` is `Sized`, this function is always safe to call.
        //
        //     If the unsized tail of `T` is: a slice `[U]`, `str`, or a trait object
        //     `dyn Trait`, then the size of the entire value (dynamic tail length + statically
        //     sized prefix) must fit in `isize`.
        let (size, align) = unsafe { (mem::size_of_val_raw(probe), mem::align_of_val_raw(probe)) };
        assert!(end <= size, "field out of bounds");
        let uniform = match (all_any && end == size, all_pad) {
          _ if size == 0 => Uniform::Any,
          (true, _) => Uniform::Any,
          (_, true) => Uniform::Pad,
          _ => Uniform::No,
        };
        check_rules(rules, end);
        let this = Self {
          kind: Kind::Struct(fields, rules),
          size,
          align,
          tail: 0,
          unsized_: false,
          leaves,
          uniform,
          deferred: false,
          has_rules,
          stable: Stable::FIXED,
          id: Id::NONE,
        };
        let this = Self { stable: this.specified_stable(), ..this };
        let mut i = 0;
        while i < rules.len() {
          assert!(
            !matches!(rules[i], Rule::Len(_)) || this.tail_shape().array,
            "`len` is about a last field that is a slice, or an array"
          );
          i += 1;
        }
        this
      }
      Some(field) => {
        // SAFETY: `T` has an unsized field, so it is unsized. By contract on the caller, it is a
        // struct whose last field is a slice, `str`, or a struct whose last field is one of these,
        // and `probe` is `<[E] as Pun>::PROBE` cast to `*const T` with `as` casts only.
        // `<[E] as Pun>::PROBE` is `slice_from_raw_parts(null(), PROBE_LEN)`. These are the
        // requirements of `verify`.
        unsafe { ptr::verify::<T>(probe) };
        // SAFETY: This satisfies the safety contract of `align_of_val_raw`, which is that of
        // `size_of_val_raw` quoted above. The tail of `T` is a slice or a `str`. The tail of
        // `probe` has `PROBE_LEN` (3) elements, and a value of `T` whose tail has 8 elements fits
        // in an `isize`, by contract on the caller.
        let align = unsafe { mem::align_of_val_raw(probe) };
        check_rules(rules, end);
        let this = Self {
          kind: Kind::Struct(fields, rules),
          size: field.offset + field.layout.size,
          align,
          tail: field.layout.tail,
          unsized_: true,
          leaves,
          uniform: Uniform::No,
          deferred: false,
          has_rules,
          stable: Stable::FIXED,
          id: Id::NONE,
        };
        let this = Self { stable: this.specified_stable(), ..this };
        // Check the size model against the compiler.
        let mut len = 0;
        while len <= 8 {
          // SAFETY: `T` is a struct whose last field is a slice, `str`, or a struct whose last
          // field is one of these, and `verify` accepted `probe` above, so a `*const T` is
          // represented as a `*const [u8]`. These are the requirements of `raw_with_len`. The
          // pointer it returns has a tail of `len` elements, at most 8. A value of `T` whose tail
          // has 8 elements fits in an `isize`, by contract on the caller, and so does one with
          // fewer. This is the safety contract of `size_of_val_raw`, quoted above.
          let size = unsafe { mem::size_of_val_raw(ptr::raw_with_len(probe, len)) };
          assert!(
            matches!(this.size_of_val(len), Some(s) if s == size),
            "unsupported layout: the trailing padding of this unsized struct is not the one of \
             its unsized field (packed struct with an aligned unsized field?)",
          );
          len += 1;
        }
        this
      }
    }
  }

  /// Returns the layout of a variant of the enum `T`, of the fields `fields`.
  ///
  /// `fields` are the fields of the variant, as [`Layout::sort_fields`] returns them, with their
  /// offsets from the start of the enum. The layout is a struct with the size and alignment of `T`,
  /// in which every byte that no field covers is padding, and [`Layout::enumeration`] checks the
  /// fields against the enum.
  ///
  /// # Examples
  ///
  /// With the types of the example of [`Enum`]:
  ///
  /// ```
  /// # use core::mem::offset_of;
  /// #
  /// # use puns::layout::{Endian, Enum, Field, Layout, Spec, Variant};
  /// #
  /// # #[derive(Clone, Copy)]
  /// # #[repr(u8)]
  /// # enum Shape {
  /// #   Dot,
  /// #   Square(u16),
  /// # }
  /// #
  /// # // The `repr(C)` structs of the variants that `repr(u8)` specifies: a tag, then fields.
  /// # #[repr(C)]
  /// # struct Dot(u8);
  /// # #[repr(C)]
  /// # struct Square(u8, u16);
  /// #
  /// # // The tags are 0 and 1.
  /// # const TAG: Layout = Layout::scalar::<u8>(Spec::ranges::<u8>(Endian::NATIVE, &[(0, 1)]));
  /// # const SIDE: Layout = Layout::scalar::<u16>(Spec::any::<u16>());
  /// // The fields of each variant, from the start of the enum.
  /// const SQUARE_FIELDS: [Field; 1] =
  ///   Layout::sort_fields([Field::new(offset_of!(Square, 1), &SIDE)]);
  /// const DOT: Layout = Layout::variant::<Shape>(&[]);
  /// const SQUARE: Layout = Layout::variant::<Shape>(&SQUARE_FIELDS);
  /// # const VARIANTS: [Variant; 2] = [Variant::new(0, &DOT), Variant::new(1, &SQUARE)];
  ///
  /// // As large as the enum. The bytes that the fields do not cover are padding.
  /// assert_eq!(SQUARE.size(), size_of::<Shape>());
  /// assert!(!SQUARE.is_plain());
  /// ```
  #[must_use]
  pub const fn variant<T>(fields: &'static [Field]) -> Self {
    let (mut i, mut leaves, mut has_rules) = (0, 0, false);
    while i < fields.len() {
      leaves += fields[i].layout.leaves;
      has_rules |= fields[i].layout.has_rules;
      i += 1;
    }
    Self {
      kind: Kind::Struct(fields, &[]),
      size: size_of::<T>(),
      align: align_of::<T>(),
      tail: 0,
      unsized_: false,
      leaves,
      uniform: Uniform::No,
      deferred: false,
      has_rules,
      stable: Stable::of_parts(fields),
      id: Id::NONE,
    }
  }

  /// Returns the layout of the enum with fields `T`, whose tag and variants `layout` describes.
  ///
  /// `layout` is the enum that its representation specifies ([`Enum::new`]) or, for the `Rust`
  /// representation, the one that `rustc` laid out.
  ///
  /// As a source, a value of the enum is known by its tag, and by the bytes that every variant with
  /// values initializes. As a target, the enum is proven variant by variant where the source proves
  /// each variant it may be, and otherwise it is checked as a whole at run time, its tag first,
  /// then what the fields of its variant require.
  ///
  /// # Panics
  ///
  /// Panics in any of the following cases.
  ///
  /// - The tag is not a scalar of at most 16 bytes that is always initialized, within the enum.
  /// - `T` is not laid out as `layout` says: the size of `T` is not the one that `layout`
  ///   specifies, rounded up to the alignment of `T`, or `T` is less aligned than `layout`
  ///   specifies.
  /// - A value of the tag of a variant is not a valid value of the tag, the values of the tag of a
  ///   variant are not sorted, or two variants share a value of the tag.
  /// - A variant does not have the size of `T`, one of its fields is unsized, overlaps the one
  ///   before it, or ends past the size of `T`, or a field overlaps the tag in a variant that does
  ///   not hold it.
  /// - More than one variant holds the tag.
  ///
  /// # Examples
  ///
  /// With the types of the example of [`Enum`]:
  ///
  /// ```
  /// # use core::mem::offset_of;
  /// #
  /// # use puns::layout::{Endian, Enum, Field, Layout, Spec, Variant};
  /// #
  /// # #[derive(Clone, Copy)]
  /// # #[repr(u8)]
  /// # enum Shape {
  /// #   Dot,
  /// #   Square(u16),
  /// # }
  /// #
  /// # // The `repr(C)` structs of the variants that `repr(u8)` specifies: a tag, then fields.
  /// # #[repr(C)]
  /// # struct Dot(u8);
  /// # #[repr(C)]
  /// # struct Square(u8, u16);
  /// #
  /// # // The tags are 0 and 1.
  /// # const TAG: Layout = Layout::scalar::<u8>(Spec::ranges::<u8>(Endian::NATIVE, &[(0, 1)]));
  /// # const SIDE: Layout = Layout::scalar::<u16>(Spec::any::<u16>());
  /// # // The fields of each variant, from the start of the enum.
  /// # const SQUARE_FIELDS: [Field; 1] =
  /// #   Layout::sort_fields([Field::new(offset_of!(Square, 1), &SIDE)]);
  /// # const DOT: Layout = Layout::variant::<Shape>(&[]);
  /// # const SQUARE: Layout = Layout::variant::<Shape>(&SQUARE_FIELDS);
  /// # const VARIANTS: [Variant; 2] = [Variant::new(0, &DOT), Variant::new(1, &SQUARE)];
  /// # const SHADOWS: [(usize, usize); 2] =
  /// #   [(size_of::<Dot>(), align_of::<Dot>()), (size_of::<Square>(), align_of::<Square>())];
  /// # const SHAPE: Enum = Enum::new(&TAG, false, &SHADOWS, 0, &VARIANTS);
  /// const LAYOUT: Layout = Layout::enumeration::<Shape>(&SHAPE);
  ///
  /// assert_eq!(LAYOUT.size(), size_of::<Shape>());
  /// // Its tag is checked.
  /// assert!(!LAYOUT.is_free());
  /// ```
  #[must_use]
  pub const fn enumeration<T>(layout: &'static Enum) -> Self {
    let (size, tag) = (size_of::<T>(), &layout.tag);
    let Kind::Scalar(spec) = tag.layout.kind else { panic!("the tag of an enum is a scalar") };
    // An `align(N)` on the enum is the only thing that the shadows do not tell.
    assert!(
      align_of::<T>() >= layout.specified.1
        && size == tail_offset(layout.specified.0, align_of::<T>(), usize::MAX),
      "the enum is not laid out as its representation specifies"
    );
    assert!(spec.is_integer() && tag.offset + spec.size <= size, "tag out of bounds");
    let (mut i, mut holders) = (0, 0);
    while i < layout.variants.len() {
      let variant = &layout.variants[i];
      let tags = variant.tags();
      let mut j = 0;
      while j < tags.len() {
        let (lo, hi) = tags.get(j);
        assert!(lo <= hi && spec.contains(lo) && spec.contains(hi), "not a value of the tag");
        assert!(j == 0 || tags.get(j - 1).1 < lo, "the tags of a variant are not sorted");
        j += 1;
      }
      holders += variant.holds_tag as usize;
      let (fields, mut j, mut end) = (variant.fields(), 0, 0);
      assert!(variant.layout.size == size, "not a variant of this enum");
      while j < fields.len() {
        let field = &fields[j];
        assert!(!field.layout.unsized_, "the fields of an enum are sized");
        if field.layout.size > 0 {
          let field_end = field.offset + field.layout.size;
          assert!(field.offset >= end && field_end <= size, "fields overlap or are out of bounds");
          // Only the variant of a niche holds the tag.
          assert!(
            variant.holds_tag || field_end <= tag.offset || field.offset >= tag.offset + spec.size,
            "a field overlaps the tag"
          );
          end = field_end;
        }
        j += 1;
      }
      let mut j = 0;
      while j < i {
        assert!(!layout.variants[j].tags().meets(&tags), "two variants have the same tag");
        j += 1;
      }
      i += 1;
    }
    assert!(holders <= 1, "the tag is in the fields of two variants");
    Self {
      kind: Kind::Enum(layout),
      size,
      align: align_of::<T>(),
      tail: 0,
      unsized_: false,
      leaves: 1,
      uniform: Uniform::No,
      // What the fields of the variants require is a matter of the enum as a whole.
      deferred: false,
      has_rules: false,
      stable: Stable::of_variants(layout.variants),
      id: Id::NONE,
    }
  }

  // -----------------------------------------------------------------------------------------------
  // What every layout that Rust allows has (see "The `unstable` feature" in the crate docs)

  /// Returns the same layout, of the type whose identity is `id`.
  #[doc(hidden)]
  #[must_use]
  pub const fn identified(self, id: Id) -> Self {
    Self { id, ..self }
  }

  /// Returns the same layout, as the layout of [`Deferred<T>`](crate::Deferred) where `self` is the
  /// layout of `T`: a target of that layout takes its opaque scalars by their values.
  pub(crate) const fn deferred(self) -> Self {
    Self { deferred: true, ..self }
  }

  /// Returns whether the layout is that of a [`Deferred<T>`](crate::Deferred).
  pub(crate) const fn is_deferred(&self) -> bool {
    self.deferred
  }

  /// Returns the same layout, marked as one that Rust leaves to the compiler, except for a size of
  /// 0.
  ///
  /// This is an enum of the `Rust` representation, with or without fields, and `Option` and
  /// `Result` except where `core` documents their layout. Rust specifies only that an enum without
  /// variants, or with one variant whose fields have a size of 0, has a size of 0. Such a layout is
  /// not `fixed` ([`Stable`]), and as a source it is known to be initialized only if it has no
  /// bytes.
  #[doc(hidden)]
  #[must_use]
  pub const fn unspecified(self) -> Self {
    // An enum without variants, or of one variant whose fields have a size of 0, has none.
    Self { stable: Stable::unordered(self.size == 0, true), ..self }
  }

  /// Returns the same layout, marked as the same in every layout that Rust allows wherever the size
  /// of `like` is. This is the layout of `MaybeUninit<T>`, where `like` is the layout of `T`.
  #[doc(hidden)]
  #[must_use]
  pub const fn sized_like(self, like: &Layout) -> Self {
    Self { stable: Stable::computed(like.stable.size, true), ..self }
  }

  /// Returns the facts of a struct whose representation specifies its offsets, as `repr(C)` and
  /// `repr(transparent)` do.
  ///
  /// The offsets are computed from the sizes and alignments of the fields, so they are the same in
  /// every layout if the sizes are. A rule that reads the bytes of the struct by offset reads the
  /// same thing in every layout only if every field is `fixed` too, at any depth. Otherwise, the
  /// struct is not `fixed`, and its bytes are not known to be initialized.
  ///
  /// # Panics
  ///
  /// Panics if `self` is not a struct.
  const fn specified_stable(&self) -> Stable {
    let Kind::Struct(fields, _) = self.kind else { panic!("not a struct") };
    let stable = Stable::of_parts(fields);
    if self.rules_read_bytes() && !stable.deep {
      return Stable::unordered(stable.size, false);
    }
    stable
  }

  /// Returns whether the size of every field of the struct is a multiple of the greatest alignment
  /// of a field.
  ///
  /// The alignment of a field counts as lowered by the struct, if it is `packed(N)`. The fields
  /// with a size are then one right after another in every order, with no padding between them.
  ///
  /// # Panics
  ///
  /// Panics if `self` is not a struct.
  pub(crate) const fn tiled(&self) -> bool {
    let Kind::Struct(fields, _) = self.kind else { panic!("not a struct") };
    let (mut align, mut i) = (1, 0);
    while i < fields.len() {
      // A field counts with at most the alignment of the struct, which `packed(N)` lowers to `N`.
      let field = fields[i].layout.align;
      let field = if field < self.align { field } else { self.align };
      if field > align {
        align = field;
      }
      i += 1;
    }
    let mut i = 0;
    while i < fields.len() {
      if !fields[i].layout.size.is_multiple_of(align) {
        return false;
      }
      i += 1;
    }
    true
  }

  /// Returns the same layout, as that of a struct or tuple of the `Rust` representation packed to
  /// `pack`, or `usize::MAX` if it is not packed, for `#[derive(Pun)]`.
  ///
  /// Rust says only that each field of such a struct is at a multiple of its alignment, and that
  /// the fields do not overlap. `rustc` places the fields one after another, each at the first
  /// offset its alignment allows, in an order of its choosing, and `-Zrandomize-layout` draws this
  /// order at random. The layout records what is the same in every order:
  ///
  /// - The whole layout, if every order makes the same bytes and no rule reads the bytes by offset.
  ///   This is so if one field has a size, which is then at offset 0. It is also so if the struct
  ///   has no rule, the size of each field is a multiple of the alignment of every field
  ///   ([`Layout::tiled`]), and the fields with a size are either all bytes of any value without
  ///   rules, as in `(u8, [u8; 3])`, or all the same layout, fixed at any depth, as in `(bool,
  ///   bool)`. In both cases, the size of the struct must be the sum of the sizes of its fields,
  ///   rounded up to its alignment. A rule names fields, so `a < b` holds for different bytes in
  ///   another order.
  /// - Otherwise, the size, if every order gives the size that `rustc` chose, and whether every
  ///   byte is initialized in every order. Orders are tried for up to 6 fields that have a size or
  ///   an alignment greater than 1. With more, the size counts as the same only if the fields need
  ///   no padding in any order.
  ///
  /// # Panics
  ///
  /// Panics if `self` is not the layout of a struct, if the struct is unsized, or if the sum of the
  /// sizes of the fields overflows a `usize`.
  #[doc(hidden)]
  #[must_use]
  pub const fn rust(self, pack: usize) -> Self {
    let Kind::Struct(fields, rules) = self.kind else { panic!("not a struct") };
    assert!(!self.unsized_, "a struct of the `Rust` representation is sized");
    let parts = Stable::of_parts(fields);
    if !parts.size {
      // If the size of a field is not the same in every layout, neither is the size of the struct.
      return Self { stable: Stable::unordered(false, false), ..self };
    }
    // The fields that matter to the order are those with a size, or with an alignment greater than
    // 1.
    let (mut sized, mut sum, mut init, mut i) = (0, 0, true, 0);
    let (mut all_any, mut all_same) = (true, true);
    let mut first: Option<&Layout> = None;
    while i < fields.len() {
      let field = fields[i].layout;
      if field.size > 0 {
        sized += 1;
        sum += field.size;
        init &= field.always_init();
        // A field with rules does not count either, because its rules are checked where the field
        // is, and that depends on the order.
        all_any &= matches!(field.uniform, Uniform::Any) && !field.has_rules;
        match first {
          None => {
            all_same &= field.stable.deep;
            first = Some(field);
          }
          // Every field has the same layout, fixed at any depth, so no other build can arrange it
          // otherwise.
          Some(first) => all_same &= first.stable.deep && field.stable.deep && first.equals(field),
        }
      }
      i += 1;
    }
    let tiled = self.tiled();
    let Some(tiled_size) = round_up(sum, self.align) else { panic!("struct is too large") };
    let same_bytes = !self.rules_read_bytes()
      && match sized {
        0 => true,
        // The one field with a size is at 0 in every layout, since nothing with a size can come
        // before it.
        1 => {
          let mut i = 0;
          while fields[i].layout.size == 0 {
            i += 1;
          }
          fields[i].offset == 0 && tiled_size == self.size
        }
        // A rule names fields, and which bytes a field holds depends on the order.
        _ => rules.is_empty() && tiled && (all_any || all_same) && tiled_size == self.size,
      };
    if same_bytes {
      return Self {
        // One field with a size is at 0 in every layout. More are in an order that the compiler
        // chooses.
        stable: Stable {
          fixed: true,
          ordered: sized <= 1,
          size: true,
          deep: parts.deep,
          init: false,
        },
        ..self
      };
    }
    let (size, padded) =
      if tiled { (tiled_size == self.size, tiled_size != sum) } else { self.orders(fields, pack) };
    Self { stable: Stable::unordered(size, init && !padded), ..self }
  }

  /// Returns whether every order of `fields`, packed to `pack`, gives the struct its size, and
  /// whether one of these orders leaves padding.
  ///
  /// With more than 6 fields that have a size or an alignment greater than 1, returns `(false,
  /// true)` without trying the orders.
  const fn orders(&self, fields: &[Field], pack: usize) -> (bool, bool) {
    // (size, alignment) of the fields that matter, at most 6.
    let mut items = [(0usize, 1usize); 6];
    let (mut n, mut i) = (0, 0);
    while i < fields.len() {
      let field = fields[i].layout;
      let field_align = if field.align < pack { field.align } else { pack };
      if field.size > 0 || field_align > 1 {
        if n == items.len() {
          return (false, true);
        }
        items[n] = (field.size, field_align);
        n += 1;
      }
      i += 1;
    }
    // Every order, by Heap's algorithm.
    let mut order = [0, 1, 2, 3, 4, 5];
    let mut counters = [0usize; 6];
    let (mut same, mut padded) = self.place(&items, &order, n);
    let mut i = 1;
    while i < n {
      if counters[i] < i {
        let j = if i % 2 == 0 { 0 } else { counters[i] };
        (order[j], order[i]) = (order[i], order[j]);
        let (size, pad) = self.place(&items, &order, n);
        same &= size;
        padded |= pad;
        counters[i] += 1;
        i = 1;
      } else {
        counters[i] = 0;
        i += 1;
      }
    }
    (same, padded)
  }

  /// Returns whether the first `n` of `items`, each a size and an alignment, placed in `order`,
  /// give the struct the size of `self`, and whether they leave padding.
  ///
  /// Each field is placed at the first offset after the previous one that its alignment allows, as
  /// `rustc` does. An overflow returns `(false, true)`.
  const fn place(&self, items: &[(usize, usize); 6], order: &[usize; 6], n: usize) -> (bool, bool) {
    let (mut end, mut padded, mut i) = (0usize, false, 0);
    while i < n {
      let (size, align) = items[order[i]];
      let Some(offset) = round_up(end, align) else { return (false, true) };
      padded |= offset != end;
      end = offset + size;
      i += 1;
    }
    match round_up(end, self.align) {
      Some(size) => (size == self.size, padded || size != end),
      None => (false, true),
    }
  }

  /// Returns whether the analysis must not look into the layout. This is so when the `unstable`
  /// feature is not enabled, and the layout is not `fixed` ([`Stable`]).
  ///
  /// A layout whose size is the same in every layout that Rust allows, that has no rule, and whose
  /// bytes are all of any value or all padding, is looked into all the same, since what its bytes
  /// are does not depend on where its fields are.
  pub(crate) const fn is_closed(&self) -> bool {
    let same_bytes =
      self.stable.size && !self.has_rules && matches!(self.uniform, Uniform::Any | Uniform::Pad);
    !UNSTABLE && !self.stable.fixed && !same_bytes
  }

  /// Returns whether every byte of every value is initialized, in every layout that Rust allows.
  ///
  /// This is [`Layout::is_plain`], where a part that is not `fixed` counts for what it is in every
  /// layout ([`Stable::init`]), and fields of size 0 are not looked at.
  pub(crate) const fn always_init(&self) -> bool {
    if !self.stable.fixed {
      return self.stable.init;
    }
    match self.kind {
      Kind::Scalar(spec) => !spec.may_be_uninit(),
      Kind::Array(elem, _) => elem.always_init(),
      Kind::Struct(fields, _) => {
        let (mut i, mut covered) = (0, 0);
        while i < fields.len() {
          if fields[i].layout.size > 0 {
            if fields[i].offset != covered || !fields[i].layout.always_init() {
              return false;
            }
            covered += fields[i].layout.size;
          }
          i += 1;
        }
        !self.unsized_ && covered == self.size
      }
      Kind::Slice(_) | Kind::Pattern(_) | Kind::Enum(_) => false,
    }
  }

  /// Returns whether no part of the layout, at any depth, depends on a layout that Rust leaves to
  /// the compiler.
  ///
  /// A struct whose field orders all make the same bytes counts, as `(u8, u8)` does, though which
  /// byte holds which field depends on the compiler. What a check reads of a value of such a layout
  /// is the same in every layout that Rust allows. The answer does not depend on the `unstable`
  /// feature.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::Pun;
  ///
  /// #[derive(Pun)]
  /// #[repr(C)]
  /// struct Header {
  ///   kind: u8,
  ///   len: u16,
  /// }
  ///
  /// // `LAYOUT` is hidden, and read here only to show the answer.
  /// // `repr(C)` places the fields in order.
  /// assert!(Header::LAYOUT.is_specified());
  /// // The same bytes in either order of the fields.
  /// assert!(<(u8, u8)>::LAYOUT.is_specified());
  /// // The padding byte is before the `u16` or after it.
  /// assert!(!<(u8, u16)>::LAYOUT.is_specified());
  /// ```
  #[must_use]
  pub const fn is_specified(&self) -> bool {
    self.stable.deep
  }

  /// Returns whether the layout is an opaque scalar ([`Spec::opaque`]).
  pub(crate) const fn is_opaque(&self) -> bool {
    matches!(self.kind, Kind::Scalar(spec) if spec.is_opaque())
  }

  /// Returns whether a part of the layout, at any depth, is an opaque scalar ([`Spec::opaque`]).
  ///
  /// Nothing proves such a part as a target, so no function of `puns` makes a value of the layout,
  /// and no function gives a `&mut` to one. The crate that declares the scalar reads a value of it
  /// as a [`Deferred<T>`](crate::Deferred).
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::{Pun, layout::Layout};
  ///
  /// assert!(!<(u8, bool)>::LAYOUT.holds_opaque());
  /// ```
  #[must_use]
  pub const fn holds_opaque(&self) -> bool {
    // Such a scalar is a leaf.
    if self.leaves == 0 {
      return false;
    }
    match self.kind {
      Kind::Scalar(spec) => spec.is_opaque(),
      Kind::Pattern(_) => false,
      Kind::Array(elem, _) | Kind::Slice(elem) => elem.holds_opaque(),
      Kind::Struct(fields, _) => {
        let mut i = 0;
        while i < fields.len() {
          if fields[i].layout.holds_opaque() {
            return true;
          }
          i += 1;
        }
        false
      }
      Kind::Enum(layout) => {
        let mut i = 0;
        while i < layout.variants.len() {
          if layout.variants[i].layout.holds_opaque() {
            return true;
          }
          i += 1;
        }
        false
      }
    }
  }

  /// Returns whether a part of the layout, at any depth, requires what a value of its type may not
  /// hold: a rule, a scalar whose valid values are not those that it is known to hold, such as a
  /// field restricted by `#[pun(valid(..))]`, or an opaque scalar, which a value of its type at
  /// another place may not be.
  ///
  /// Safe code may write a value that breaks such a requirement. So the check of the layout may
  /// fail on a value of the same type, or of a type that proves it through the same requirement.
  pub(crate) const fn holds_requirement(&self) -> bool {
    // Every requirement is a leaf.
    if self.leaves == 0 {
      return false;
    }
    match self.kind {
      // An opaque scalar requires to be correct at its place, which a value of its type elsewhere
      // may not be.
      Kind::Scalar(spec) => {
        spec.is_opaque() || !same_valid(spec.valid, spec.known, spec.endian, spec.endian)
      }
      Kind::Pattern(_) => false,
      Kind::Array(elem, _) | Kind::Slice(elem) => elem.holds_requirement(),
      Kind::Struct(fields, rules) => {
        let mut i = 0;
        while i < fields.len() {
          if fields[i].layout.holds_requirement() {
            return true;
          }
          i += 1;
        }
        !rules.is_empty()
      }
      Kind::Enum(layout) => {
        if !layout.union && layout.tag.layout.holds_requirement() {
          return true;
        }
        let mut i = 0;
        while i < layout.variants.len() {
          if layout.variants[i].layout.holds_requirement() {
            return true;
          }
          i += 1;
        }
        false
      }
    }
  }

  /// Returns whether no part that is not `fixed`, at any depth, has something to check, or whether
  /// the `unstable` feature is enabled.
  ///
  /// What a check of every part of a value of the layout does, as an enum checked as a whole makes,
  /// is then the same in every layout that Rust allows.
  pub(crate) const fn checks_kept(&self) -> bool {
    if UNSTABLE {
      return true;
    }
    if self.stable.deep {
      return true;
    }
    if !self.stable.fixed {
      return self.leaves == 0;
    }
    match self.kind {
      Kind::Array(elem, _) | Kind::Slice(elem) => elem.checks_kept(),
      Kind::Struct(fields, _) => {
        let mut i = 0;
        while i < fields.len() {
          if !fields[i].layout.checks_kept() {
            return false;
          }
          i += 1;
        }
        true
      }
      Kind::Enum(layout) => {
        let mut i = 0;
        while i < layout.variants.len() {
          if !layout.variants[i].layout.checks_kept() {
            return false;
          }
          i += 1;
        }
        true
      }
      Kind::Scalar(_) | Kind::Pattern(_) => true,
    }
  }

  /// Returns the expression of the `rule`th rule of a struct, or no operation if there is no such
  /// rule or it has no expression.
  pub(crate) const fn rule_ops(&self, rule: usize) -> &'static [Op] {
    let rules = self.rules();
    if rule >= rules.len() {
      return &[];
    }
    match rules[rule].ops() {
      Some(ops) => ops,
      None => &[],
    }
  }

  /// Returns the `index`th rule of a struct, if there is one.
  pub(crate) const fn rule(&self, index: usize) -> Option<Rule> {
    let rules = self.rules();
    if index < rules.len() { Some(rules[index]) } else { None }
  }

  /// Returns the tag of the `variant`th variant of an enum, or its index if the enum is of the
  /// `Rust` representation (`Variant::discovered`), for `#[derive(Pun)]`.
  ///
  /// # Panics
  ///
  /// Panics if `self` is not an enum, or has no `variant`th variant.
  #[doc(hidden)]
  #[must_use]
  pub const fn variant_tag(&self, variant: usize) -> u128 {
    let Kind::Enum(layout) = self.kind else { panic!("not an enum") };
    layout.variants[variant].tag
  }

  /// Returns the values of the tag for the `variant`th variant of an enum, if they are not
  /// [`Layout::variant_tag`] alone, for `#[derive(Pun)]`.
  ///
  /// # Panics
  ///
  /// Panics if `self` is not an enum, or has no `variant`th variant.
  #[doc(hidden)]
  #[must_use]
  pub const fn variant_ranges(&self, variant: usize) -> Option<&'static [(u128, u128)]> {
    let Kind::Enum(layout) = self.kind else { panic!("not an enum") };
    layout.variants[variant].ranges
  }

  /// Returns the layout of the type `T`, which has no value, like `Infallible` and an enum without
  /// variants.
  ///
  /// The layout is an enum without variants. No value of a type that has values is a value of this
  /// one. As a source, a variant of an enum that holds it needs no proof where the variants of two
  /// enums are related, since no value is that variant.
  ///
  /// # Examples
  ///
  /// ```
  /// use core::convert::Infallible;
  ///
  /// use puns::layout::Layout;
  ///
  /// const NEVER: Layout = Layout::uninhabited::<Infallible>();
  /// assert!(!NEVER.is_inhabited());
  /// assert_eq!(NEVER.size(), 0);
  /// ```
  #[must_use]
  pub const fn uninhabited<T>() -> Self {
    Self {
      kind: Kind::Enum(&Enum::NEVER),
      size: size_of::<T>(),
      align: align_of::<T>(),
      tail: 0,
      unsized_: false,
      leaves: 1,
      uniform: Uniform::No,
      deferred: false,
      has_rules: false,
      stable: Stable::FIXED,
      id: Id::NONE,
    }
  }

  /// Returns whether a value of the layout can be made, valid under every rule that the language
  /// may settle for unions.
  ///
  /// This is the question for a target. As a source, a value may exist where none can be made: a
  /// union may hold any bytes under one of these rules, even if none of its fields has values.
  ///
  /// A type without values has none, and neither does an enum whose variants have none, nor a
  /// struct, or an array of at least one element, that holds a type without values. A slice has
  /// values, since it may be empty, and so does an array of no elements, whose one value is empty.
  /// So a struct that ends with a slice or a string has values if its sized fields do, and one that
  /// ends with a struct has values if that struct does. A union has values if one of its fields
  /// has.
  ///
  /// # Examples
  ///
  /// ```
  /// use core::convert::Infallible;
  ///
  /// use puns::layout::Layout;
  ///
  /// const NEVER: Layout = Layout::uninhabited::<Infallible>();
  /// assert!(!Layout::array(&NEVER, 1).is_inhabited());
  /// // The empty array.
  /// assert!(Layout::array(&NEVER, 0).is_inhabited());
  /// ```
  #[must_use]
  pub const fn is_inhabited(&self) -> bool {
    match self.kind {
      Kind::Scalar(_) | Kind::Pattern(_) | Kind::Slice(_) => true,
      Kind::Array(elem, len) => len == 0 || elem.is_inhabited(),
      Kind::Struct(fields, _) => {
        let mut i = 0;
        while i < fields.len() {
          if !fields[i].layout.is_inhabited() {
            return false;
          }
          i += 1;
        }
        true
      }
      Kind::Enum(layout) => {
        let mut i = 0;
        while i < layout.variants.len() {
          if layout.variants[i].is_present() {
            return true;
          }
          i += 1;
        }
        false
      }
    }
  }

  /// Returns whether a value of the layout may exist, under some rule that the language may settle
  /// for unions.
  ///
  /// This is the question for a source. A union may hold any bytes under one of these rules, so it
  /// may exist even if none of its fields has values, though no such union can be made
  /// ([`Layout::is_inhabited`]). Otherwise the answer is that of `is_inhabited`: a struct may exist
  /// if each of its fields may, an array if it is empty or its element may, an enum if one of its
  /// variants may, and a slice, a string and a scalar always.
  #[must_use]
  pub(crate) const fn may_exist(&self) -> bool {
    match self.kind {
      Kind::Scalar(_) | Kind::Pattern(_) | Kind::Slice(_) => true,
      Kind::Array(elem, len) => len == 0 || elem.may_exist(),
      Kind::Struct(fields, _) => {
        let mut i = 0;
        while i < fields.len() {
          if !fields[i].layout.may_exist() {
            return false;
          }
          i += 1;
        }
        true
      }
      Kind::Enum(layout) if layout.union => true,
      Kind::Enum(layout) => {
        let mut i = 0;
        while i < layout.variants.len() {
          if layout.variants[i].may_exist() {
            return true;
          }
          i += 1;
        }
        false
      }
    }
  }

  /// Returns the layout of the `repr(C)` union `T`, whose fields are the variants of `layout`, for
  /// `#[derive(Pun)]`.
  ///
  /// Only `Enum::union` makes a `layout` that this function accepts.
  ///
  /// The language has not settled the validity of unions, and this layout takes what is sound under
  /// each of the rules being considered, as [Unions](crate#unions) describes.
  ///
  /// # Panics
  ///
  /// Panics if `layout` is not the layout of a union as `#[derive(Pun)]` makes it, if the layout of
  /// a variant does not have the size of `T`, has more than one field, or holds an opaque scalar, or
  /// if a field is not at offset 0, is larger than `T`, or is unsized.
  ///
  /// # Examples
  ///
  /// What `#[derive(Pun)]` makes of a union:
  ///
  /// ```
  /// use puns::{Pun, num::u8in};
  ///
  /// #[derive(Pun, Clone, Copy)]
  /// #[repr(C)]
  /// union Choice {
  ///   flag: bool,
  ///   small: u8in<0, 3>,
  /// }
  ///
  /// // As a target: one of its fields.
  /// assert!(puns::try_pun::<u8, Choice>(1).is_ok());
  /// assert!(puns::try_pun::<u8, Choice>(3).is_ok());
  /// assert!(puns::try_pun::<u8, Choice>(4).is_err());
  /// ```
  ///
  /// As a source, nothing is known about it, not even that its byte is initialized:
  ///
  /// ```compile_fail,E0080
  /// # use puns::{Pun, num::u8in};
  /// # #[derive(Pun, Clone, Copy)]
  /// # #[repr(C)]
  /// # union Choice { flag: bool, small: u8in<0, 3> }
  /// let _: u8 = puns::pun(Choice { flag: true });
  /// ```
  #[doc(hidden)]
  #[must_use]
  pub const fn union<T>(layout: &'static Enum) -> Self {
    assert!(layout.union, "not a union");
    let (size, mut i) = (size_of::<T>(), 0);
    while i < layout.variants.len() {
      let variant = &layout.variants[i];
      let fields = variant.fields();
      assert!(variant.layout.size == size, "not a field of this union");
      assert!(fields.len() <= 1, "a field of a union is one field");
      if fields.len() == 1 {
        let field = &fields[0];
        assert!(field.offset == 0 && field.layout.size <= size, "the fields of a union are at 0");
        assert!(!field.layout.unsized_, "the fields of a union are sized");
      }
      assert!(!variant.layout.holds_opaque(), "a union holds no opaque scalar");
      i += 1;
    }
    Self {
      kind: Kind::Enum(layout),
      size,
      align: align_of::<T>(),
      tail: 0,
      unsized_: false,
      leaves: 1,
      uniform: Uniform::No,
      deferred: false,
      has_rules: false,
      stable: Stable::of_variants(layout.variants),
      id: Id::NONE,
    }
  }

  /// Returns the `index`th declared field of the `variant`th variant of an enum, for
  /// `#[derive(Pun)]`.
  ///
  /// # Panics
  ///
  /// Panics if `self` is not an enum, has no `variant`th variant, or if that variant has no
  /// `index`th declared field. A field of a variant without values of an enum of the `Rust`
  /// representation is not in the layout.
  #[doc(hidden)]
  #[must_use]
  pub const fn variant_field(&self, variant: usize, index: usize) -> &Field {
    let Kind::Enum(layout) = self.kind else { panic!("not an enum") };
    let (fields, mut i) = (layout.variants[variant].fields(), 0);
    while i < fields.len() {
      if fields[i].index == index {
        return &fields[i];
      }
      i += 1;
    }
    panic!("no such field")
  }

  /// Returns whether the bytes from `offset` are initialized, and the offset where this may stop
  /// being so.
  ///
  /// With `always`, the bytes are initialized in every value of the layout, which is what a source
  /// guarantees. Without it, they are initialized in some value, which is what a target may
  /// require. The returned end is an offset up to which the answer holds for every byte, and may be
  /// before the point where the answer changes.
  ///
  /// The function does not look into an array whose bytes are not all initialized or all padding.
  /// With `always`, its bytes are not known to be initialized, and without it, they are all taken
  /// to be required.
  ///
  /// With `checked` and without `always`, the padding of a struct whose rules read the bytes of its
  /// value counts as required to be initialized, since these rules may be checked.
  ///
  /// # Panics
  ///
  /// Panics if it must look into a slice or a pattern at `offset`, whose bytes it does not know.
  pub(crate) const fn segment(&self, offset: usize, always: bool, checked: bool) -> (bool, usize) {
    if checked && !always && self.rules_read_bytes() {
      return (true, self.size);
    }
    match self.uniform {
      Uniform::Any => return (true, self.size),
      Uniform::Pad => return (false, self.size),
      Uniform::No => {}
    }
    match self.kind {
      // With `always`, the answer is what a scalar is known to hold, and otherwise what it
      // requires.
      Kind::Scalar(spec) => {
        (!matches!(if always { spec.known } else { spec.valid }, Valid::Uninit), self.size)
      }
      Kind::Pattern(_) | Kind::Slice(_) => panic!("not sized"),
      Kind::Array(..) => (!always, self.size),
      Kind::Struct(fields, _) => {
        let mut i = 0;
        while i < fields.len() {
          let field = &fields[i];
          if offset < field.offset {
            // Padding before the field.
            return (false, field.offset);
          }
          if offset < field.offset + field.layout.size {
            let (init, end) = field.layout.segment(offset - field.offset, always, checked);
            return (init, field.offset + end);
          }
          i += 1;
        }
        (false, self.size)
      }
      // Nothing is known of what a union holds. As a target, it may need what any of its fields
      // needs.
      Kind::Enum(layout) if layout.union && always => (false, self.size),
      Kind::Enum(layout) => {
        let (start, end) = (layout.tag.offset, layout.tag.offset + layout.tag.layout.size);
        if offset >= start && offset < end {
          return (true, end);
        }
        // The same in every variant up to `stop`.
        let (mut i, mut every, mut some) = (0, true, false);
        let mut stop = if offset < start { start } else { self.size };
        while i < layout.variants.len() {
          // A variant that no value can be has nothing to tell. As a source, a variant that holds a
          // union may be one, whatever the fields of the union.
          let variant = &layout.variants[i];
          if !(if always { variant.may_exist() } else { variant.is_present() }) {
            i += 1;
            continue;
          }
          let (init, end) = variant.layout.segment(offset, always, checked);
          (every, some) = (every && init, some || init);
          if end < stop {
            stop = end;
          }
          i += 1;
        }
        (if always { every } else { some }, stop)
      }
    }
  }

  /// Returns the fields of a struct, given in declaration order, numbered and sorted by offset.
  ///
  /// Each field gets its index in declaration order, and the number of leaves of the fields
  /// declared before it. The sort is stable, so fields at the same offset, which all have a size of
  /// 0 except for one, keep their declaration order.
  ///
  /// # Examples
  ///
  /// ```
  /// use core::mem::offset_of;
  ///
  /// use puns::layout::{Field, Layout, Spec};
  ///
  /// // With the `Rust` representation, `rustc` chooses the order of the fields.
  /// struct Mixed {
  ///   narrow: u8,
  ///   wide: u32,
  /// }
  ///
  /// const NARROW: Layout = Layout::scalar::<u8>(Spec::any::<u8>());
  /// const WIDE: Layout = Layout::scalar::<u32>(Spec::any::<u32>());
  /// const FIELDS: [Field; 2] = Layout::sort_fields([
  ///   Field::new(offset_of!(Mixed, narrow), &NARROW),
  ///   Field::new(offset_of!(Mixed, wide), &WIDE),
  /// ]);
  /// // SAFETY: `Mixed` is `Sized`, and `Layout::structure` requires nothing of a `Sized` type.
  /// const MIXED: Layout =
  ///   unsafe { Layout::structure::<Mixed>(core::ptr::null(), &FIELDS, &[]) };
  ///
  /// assert_eq!(MIXED.size(), size_of::<Mixed>());
  /// ```
  #[must_use]
  pub const fn sort_fields<const N: usize>(mut fields: [Field; N]) -> [Field; N] {
    let (mut i, mut leaf) = (0, 0);
    while i < N {
      (fields[i].index, fields[i].leaf) = (i, leaf);
      leaf += fields[i].layout.leaves;
      i += 1;
    }
    // Stable insertion sort.
    let mut i = 1;
    while i < N {
      let mut j = i;
      while j > 0 && fields[j - 1].offset > fields[j].offset {
        fields.swap(j - 1, j);
        j -= 1;
      }
      i += 1;
    }
    fields
  }

  /// Returns the alignment of the type.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::layout::{Layout, Spec};
  ///
  /// const WORD: Layout = Layout::scalar::<u64>(Spec::any::<u64>());
  /// assert_eq!(WORD.align(), align_of::<u64>());
  /// assert_eq!(Layout::array(&WORD, 2).align(), align_of::<u64>());
  /// ```
  #[inline]
  #[must_use]
  pub const fn align(&self) -> usize {
    self.align
  }

  /// Returns the size of the type if it is `Sized`, and otherwise the offset of its tail.
  ///
  /// The size of an unsized value is this offset, plus the size of its tail, rounded up to the
  /// alignment of the type.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::{
  ///   Pun,
  ///   layout::{Layout, Spec},
  /// };
  ///
  /// const BYTE: Layout = Layout::scalar::<u8>(Spec::any::<u8>());
  /// assert_eq!(Layout::array(&BYTE, 5).size(), 5);
  /// // SAFETY: The probe is `<[u8] as Pun>::PROBE`, as `Layout::slice` requires.
  /// const BYTES: Layout = unsafe { Layout::slice::<u8>(<[u8] as Pun>::PROBE, &BYTE) };
  /// assert_eq!(BYTES.size(), 0);
  /// ```
  #[inline]
  #[must_use]
  pub const fn size(&self) -> usize {
    self.size
  }

  /// Returns whether the type is unsized.
  #[inline]
  pub(crate) const fn is_unsized(&self) -> bool {
    self.unsized_
  }

  /// Returns whether the size of a value of the type depends on the length of its tail.
  #[inline]
  pub(crate) const fn is_dynamic(&self) -> bool {
    self.unsized_ && self.tail > 0
  }

  /// Returns the size of an element of the tail if the type is unsized, and 0 otherwise.
  #[inline]
  pub(crate) const fn stride(&self) -> usize {
    if self.unsized_ { self.tail } else { 0 }
  }

  /// Returns the size of a value whose tail has `len` elements, trailing padding included, or
  /// `None` if it overflows a `usize`. The size of a `Sized` type does not depend on `len`.
  #[inline]
  pub(crate) const fn size_of_val(&self, len: usize) -> Option<usize> {
    if !self.unsized_ {
      return Some(self.size);
    }
    match self.tail.checked_mul(len) {
      Some(tail) => match self.size.checked_add(tail) {
        Some(extent) => round_up(extent, self.align),
        None => None,
      },
      None => None,
    }
  }

  /// Returns the `index`th declared field of a struct.
  ///
  /// # Panics
  ///
  /// Panics if `self` is not a struct, or has no `index`th declared field.
  #[doc(hidden)]
  #[must_use]
  pub const fn field(&self, index: usize) -> &Field {
    let Kind::Struct(fields, _) = self.kind else { panic!("not a struct") };
    let mut i = 0;
    while i < fields.len() {
      if fields[i].index == index {
        return &fields[i];
      }
      i += 1;
    }
    panic!("no such field")
  }

  /// Returns the offset of the `index`th declared field of a struct.
  ///
  /// # Panics
  ///
  /// Panics if `self` is not a struct, or has no `index`th declared field.
  #[doc(hidden)]
  #[must_use]
  pub const fn field_offset(&self, index: usize) -> usize {
    self.field(index).offset
  }

  /// Returns the number of leaves in the fields of a struct that are declared before its `index`th
  /// field ([`Field::leaf`]).
  ///
  /// # Panics
  ///
  /// Panics if `self` is not a struct, or has no `index`th declared field.
  #[doc(hidden)]
  #[must_use]
  pub const fn field_leaf(&self, index: usize) -> usize {
    self.field(index).leaf
  }

  /// Returns the kind of the layout, without its parts.
  #[inline]
  pub(crate) const fn shape(&self) -> Shape {
    match self.kind {
      Kind::Scalar(spec) => Shape::Scalar(spec),
      Kind::Array(_, len) => Shape::Array(len),
      Kind::Slice(_) => Shape::Slice,
      Kind::Struct(..) => Shape::Struct,
      Kind::Pattern(dfa) => Shape::Pattern(dfa),
      Kind::Enum(layout) => Shape::Enum(layout),
    }
  }

  /// Returns whether a rule of the struct reads the bytes of its value by offset, rather than its
  /// fields, with `le`, `be`, a variable-length integer, a checksum, a CRC, `matches`, or a walk
  /// that does one of these.
  ///
  /// The padding of the struct, if any, is then read when the rule is checked.
  pub(crate) const fn rules_read_bytes(&self) -> bool {
    let (rules, mut i) = (self.rules(), 0);
    while i < rules.len() {
      if let Some(ops) = rules[i].ops()
        && ops_read_bytes(ops)
      {
        return true;
      }
      i += 1;
    }
    false
  }

  /// Returns the first scalar, by offset, in the sized part of the layout that accepts only some
  /// values, such as the EtherType of a frame or the version of a header, as its offset and the
  /// scalar.
  ///
  /// Whatever bytes a value of the layout is made from, the validation checks them there. The
  /// function looks into structs and arrays, but not into enums.
  pub(crate) const fn key(&self) -> Option<(usize, Spec)> {
    match self.kind {
      Kind::Scalar(spec) if spec.is_constrained() => Some((0, spec)),
      Kind::Struct(fields, _) => {
        let (mut best, mut i): (Option<(usize, Spec)>, usize) = (None, 0);
        while i < fields.len() {
          if let Some((offset, spec)) = fields[i].layout.key() {
            let offset = fields[i].offset + offset;
            if !matches!(best, Some((first, _)) if first <= offset) {
              best = Some((offset, spec));
            }
          }
          i += 1;
        }
        best
      }
      Kind::Array(elem, len) if len > 0 => elem.key(),
      _ => None,
    }
  }

  /// Returns how many elements of this layout the validation of a slice checks at a time, without
  /// stopping between them (`validate::validate_elems`).
  ///
  /// This is as many as fit in 64 bytes, if an element is 1 to 16 bytes and checking one takes a
  /// few comparisons ([`Layout::is_cheap`]), and 1 otherwise.
  pub(crate) const fn block(&self) -> usize {
    if self.size == 0 || self.size > 16 || self.unsized_ || !self.is_cheap() {
      return 1;
    }
    64 / self.size
  }

  /// Returns whether checking a value of this layout takes a few comparisons.
  ///
  /// This is so for a scalar, for an array of at most 16 such elements, and for a struct of such
  /// fields whose rules compute on its fields alone ([`cheap_ops`]).
  const fn is_cheap(&self) -> bool {
    match self.kind {
      Kind::Scalar(_) => true,
      Kind::Array(elem, len) => len <= 16 && elem.is_cheap(),
      Kind::Struct(fields, rules) => {
        let mut i = 0;
        while i < fields.len() {
          if !fields[i].layout.is_cheap() {
            return false;
          }
          i += 1;
        }
        let mut i = 0;
        while i < rules.len() {
          if let Some(ops) = rules[i].ops()
            && !cheap_ops(ops)
          {
            return false;
          }
          i += 1;
        }
        true
      }
      Kind::Slice(_) | Kind::Pattern(_) | Kind::Enum(_) => false,
    }
  }

  /// Returns the rules of a struct, or none if the layout is not a struct.
  #[inline]
  pub(crate) const fn rules(&self) -> &'static [Rule] {
    match self.kind {
      Kind::Struct(_, rules) => rules,
      _ => &[],
    }
  }

  /// Returns the fields of a struct sorted by offset, or none if the layout is not a struct.
  pub(crate) const fn struct_fields(&self) -> &'static [Field] {
    match self.kind {
      Kind::Struct(fields, _) => fields,
      _ => &[],
    }
  }

  /// Returns whether the last rule of a struct is a region without an expression, which takes
  /// whatever remains of the tail.
  pub(crate) const fn has_last_region(&self) -> bool {
    matches!(self.rules().last(), Some(Rule::Region(None)))
  }

  /// Returns the rule that tells how long a value is from its first bytes, if there is one.
  ///
  /// This is the first `Size` or `Len` rule of the struct, or else of its last field if the struct
  /// is unsized, and so on, at the offset of the struct that has it.
  pub(crate) const fn length_rule(&self) -> Option<LengthRule> {
    let (mut layout, mut base) = (self, 0);
    loop {
      let Kind::Struct(fields, rules) = layout.kind else { return None };
      let mut i = 0;
      while i < rules.len() {
        match rules[i] {
          Rule::Size(ops) => return Some(LengthRule { base, elements: false, ops }),
          Rule::Len(ops) => return Some(LengthRule { base, elements: true, ops }),
          _ => {}
        }
        i += 1;
      }
      if fields.is_empty() || !layout.unsized_ {
        return None;
      }
      let last = &fields[fields.len() - 1];
      (layout, base) = (last.layout, base + last.offset);
    }
  }

  /// Returns the scalar of the unsigned integer that the layout is, if it is one.
  ///
  /// This is a scalar of 1 to 16 bytes, always initialized, whose value is a number
  /// ([`Spec::not_a_number`]), or a sized struct whose only field with a size is such a scalar, at
  /// offset 0 and with the size of the struct. Rules read such a field.
  #[doc(hidden)]
  #[must_use]
  pub const fn as_integer(&self) -> Option<Spec> {
    match self.kind {
      Kind::Scalar(spec) if spec.is_integer() && spec.number && spec.size >= 1 => Some(spec),
      Kind::Struct(fields, _) if !self.unsized_ => {
        let (mut inner, mut i): (Option<&Layout>, usize) = (None, 0);
        while i < fields.len() {
          let field = &fields[i];
          if field.layout.size > 0 {
            if inner.is_some() || field.offset != 0 || field.layout.size != self.size {
              return None;
            }
            inner = Some(field.layout);
          }
          i += 1;
        }
        match inner {
          Some(inner) => inner.as_integer(),
          None => None,
        }
      }
      _ => None,
    }
  }

  /// Returns the tail of a struct, as rules see it. This is its slice if it is unsized, and
  /// otherwise its last declared field, along with the array that this field ends with, if any.
  ///
  /// The tail of a layout that is not a struct, or of a struct without fields, is empty and starts
  /// at its size.
  pub(crate) const fn tail_shape(&self) -> TailShape {
    let mut shape = TailShape { start: self.size, bytes: 0, len: 0, array: false };
    let Kind::Struct(fields, _) = self.kind else { return shape };
    if self.unsized_ || fields.is_empty() {
      return shape;
    }
    // The last field, as declared.
    let (mut last, mut i) = (&fields[0], 1);
    while i < fields.len() {
      if fields[i].index > last.index {
        last = &fields[i];
      }
      i += 1;
    }
    (shape.start, shape.bytes) = (last.offset, last.layout.size);
    // The array it ends with, if any.
    let mut layout = last.layout;
    loop {
      match layout.kind {
        Kind::Array(_, len) => {
          (shape.len, shape.array) = (len, true);
          return shape;
        }
        Kind::Struct(fields, _) if !fields.is_empty() => layout = fields[fields.len() - 1].layout,
        _ => return shape,
      }
    }
  }

  /// Returns the offset of the first byte of the tail, which is the size of a `Sized` type.
  #[inline]
  pub(crate) const fn head(&self) -> usize {
    self.size
  }

  /// Returns whether bytes that are all zero are a valid value of the layout.
  ///
  /// Without the `unstable` feature, this is `false` for a layout that is not `fixed`, except for a
  /// struct, since the tag values of an enum of the `Rust` representation are not specified. The
  /// fields of a struct are zeros in every order.
  pub(crate) const fn zeroable(&self) -> bool {
    if !UNSTABLE && !self.stable.fixed && !matches!(self.kind, Kind::Struct(..)) {
      return false;
    }
    match self.kind {
      // No function makes an opaque scalar, whatever its values.
      Kind::Scalar(spec) => !spec.is_opaque() && spec.contains(0),
      Kind::Array(elem, len) => len == 0 || elem.zeroable(),
      Kind::Slice(elem) => elem.zeroable(),
      Kind::Struct(fields, _) => {
        let mut i = 0;
        while i < fields.len() {
          if !fields[i].layout.zeroable() {
            return false;
          }
          i += 1;
        }
        true
      }
      Kind::Pattern(dfa) => dfa.states[0].accept,
      // A field of a union that zeros are.
      Kind::Enum(layout) if layout.union => {
        let mut i = 0;
        while i < layout.variants.len() {
          if layout.variants[i].is_present() && layout.variants[i].layout.zeroable() {
            return true;
          }
          i += 1;
        }
        false
      }
      // The variant whose tag is 0, if any.
      Kind::Enum(layout) => {
        let mut i = 0;
        while i < layout.variants.len() {
          if layout.variants[i].tags().has(0) && layout.variants[i].is_present() {
            let (fields, mut j) = (layout.variants[i].fields(), 0);
            while j < fields.len() {
              if !fields[j].layout.zeroable() {
                return false;
              }
              j += 1;
            }
            return true;
          }
          i += 1;
        }
        false
      }
    }
  }

  /// Returns whether two structs have rules that are the same function of the bytes of each.
  ///
  /// A rule is a function of the bytes of its struct, of its size (`size_of_val(self)`, a `size`
  /// rule), of the length of its tail (a `len` rule), and of the bytes of its tail (regions). So
  /// two structs compute the same rules over the same bytes, whatever their fields, if they have at
  /// least one rule, the same rules in the same order ([`Rule::equals`]), the same sized part, and
  /// the same tail (where it starts, the size of an element, and, for a sized struct, the size of
  /// the tail and the length of the array it ends with).
  pub(crate) const fn same_rules(&self, other: &Layout) -> bool {
    let (Kind::Struct(_, mine), Kind::Struct(_, theirs)) = (self.kind, other.kind) else {
      return false;
    };
    let (tail, other_tail) = (self.tail_shape(), other.tail_shape());
    if mine.is_empty()
      || mine.len() != theirs.len()
      || self.size != other.size
      || self.tail != other.tail
      || self.unsized_ != other.unsized_
      || tail.start != other_tail.start
      || tail.bytes != other_tail.bytes
      || tail.len != other_tail.len
      || tail.array != other_tail.array
    {
      return false;
    }
    let mut i = 0;
    while i < mine.len() {
      if !mine[i].equals(&theirs[i]) {
        return false;
      }
      i += 1;
    }
    true
  }

  /// Returns whether two layouts are the same. They are the same if they require the same values,
  /// and guarantee the same values, at the same places.
  ///
  /// Identities are not compared. A scalar that may be uninitialized is equal to none. Two patterns
  /// are equal only if their automata are, state by state. An opaque scalar equals only an opaque
  /// scalar, and two layouts that are the same prove nothing of it: the analysis refuses it whatever
  /// the source.
  #[expect(
    clippy::many_single_char_names,
    reason = "`a` and `x` are parts of `self`, `b` and `y` the same parts of `other`"
  )]
  pub(crate) const fn equals(&self, other: &Layout) -> bool {
    if self.size != other.size
      || self.align != other.align
      || self.tail != other.tail
      || self.unsized_ != other.unsized_
    {
      return false;
    }
    match (self.kind, other.kind) {
      // Both what a scalar requires and what it is known to hold are compared, so a layout that
      // only requires what another guarantees (`valid(..)` on a `u8`, and a `bool`) is not the
      // same.
      //
      // A scalar that may be uninitialized is equal to none. Two such scalars can require the same
      // values and be known to hold the same values while one is padding where the other is not.
      (Kind::Scalar(a), Kind::Scalar(b)) => {
        !a.may_be_uninit()
          && a.size == b.size
          && a.opaque == b.opaque
          && same_valid(a.valid, b.valid, a.endian, b.endian)
          && same_valid(a.known, b.known, a.endian, b.endian)
      }
      (Kind::Array(a, n), Kind::Array(b, m)) => n == m && a.equals(b),
      (Kind::Slice(a), Kind::Slice(b)) => a.equals(b),
      (Kind::Struct(a, ra), Kind::Struct(b, rb)) => {
        if a.len() != b.len() || ra.len() != rb.len() {
          return false;
        }
        let mut i = 0;
        while i < a.len() {
          if a[i].offset != b[i].offset || !a[i].layout.equals(b[i].layout) {
            return false;
          }
          i += 1;
        }
        let mut i = 0;
        while i < ra.len() {
          if !ra[i].equals(&rb[i]) {
            return false;
          }
          i += 1;
        }
        true
      }
      // The same automaton, state by state: an `id` only tells two apart (it is a field that
      // safe code writes, and proves nothing).
      (Kind::Pattern(a), Kind::Pattern(b)) => a.id == b.id && a.same(b),
      (Kind::Enum(a), Kind::Enum(b)) => {
        if a.tag.offset != b.tag.offset
          || a.union != b.union
          || !a.tag.layout.equals(b.tag.layout)
          || a.variants.len() != b.variants.len()
        {
          return false;
        }
        let mut i = 0;
        while i < a.variants.len() {
          let (x, y) = (&a.variants[i], &b.variants[i]);
          if !x.tags().equals(&y.tags()) || x.holds_tag != y.holds_tag || !x.layout.equals(y.layout)
          {
            return false;
          }
          i += 1;
        }
        true
      }
      _ => false,
    }
  }
}

/// Returns whether two sets of values, of scalars of the same size, are the same.
///
/// Two sets of ranges are the same only if the scalars have the same byte order.
#[expect(
  clippy::many_single_char_names,
  reason = "`a` and `x` are of one scalar, `b` and `y` the same of the other"
)]
const fn same_valid(a: Valid, b: Valid, endian_a: Endian, endian_b: Endian) -> bool {
  match (a, b) {
    (Valid::Uninit, Valid::Uninit) | (Valid::Any, Valid::Any) => true,
    (Valid::In(x), Valid::In(y)) => {
      let mut i = 0;
      while i < x.len() && i < y.len() && x[i].0 == y[i].0 && x[i].1 == y[i].1 {
        i += 1;
      }
      endian_a.equals(endian_b) && x.len() == y.len() && i == x.len()
    }
    _ => false,
  }
}

// -------------------------------------------------------------------------------------------------
// `Dfa`

/// A deterministic finite automaton over bytes, for the values of an unsized type such as `str`.
///
/// An automaton has 1 to 64 states, the first of which is the initial one. It accepts bytes if,
/// from the initial state, each byte leads by an edge to a next state, and the state reached after
/// the last byte accepts. [`Layout::pattern`] makes the layout of a type whose values are the bytes
/// that an automaton accepts. A rule of `#[derive(Pun)]` can require bytes of a struct to be
/// accepted by one, with `matches(DFA, start, end)`, which, like any rule, proves nothing about
/// them.
///
/// # Examples
///
/// An automaton of your own, in a rule:
///
/// ```
/// use puns::{
///   Pun,
///   layout::{Dfa, DfaState},
/// };
///
/// /// Binary digits, at least one.
/// const BINARY: &Dfa = &Dfa {
///   id: u32::from_be_bytes(*b"bits"),
///   states: &[
///     // Nothing yet.
///     DfaState { accept: false, edges: &[(b'0', b'1', 1)] },
///     // A digit or more.
///     DfaState { accept: true, edges: &[(b'0', b'1', 1)] },
///   ],
/// };
///
/// #[derive(Pun, Clone, Copy)]
/// #[repr(C)]
/// #[pun(assert(matches(BINARY, 0, 4)))]
/// struct Nibble([u8; 4]);
///
/// assert!(puns::try_pun::<_, Nibble>(*b"0110").is_ok());
/// assert!(puns::try_pun::<_, Nibble>(*b"0120").is_err());
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct Dfa {
  /// A number that tells automata apart. The automata of this crate are numbered 1 to 6.
  ///
  /// Two pattern types whose automata have different `id`s are different layouts, which saves
  /// comparing their states. Nothing else is taken from the `id`. Two automata are the same only if
  /// their states and edges are, and bytes are checked as UTF-8 by `str::from_utf8` only for an
  /// automaton with the `id`, states and edges of [`Dfa::UTF8`].
  pub id: u32,
  /// The states, the first of which is the initial one. An edge leads to the index of a state in
  /// this slice.
  pub states: &'static [DfaState],
}

/// A state of a [`Dfa`]: whether bytes may end in it, and where each byte leads from it.
///
/// # Examples
///
/// ```
/// use puns::layout::Dfa;
///
/// let start = &Dfa::HEX.states[0];
/// let leads = |byte: u8| start.edges.iter().any(|edge| (edge.0..=edge.1).contains(&byte));
/// assert!(start.accept && leads(b'c') && leads(b'C') && !leads(b'g'));
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct DfaState {
  /// Whether bytes that end in this state are accepted.
  pub accept: bool,
  /// The edges, each `(first byte, last byte, next state)`, where a byte from the first to the
  /// last, inclusive, leads to the next state. The edges are sorted and disjoint. A byte that no
  /// edge covers is not accepted.
  pub edges: &'static [(u8, u8, u8)],
}

impl Dfa {
  /// UTF-8, as the Unicode Standard defines it (table 3-7, "Well-Formed UTF-8 Byte Sequences"), the
  /// values of `str`.
  pub const UTF8: &'static Dfa = &Dfa {
    id: 1,
    states: &[
      // 0: between two characters.
      DfaState {
        accept: true,
        edges: &[
          (0x00, 0x7f, 0),
          (0xc2, 0xdf, 1),
          (0xe0, 0xe0, 4),
          (0xe1, 0xec, 2),
          (0xed, 0xed, 5),
          (0xee, 0xef, 2),
          (0xf0, 0xf0, 6),
          (0xf1, 0xf3, 3),
          (0xf4, 0xf4, 7),
        ],
      },
      // 1, 2, 3: that many continuation bytes to go.
      DfaState { accept: false, edges: &[(0x80, 0xbf, 0)] },
      DfaState { accept: false, edges: &[(0x80, 0xbf, 1)] },
      DfaState { accept: false, edges: &[(0x80, 0xbf, 2)] },
      // 4: after `E0`, no overlong encoding.
      DfaState { accept: false, edges: &[(0xa0, 0xbf, 1)] },
      // 5: after `ED`, no surrogate.
      DfaState { accept: false, edges: &[(0x80, 0x9f, 1)] },
      // 6: after `F0`, no overlong encoding.
      DfaState { accept: false, edges: &[(0x90, 0xbf, 2)] },
      // 7: after `F4`, nothing above `U+10FFFF`.
      DfaState { accept: false, edges: &[(0x80, 0x8f, 2)] },
    ],
  };

  /// ASCII, the bytes below `0x80`.
  pub const ASCII: &'static Dfa =
    &Dfa { id: 2, states: &[DfaState { accept: true, edges: &[(0x00, 0x7f, 0)] }] };

  /// Decimal digits, `0` to `9`.
  pub const DIGITS: &'static Dfa =
    &Dfa { id: 3, states: &[DfaState { accept: true, edges: &[(b'0', b'9', 0)] }] };

  /// Octal digits, `0` to `7`.
  pub const OCTAL: &'static Dfa =
    &Dfa { id: 4, states: &[DfaState { accept: true, edges: &[(b'0', b'7', 0)] }] };

  /// Hexadecimal digits, of either case.
  pub const HEX: &'static Dfa = &Dfa {
    id: 5,
    states: &[DfaState {
      accept: true,
      edges: &[(b'0', b'9', 0), (b'A', b'F', 0), (b'a', b'f', 0)],
    }],
  };

  /// UTF-8 without `U+0000` (the strings of MQTT).
  pub const UTF8_NO_NUL: &'static Dfa = &Dfa {
    id: 6,
    states: &[
      DfaState {
        accept: true,
        edges: &[
          (0x01, 0x7f, 0),
          (0xc2, 0xdf, 1),
          (0xe0, 0xe0, 4),
          (0xe1, 0xec, 2),
          (0xed, 0xed, 5),
          (0xee, 0xef, 2),
          (0xf0, 0xf0, 6),
          (0xf1, 0xf3, 3),
          (0xf4, 0xf4, 7),
        ],
      },
      DfaState { accept: false, edges: &[(0x80, 0xbf, 0)] },
      DfaState { accept: false, edges: &[(0x80, 0xbf, 1)] },
      DfaState { accept: false, edges: &[(0x80, 0xbf, 2)] },
      DfaState { accept: false, edges: &[(0xa0, 0xbf, 1)] },
      DfaState { accept: false, edges: &[(0x80, 0x9f, 1)] },
      DfaState { accept: false, edges: &[(0x90, 0xbf, 2)] },
      DfaState { accept: false, edges: &[(0x80, 0x8f, 2)] },
    ],
  };

  /// Returns whether two automata have the same states and edges, which their `id`s do not prove.
  pub(crate) const fn same(&self, other: &Dfa) -> bool {
    if self.states.len() != other.states.len() {
      return false;
    }
    let mut state = 0;
    while state < self.states.len() {
      let (ours, theirs) = (&self.states[state], &other.states[state]);
      if ours.accept != theirs.accept || ours.edges.len() != theirs.edges.len() {
        return false;
      }
      let mut edge = 0;
      while edge < ours.edges.len() {
        let (first, second) = (ours.edges[edge], theirs.edges[edge]);
        if first.0 != second.0 || first.1 != second.1 || first.2 != second.2 {
          return false;
        }
        edge += 1;
      }
      state += 1;
    }
    true
  }

  /// Returns whether the automaton accepts `bytes`, by its states and edges alone.
  pub(crate) const fn accepts_bytes(&self, bytes: &[u8]) -> bool {
    let (mut state, mut i) = (0, 0);
    'bytes: while i < bytes.len() {
      let (edges, mut j) = (self.states[state].edges, 0);
      while j < edges.len() {
        if edges[j].0 <= bytes[i] && bytes[i] <= edges[j].1 {
          state = edges[j].2 as usize;
          i += 1;
          continue 'bytes;
        }
        j += 1;
      }
      return false;
    }
    self.states[state].accept
  }

  /// Asserts that the automaton is well formed.
  ///
  /// # Panics
  ///
  /// Panics if the automaton has no state or more than 64, or if an edge of a state is empty, leads
  /// to no state, or does not follow the edge before it.
  const fn check(&self) {
    assert!(!self.states.is_empty() && self.states.len() <= 64, "a DFA has 1 to 64 states");
    let mut i = 0;
    while i < self.states.len() {
      let (edges, mut j) = (self.states[i].edges, 0);
      while j < edges.len() {
        assert!(
          edges[j].0 <= edges[j].1 && (edges[j].2 as usize) < self.states.len(),
          "a DFA edge is empty or leads to no state"
        );
        assert!(j == 0 || edges[j - 1].1 < edges[j].0, "DFA edges must be sorted and disjoint");
        j += 1;
      }
      i += 1;
    }
  }

  /// Returns the states reached from the states of `set` by a byte of `lo..=hi`, and whether every
  /// such byte leads somewhere from every state of `set`.
  ///
  /// A set of states has one bit per state, the bit `1 << i` for the state `i`.
  pub(crate) const fn step(&self, set: u64, lo: u8, hi: u8) -> (u64, bool) {
    let (mut next, mut total, mut state) = (0, true, 0);
    while state < self.states.len() {
      if set & (1 << state) != 0 {
        // `need` is the first byte of `lo..=hi` that no edge covers so far.
        let (edges, mut need, mut i) = (self.states[state].edges, lo as u16, 0);
        while i < edges.len() && need <= hi as u16 {
          let (first, last, to) = edges[i];
          if last as u16 >= need && first <= hi {
            total &= first as u16 <= need;
            next |= 1 << to;
            need = last as u16 + 1;
          }
          i += 1;
        }
        total &= need > hi as u16;
      }
      state += 1;
    }
    (next, total)
  }

  /// Returns whether every state of `set` accepts.
  pub(crate) const fn accepts(&self, set: u64) -> bool {
    let mut state = 0;
    while state < self.states.len() {
      if set & (1 << state) != 0 && !self.states[state].accept {
        return false;
      }
      state += 1;
    }
    true
  }

  /// Returns the set of the states that bytes can reach from the initial one.
  pub(crate) const fn reachable(&self) -> u64 {
    let mut set = 1;
    loop {
      let next = set | self.step(set, 0, 0xff).0;
      if next == set {
        return set;
      }
      set = next;
    }
  }

  /// Returns whether the automaton is [`Dfa::UTF8`], by its `id`, its states and its edges.
  ///
  /// It is meant to be called in a `const`, where it costs nothing at run time.
  pub(crate) const fn is_utf8(&self) -> bool {
    self.id == Self::UTF8.id && self.same(Self::UTF8)
  }

  /// Returns whether the automaton accepts the `len` bytes at `ptr`, by its states and edges.
  ///
  /// # Safety
  ///
  /// The caller must ensure that:
  ///
  /// - `ptr` is valid for reads of `len` bytes, which are initialized.
  #[inline]
  pub(crate) const unsafe fn matches(&self, ptr: *const u8, len: usize) -> bool {
    // SAFETY: This satisfies the safety contract of `slice::from_raw_parts` [1]. `ptr` is valid for
    // reads of `len` initialized bytes, by contract on the caller, so it is non-null [2], and the
    // bytes are in one allocation, whose size fits in an `isize`. A `u8` is aligned at any
    // address. The slice lives only during this call, which writes no memory.
    //
    // [1] Per https://doc.rust-lang.org/1.99.0/core/slice/fn.from_raw_parts.html:
    //
    //     Behavior is undefined if any of the following conditions are violated:
    //
    //     - `data` must be non-null, valid for reads for `len * size_of::<T>()` many bytes, and
    //       it must be properly aligned. [...]
    //     - `data` must point to `len` consecutive properly initialized values of type `T`.
    //     - The memory referenced by the returned slice must not be mutated for the duration of
    //       lifetime `'a`, except inside an `UnsafeCell`.
    //     - The total size `len * size_of::<T>()` of the slice must be no larger than
    //       `isize::MAX`, and adding that size to `data` must not "wrap around" the address
    //       space.
    //
    // [2] Per https://doc.rust-lang.org/1.99.0/core/ptr/index.html#safety:
    //
    //     A null pointer is never valid for reads/writes.
    self.accepts_bytes(unsafe { core::slice::from_raw_parts(ptr, len) })
  }

  /// Returns whether the `len` bytes at `ptr` are UTF-8, using `str::from_utf8`. This is what
  /// [`Dfa::UTF8`] accepts, faster than by its states.
  ///
  /// # Safety
  ///
  /// The caller must ensure that:
  ///
  /// - `ptr` is valid for reads of `len` bytes, which are initialized.
  #[inline]
  pub(crate) const unsafe fn matches_utf8(ptr: *const u8, len: usize) -> bool {
    // SAFETY: As in `Dfa::matches`, the requirements of `slice::from_raw_parts` hold by contract
    // on the caller, and the slice lives only during this call.
    core::str::from_utf8(unsafe { core::slice::from_raw_parts(ptr, len) }).is_ok()
  }
}

// -------------------------------------------------------------------------------------------------
// `Rule`

/// An operation of the expression of a [`Rule`], in postfix order.
///
/// An expression is a sequence of operations over a stack of values and a set of variables. It
/// means what the same Rust expression would mean if every integer were a `u128`, and where every
/// value is known to fit in a register, it is computed there, with the same results. A binary
/// operation replaces the two values at the top of the stack with its result, the value pushed
/// first being its left side. A comparison gives 1 if it holds, and 0 otherwise.
///
/// Where Rust would panic, the rule that holds the expression fails: on an overflow, a difference
/// below zero, a division or remainder by zero, a shift by 128 or more, and a read outside the
/// value. What Rust does not compute, the right side of a `&&` or `||` that the left side decides
/// and the branch of an `if` that is not taken, is not computed either ([`Op::Shortcut`],
/// [`Op::Then`], [`Op::Else`]). So `kind != 7 || be(40, 2) == 0` holds for a value that is not of
/// kind 7, however short, while `be(40, 2) == 0 || kind != 7` fails for one that is too short.
/// Everything that is computed counts toward the value of the expression, so its first failure is
/// the failure of the expression, and the evaluation stops there. `<<` drops the bits it shifts
/// out, as in Rust.
///
/// An expression has at most 256 operations, needs a stack of at most 16 values, has at most 8
/// variables, and nests at most 4 walks. [`Layout::structure`] checks that the expressions of its
/// rules are well formed.
///
/// # Examples
///
/// `#[derive(Pun)]` compiles the expressions of its rules to operations:
///
/// ```
/// use puns::Pun;
///
/// #[derive(Pun, Clone, Copy)]
/// #[repr(C)]
/// // `Load` of `end`, `Load` of `start`, `Sub`, `Lit(2)`, `Ge`.
/// #[pun(assert(end - start >= 2))]
/// struct Span {
///   start: u8,
///   end: u8,
/// }
///
/// assert!(puns::try_pun::<_, Span>([1u8, 3]).is_ok());
/// assert!(puns::try_pun::<_, Span>([1u8, 2]).is_err());
/// // `end - start` would overflow: where Rust panics, the rule fails.
/// assert!(puns::try_pun::<_, Span>([3u8, 1]).is_err());
/// ```
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Op {
  /// Pushes a constant.
  Lit(u128),
  /// Pushes the unsigned integer of `size` bytes at `offset` in the struct, a field.
  Load {
    /// Where the integer is, in bytes from the start of the struct.
    offset: usize,
    /// Its size in bytes.
    size: u8,
    /// `true` if it is big-endian, `false` if little-endian.
    big: bool,
  },
  /// Pushes the extent of the value in bytes, `size_of_val(self)` in a rule. The extent is its size
  /// up to the end of its slice, without the padding that may follow the slice. A `size` or `len`
  /// rule, which defines it, does not read it.
  Size,
  /// Replaces an offset with the unsigned integer of `size` bytes at that offset in the value.
  /// Fails if these bytes are not all within the value.
  At {
    /// The size in bytes of the integer.
    size: u8,
    /// `true` if it is big-endian, `false` if little-endian.
    big: bool,
  },
  /// Replaces an offset with the variable-length integer of the form `form` at that offset in the
  /// value or, with `length`, with its length in bytes. Fails if the integer does not end within
  /// the value, does not fit in a `u128`, or is in LEB128 and longer than 19 bytes.
  Varint {
    /// The form of the integer.
    form: Varint,
    /// `true` for its length in bytes, `false` for its value.
    length: bool,
  },
  /// Replaces two offsets, a start and an end, with the Ethernet CRC-32 ([`crc32`]) of the bytes
  /// between them. Fails if the start is after the end, or the end is past the value.
  Crc32,
  /// Replaces two offsets, a start and an end, with the ones' complement sum of the big-endian
  /// 16-bit words between them ([`sum16`]), which is the IP checksum before it is complemented.
  /// Fails as [`Op::Crc32`] does.
  Sum16,
  /// Replaces two offsets, a start and an end, with this CRC of the bytes between them. Fails as
  /// [`Op::Crc32`] does.
  Crc(&'static Crc),
  /// Replaces four offsets with this CRC of the bytes between the first two, with the bytes between
  /// the last two read as zeros, for a CRC that covers the field that holds it. Fails as
  /// [`Op::Crc32`] does, for either pair.
  CrcZeros(&'static Crc),
  /// Replaces two offsets, a start and an end, with this checksum of the bytes between them. Fails
  /// as [`Op::Crc32`] does.
  Checksum(Checksum),
  /// Replaces two offsets, a start and an end, with 1 if the automaton accepts the bytes between
  /// them, and 0 otherwise. Fails as [`Op::Crc32`] does. This is a pattern that the struct requires
  /// of these bytes, which, like any rule, proves nothing about them.
  Matches(&'static Dfa),
  /// `+`. Fails if the sum overflows.
  Add,
  /// `-`. Fails if the difference is below zero.
  Sub,
  /// `*`. Fails if the product overflows.
  Mul,
  /// `/`, rounded down. Fails if the right side is zero.
  Div,
  /// `%`. Fails if the right side is zero.
  Rem,
  /// `&`, bitwise.
  And,
  /// `|`, bitwise.
  Or,
  /// `^`, bitwise.
  Xor,
  /// `<<`, which drops the bits it shifts out. Fails if the right side is 128 or more.
  Shl,
  /// `>>`. Fails if the right side is 128 or more.
  Shr,
  /// `==`.
  Eq,
  /// `!=`.
  Ne,
  /// `<`.
  Lt,
  /// `<=`.
  Le,
  /// `>`.
  Gt,
  /// `>=`.
  Ge,
  /// `&&`, after its [`Op::Shortcut`] and its right side. Replaces the left side, which did not
  /// decide, and the right side with the right side, as 1 or 0.
  Both,
  /// `||`, after its [`Op::Shortcut`] and its right side, as [`Op::Both`].
  Either,
  /// Follows the left side of a `&&` (`either: false`) or of a `||` (`either: true`). If the left
  /// side is `either`, it decides, so it becomes the value, as 1 or 0, and the `len` operations
  /// that follow, the right side and the [`Op::Both`] or [`Op::Either`], are not computed.
  Shortcut {
    /// The value of the left side that decides, `true` for a `||` and `false` for a `&&`.
    either: bool,
    /// The number of operations that follow, and that are not computed if the left side decides.
    len: u16,
  },
  /// `!` on a `bool`, which replaces 0 with 1, and any other value with 0. `#[derive(Pun)]` writes
  /// `!` on an integer, which is bitwise, as an [`Op::Xor`] with `u128::MAX`.
  Not,
  /// Follows the condition of an `if`. If the condition is false, the operations that follow, as
  /// many as the `u16` says, are not computed. They are the branch of the `if` and its
  /// [`Op::Else`]. A placeholder stands for the value of the branch, which [`Op::Select`] does not
  /// take.
  Then(u16),
  /// Follows the branch of an `if` that is taken when its condition is true. The operations that
  /// follow, as many as the `u16` says, are the `else` branch, and are not computed. A placeholder
  /// stands for its value.
  Else(u16),
  /// Follows an [`Op::Then`] and an [`Op::Else`], and their branches. Replaces a condition and two
  /// values with the first value if the condition is true, and with the second otherwise. Only that
  /// value was computed.
  Select,
  /// Takes a value off the stack, which becomes the variable with this index. As in Rust, what a
  /// `let` binds is computed whether it is used or not.
  Set(u8),
  /// Pushes the variable with this index.
  Get(u8),
  /// Replaces a value with what it becomes when it is replaced by the expression `next` for as long
  /// as the expression `cond` holds for it. In both expressions, the value is the variable `var`.
  ///
  /// A walk reaches something at an offset that depends on what comes before it: after options, or
  /// at the end of a chain of headers. A walk takes at most `8 * size + 128` steps, one for each
  /// bit of the `size` bytes that the expression may read and of a `u128`. These bytes are the
  /// value, or, for a `size` or `len` rule, which tells how large the value is, the bytes from the
  /// start of the value to the end of the input. It fails if `cond` still holds after them, or if
  /// `cond` or `next` fails.
  Walk {
    /// The variable that holds the value, in `cond` and in `next`.
    var: u8,
    /// The expression that must hold for the value for the walk to take a step.
    cond: &'static [Op],
    /// The expression of what the value becomes at each step.
    next: &'static [Op],
  },
}

/// The types of the constants of rules, the unsigned integer types, on which rules compute.
///
/// The trait is sealed, so only this crate implements it.
#[doc(hidden)]
#[diagnostic::on_unimplemented(
  message = "a constant of `#[pun(..)]` is an unsigned integer: `{Self}` is not one",
  label = "`#[pun(..)]` computes on unsigned integers",
  note = "an expression of `#[pun(..)]` means what it means in Rust, where every integer is a `u128`"
)]
pub trait Unsigned: Copy + seal::Unsigned {}

/// The seal of [`Unsigned`], which only this crate implements.
mod seal {
  /// The supertrait of [`Unsigned`](super::Unsigned), which no other crate can name.
  pub trait Unsigned {}
}

impl seal::Unsigned for u8 {}
impl seal::Unsigned for u16 {}
impl seal::Unsigned for u32 {}
impl seal::Unsigned for u64 {}
impl seal::Unsigned for u128 {}
impl seal::Unsigned for usize {}
impl Unsigned for u8 {}
impl Unsigned for u16 {}
impl Unsigned for u32 {}
impl Unsigned for u64 {}
impl Unsigned for u128 {}
impl Unsigned for usize {}

impl Op {
  /// Returns [`Op::Matches`] of `dfa`, after checking that `dfa` is well formed, for
  /// `#[derive(Pun)]`.
  ///
  /// # Panics
  ///
  /// Panics if `dfa` has no state or more than 64, or if an edge of a state is empty, leads to no
  /// state, or does not follow the edge before it.
  #[doc(hidden)]
  #[must_use]
  pub const fn matches(dfa: &'static Dfa) -> Self {
    dfa.check();
    Op::Matches(dfa)
  }

  /// The greatest value of a `usize`, as a `u128`. `as usize` keeps the bits of a value below it.
  #[doc(hidden)]
  pub const USIZE_MAX: u128 = usize::MAX as u128;

  /// Returns the value of a constant of a rule, for `#[derive(Pun)]`.
  #[doc(hidden)]
  #[must_use]
  pub const fn constant<T: Unsigned>(value: &T) -> u128 {
    // SAFETY: `Unsigned` is sealed, and implemented only for `u8`, `u16`, `u32`, `u64`, `u128` and
    // `usize`, which are at most 16 bytes. `value` is a reference, so valid for reads of
    // `size_of::<T>()` bytes. An integer is initialized and has no padding, so these bytes are
    // initialized. These are the requirements of `load`.
    unsafe { load(core::ptr::from_ref::<T>(value).cast(), size_of::<T>(), Endian::NATIVE) }
  }

  /// Returns the operation that pushes the value of the field of type `T` at `offset`.
  ///
  /// `T` must be an unsigned integer, or a sized struct whose only field with a size is one and is
  /// as large as the struct ([`Spec::not_a_number`] says what is not an integer). The operation
  /// reads it in its byte order.
  ///
  /// # Panics
  ///
  /// Panics if `T` is neither an unsigned integer nor such a struct.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::{Pun, layout::Op, num::u16be};
  ///
  /// #[derive(Pun)]
  /// #[repr(transparent)]
  /// struct Port(u16be);
  ///
  /// assert_eq!(Op::load::<u16be>(4), Op::Load { offset: 4, size: 2, big: true });
  /// // A struct that holds one integer is that integer.
  /// assert_eq!(Op::load::<Port>(4), Op::load::<u16be>(4));
  /// ```
  #[must_use]
  pub const fn load<T: crate::Pun>(offset: usize) -> Self {
    let Some(spec) = T::LAYOUT.as_integer() else {
      panic!(
        "a rule reads the unsigned integer fields of its struct, and this field is not one (a \
         signed or floating-point number, a `bool`, a `char` or an enum is not): read its bytes \
         with `le(offset, size)` or `be(offset, size)`"
      )
    };
    Op::Load { offset, size: spec.size as u8, big: !spec.endian.is_little() }
  }

  /// Returns whether two operations are the same, data included. Two [`Op::Matches`] are the same
  /// if their automata have the same states and edges.
  #[expect(
    clippy::many_single_char_names,
    reason = "`a`, `b`, `c` of one side are compared with `x`, `y`, `z` of the other"
  )]
  const fn equals(&self, other: &Op) -> bool {
    match (*self, *other) {
      (Op::Lit(a), Op::Lit(b)) => a == b,
      (Op::Load { offset: a, size: b, big: c }, Op::Load { offset: x, size: y, big: z }) => {
        a == x && b == y && c == z
      }
      (Op::At { size: a, big: b }, Op::At { size: x, big: y }) => a == x && b == y,
      (Op::Varint { form: a, length: b }, Op::Varint { form: x, length: y }) => {
        a as u8 == x as u8 && b == y
      }
      (Op::Crc(a), Op::Crc(b)) | (Op::CrcZeros(a), Op::CrcZeros(b)) => a.equals(b),
      (Op::Checksum(a), Op::Checksum(b)) => a as u8 == b as u8,
      (Op::Matches(a), Op::Matches(b)) => a.same(b),
      (Op::Set(a), Op::Set(b)) | (Op::Get(a), Op::Get(b)) => a == b,
      (Op::Shortcut { either: a, len: b }, Op::Shortcut { either: x, len: y }) => a == x && b == y,
      (Op::Then(a), Op::Then(b)) | (Op::Else(a), Op::Else(b)) => a == b,
      (Op::Walk { var: a, cond: b, next: c }, Op::Walk { var: x, cond: y, next: z }) => {
        a == x && same_ops(b, y) && same_ops(c, z)
      }
      (
        Op::Lit(_)
        | Op::Load { .. }
        | Op::At { .. }
        | Op::Varint { .. }
        | Op::Crc(_)
        | Op::CrcZeros(_)
        | Op::Checksum(_)
        | Op::Matches(_)
        | Op::Set(_)
        | Op::Get(_)
        | Op::Shortcut { .. }
        | Op::Then(_)
        | Op::Else(_)
        | Op::Walk { .. },
        _,
      )
      | (
        _,
        Op::Lit(_)
        | Op::Load { .. }
        | Op::At { .. }
        | Op::Varint { .. }
        | Op::Crc(_)
        | Op::CrcZeros(_)
        | Op::Checksum(_)
        | Op::Matches(_)
        | Op::Set(_)
        | Op::Get(_)
        | Op::Shortcut { .. }
        | Op::Then(_)
        | Op::Else(_)
        | Op::Walk { .. },
      ) => false,
      // Other operations hold no data.
      (a, b) => a.code() == b.code(),
    }
  }

  /// Returns a number that tells the kind of the operation apart from the others, whatever its
  /// data.
  const fn code(self) -> u8 {
    // A number for each variant, by an exhaustive `match`, so that a new variant needs one.
    match self {
      Op::Lit(_) => 0,
      Op::Load { .. } => 1,
      Op::Add => 2,
      Op::Sub => 3,
      Op::Mul => 4,
      Op::Div => 5,
      Op::Rem => 6,
      Op::And => 7,
      Op::Or => 8,
      Op::Xor => 9,
      Op::Shl => 10,
      Op::Shr => 11,
      Op::Eq => 12,
      Op::Ne => 13,
      Op::Lt => 14,
      Op::Le => 15,
      Op::Gt => 16,
      Op::Ge => 17,
      Op::Both => 18,
      Op::Either => 19,
      Op::Not => 20,
      Op::Size => 21,
      Op::At { .. } => 22,
      Op::Crc32 => 23,
      Op::Sum16 => 24,
      Op::Select => 25,
      Op::Set(_) => 26,
      Op::Get(_) => 27,
      Op::Walk { .. } => 28,
      Op::Shortcut { .. } => 29,
      Op::Varint { .. } => 30,
      Op::Crc(_) => 31,
      Op::CrcZeros(_) => 32,
      Op::Checksum(_) => 33,
      Op::Matches(_) => 34,
      Op::Then(_) => 35,
      Op::Else(_) => 36,
    }
  }
}

/// A CRC of 1 to 64 bits, given by its parameters in the [Catalogue of parametrised CRC
/// algorithms][catalogue].
///
/// The parameters are the `width` in bits, the polynomial `poly` without its high bit, the value
/// `init` that the register starts with, whether the bits are `reflect`ed, and `xorout`, with which
/// the result is combined by XOR. With reflected bits, each byte is taken least significant bit
/// first, and the result is read the same way. `crc::<WIDTH, POLY, INIT, REFLECT, XOROUT>(start,
/// end)` in a rule of `#[derive(Pun)]` is this CRC of the bytes from `start` to `end`.
///
/// A `Crc` holds 32 KiB of tables, or 2 KiB on a 16-bit target, made in const evaluation, so it is
/// meant to be a constant.
///
/// [catalogue]: https://reveng.sourceforge.io/crc-catalogue/all.htm
///
/// # Examples
///
/// ```
/// use puns::layout::Crc;
///
/// // CRC-32C (Castagnoli, iSCSI, SCTP), CRC-16/MODBUS, CRC-16/XMODEM, CRC-15/CAN.
/// static CASTAGNOLI: Crc = Crc::new(32, 0x1edc_6f41, 0xffff_ffff, true, 0xffff_ffff);
/// assert_eq!(CASTAGNOLI.check(b"123456789"), 0xe306_9283);
/// assert_eq!(Crc::new(16, 0x8005, 0xffff, true, 0).check(b"123456789"), 0x4b37);
/// assert_eq!(Crc::new(16, 0x1021, 0, false, 0).check(b"123456789"), 0x31c3);
/// assert_eq!(Crc::new(15, 0x4599, 0, false, 0).check(b"123456789"), 0x059e);
/// ```
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Crc {
  width: u32,
  poly: u64,
  init: u64,
  reflect: bool,
  xorout: u64,
  /// `tables[k][b]` is the register after the byte `b` followed by `k` zero bytes, starting from an
  /// all-zero register, in its low bits if reflected, and in its high bits otherwise. Sixteen bytes
  /// then take sixteen lookups that do not wait for each other (slicing by 16), instead of a chain
  /// of sixteen.
  tables: Tables,
}

/// The tables of a [`Crc`], [`CRC_TABLES`] of them, of `u32`s for a CRC of at most 32 bits, and of
/// `u64`s otherwise.
///
/// Tables of `u32`s take 16 KiB, half of what tables of `u64`s take in the cache.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[expect(
  clippy::large_enum_variant,
  reason = "a `Crc` is a constant of 32 KiB, as it was with tables of `u64`s only: nothing to box"
)]
enum Tables {
  /// The register in the low bits of a `u32` if reflected, and in its high bits otherwise.
  Narrow([[u32; 256]; CRC_TABLES]),
  /// The register in the low bits of a `u64` if reflected, and in its high bits otherwise.
  Wide([[u64; 256]; CRC_TABLES]),
}

/// The number of tables of a [`Crc`], 16, for sixteen bytes at a time, but 1, for one byte at a
/// time, on a 16-bit target.
///
/// Sixteen tables of `u64`s take 32 KiB, while a value on a 16-bit target is at most `isize::MAX`
/// bytes, 32 KiB minus 1. One table takes 2 KiB, and the CRC is the same either way.
pub(crate) const CRC_TABLES: usize = if cfg!(target_pointer_width = "16") { 1 } else { 16 };

// The tables of a `Crc` are a value, which is at most `isize::MAX` bytes.
const _: () = assert!(size_of::<[[u64; 256]; CRC_TABLES]>() <= isize::MAX as usize);

impl Crc {
  /// The CRC-32 of Ethernet, zlib and PNG, which [`crc32`] computes, and which is `crc32(start,
  /// end)` in a rule.
  pub const CRC32: Self = Self::new(32, 0x04c1_1db7, 0xffff_ffff, true, 0xffff_ffff);

  /// Returns the CRC of these parameters ([`Crc`]).
  ///
  /// The tables of a `Crc` take 32 KiB, of which a CRC of up to 32 bits uses 16 KiB, or 2 KiB on a
  /// 16-bit target. A `Crc` is meant to be a constant, made in const evaluation.
  ///
  /// # Panics
  ///
  /// Panics if `width` is not 1 to 64, or if `poly`, `init` or `xorout` has more than `width` bits.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::layout::Crc;
  ///
  /// // CRC-16/MODBUS, of its check value.
  /// const MODBUS: Crc = Crc::new(16, 0x8005, 0xffff, true, 0);
  /// assert_eq!(MODBUS.check(b"123456789"), 0x4b37);
  /// ```
  #[must_use]
  #[expect(clippy::large_stack_arrays, reason = "made in const evaluation, as a constant")]
  pub const fn new(width: u32, poly: u64, init: u64, reflect: bool, xorout: u64) -> Self {
    assert!(width >= 1 && width <= 64, "a CRC has 1 to 64 bits");
    let mask = u64::MAX >> (64 - width);
    assert!(
      poly <= mask && init <= mask && xorout <= mask,
      "a parameter has more bits than the CRC"
    );
    let (mut tables, mut i) = ([[0; 256]; CRC_TABLES], 0);
    while i < 256 {
      let mut register = if reflect { i as u64 } else { (i as u64) << 56 };
      let mut bit = 0;
      while bit < 8 {
        register = match (reflect, register & 1, register >> 63) {
          (true, 1, _) => (register >> 1) ^ (poly.reverse_bits() >> (64 - width)),
          (true, ..) => register >> 1,
          (false, _, 1) => (register << 1) ^ (poly << (64 - width)),
          (false, ..) => register << 1,
        };
        bit += 1;
      }
      tables[0][i] = register;
      i += 1;
    }
    // A zero byte more.
    let mut k = 1;
    while k < CRC_TABLES {
      let mut i = 0;
      while i < 256 {
        let register = tables[k - 1][i];
        tables[k][i] = if reflect {
          tables[0][(register & 0xff) as usize] ^ (register >> 8)
        } else {
          tables[0][(register >> 56) as usize] ^ (register << 8)
        };
        i += 1;
      }
      k += 1;
    }
    let tables = if width > 32 {
      Tables::Wide(tables)
    } else {
      // The register of the `u64`s, in a `u32`: its other bits are zeros.
      let (mut narrow, mut k) = ([[0; 256]; CRC_TABLES], 0);
      while k < CRC_TABLES {
        let mut i = 0;
        while i < 256 {
          narrow[k][i] = if reflect { tables[k][i] as u32 } else { (tables[k][i] >> 32) as u32 };
          i += 1;
        }
        k += 1;
      }
      Tables::Narrow(narrow)
    };
    Self { width, poly, init, reflect, xorout, tables }
  }

  /// Returns the CRC of `bytes`.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::layout::Crc;
  ///
  /// let bytes = b"The quick brown fox jumps over the lazy dog";
  /// assert_eq!(Crc::CRC32.check(bytes), u64::from(puns::crc32(bytes)));
  /// assert_eq!(Crc::CRC32.check(&[]), 0);
  /// ```
  #[must_use]
  pub const fn check(&self, bytes: &[u8]) -> u64 {
    self.compute(bytes, (0, 0))
  }

  /// Returns the CRC of `bytes`, with the bytes from `zeros.0` to `zeros.1` read as zeros.
  ///
  /// The range of zeros is clipped to the bytes, and is empty if `zeros.0` is not less than
  /// `zeros.1`.
  #[inline]
  pub(crate) const fn compute(&self, bytes: &[u8], zeros: (usize, usize)) -> u64 {
    let mut register = if self.reflect {
      self.init.reverse_bits() >> (64 - self.width)
    } else {
      self.init << (64 - self.width)
    };
    let to = if zeros.1 < bytes.len() { zeros.1 } else { bytes.len() };
    let from = if zeros.0 < to { zeros.0 } else { to };
    let (before, rest) = bytes.split_at(from);
    let (zeros, after) = rest.split_at(to - from);
    register = self.update(register, before);
    // As many zeros, a block at a time.
    let mut zeros = zeros.len();
    while zeros > 0 {
      let count = if zeros < 64 { zeros } else { 64 };
      register = self.update(register, [0; 64].split_at(count).0);
      zeros -= count;
    }
    register = self.update(register, after);
    if !self.reflect {
      register >>= 64 - self.width;
    }
    register ^ self.xorout
  }

  /// Returns the register after `bytes`, from `register`.
  #[inline]
  const fn update(&self, register: u64, bytes: &[u8]) -> u64 {
    match &self.tables {
      // In a `u32`: the low bits of the register if reflected, else its high bits.
      Tables::Narrow(t) => {
        let narrow = if self.reflect { register as u32 } else { (register >> 32) as u32 };
        let narrow = Self::update_narrow(t, self.reflect, narrow, bytes);
        if self.reflect { narrow as u64 } else { (narrow as u64) << 32 }
      }
      Tables::Wide(t) => Self::update_wide(t, self.reflect, register, bytes),
    }
  }

  /// Returns [`Crc::update`] with tables of `u32`s, which hold the register in the low bits if
  /// `reflect`, and in the high bits otherwise, as [`crc32`] does.
  #[inline]
  const fn update_narrow(
    t: &[[u32; 256]; CRC_TABLES],
    reflect: bool,
    mut register: u32,
    bytes: &[u8],
  ) -> u32 {
    // With one table, every byte goes through the loop below.
    let (blocks, rest): (&[[u8; 16]], &[u8]) =
      if CRC_TABLES == 16 { bytes.as_chunks::<16>() } else { (&[], bytes) };
    // A slice: with one table, the indices of the sixteen are never reached (no block), which
    // the compiler cannot tell of an array.
    #[cfg(target_pointer_width = "16")]
    let t: &[[u32; 256]] = t;
    let mut i = 0;
    while i < blocks.len() {
      let b = &blocks[i];
      // The register is the first 4 bytes of a block: what it holds after them, and after the 12
      // that follow, is a lookup for each byte.
      let head = [b[0], b[1], b[2], b[3]];
      let x = register ^ if reflect { u32::from_le_bytes(head) } else { u32::from_be_bytes(head) };
      let x = if reflect { x.to_le_bytes() } else { x.to_be_bytes() };
      register = t[15][x[0] as usize]
        ^ t[14][x[1] as usize]
        ^ t[13][x[2] as usize]
        ^ t[12][x[3] as usize]
        ^ t[11][b[4] as usize]
        ^ t[10][b[5] as usize]
        ^ t[9][b[6] as usize]
        ^ t[8][b[7] as usize]
        ^ t[7][b[8] as usize]
        ^ t[6][b[9] as usize]
        ^ t[5][b[10] as usize]
        ^ t[4][b[11] as usize]
        ^ t[3][b[12] as usize]
        ^ t[2][b[13] as usize]
        ^ t[1][b[14] as usize]
        ^ t[0][b[15] as usize];
      i += 1;
    }
    let mut i = 0;
    while i < rest.len() {
      register = if reflect {
        t[0][((register ^ rest[i] as u32) & 0xff) as usize] ^ (register >> 8)
      } else {
        t[0][((register >> 24) ^ rest[i] as u32) as usize] ^ (register << 8)
      };
      i += 1;
    }
    register
  }

  /// Returns [`Crc::update`] with tables of `u64`s.
  #[inline]
  const fn update_wide(
    t: &[[u64; 256]; CRC_TABLES],
    reflect: bool,
    mut register: u64,
    bytes: &[u8],
  ) -> u64 {
    // With one table, every byte goes through the loop below.
    let (blocks, rest): (&[[u8; 16]], &[u8]) =
      if CRC_TABLES == 16 { bytes.as_chunks::<16>() } else { (&[], bytes) };
    // As above.
    #[cfg(target_pointer_width = "16")]
    let t: &[[u64; 256]] = t;
    let mut i = 0;
    while i < blocks.len() {
      let b = &blocks[i];
      // The register is the first 8 bytes of a block, as many as a `u64` holds (its bits that
      // are not those of the CRC are zeros): what it holds after them, and after the 8 bytes
      // that follow, is a lookup for each byte.
      let head = [b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]];
      let x = register ^ if reflect { u64::from_le_bytes(head) } else { u64::from_be_bytes(head) };
      let x = if reflect { x.to_le_bytes() } else { x.to_be_bytes() };
      register = t[15][x[0] as usize]
        ^ t[14][x[1] as usize]
        ^ t[13][x[2] as usize]
        ^ t[12][x[3] as usize]
        ^ t[11][x[4] as usize]
        ^ t[10][x[5] as usize]
        ^ t[9][x[6] as usize]
        ^ t[8][x[7] as usize]
        ^ t[7][b[8] as usize]
        ^ t[6][b[9] as usize]
        ^ t[5][b[10] as usize]
        ^ t[4][b[11] as usize]
        ^ t[3][b[12] as usize]
        ^ t[2][b[13] as usize]
        ^ t[1][b[14] as usize]
        ^ t[0][b[15] as usize];
      i += 1;
    }
    let mut i = 0;
    while i < rest.len() {
      register = if reflect {
        t[0][((register ^ rest[i] as u64) & 0xff) as usize] ^ (register >> 8)
      } else {
        t[0][((register >> 56) ^ rest[i] as u64) as usize] ^ (register << 8)
      };
      i += 1;
    }
    register
  }

  /// Returns the CRC of `bytes` using only the first table, one byte at a time, as a target with
  /// one table computes it, for the tests.
  #[cfg(test)]
  pub(crate) fn bytewise(&self, bytes: &[u8]) -> u64 {
    let mut register = if self.reflect {
      self.init.reverse_bits() >> (64 - self.width)
    } else {
      self.init << (64 - self.width)
    };
    // The first table, as the register of a `u64`.
    let t = |i: u64| match &self.tables {
      Tables::Narrow(t) if self.reflect => u64::from(t[0][i as usize]),
      Tables::Narrow(t) => u64::from(t[0][i as usize]) << 32,
      Tables::Wide(t) => t[0][i as usize],
    };
    for &byte in bytes {
      register = if self.reflect {
        t((register ^ u64::from(byte)) & 0xff) ^ (register >> 8)
      } else {
        t((register >> 56) ^ u64::from(byte)) ^ (register << 8)
      };
    }
    if !self.reflect {
      register >>= 64 - self.width;
    }
    register ^ self.xorout
  }

  /// Returns whether two CRCs have the same parameters.
  const fn equals(&self, other: &Crc) -> bool {
    self.width == other.width
      && self.poly == other.poly
      && self.init == other.init
      && self.reflect == other.reflect
      && self.xorout == other.xorout
  }
}

/// A checksum of bytes, which [`Op::Checksum`] computes.
///
/// It is `sum(start, end)`, `fletcher16(start, end)` or `adler32(start, end)` in a rule of
/// `#[derive(Pun)]`.
///
/// # Examples
///
/// ```
/// use puns::Pun;
///
/// /// Four bytes and their sum, modulo 256.
/// #[derive(Pun, Clone, Copy)]
/// #[repr(C)]
/// #[pun(assert(sum(0, 4) % 256 == check))]
/// struct Summed {
///   data: [u8; 4],
///   check: u8,
/// }
///
/// /// The Adler-32 of "Wikipedia".
/// #[derive(Pun, Clone, Copy)]
/// #[repr(C)]
/// #[pun(assert(adler32(0, 9) == 0x11e6_0398))]
/// struct Wikipedia([u8; 9]);
///
/// assert!(puns::try_pun::<_, Summed>([1u8, 2, 3, 250, 0]).is_ok());
/// assert!(puns::try_pun::<_, Summed>([1u8, 2, 3, 250, 1]).is_err());
/// assert!(puns::try_pun::<_, Wikipedia>(*b"Wikipedia").is_ok());
/// assert!(puns::try_pun::<_, Wikipedia>(*b"wikipedia").is_err());
/// ```
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Checksum {
  /// The sum of the bytes, not reduced, as the tar header holds it. `% 256` makes it a one-byte
  /// sum.
  Sum,
  /// Fletcher-16, two sums modulo 255, the first of the bytes and the second of the successive
  /// values of the first, as `second << 8 | first`. It is 0 over data that holds its own ISO 8473
  /// checksum, as in OSPF and IS-IS.
  Fletcher16,
  /// Adler-32, of zlib, two sums modulo 65521, the first starting at 1, as `second << 16 | first`.
  Adler32,
}

impl Checksum {
  /// Returns the checksum of `bytes`.
  const fn compute(self, bytes: &[u8]) -> u128 {
    let (modulo, mut first, mut second, mut i) = match self {
      Checksum::Sum => (0, 0u128, 0u128, 0),
      Checksum::Fletcher16 => (255, 0, 0, 0),
      Checksum::Adler32 => (65521, 1, 0, 0),
    };
    while i < bytes.len() {
      first += bytes[i] as u128;
      if modulo > 0 {
        first %= modulo;
        second = (second + first) % modulo;
      }
      i += 1;
    }
    match self {
      Checksum::Sum => first,
      Checksum::Fletcher16 => second << 8 | first,
      Checksum::Adler32 => second << 16 | first,
    }
  }
}

/// A form of variable-length integer, which [`Op::Varint`] reads.
///
/// It is `leb128(offset)`, `quic(offset)` or `vlq(offset)` in a rule of `#[derive(Pun)]`, and
/// `leb128_len(offset)`, `quic_len(offset)` or `vlq_len(offset)` for its length in bytes. A longer
/// encoding than needed is read as it is, except that LEB128 is read up to 19 bytes, the most that
/// a `u128` needs, so a longer one fails even if its value fits.
///
/// # Examples
///
/// ```
/// use puns::Pun;
///
/// /// A kind, then the length of what follows in LEB128, then that.
/// #[derive(Pun)]
/// #[repr(C)]
/// #[pun(size = 1 + leb128_len(1) + leb128(1))]
/// struct Packet {
///   kind: u8,
///   rest: [u8],
/// }
///
/// /// A kind, then the length of what follows as QUIC writes it, then that.
/// #[derive(Pun)]
/// #[repr(C)]
/// #[pun(size = 1 + quic_len(1) + quic(1))]
/// struct Frame {
///   kind: u8,
///   rest: [u8],
/// }
///
/// let (packet, after) = puns::from_head::<Packet>(&[0x30, 2, 0xaa, 0xbb, 0xcc]).unwrap();
/// assert_eq!((packet.rest.len(), after), (3, &[0xcc][..]));
/// // Two bytes (`0b01` in the high bits of the first), whose value is 2.
/// let (frame, after) = puns::from_head::<Frame>(&[7, 0x40, 2, 0xaa, 0xbb, 0xcc]).unwrap();
/// assert_eq!((frame.rest.len(), after), (4, &[0xcc][..]));
/// ```
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Varint {
  /// Unsigned LEB128, of Protocol Buffers, WebAssembly, MQTT and DWARF, which has 7 bits per byte,
  /// least significant first, with the high bit set on every byte but the last.
  Leb128,
  /// The variable-length integer of QUIC (RFC 9000), in which the two high bits of the first byte
  /// give its length, 1, 2, 4 or 8 bytes, and the other bits are the value, big-endian.
  Quic,
  /// 7 bits per byte, most significant first, with the high bit set on every byte but the last, as
  /// ASN.1 writes tag numbers and the arcs of object identifiers, and as MIDI does.
  Vlq,
}

impl Varint {
  /// Returns the value of the integer at `ptr`, of at most `size` bytes, and its length in bytes.
  ///
  /// # Errors
  ///
  /// Returns `None` if the integer does not end within `size` bytes, does not fit in a `u128`, or
  /// is in LEB128 and longer than 19 bytes.
  ///
  /// # Safety
  ///
  /// The caller must ensure that:
  ///
  /// - `ptr` is valid for reads of `size` bytes, which are initialized.
  #[inline]
  const unsafe fn read(self, ptr: *const u8, size: usize) -> Option<(u128, usize)> {
    // SAFETY: As in `Dfa::matches`: `ptr` is valid for reads of `size` initialized bytes, by
    // contract on the caller, which meets the requirements of `slice::from_raw_parts`. The slice
    // lives only during this call, which writes no memory.
    let bytes = unsafe { core::slice::from_raw_parts(ptr, size) };
    if matches!(self, Varint::Quic) {
      if bytes.is_empty() {
        return None;
      }
      let length = 1 << (bytes[0] >> 6);
      if bytes.len() < length {
        return None;
      }
      let (mut value, mut i) = ((bytes[0] & 0x3f) as u128, 1);
      while i < length {
        value = value << 8 | bytes[i] as u128;
        i += 1;
      }
      return Some((value, length));
    }
    let (mut value, mut i) = (0u128, 0);
    while i < bytes.len() {
      let bits = (bytes[i] & 0x7f) as u128;
      match self {
        // 18 bytes are 126 bits: the 19th can only add 2, and there is no 20th.
        Varint::Leb128 if i < 18 || (i == 18 && bits < 4) => value |= bits << (7 * i),
        Varint::Vlq if value >> 121 == 0 => value = value << 7 | bits,
        _ => return None,
      }
      i += 1;
      if bytes[i - 1] & 0x80 == 0 {
        return Some((value, i));
      }
    }
    None
  }
}

/// Returns whether an expression computes on fields alone, with a few instructions each. This is so
/// if it reads no byte by offset, computes no checksum, CRC or pattern, does not walk, and does not
/// divide.
const fn cheap_ops(ops: &[Op]) -> bool {
  let mut i = 0;
  while i < ops.len() {
    if matches!(
      ops[i],
      Op::At { .. }
        | Op::Varint { .. }
        | Op::Crc32
        | Op::Sum16
        | Op::Crc(_)
        | Op::CrcZeros(_)
        | Op::Checksum(_)
        | Op::Matches(_)
        | Op::Walk { .. }
        | Op::Div
        | Op::Rem
    ) {
      return false;
    }
    i += 1;
  }
  true
}

/// Returns whether two expressions are the same, operation by operation ([`Op::equals`]).
pub(crate) const fn same_ops(a: &[Op], b: &[Op]) -> bool {
  let mut i = 0;
  while i < a.len() && i < b.len() && a[i].equals(&b[i]) {
    i += 1;
  }
  a.len() == b.len() && i == a.len()
}

/// A rule of a struct, what a value requires beyond the validity of its fields.
///
/// A rule is checked when a value of the struct is made from another type, as by `try_pun` or
/// `from_head`, unless the source has the same rules at the same place. A `size` or `len` rule also
/// tells `from_head` how long a value is. A rule is a requirement, not a guarantee. Safe code may
/// write the fields of a value so that its rules do not hold, and unsafe code must not rely on a
/// rule of a value that it did not check.
///
/// `#[derive(Pun)]` makes the rules of `#[pun(size = ..)]` and `#[pun(assert(..))]` on a struct,
/// and of `#[pun(len = ..)]` and `#[pun(regions(..))]` on its last field.
///
/// # Examples
///
/// ```
/// use puns::{Pun, num::u16be};
///
/// /// A count, then as many numbers.
/// #[derive(Pun)]
/// #[repr(C)]
/// struct List {
///   count: u8,
///   #[pun(len = count)]
///   numbers: [u16be],
/// }
///
/// let (list, after) = puns::from_head::<List>(&[2, 0, 1, 0, 2, 9]).unwrap();
/// assert_eq!((list.numbers.len(), after), (2, &[9][..]));
/// assert!(puns::from_bytes::<List>(&[2, 0, 1]).is_err());
/// ```
#[derive(Copy, Clone, Debug)]
pub enum Rule {
  /// The extent of the value in bytes, up to the end of its slice and without the padding that may
  /// follow it, is the value of the expression.
  Size(&'static [Op]),
  /// The length of the tail of the value, an array, a slice or a `str`, is the value of the
  /// expression.
  Len(&'static [Op]),
  /// The value of the expression is not 0.
  Assert(&'static [Op]),
  /// The tail of the value has a region of as many bytes as the value of the expression, or of
  /// whatever remains of the tail for `None`. The regions follow each other from the start of the
  /// tail, in the order of the rules.
  Region(Option<&'static [Op]>),
}

/// Returns whether an expression reads the bytes of the value by offset, rather than its fields:
/// [`Op::At`], [`Op::Varint`], a checksum, a CRC, [`Op::Matches`], or a walk that does one of
/// these.
const fn ops_read_bytes(ops: &[Op]) -> bool {
  let mut i = 0;
  while i < ops.len() {
    match ops[i] {
      Op::At { .. }
      | Op::Varint { .. }
      | Op::Crc32
      | Op::Sum16
      | Op::Crc(_)
      | Op::CrcZeros(_)
      | Op::Checksum(_)
      | Op::Matches(_) => return true,
      Op::Walk { cond, next, .. } if ops_read_bytes(cond) || ops_read_bytes(next) => return true,
      _ => {}
    }
    i += 1;
  }
  false
}

impl Rule {
  /// Returns the expression of the rule, or `None` for a region of what remains.
  pub(crate) const fn ops(&self) -> Option<&'static [Op]> {
    match *self {
      Rule::Size(ops) | Rule::Len(ops) | Rule::Assert(ops) | Rule::Region(Some(ops)) => Some(ops),
      Rule::Region(None) => None,
    }
  }

  /// Returns whether two rules are of the same kind, with the same expression.
  pub(crate) const fn equals(&self, other: &Rule) -> bool {
    let same_kind = matches!(
      (self, other),
      (Rule::Size(_), Rule::Size(_))
        | (Rule::Len(_), Rule::Len(_))
        | (Rule::Assert(_), Rule::Assert(_))
        | (Rule::Region(_), Rule::Region(_))
    );
    match (self.ops(), other.ops()) {
      (None, None) => same_kind,
      (Some(a), Some(b)) => same_kind && same_ops(a, b),
      _ => false,
    }
  }
}

/// The tail of a struct, as rules see it ([`Layout::tail_shape`]).
#[derive(Copy, Clone)]
pub(crate) struct TailShape {
  /// The offset of the tail.
  pub(crate) start: usize,
  /// The size of the tail if the struct is `Sized`, and 0 otherwise.
  pub(crate) bytes: usize,
  /// The number of elements of the array that the tail ends with, if the struct is `Sized`, and 0
  /// otherwise.
  pub(crate) len: usize,
  /// Whether the tail of a `Sized` struct ends with an array.
  pub(crate) array: bool,
}

/// The rule that tells how long a value is from its first bytes ([`Layout::length_rule`]).
#[derive(Copy, Clone)]
pub(crate) struct LengthRule {
  /// The offset of the struct that has the rule.
  pub(crate) base: usize,
  /// Whether the expression is a number of elements of the tail (a `len` rule), rather than the
  /// size in bytes of the struct (a `size` rule).
  pub(crate) elements: bool,
  /// The expression of the rule.
  pub(crate) ops: &'static [Op],
}

/// The depth of the stack of the evaluator ([`Machine`]).
pub(crate) const STACK: usize = 16;
/// The number of variables of an expression.
pub(crate) const VARIABLES: usize = 8;
/// The greatest number of operations of an expression.
///
/// The validation applies each operation in an instance of its own function, reached by recursion
/// over a tree of the operations (`validate::steps`), which this limit and [`MAX_WALKS`] keep
/// shallow. `#[derive(Pun)]` has the same limits, and says where an expression goes beyond them.
pub(crate) const MAX_OPS: usize = 256;
/// The greatest depth of walks in an expression, where a walk inside a walk is two deep. See
/// [`MAX_OPS`].
pub(crate) const MAX_WALKS: usize = 4;

/// Asserts that the expressions of `rules` are well formed, and read the fields of the struct only
/// within its first `head` bytes.
///
/// # Panics
///
/// Panics as [`check_ops`] does.
const fn check_rules(rules: &[Rule], head: usize) {
  let mut i = 0;
  while i < rules.len() {
    if let Some(ops) = rules[i].ops() {
      let sizes = matches!(rules[i], Rule::Size(_) | Rule::Len(_));
      check_ops(ops, head, sizes, 0, 0);
    }
    i += 1;
  }
}

/// Returns the `index`th operation of `ops`, if there is one.
pub(crate) const fn op_at(ops: &[Op], index: usize) -> Option<Op> {
  if index < ops.len() { Some(ops[index]) } else { None }
}

/// Asserts that an expression is well formed.
///
/// The variables of `defined`, one bit each, are defined before the expression, which is inside
/// `walks` walks. With `sizes`, the expression is a size or a length, which cannot be computed from
/// the size of the value. A field that the expression loads must be within the first `head` bytes
/// of the struct.
///
/// What an [`Op::Shortcut`], an [`Op::Then`] or an [`Op::Else`] may leave out is a *part*, the
/// right side of a `&&` or a `||`, or a branch of an `if`. A part must end with what it is a part
/// of (the `Both`, `Either`, `Else` or `Select`), lie within the part that holds it, and push one
/// value without taking any value from below it. The stack is then as deep after a part whether or
/// not the part was computed, since the evaluator pushes a placeholder for a branch that is not
/// computed. What a part defines is not defined after it. Every `&&`, `||` and `if` has its parts,
/// so what is computed is what Rust computes, and no failure is left out of the value of the
/// expression.
///
/// # Panics
///
/// Panics in any of the following cases.
///
/// - The expression has more than [`MAX_OPS`] operations, nests walks deeper than [`MAX_WALKS`],
///   needs a stack deeper than [`STACK`], or uses a variable past [`VARIABLES`] or before it is
///   defined.
/// - A load is not within the first `head` bytes, a read of the value is not of 1 to 16 bytes, or a
///   size or a length reads the size of the value.
/// - The parts are not as above, or the expression does not leave exactly one value.
pub(crate) const fn check_ops(ops: &[Op], head: usize, sizes: bool, mut defined: u8, walks: usize) {
  assert!(ops.len() <= MAX_OPS, "an expression is too long");
  let (mut depth, mut i) = (0usize, 0);
  // The parts that are open, innermost last: the index of the operation that ends each, the depth
  // of the stack before it, the variables defined before it, and the operation that opened it.
  let (mut parts, mut open) = ([(0usize, 0usize, 0u8, Op::Size); STACK], 0);
  while i < ops.len() {
    let op = ops[i];
    // An automaton written without `Op::matches` is checked here, before a validation runs it.
    if let Op::Matches(dfa) = op {
      dfa.check();
    }
    assert!(
      !(sizes && matches!(op, Op::Size)),
      "a size or a length cannot be computed from `size_of_val(self)`, which it defines"
    );
    // The part that this operation ends, if any, and what it must be.
    let ends = open > 0 && parts[open - 1].0 == i;
    if ends {
      let (_, floor, before, opener) = parts[open - 1];
      assert!(depth == floor + 1, "malformed expression");
      let fits = match (opener, op) {
        (Op::Shortcut { either, .. }, Op::Both) => !either,
        (Op::Shortcut { either, .. }, Op::Either) => either,
        (Op::Then(_), Op::Else(_)) | (Op::Else(_), Op::Select) => true,
        _ => false,
      };
      assert!(fits, "malformed expression");
      (open, defined) = (open - 1, before);
    } else {
      assert!(
        !matches!(op, Op::Both | Op::Either | Op::Else(_) | Op::Select),
        "malformed expression"
      );
    }
    let (pops, pushes) = effect(op);
    assert!(depth >= pops, "malformed expression");
    assert!(open == 0 || depth - pops >= parts[open - 1].1, "malformed expression");
    // A part that starts after this operation, and the operation that ends it: the one after it,
    // except for the part of an `Else`, which is followed by the `Select` (which it does not skip).
    if let Op::Shortcut { len, .. } | Op::Then(len) | Op::Else(len) = op {
      let end = i + len as usize + matches!(op, Op::Else(_)) as usize;
      assert!(depth >= 1 && len >= 1 && end < ops.len(), "malformed expression");
      assert!(open == 0 || end < parts[open - 1].0, "malformed expression");
      assert!(open < STACK, "expression is too deep");
      parts[open] = (end, depth, defined, op);
      open += 1;
    }
    match op {
      Op::At { size, .. } => assert!(size >= 1 && size <= 16, "malformed expression"),
      Op::Load { offset, size, .. } => {
        assert!(size >= 1 && size <= 16 && offset + size as usize <= head, "field out of bounds");
      }
      Op::Set(index) => {
        assert!((index as usize) < VARIABLES, "too many variables in an expression");
        defined |= 1 << index;
      }
      Op::Get(index) => {
        assert!((index as usize) < VARIABLES && defined & (1 << index) != 0, "no such variable");
      }
      Op::Walk { var, cond, next } => {
        assert!(walks < MAX_WALKS, "walks are nested too deeply");
        assert!((var as usize) < VARIABLES, "too many variables in an expression");
        check_ops(cond, head, sizes, defined | 1 << var, walks + 1);
        check_ops(next, head, sizes, defined | 1 << var, walks + 1);
      }
      _ => {}
    }
    depth = depth - pops + pushes;
    assert!(depth <= STACK, "expression is too deep");
    i += 1;
  }
  assert!(depth == 1 && open == 0, "malformed expression");
}

/// Returns how many values an operation takes off the stack, and how many it puts on it, where it
/// is computed ([`check_ops`] says what happens where it is not).
const fn effect(op: Op) -> (usize, usize) {
  match op {
    Op::Lit(_) | Op::Load { .. } | Op::Size | Op::Get(_) => (0, 1),
    Op::At { .. }
    | Op::Varint { .. }
    | Op::Not
    | Op::Walk { .. }
    | Op::Shortcut { .. }
    | Op::Then(_) => (1, 1),
    // After the branch that is taken when the condition is true, which it does not take off.
    Op::Else(_) => (0, 0),
    Op::Set(_) => (1, 0),
    Op::Select => (3, 1),
    Op::CrcZeros(_) => (4, 1),
    Op::Crc32
    | Op::Sum16
    | Op::Crc(_)
    | Op::Checksum(_)
    | Op::Matches(_)
    | Op::Add
    | Op::Sub
    | Op::Mul
    | Op::Div
    | Op::Rem
    | Op::And
    | Op::Or
    | Op::Xor
    | Op::Shl
    | Op::Shr
    | Op::Eq
    | Op::Ne
    | Op::Lt
    | Op::Le
    | Op::Gt
    | Op::Ge
    | Op::Both
    | Op::Either => (2, 1),
  }
}

/// The evaluator of an expression: a stack of values, and variables.
///
/// The machine is given one operation at a time, each a constant of the instance of the function
/// that applies it (`validate::steps`), so there is no interpreter. Once inlined there, what
/// remains is the arithmetic of the expression, without relying much on the optimizer. The stack is
/// not an array but one field per slot. What is not computed is marked by an index (`skip`), which
/// is a constant on each path through the code. A failure is the failure of the expression, since
/// everything that is computed counts toward its value, so it returns at once.
///
/// The operations of an expression must be applied in order, each once, to a machine that starts as
/// [`Machine::NEW`] or as [`Machine::enter`] makes it. [`check_ops`] ensures that the stack then
/// never holds more than [`STACK`] values, nor fewer than an operation takes.
#[derive(Copy, Clone)]
pub(crate) struct Machine {
  /// The values of the stack, the first at the bottom.
  stack: Stack,
  /// The variables.
  variables: Variables,
  /// The number of values on the stack.
  depth: usize,
  /// The operations before this index are not computed. They are the part that an [`Op::Shortcut`],
  /// an [`Op::Then`] or an [`Op::Else`] leaves out.
  skip: usize,
}

/// Declares, for `slots!(Name: [0 1 2 ..] last)`, a type that holds `u128` values in fields, one
/// per index, rather than in an array, so that the optimizer makes a variable of each.
///
/// The `get` and `set` of an index past `last` reach the field `last`, so that a `match` on the
/// index has no case that the others do not cover ([`Machine`]).
macro_rules! slots {
  ($($name:ident: [$($index:tt)+] $last:tt)+) => {$(
    /// Values in fields, one per index.
    #[derive(Copy, Clone)]
    struct $name($(slots!(@u128 $index),)+ u128);

    impl $name {
      /// The values, all 0.
      const NEW: Self = Self($(slots!(@zero $index),)+ 0);

      /// Returns the value at `index`, or at the last index if `index` is past it.
      // The last one is that of any other index, which is not asked for: no other case.
      #[inline(always)]
      const fn get(&self, index: usize) -> u128 {
        match index {
          $($index => self.$index,)+
          _ => self.$last,
        }
      }

      /// Sets the value at `index`, or at the last index if `index` is past it.
      #[inline(always)]
      const fn set(&mut self, index: usize, value: u128) {
        match index {
          $($index => self.$index = value,)+
          _ => self.$last = value,
        }
      }
    }
  )+};
  (@u128 $index:tt) => { u128 };
  (@zero $index:tt) => { 0 };
}

slots! {
  Stack: [0 1 2 3 4 5 6 7 8 9 10 11 12 13 14] 15
  Variables: [0 1 2 3 4 5 6] 7
}

impl Machine {
  /// A machine with an empty stack, whose variables are 0.
  pub(crate) const NEW: Self =
    Self { stack: Stack::NEW, variables: Variables::NEW, depth: 0, skip: 0 };

  /// Returns the value of the stack at `slot`.
  #[inline(always)]
  const fn get(&self, slot: usize) -> u128 {
    self.stack.get(slot)
  }

  /// Sets the value of the stack at `slot`.
  #[inline(always)]
  const fn set(&mut self, slot: usize, value: u128) {
    self.stack.set(slot, value);
  }

  /// Pushes `value` on the stack.
  #[inline(always)]
  const fn push(&mut self, value: u128) {
    self.stack.set(self.depth, value);
    self.depth += 1;
  }

  /// Returns whether the operation at the index `at` of the expression is computed.
  #[inline(always)]
  pub(crate) const fn computes(&self, at: usize) -> bool {
    at >= self.skip
  }

  /// Returns the value of the expression, once all of its operations are applied without a failure.
  #[inline(always)]
  pub(crate) const fn result(&self) -> u128 {
    self.get(0)
  }

  /// Returns the value at the top of the stack.
  ///
  /// # Panics
  ///
  /// Panics if the stack is empty, where overflow checks are enabled.
  #[inline(always)]
  pub(crate) const fn top(&self) -> u128 {
    self.get(self.depth - 1)
  }

  /// Replaces the value at the top of the stack by `value`, if there is one.
  ///
  /// Returns `false` if `value` is `None`, which is then the failure of the expression.
  #[must_use]
  #[inline(always)]
  pub(crate) const fn replace(&mut self, value: Option<u128>) -> bool {
    match value {
      Some(value) => {
        self.set(self.depth - 1, value);
        true
      }
      None => false,
    }
  }

  /// Returns a machine to evaluate another expression, an expression of a walk, with an empty
  /// stack, in which the variable `index` is `value` and the other variables are what they are
  /// here.
  #[inline(always)]
  pub(crate) const fn enter(&self, index: u8, value: u128) -> Self {
    let mut inner = Self { depth: 0, skip: 0, ..*self };
    inner.variables.set(index as usize, value);
    inner
  }

  /// Applies the operation `op`, at the index `at` of an expression, computed as `how` says
  /// (`narrow.rs`), over the value at `base`.
  ///
  /// `size` is the size of the value or, for a `size` or a `len` rule, which tells it, the number
  /// of bytes that can be read. Returns `false` if the operation fails, which is then the failure
  /// of the expression. Does nothing, and returns `true`, if the operation is not computed. It
  /// never applies an [`Op::Walk`], which `validate::walk` applies, and returns `false` for one.
  ///
  /// # Safety
  ///
  /// The caller must ensure that:
  ///
  /// - `op` is the operation at the index `at` of an expression of a rule in the layout of a
  ///   struct, which [`check_rules`] accepted, so that a [`Op::Load`] reads within the sized fields
  ///   of the struct.
  /// - `base` is valid for reads of the bytes of the sized fields of the struct, and of `size`
  ///   bytes.
  /// - The bytes that `op` reads are initialized: those of the field of an [`Op::Load`] and, for
  ///   an operation that reads bytes by offset ([`Layout::rules_read_bytes`]), all `size` bytes.
  // Not in debug builds, where this would only make every struct that has rules slow to build.
  #[must_use]
  #[cfg_attr(not(debug_assertions), inline(always))]
  pub(crate) const unsafe fn apply(
    &mut self,
    op: Op,
    at: usize,
    how: How,
    base: *const u8,
    size: usize,
  ) -> bool {
    if !self.computes(at) {
      return true;
    }
    let depth = self.depth;
    match op {
      Op::Lit(value) => self.push(value),
      Op::Load { offset, size, big } => {
        let endian = if big { Endian::Big } else { Endian::Little };
        // SAFETY: `op` is an operation of a rule that `Layout::structure` accepted, by contract on
        // the caller. `check_ops` asserted that a load reads 1 to 16 bytes within the sized
        // fields of the struct. `base` is valid for reads of these bytes, which are initialized,
        // by contract on the caller. So `add(offset)` stays within them, and they meet the
        // requirements of `load`.
        self.push(unsafe { load(base.add(offset), size as usize, endian) });
      }
      Op::Size => self.push(size as u128),
      Op::Get(index) => self.push(fit(self.variables.get(index as usize), how.result)),
      Op::Set(index) => {
        self.variables.set(index as usize, self.get(depth - 1));
        self.depth -= 1;
      }
      Op::Not => self.set(depth - 1, (self.get(depth - 1) == 0) as u128),
      Op::Varint { form, length } => {
        let offset = self.get(depth - 1);
        let value = if offset <= size as u128 {
          // SAFETY: `offset <= size`, which is checked above, and `base` is valid for reads of
          // `size` bytes, initialized since `op` reads bytes by offset, by contract on the caller.
          // So `add(offset)` stays within them, and the `size - offset` bytes from there meet the
          // requirements of `Varint::read`.
          match unsafe { form.read(base.add(offset as usize), size - offset as usize) } {
            Some((value, bytes)) => Some(if length { bytes as u128 } else { value }),
            None => None,
          }
        } else {
          None
        };
        return self.replace(value);
      }
      Op::At { size: width, big } => {
        let (offset, endian) =
          (self.get(depth - 1), if big { Endian::Big } else { Endian::Little });
        // In the value: one comparison of the offset, with what does not change in a walk.
        if size >= width as usize && offset <= (size - width as usize) as u128 {
          // SAFETY: `offset + width <= size`, which is checked above, and `base` is valid for reads
          // of `size` bytes, initialized since `op` reads bytes by offset, by contract on the
          // caller. `check_ops` asserted that `width` is 1 to 16. These are the requirements of
          // `add` and of `load`.
          self.set(depth - 1, unsafe { load(base.add(offset as usize), width as usize, endian) });
        } else {
          return false;
        }
      }
      op @ (Op::Crc32
      | Op::Sum16
      | Op::Crc(_)
      | Op::CrcZeros(_)
      | Op::Checksum(_)
      | Op::Matches(_)) => {
        // The range, and the one taken as zeros (none except for `CrcZeros`).
        let operands = if matches!(op, Op::CrcZeros(_)) { 4 } else { 2 };
        let (start, end) = (self.get(depth - operands), self.get(depth - operands + 1));
        let (zeros, to) =
          if operands == 4 { (self.get(depth - 2), self.get(depth - 1)) } else { (0, 0) };
        self.depth -= operands - 1;
        if start <= end && end <= size as u128 && zeros <= to && to <= size as u128 {
          // SAFETY: `start <= end <= size`, which is checked above, and `base` is valid for reads
          // of `size` bytes, initialized since `op` reads bytes by offset, by contract on the
          // caller. So `add(start)` stays within them, and the `end - start` bytes from there meet
          // the requirements of `slice::from_raw_parts`, as in `Dfa::matches`. The slice lives only
          // during this call, which writes no memory but `self`.
          let bytes = unsafe {
            core::slice::from_raw_parts(base.add(start as usize), (end - start) as usize)
          };
          // Where the zeros are in `bytes` (in the value: they fit a `usize`).
          let zeros = (zeros.saturating_sub(start) as usize, to.saturating_sub(start) as usize);
          let value = match op {
            Op::Crc32 => crc32_inline(bytes) as u128,
            Op::Sum16 => sum16_inline(bytes) as u128,
            Op::Crc(crc) | Op::CrcZeros(crc) => crc.compute(bytes, zeros) as u128,
            Op::Checksum(checksum) => checksum.compute(bytes),
            Op::Matches(dfa) => dfa.accepts_bytes(bytes) as u128,
            _ => 0,
          };
          self.set(depth - operands, value);
        } else {
          return false;
        }
      }
      // The left side decides: it is the value, and the right side is not computed.
      Op::Shortcut { either, len } => {
        if (self.get(depth - 1) != 0) == either {
          self.set(depth - 1, either as u128);
          self.skip = at + 1 + len as usize;
        }
      }
      // The condition is false: the branch is not computed, a placeholder is its value.
      Op::Then(len) => {
        if self.get(depth - 1) == 0 {
          self.push(0);
          self.skip = at + 1 + len as usize;
        }
      }
      // The branch was computed: the `else` branch is not, a placeholder is its value.
      Op::Else(len) => {
        self.push(0);
        self.skip = at + 1 + len as usize;
      }
      // The left side did not decide (see `Op::Shortcut`): the right side is the value.
      Op::Both | Op::Either => {
        self.set(depth - 2, (self.get(depth - 1) != 0) as u128);
        self.depth -= 1;
      }
      // One of the values is a placeholder, and not the one that the condition selects.
      Op::Select => {
        let value =
          if self.get(depth - 3) != 0 { self.get(depth - 2) } else { self.get(depth - 1) };
        self.set(depth - 3, fit(value, how.result));
        self.depth -= 2;
      }
      // `validate::walk` applies a walk, with the instances of the functions that apply its
      // expressions: the validation never gives one to this function (`validate::covers`).
      Op::Walk { .. } => return false,
      op => {
        match narrow::binary(op, how, self.get(depth - 2), self.get(depth - 1)) {
          Some(value) => self.set(depth - 2, fit(value, how.result)),
          None => return false,
        }
        self.depth -= 1;
      }
    }
    true
  }
}

// `walk_limit` adds 16 to the size of a value, at most `isize::MAX`: it does not overflow.
const _: () = assert!(isize::MAX as usize <= usize::MAX - 16);

/// Returns the greatest number of steps of a walk that may read `size` bytes, a step for each bit
/// of these bytes and for each bit of a `u128`, so that a walk ends.
///
/// `size` is at most `isize::MAX`, so adding 16 does not overflow.
#[inline(always)]
pub(crate) const fn walk_limit(size: usize) -> usize {
  (size + 16).saturating_mul(8)
}

/// Returns the binary operation `op` of `a` and `b` on `u128`, as an expression means it.
///
/// Returns `None` where Rust would panic, and for an operation that is not binary.
#[inline(always)]
pub(crate) const fn binary(op: Op, a: u128, b: u128) -> Option<u128> {
  match op {
    Op::Add => a.checked_add(b),
    Op::Sub => a.checked_sub(b),
    Op::Mul => a.checked_mul(b),
    Op::Div => a.checked_div(b),
    Op::Rem => a.checked_rem(b),
    Op::And => Some(a & b),
    Op::Or => Some(a | b),
    Op::Xor => Some(a ^ b),
    Op::Shl => {
      if b < 128 {
        Some(a << b)
      } else {
        None
      }
    }
    Op::Shr => {
      if b < 128 {
        Some(a >> b)
      } else {
        None
      }
    }
    Op::Eq => Some((a == b) as u128),
    Op::Ne => Some((a != b) as u128),
    Op::Lt => Some((a < b) as u128),
    Op::Le => Some((a <= b) as u128),
    Op::Gt => Some((a > b) as u128),
    Op::Ge => Some((a >= b) as u128),
    Op::Lit(_)
    | Op::Load { .. }
    | Op::Size
    | Op::Not
    | Op::At { .. }
    | Op::Varint { .. }
    | Op::Crc32
    | Op::Sum16
    | Op::Crc(_)
    | Op::CrcZeros(_)
    | Op::Checksum(_)
    | Op::Matches(_)
    | Op::Both
    | Op::Either
    | Op::Select
    | Op::Set(_)
    | Op::Get(_)
    | Op::Shortcut { .. }
    | Op::Then(_)
    | Op::Else(_)
    | Op::Walk { .. } => None,
  }
}

/// Returns the CRC-32 of `bytes`, as Ethernet, zlib and PNG compute it.
///
/// This is [`Crc::CRC32`], computed sixteen bytes at a time. The frame check sequence of an
/// Ethernet frame is the CRC-32 of what is before it, least significant byte first. In a rule of
/// `#[derive(Pun)]`, `crc32(start, end)` is this CRC of the bytes from `start` to `end`.
///
/// # Examples
///
/// ```
/// assert_eq!(puns::crc32(b"123456789"), 0xcbf43926);
/// ```
#[must_use]
pub const fn crc32(bytes: &[u8]) -> u32 {
  crc32_inline(bytes)
}

/// Returns [`crc32`] of `bytes`, inlined where a rule computes it.
#[inline(always)]
const fn crc32_inline(bytes: &[u8]) -> u32 {
  // Slicing by 16, as `Crc`, with tables of `u32`s (16 KiB, built at compile time). Without
  // carry-less multiplication, which a `const fn` cannot reach on stable Rust, it is the fastest.
  const TABLES: [[u32; 256]; 16] = {
    let mut tables = [[0u32; 256]; 16];
    let mut i = 0;
    while i < 256 {
      let (mut value, mut bit) = (i as u32, 0);
      while bit < 8 {
        value = if value & 1 != 0 { (value >> 1) ^ 0xedb8_8320 } else { value >> 1 };
        bit += 1;
      }
      tables[0][i] = value;
      i += 1;
    }
    let mut k = 1;
    while k < 16 {
      let mut i = 0;
      while i < 256 {
        let value = tables[k - 1][i];
        tables[k][i] = tables[0][(value & 0xff) as usize] ^ (value >> 8);
        i += 1;
      }
      k += 1;
    }
    tables
  };
  let (blocks, rest) = bytes.as_chunks::<16>();
  let (t, mut crc, mut i) = (&TABLES, !0u32, 0);
  while i < blocks.len() {
    let b = &blocks[i];
    let x = (crc ^ u32::from_le_bytes([b[0], b[1], b[2], b[3]])).to_le_bytes();
    crc = t[15][x[0] as usize]
      ^ t[14][x[1] as usize]
      ^ t[13][x[2] as usize]
      ^ t[12][x[3] as usize]
      ^ t[11][b[4] as usize]
      ^ t[10][b[5] as usize]
      ^ t[9][b[6] as usize]
      ^ t[8][b[7] as usize]
      ^ t[7][b[8] as usize]
      ^ t[6][b[9] as usize]
      ^ t[5][b[10] as usize]
      ^ t[4][b[11] as usize]
      ^ t[3][b[12] as usize]
      ^ t[2][b[13] as usize]
      ^ t[1][b[14] as usize]
      ^ t[0][b[15] as usize];
    i += 1;
  }
  let mut i = 0;
  while i < rest.len() {
    crc = t[0][((crc ^ rest[i] as u32) & 0xff) as usize] ^ (crc >> 8);
    i += 1;
  }
  !crc
}

/// Returns the ones' complement sum of the big-endian 16-bit words of `bytes`.
///
/// A last byte alone is the high byte of a word. The sum is 0 for no bytes or only zero bytes, else
/// the value of `1..=0xffff` that is congruent to the sum of the words modulo `0xffff`. The
/// checksums of IP, UDP and TCP are the complement of this sum, so over data that holds its own
/// checksum, the sum is `0xffff`. In a rule of `#[derive(Pun)]`, `sum16(start, end)` is this sum of
/// the bytes from `start` to `end`.
///
/// # Examples
///
/// ```
/// assert_eq!(puns::sum16(&[0x00, 0x01, 0xf2, 0x03, 0xf4, 0xf5, 0xf6, 0xf7]), 0xddf2);
/// ```
#[must_use]
pub const fn sum16(bytes: &[u8]) -> u16 {
  sum16_inline(bytes)
}

/// The number of blocks of 8 bytes that [`sum16`] adds before it folds the sum, 2^20, or all of
/// them where a `usize` does not count that far.
///
/// On a 16-bit target, a value has at most 2^12 blocks, far from a lost carry.
const SUM16_FOLD: usize = if usize::BITS > 20 { 1 << (20 % usize::BITS) } else { usize::MAX };

// A block adds less than 2^33 to the sum: `SUM16_FOLD` of them never carry out of a `u64`.
const _: () = assert!((SUM16_FOLD as u128) << 33 < 1 << 64);

/// Returns [`sum16`] of `bytes`, inlined where a rule computes it.
#[inline(always)]
const fn sum16_inline(bytes: &[u8]) -> u16 {
  // Eight bytes at a time, in the byte order of the target, as two halves of 32 bits added into a
  // `u64` (RFC 1071): words of the other byte order sum to the sum with its bytes swapped, which
  // is swapped back. Folded every 2^20 blocks, long before a carry could be lost (2^31). As the
  // sum word by word folded at each step: `0` for no or only zero bytes, else the value of
  // `1..=0xffff` that is congruent to the sum modulo `0xffff`.
  let (mut blocks, rest) = bytes.as_chunks::<8>();
  let mut sum = 0u64;
  while !blocks.is_empty() {
    let (now, later) =
      blocks.split_at(if blocks.len() > SUM16_FOLD { SUM16_FOLD } else { blocks.len() });
    let (mut wide, mut i) = (0u64, 0);
    while i < now.len() {
      let block = u64::from_ne_bytes(now[i]);
      wide += (block & 0xffff_ffff) + (block >> 32);
      i += 1;
    }
    sum = fold(sum + fold(wide) as u64) as u64;
    blocks = later;
  }
  if cfg!(target_endian = "little") {
    sum = (sum as u16).swap_bytes() as u64;
  }
  // What remains, big-endian: fewer than 8 bytes, from an even offset.
  let mut i = 0;
  while i < rest.len() {
    let low = if i + 1 < rest.len() { rest[i + 1] } else { 0 };
    sum += ((rest[i] as u64) << 8) | low as u64;
    i += 2;
  }
  fold(sum)
}

/// Returns `sum`, a sum of 16-bit words, folded into 16 bits with end-around carries.
#[inline(always)]
const fn fold(mut sum: u64) -> u16 {
  while sum > 0xffff {
    sum = (sum & 0xffff) + (sum >> 16);
  }
  sum as u16
}

/// Returns the inclusive bounds of a range of `#[pun(valid(..))]`, of a field of type `T`.
///
/// `start` is the start of the range, 0 if it has none. `end` is its end, inclusive if `inclusive`,
/// and the greatest value of an unsigned integer of the size of `T` if it has none.
///
/// # Panics
///
/// Panics if the range is empty.
#[doc(hidden)]
#[must_use]
pub const fn range<T>(start: Option<u128>, end: Option<u128>, inclusive: bool) -> (u128, u128) {
  let start = match start {
    Some(start) => start,
    None => 0,
  };
  let end = match end {
    Some(end) if inclusive => end,
    Some(end) => {
      assert!(end > start, "empty range");
      end - 1
    }
    None => max_value(size_of::<T>()),
  };
  assert!(start <= end, "empty range");
  (start, end)
}

/// Returns inclusive `ranges` sorted, with the ranges that overlap or are adjacent merged, and
/// their count.
///
/// The first `count` elements of the array are the ranges. The others are `(0, 0)`. Every range
/// must have its low bound at most its high bound.
#[doc(hidden)]
#[must_use]
pub const fn merge_ranges<const N: usize>(
  mut ranges: [(u128, u128); N],
) -> ([(u128, u128); N], usize) {
  let mut i = 1;
  while i < N {
    let mut j = i;
    while j > 0 && ranges[j - 1].0 > ranges[j].0 {
      ranges.swap(j - 1, j);
      j -= 1;
    }
    i += 1;
  }
  let (mut merged, mut len, mut i) = ([(0, 0); N], 0, 0);
  while i < N {
    if len > 0 && (ranges[i].0 <= merged[len - 1].1 || ranges[i].0 - merged[len - 1].1 == 1) {
      if ranges[i].1 > merged[len - 1].1 {
        merged[len - 1].1 = ranges[i].1;
      }
    } else {
      merged[len] = ranges[i];
      len += 1;
    }
    i += 1;
  }
  (merged, len)
}

/// Returns the bytes of `value`, of at most 16 bytes, as an unsigned integer in the byte order of
/// the target.
///
/// This is how `#[derive(Pun)]` reads the discriminant of each variant of an enum without fields.
///
/// # Panics
///
/// Panics if `T` is larger than 16 bytes.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - Every byte of `value` is initialized.
#[doc(hidden)]
pub const unsafe fn bits_of<T>(value: T) -> u128 {
  assert!(size_of::<T>() <= 16, "an enum without fields cannot be larger than 16 bytes");
  // Not dropped: a `const fn` cannot drop a value of a generic type.
  let value = core::mem::ManuallyDrop::new(value);
  // SAFETY: `size_of::<T>() <= 16`, which is asserted above. `value` is a local of this function,
  // a `ManuallyDrop<T>`, which has the layout of a `T` [1]. So it is valid for reads of
  // `size_of::<T>()` bytes, and no other code writes it. These bytes are initialized, by contract
  // on the caller. These are the requirements of `load`.
  //
  // [1] Per https://doc.rust-lang.org/1.99.0/core/mem/struct.ManuallyDrop.html:
  //
  //     `ManuallyDrop<T>` is guaranteed to have the same layout and bit validity as `T`, and is
  //     subject to the same layout optimizations as `T`.
  unsafe { load((&raw const value).cast(), size_of::<T>(), Endian::NATIVE) }
}

/// A value of each of the `N` variants of an enum `E` whose fields have samples, for
/// `#[derive(Pun)]`, which reads tags and offsets on them ([`Variant::declared_tags`],
/// [`Layout::sample_of`]).
#[doc(hidden)]
pub type Samples<E, const N: usize> = [Option<mem::MaybeUninit<E>>; N];
