//! Value-level description of the memory layout and of the valid bit patterns of a type.
//!
//! Every [`Pun`](crate::Pun) type exposes a `const LAYOUT: Layout`: a small tree, built at
//! compile time, that tells for every byte of the type whether it is padding, an unconstrained
//! initialized byte, or part of a *scalar* with a restricted set of valid values.
//!
//! The whole static analysis of the crate is ordinary `const fn` code running over those values
//! (see `analysis.rs`); nothing is encoded in the type system.

use core::mem;

#[doc(hidden)]
pub use crate::discover::Desc;
use crate::{
  narrow::{self, How, fit},
  ptr,
};

// -------------------------------------------------------------------------------------------------
// `Endian`

/// Byte order of a scalar.
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
  /// Byte order of the target.
  pub const NATIVE: Endian =
    if cfg!(target_endian = "little") { Endian::Little } else { Endian::Big };

  #[inline]
  pub(crate) const fn is_little(self) -> bool {
    matches!(self, Endian::Little)
  }

  #[inline]
  pub(crate) const fn equals(self, other: Endian) -> bool {
    self.is_little() == other.is_little()
  }
}

// -------------------------------------------------------------------------------------------------
// `Spec`

/// Values a scalar is allowed to take.
#[derive(Copy, Clone, Debug)]
pub(crate) enum Valid {
  /// Anything, including uninitialized bytes.
  Uninit,
  /// Any initialized value.
  Any,
  /// Sorted, disjoint and non-adjacent inclusive ranges. Never empty, never the full range.
  In(&'static [(u128, u128)]),
}

/// Specification of a scalar: its size, its byte order and the values it accepts.
///
/// It is the [`SPEC`](crate::Scalar::SPEC) of a [`Scalar`](crate::Scalar) type, and the layout of
/// one ([`Layout::scalar`]).
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
  pub(crate) size: usize,
  pub(crate) endian: Endian,
  /// What a value must be to be accepted: what is required of a target.
  pub(crate) valid: Valid,
  /// What a value is known to be, whatever safe code does: what a source guarantees.
  ///
  /// This is `valid`, but for fields that are restricted by `#[pun(valid(..))]`: nothing
  /// prevents safe code from writing any value of the type of the field to it.
  pub(crate) known: Valid,
  /// Whatever its bits are the value of an unsigned integer, which rules read. Not those of a
  /// signed or floating-point number, a `bool`, a `char` or an enum.
  pub(crate) number: bool,
}

/// Greatest value of an unsigned integer of `size` bytes.
#[inline]
pub(crate) const fn max_value(size: usize) -> u128 {
  if size >= 16 { u128::MAX } else { (1u128 << (8 * size)) - 1 }
}

impl Spec {
  /// A scalar the size of `T` that accepts every initialized bit pattern.
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
    Self { size, endian: Endian::NATIVE, valid: Valid::Any, known: Valid::Any, number: true }
  }

  /// A scalar of `size` bytes (at most 16) of native byte order that accepts every initialized
  /// bit pattern: the tag of an enum that `discover.rs` found.
  pub(crate) const fn of_size(size: usize) -> Self {
    assert!(size <= 16, "a scalar is at most 16 bytes wide");
    Self { size, endian: Endian::NATIVE, valid: Valid::Any, known: Valid::Any, number: true }
  }

  /// A scalar the size of `T` that accepts everything, uninitialized bytes included.
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
    Self { size, endian, valid: Valid::Uninit, known: Valid::Uninit, number: true }
  }

  /// A scalar the size of `T` whose value, read as an unsigned integer in `endian` byte order,
  /// must be in one of `ranges`.
  ///
  /// `ranges` are inclusive and must be sorted, disjoint and non-adjacent; this is checked at
  /// compile time. See [`sorted_ranges`] to build them from a list of values.
  ///
  /// ```
  /// use puns::layout::{Endian, Spec};
  ///
  /// let spec = Spec::ranges::<u8>(Endian::NATIVE, &[(1, 3), (7, 7)]);
  /// assert!(spec.contains(1) && spec.contains(3) && spec.contains(7));
  /// assert!(!spec.contains(0) && !spec.contains(5));
  /// ```
  ///
  /// # Panics
  /// As [`Spec::with_ranges`].
  #[must_use]
  pub const fn ranges<T>(endian: Endian, ranges: &'static [(u128, u128)]) -> Self {
    let size = size_of::<T>();
    Self { size, endian, valid: Valid::Any, known: Valid::Any, number: true }.with_ranges(ranges)
  }

  /// Same as `self`, with other valid `ranges`, see [`Spec::ranges`].
  ///
  /// ```
  /// use puns::layout::Spec;
  ///
  /// let digit = Spec::any::<u8>().with_ranges(&[(b'0' as u128, b'9' as u128)]);
  /// assert!(digit.contains(b'7'.into()) && !digit.contains(b'a'.into()));
  /// ```
  ///
  /// Ranges that are adjacent are one range, which must be written so:
  ///
  /// ```should_panic
  /// use puns::layout::Spec;
  ///
  /// let _ = Spec::any::<u8>().with_ranges(&[(0, 4), (5, 9)]);
  /// ```
  ///
  /// # Panics
  /// If the scalar is not 1 to 16 bytes wide, or if `ranges` is empty, not sorted, overlapping,
  /// adjacent or out of the bounds of the scalar.
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

  /// Same as `self`, for a scalar that may be uninitialized: what a target requires is still
  /// `self` (initialized, and one of its values), but nothing is known of what a source holds
  /// there, not even that it is initialized.
  ///
  /// This is a byte that is either a byte of a tag, whose value is the one that `self` accepts,
  /// or padding, without a way to tell which: the bytes of a tag that `rustc` widened past its
  /// values, in an enum without `repr` (see `discover.rs`). Requiring the value is right in
  /// either case, and so is knowing nothing.
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
  ///
  /// # Panics
  /// If `self` accepts every initialized value, or uninitialized bytes: such a scalar is
  /// [`Spec::uninit`].
  #[must_use]
  pub const fn maybe_uninit(self) -> Self {
    assert!(matches!(self.valid, Valid::In(_)), "a scalar that may be uninitialized restricts");
    Self { known: Valid::Uninit, ..self }
  }

  /// Whatever a source may hold uninitialized bytes in this scalar.
  #[inline]
  pub(crate) const fn may_be_uninit(&self) -> bool {
    matches!(self.known, Valid::Uninit)
  }

  /// Same as `self`, for a scalar whose bits are not the value of an unsigned integer: a signed
  /// or floating-point number, a `bool`, a `char`, an enum. The expressions of `#[pun(..)]`,
  /// which compute on unsigned integers, do not read it.
  ///
  /// The `Spec` of `i16` is `Spec::any::<i16>().not_a_number()`, of which a rule reads nothing:
  ///
  /// ```should_panic
  /// use puns::layout::Op;
  ///
  /// // A `u16` is read by a rule; an `i16` is not.
  /// let _ = Op::load::<u16>(0);
  /// let _ = Op::load::<i16>(0);
  /// ```
  #[must_use]
  pub const fn not_a_number(self) -> Self {
    Self { number: false, ..self }
  }

  /// Whatever `T` has no size, for `#[derive(Pun)]`.
  #[doc(hidden)]
  #[must_use]
  pub const fn is_zero_sized<T>() -> bool {
    size_of::<T>() == 0
  }

  /// [`Spec::ranges`], in the byte order of the target.
  #[doc(hidden)]
  #[must_use]
  pub const fn native_ranges<T>(ranges: &'static [(u128, u128)]) -> Self {
    Self::ranges::<T>(Endian::NATIVE, ranges).not_a_number()
  }

  /// [`sorted_ranges`], for `#[derive(Pun)]`.
  #[doc(hidden)]
  #[must_use]
  pub const fn sorted_ranges<const N: usize>(values: [u128; N]) -> ([(u128, u128); N], usize) {
    sorted_ranges(values)
  }

  /// [`range`], for `#[derive(Pun)]`.
  #[doc(hidden)]
  #[must_use]
  pub const fn range<T>(start: Option<u128>, end: Option<u128>, inclusive: bool) -> (u128, u128) {
    range::<T>(start, end, inclusive)
  }

  /// [`merge_ranges`], for `#[derive(Pun)]`.
  #[doc(hidden)]
  #[must_use]
  pub const fn merge_ranges<const N: usize>(
    ranges: [(u128, u128); N],
  ) -> ([(u128, u128); N], usize) {
    merge_ranges(ranges)
  }

  /// [`bits_of`], for `#[derive(Pun)]`.
  ///
  /// # Safety
  /// Every byte of `value` must be initialized.
  #[doc(hidden)]
  #[must_use]
  pub const unsafe fn bits_of<T>(value: &T) -> u128 {
    // SAFETY: guaranteed by the caller.
    unsafe { bits_of(value) }
  }

  /// Whatever the scalar is only restricted by a `#[pun(valid(..))]` that the type system does
  /// not enforce: it is then proven by restrictions of the same kind.
  #[inline]
  pub(crate) const fn is_soft(&self) -> bool {
    matches!(self.known, Valid::Any) && matches!(self.valid, Valid::In(_))
  }
  /// Same as `self`, read in another byte order.
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
  /// // SAFETY: any two bytes are one.
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

  /// Whatever the scalar is an initialized integer of at most 16 bytes.
  #[doc(hidden)]
  #[must_use]
  pub const fn is_integer(&self) -> bool {
    self.size <= 16 && !matches!(self.known, Valid::Uninit) && !matches!(self.valid, Valid::Uninit)
  }

  /// The scalar at `ptr`, as an unsigned integer.
  ///
  /// # Safety
  /// `self` must be [an integer](Self::is_integer), and `ptr` must be valid for reads of one.
  #[doc(hidden)]
  #[inline(always)]
  #[must_use]
  pub const unsafe fn load(&self, ptr: *const u8) -> u128 {
    // SAFETY: guaranteed by the caller.
    unsafe { load(ptr, self.size, self.endian) }
  }

  /// Whatever the scalar restricts the values it accepts.
  #[inline]
  pub(crate) const fn is_constrained(&self) -> bool {
    matches!(self.valid, Valid::In(_))
  }

  /// Whatever `value` is valid for this scalar.
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

  /// Whatever the scalar stored at `ptr` is valid.
  ///
  /// # Safety
  /// `ptr` must be valid for reads of `self.size` initialized bytes (no alignment requirement).
  #[inline(always)]
  pub(crate) const unsafe fn matches(&self, ptr: *const u8) -> bool {
    match self.valid {
      Valid::Uninit | Valid::Any => true,
      // SAFETY: guaranteed by the caller.
      Valid::In(ranges) => contains(ranges, unsafe { load(ptr, self.size, self.endian) }),
    }
  }
}

#[inline(always)]
pub(crate) const fn contains(ranges: &[(u128, u128)], value: u128) -> bool {
  let mut i = 0;
  // A few ranges: a comparison each, without a branch, so that the validation of a slice can be
  // vectorised (see `validate::validate_elems`).
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

/// Read an unsigned integer of `size <= 16` bytes.
///
/// # Safety
/// `ptr` must be valid for reads of `size` initialized bytes.
#[inline(always)]
pub(crate) const unsafe fn load(ptr: *const u8, size: usize, endian: Endian) -> u128 {
  let mut buf = [0u8; 16];
  // SAFETY: `size <= 16` and `ptr` is readable for `size` bytes.
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

/// Sort `values` and merge them into inclusive ranges; returns the ranges and their count.
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

/// What all the bytes of a type have in common, if anything.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Uniform {
  /// Nothing: the type must be looked into.
  No,
  /// Every byte is padding.
  Pad,
  /// Every byte is initialized and unconstrained.
  Any,
}

/// What a layout keeps in every layout that Rust allows for its type (section 12 of the design):
/// the compiler may arrange a struct or a tuple of the `Rust` representation, or an enum without
/// `repr`, otherwise in its next version (or with `-Zrandomize-layout`).
///
/// A layout that is not `fixed` is, without the `unstable` feature, *opaque* to the analysis: it
/// is known as a source by what every arrangement of it has (`init`), and only proves itself as a
/// target (the same type at the same place, [`Id`]).
#[derive(Copy, Clone, Debug)]
#[expect(clippy::struct_excessive_bools, reason = "five facts of a layout, each asked alone")]
pub(crate) struct Stable {
  /// The offsets of its parts and its size are the same in every layout that Rust allows, given
  /// the sizes of its parts, or every layout of it is the same bytes (see [`Layout::rust`]): it
  /// may be looked into.
  pub(crate) fixed: bool,
  /// Every byte of a value is of the same part in every layout that Rust allows. Not of a layout
  /// that is not `fixed`, nor of a struct whose orders all make the same bytes (`(u8, u8)`): the
  /// bytes are the same, but which field each holds is not, so what a check reads of a value is
  /// not either.
  pub(crate) ordered: bool,
  /// Its size is the same in every layout that Rust allows.
  pub(crate) size: bool,
  /// Nothing in it, at any depth, is not `fixed`.
  pub(crate) deep: bool,
  /// Of a layout that is not `fixed`: its bytes are initialized in every layout that Rust allows.
  pub(crate) init: bool,
}

impl Stable {
  /// A layout that Rust specifies, of parts that it specifies.
  pub(crate) const FIXED: Self =
    Self { fixed: true, ordered: true, size: true, deep: true, init: false };

  /// A layout of which Rust specifies nothing but the size, if `size`.
  const fn opaque(size: bool, init: bool) -> Self {
    Self { fixed: false, ordered: false, size, deep: false, init: size && init }
  }

  /// Of a node whose own offsets are computed from the sizes and the alignments of its `parts`
  /// (`repr(C)`, an array...): fixed if their sizes are.
  const fn of_parts(parts: &[Field]) -> Self {
    let (mut size, mut deep, mut i) = (true, true, 0);
    while i < parts.len() {
      size &= parts[i].layout.stable.size;
      deep &= parts[i].layout.stable.deep;
      i += 1;
    }
    Self { fixed: size, ordered: size, size, deep: size && deep, init: false }
  }

  /// Of an enum of a representation that Rust specifies, or of a union: its variants are
  /// `repr(C)` structs, whose offsets are computed from the sizes of their fields.
  const fn of_variants(variants: &[Variant]) -> Self {
    let (mut size, mut deep, mut i) = (true, true, 0);
    while i < variants.len() {
      let fields = variants[i].layout.stable;
      size &= fields.size;
      deep &= fields.deep;
      i += 1;
    }
    Self { fixed: size, ordered: size, size, deep: size && deep, init: false }
  }

  /// Of a node of one part (the element of an array), as [`Stable::of_parts`].
  const fn of_part(part: &Layout) -> Self {
    let size = part.stable.size;
    Self { fixed: size, ordered: size, size, deep: size && part.stable.deep, init: false }
  }
}

/// The identity of a type, which tells two types apart where their layouts may be the same.
///
/// A type has one layout in a program, but two types, however similar (`u8` and `i8`, two
/// structs of the same fields), may be arranged otherwise (`core::mem::offset_of!`, "Layout is
/// subject to change"). Stable Rust does not compare types in `const`: this is a name, and the
/// identities of the parameters that make the type.
///
/// [`Id::NONE`] is the identity of a type that has none (a generic type that derives `Pun`, a
/// `Scalar` implemented by hand): it is equal to none, not even itself. Where two types have
/// the same identity, the analysis still requires their layouts to be the same: an identity
/// only ever refuses.
#[doc(hidden)]
#[derive(Copy, Clone, Debug)]
pub struct Id {
  name: &'static str,
  args: &'static [Id],
  len: usize,
}

impl Id {
  /// No identity.
  pub const NONE: Self = Self { name: "", args: &[], len: 0 };

  /// The type `name`, of the types whose identities are `args`.
  #[doc(hidden)]
  #[must_use]
  pub const fn new(name: &'static str, args: &'static [Id]) -> Self {
    Self { name, args, len: 0 }
  }

  /// The type `name` of a length (an array).
  #[doc(hidden)]
  #[must_use]
  pub const fn sized(name: &'static str, args: &'static [Id], len: usize) -> Self {
    Self { name, args, len }
  }

  /// Whatever the type has an identity: it and its parameters have names.
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

  /// Whatever `self` and `other` are the identity of the same type.
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

#[derive(Copy, Clone, Debug)]
pub(crate) enum Kind {
  Scalar(Spec),
  Array(&'static Layout, usize),
  Slice(&'static Layout),
  Struct(&'static [Field], &'static [Rule]),
  /// A tail of bytes that a [`Dfa`] accepts.
  Pattern(&'static Dfa),
  /// A tag, and the fields of the variant that the tag tells.
  Enum(&'static Enum),
  /// A self-relative pointer, see [`Pointer`].
  Pointer(Pointer),
}

/// What a self-relative pointer aims at, elsewhere in the buffer that holds it.
///
/// ```
/// use puns::layout::{Aim, Endian, Pointer};
///
/// let list = Pointer::sequence::<u32>("my crate", 4, Endian::Little, 8);
/// assert_eq!(list.aim(), Aim::Sequence { align: 8 });
/// assert_eq!(Pointer::text("my crate", 4, Endian::Little).aim(), Aim::Text);
/// ```
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Aim {
  /// One value of the target type, at an address that is aligned for it.
  One,
  /// Values of the target type one after the other, `size_of` apart, the first at an address
  /// that is a multiple of `align`. How many there are is told by the bytes of the buffer, as
  /// the scheme of the pointer says.
  Sequence {
    /// The alignment of the first element that the declarer of the pointer guarantees: a power
    /// of two, at least the alignment of the element type.
    align: usize,
  },
  /// Bytes that are UTF-8. How many there are is told by the bytes of the buffer, as the scheme
  /// of the pointer says.
  Text,
}

/// A self-relative pointer: an offset from its own address to its target, in the same buffer.
///
/// This is what the analysis needs to know of a pointer, and nothing of how the crate that
/// declares it keeps its targets ([`SelfRelative`](crate::SelfRelative) says how it is
/// declared). [`Pointer::one`], [`Pointer::sequence`] and [`Pointer::text`] make one, with:
///
/// - a **scheme**: a name that the declaring crate chooses, which stands for the way a pointer
///   and the bytes of its buffer tell where its target is, and how long it is. A pointer is only
///   read as a pointer of the same scheme;
/// - its **offset**: the width in bytes of the integer (the size of the pointer) and its byte
///   order;
/// - what it **aims** at ([`Aim`]), with the size and the alignment of the target type (of an
///   element, for a sequence);
/// - whatever it is **nullable** ([`Pointer::or_null`]): an offset whose bytes are all zero is
///   then no pointer.
///
/// What is at the target is not part of the layout (the layout of a recursive type would have no
/// end): it is the [`Pun::Pointee`](crate::Pun::Pointee) of the type, which the analysis reaches
/// through types (see `deep.rs`).
///
/// What a pointer guarantees is not a property of its bytes. So no bytes ever prove one (only a
/// pointer does, whose target can be read as the target of the other), no check makes up for
/// one, and no `&mut` to one is ever handed out.
///
/// ```
/// use puns::layout::{Aim, Endian, Pointer};
///
/// // An offset of 4 bytes, little-endian, to a `u64`.
/// const NEXT: Pointer = Pointer::one::<u64>("my crate: an offset", 4, Endian::Little);
///
/// assert_eq!(NEXT.scheme(), "my crate: an offset");
/// assert_eq!((NEXT.width(), NEXT.endian(), NEXT.aim()), (4, Endian::Little, Aim::One));
/// assert_eq!((NEXT.size(), NEXT.align()), (size_of::<u64>(), align_of::<u64>()));
/// assert!(!NEXT.is_nullable() && NEXT.or_null().is_nullable());
/// ```
#[derive(Copy, Clone, Debug)]
pub struct Pointer {
  pub(crate) scheme: &'static str,
  pub(crate) aim: Aim,
  /// An offset whose bytes are all zero is no pointer.
  pub(crate) nullable: bool,
  /// The type of the pointer has no value whose bytes are all zero (its offset is a `NonZero`):
  /// found by [`Layout::pointer`], not told.
  pub(crate) nonzero: bool,
  /// Size of the offset, which is the size of the pointer: 1 to 16 bytes.
  pub(crate) width: usize,
  /// Byte order of the offset.
  pub(crate) endian: Endian,
  /// Size and alignment of the target type (`One`) or of an element (`Sequence`); those of
  /// `()` for `Text`.
  pub(crate) size: usize,
  pub(crate) align: usize,
}

impl Pointer {
  const fn new(
    scheme: &'static str,
    aim: Aim,
    width: usize,
    endian: Endian,
    (size, align): (usize, usize),
  ) -> Self {
    assert!(width >= 1 && width <= 16, "the offset of a pointer is 1 to 16 bytes");
    Self { scheme, aim, nullable: false, nonzero: false, width, endian, size, align }
  }

  /// The size and the alignment of `T`, which a pointer aims at.
  const fn target<T>() -> (usize, usize) {
    // A target without size could be anywhere, the pointer itself included, and is nothing that
    // a pointer can be read as: the proof stops at it (`deep::behind`).
    assert!(size_of::<T>() != 0, "a pointer aims at a type with a size");
    (size_of::<T>(), align_of::<T>())
  }

  /// A pointer of the scheme `scheme` to one `T`, whose offset is an integer of `width` bytes in
  /// the byte order `endian`.
  ///
  /// ```
  /// use puns::layout::{Aim, Endian, Pointer};
  ///
  /// let pointer = Pointer::one::<[u16; 3]>("my crate", 2, Endian::Big);
  /// assert_eq!(pointer.aim(), Aim::One);
  /// assert_eq!((pointer.size(), pointer.align()), (size_of::<[u16; 3]>(), align_of::<u16>()));
  /// ```
  ///
  /// # Panics
  /// If `width` is not 1 to 16, or if `T` has no size.
  #[must_use]
  pub const fn one<T>(scheme: &'static str, width: usize, endian: Endian) -> Self {
    Self::new(scheme, Aim::One, width, endian, Self::target::<T>())
  }

  /// A pointer of the scheme `scheme` to a sequence of `T`, the first of which is at an address
  /// that is a multiple of `align`, whose offset is an integer of `width` bytes in the byte
  /// order `endian`.
  ///
  /// ```
  /// use puns::layout::{Aim, Endian, Pointer};
  ///
  /// // Elements of 2 bytes, the first at a multiple of 4.
  /// let pointer = Pointer::sequence::<u16>("my crate", 4, Endian::Little, 4);
  /// assert_eq!(pointer.aim(), Aim::Sequence { align: 4 });
  /// assert_eq!(pointer.size(), size_of::<u16>());
  /// ```
  ///
  /// ```should_panic
  /// use puns::layout::{Endian, Pointer};
  ///
  /// // Not a power of two.
  /// let _ = Pointer::sequence::<u8>("my crate", 4, Endian::Little, 3);
  /// ```
  ///
  /// # Panics
  /// If `width` is not 1 to 16, if `T` has no size, or if `align` is not a power of two at least
  /// as large as the alignment of `T`.
  #[must_use]
  pub const fn sequence<T>(
    scheme: &'static str,
    width: usize,
    endian: Endian,
    align: usize,
  ) -> Self {
    assert!(
      align.is_power_of_two() && align >= align_of::<T>(),
      "the elements of a sequence are at an address aligned for them: a power of two, at least \
       the alignment of their type"
    );
    Self::new(scheme, Aim::Sequence { align }, width, endian, Self::target::<T>())
  }

  /// A pointer of the scheme `scheme` to text, whose offset is an integer of `width` bytes in the
  /// byte order `endian`. Its target type is `()`: there is nothing to relate behind it.
  ///
  /// ```
  /// use puns::layout::{Aim, Endian, Pointer};
  ///
  /// let pointer = Pointer::text("my crate", 4, Endian::Little);
  /// assert_eq!((pointer.aim(), pointer.size(), pointer.align()), (Aim::Text, 0, 1));
  /// ```
  ///
  /// # Panics
  /// If `width` is not 1 to 16.
  #[must_use]
  pub const fn text(scheme: &'static str, width: usize, endian: Endian) -> Self {
    Self::new(scheme, Aim::Text, width, endian, (size_of::<()>(), align_of::<()>()))
  }

  /// The same pointer, but nullable: an offset whose bytes are all zero is no pointer, and aims
  /// at nothing.
  ///
  /// ```
  /// use puns::layout::{Endian, Pointer};
  ///
  /// let pointer = Pointer::one::<u32>("my crate", 4, Endian::Little).or_null();
  /// assert!(pointer.is_nullable());
  /// assert_eq!(pointer.size(), size_of::<u32>());
  /// ```
  #[must_use]
  pub const fn or_null(self) -> Self {
    Self { nullable: true, nonzero: false, ..self }
  }

  /// Whatever an offset whose bytes are all zero is no pointer ([`Pointer::or_null`]).
  ///
  /// ```
  /// use puns::layout::{Endian, Pointer};
  ///
  /// let pointer = Pointer::one::<u32>("my crate", 4, Endian::Little);
  /// assert!(!pointer.is_nullable());
  /// assert!(pointer.or_null().is_nullable());
  /// ```
  #[must_use]
  pub const fn is_nullable(&self) -> bool {
    self.nullable
  }

  /// The scheme: the name that the declaring crate gave the way to the target.
  ///
  /// ```
  /// use puns::layout::{Endian, Pointer};
  ///
  /// let pointer = Pointer::text("my crate: a length before the text", 4, Endian::Little);
  /// assert_eq!(pointer.scheme(), "my crate: a length before the text");
  /// ```
  #[must_use]
  pub const fn scheme(&self) -> &'static str {
    self.scheme
  }

  /// What the pointer aims at: one value, a sequence (and the alignment that the declarer
  /// guarantees of its first element), or text.
  ///
  /// ```
  /// use puns::layout::{Aim, Endian, Pointer};
  ///
  /// let pointer = Pointer::sequence::<u8>("my crate", 4, Endian::Little, 16);
  /// assert_eq!(pointer.aim(), Aim::Sequence { align: 16 });
  /// ```
  #[must_use]
  pub const fn aim(&self) -> Aim {
    self.aim
  }

  /// The size of the offset in bytes, which is the size of the pointer.
  ///
  /// ```
  /// use puns::layout::{Endian, Pointer};
  ///
  /// assert_eq!(Pointer::one::<u64>("my crate", 2, Endian::Little).width(), 2);
  /// ```
  #[must_use]
  pub const fn width(&self) -> usize {
    self.width
  }

  /// The byte order of the offset.
  ///
  /// ```
  /// use puns::layout::{Endian, Pointer};
  ///
  /// assert_eq!(Pointer::one::<u64>("my crate", 4, Endian::Big).endian(), Endian::Big);
  /// ```
  #[must_use]
  pub const fn endian(&self) -> Endian {
    self.endian
  }

  /// The size of the target type (of an element of a sequence; 0 for text).
  ///
  /// ```
  /// use puns::layout::{Endian, Pointer};
  ///
  /// let pointer = Pointer::sequence::<[u8; 3]>("my crate", 4, Endian::Little, 1);
  /// assert_eq!(pointer.size(), 3);
  /// ```
  #[must_use]
  pub const fn size(&self) -> usize {
    self.size
  }

  /// The alignment of the target type (of an element of a sequence; 1 for text).
  ///
  /// ```
  /// use puns::layout::{Endian, Pointer};
  ///
  /// // Not the alignment that the declarer guarantees of the first element: see `aim`.
  /// let pointer = Pointer::sequence::<u16>("my crate", 4, Endian::Little, 8);
  /// assert_eq!(pointer.align(), align_of::<u16>());
  /// ```
  #[must_use]
  pub const fn align(&self) -> usize {
    self.align
  }

  /// Whatever the target of the pointer `self` can be read through the pointer `to`, as far as
  /// the pointers themselves go (what the targets hold is related through the types, see
  /// `deep.rs`): `None` if never, `Some(false)` if only when `self` is not null, which is then to
  /// be checked at run time.
  ///
  /// Both are of the same scheme, with the same offset (width and byte order: one is read as the
  /// other), so they find the same target in the same buffer. A value is read as a prefix of it
  /// that needs no more alignment than it has; a sequence as a sequence of elements of the same
  /// size (its length is a number of elements) that needs no more alignment than the declarer of
  /// `self` guarantees; text as text.
  pub(crate) const fn reads_as(&self, to: &Pointer) -> Option<bool> {
    let fits = str_equals(self.scheme, to.scheme)
      && self.width == to.width
      && self.endian.equals(to.endian)
      && match (self.aim, to.aim) {
        (Aim::One, Aim::One) => to.size <= self.size && to.align <= self.align,
        (Aim::Sequence { align: from }, Aim::Sequence { align: into }) => {
          to.size == self.size && into <= from
        }
        (Aim::Text, Aim::Text) => true,
        _ => false,
      };
    // What may be zero (a null, or an offset of zero of a type that has one) is read as what may
    // not be only when it is not, which is then checked at run time.
    let zero = self.nullable || !self.nonzero;
    if fits { Some(to.nullable || !to.nonzero && !self.nullable || !zero) } else { None }
  }

  /// Whatever two pointers are the same, as far as the pointers themselves go.
  const fn same(&self, other: &Pointer) -> bool {
    let aims = match (self.aim, other.aim) {
      (Aim::One, Aim::One) | (Aim::Text, Aim::Text) => true,
      (Aim::Sequence { align: a }, Aim::Sequence { align: b }) => a == b,
      _ => false,
    };
    aims
      && str_equals(self.scheme, other.scheme)
      && self.nullable == other.nullable
      && self.nonzero == other.nonzero
      && self.width == other.width
      && self.endian.equals(other.endian)
      && self.size == other.size
      && self.align == other.align
  }
}

/// Whatever two strings are the same, in `const`.
const fn str_equals(a: &str, b: &str) -> bool {
  let (a, b) = (a.as_bytes(), b.as_bytes());
  if a.len() != b.len() {
    return false;
  }
  let mut i = 0;
  while i < a.len() {
    if a[i] != b[i] {
      return false;
    }
    i += 1;
  }
  true
}

/// Layout of an enum with fields, see [`Layout::enumeration`].
///
/// The tag is a scalar at a fixed place: of the representation that Rust specifies, or of the one
/// that `rustc` chose, as `discover.rs` found it. In the second case it may be a niche: a scalar
/// of the fields of one variant (the one that holds the tag), whose other values are the
/// other variants.
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
  /// A scalar whose valid values are the tags of the variants.
  pub(crate) tag: Field,
  pub(crate) variants: &'static [Variant],
  /// Size and alignment that the representation of the enum specifies.
  pub(crate) specified: (usize, usize),
  /// A union: no tag, its variants are its fields, and a value is valid if it is one of them.
  pub(crate) union: bool,
}

impl Enum {
  /// The enum that `repr(C)` and `repr(u8)` (or any other integer) specify: a tag, which is a
  /// scalar whose valid values are the tags of the `variants`, then one of the `shadows`, the
  /// `repr(C)` structs of the fields of each variant, of which the sizes and alignments are
  /// given.
  ///
  /// With `repr(C)`, the tag is `outside`: the fields of a variant are at the offset of the
  /// shadows (`fields`) plus their offset in the shadow of the variant. Without, the tag is the
  /// first field of each shadow.
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
  ///
  /// # Panics
  /// If `fields` is not where the fields of the variants are.
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

  /// The enum that `rustc` laid out, as `discover.rs` found it: a tag at `offset`, and the
  /// `variants` in the order of their declarations, of an enum of size and alignment `specified`.
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

  /// The `repr(C)` union whose `fields` are given, each as a variant of one field (at offset 0),
  /// in the order of their declarations (see [`Layout::union`]).
  #[doc(hidden)]
  #[must_use]
  pub const fn union(fields: &'static [Variant]) -> Self {
    Self { tag: Field::new(0, &NO_TAG), variants: fields, specified: (0, 1), union: true }
  }

  /// An enum without a variant that a value can be: no value.
  const NEVER: Self =
    Self { tag: Field::new(0, &NO_TAG), variants: &[], specified: (0, 1), union: false };

  /// Offset of the fields of the variants of a `repr(C)` enum, whose tag is outside of the
  /// `shadows` (see [`Enum::new`]).
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

/// A variant of an enum with fields, see [`Layout::enumeration`].
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
  /// Value of the tag; with `ranges`, what tells the variant from the others in the lists of
  /// types (its index, see `deep.rs`).
  pub(crate) tag: u128,
  /// The values of the tag of this variant, sorted, if it is not `tag` alone: several for the
  /// variant of a niche, none for a variant that no value can be (it holds an uninhabited type).
  pub(crate) ranges: Option<&'static [(u128, u128)]>,
  /// The tag is in the fields of this variant: a niche.
  pub(crate) holds_tag: bool,
  /// The enum when it is this variant: a struct of its fields, in which the tag is padding (but
  /// for the variant that holds it).
  pub(crate) layout: &'static Layout,
}

/// The values of the tag of a variant, one range at a time.
#[derive(Copy, Clone)]
pub(crate) struct Tags {
  one: (u128, u128),
  ranges: Option<&'static [(u128, u128)]>,
}

impl Tags {
  /// The values `lo..=hi`.
  pub(crate) const fn range(lo: u128, hi: u128) -> Self {
    Self { one: (lo, hi), ranges: None }
  }

  /// The values of `ranges`, sorted.
  pub(crate) const fn of(ranges: &'static [(u128, u128)]) -> Self {
    Self { one: (0, 0), ranges: Some(ranges) }
  }

  pub(crate) const fn len(&self) -> usize {
    match self.ranges {
      Some(ranges) => ranges.len(),
      None => 1,
    }
  }

  pub(crate) const fn get(&self, index: usize) -> (u128, u128) {
    match self.ranges {
      Some(ranges) => ranges[index],
      None => self.one,
    }
  }

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

  /// Whatever a value of `self` is one of `other`.
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

  /// Whatever every value of `self` is one of `other`.
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
  /// - `tag` is what the tag of the enum is for this variant, as an integer of native endianness,
  /// - `layout` is the [`Layout::variant`] of its fields.
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
  /// // Checked at compile time: the tags are values of the tag of the enum, and its own.
  /// assert_eq!(LAYOUT.size(), size_of::<Shape>());
  /// ```
  #[must_use]
  pub const fn new(tag: u128, layout: &'static Layout) -> Self {
    Self { tag, ranges: None, holds_tag: false, layout }
  }

  /// The `index`th variant of an enum that `discover.rs` found: its tag is one of `ranges`
  /// (sorted, disjoint, none if no value can be this variant), and if it `holds_tag`, the tag is
  /// in its fields (a niche).
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

  /// The values of the tag of this variant.
  pub(crate) const fn tags(&self) -> Tags {
    Tags { one: (self.tag, self.tag), ranges: self.ranges }
  }

  /// Whatever a value can be this variant: a value of the tag is, and its fields can be.
  pub(crate) const fn is_present(&self) -> bool {
    !matches!(self.ranges, Some(ranges) if ranges.is_empty()) && self.layout.is_inhabited()
  }

  /// Fields, by offset from the start of the enum.
  pub(crate) const fn fields(&self) -> &'static [Field] {
    let Kind::Struct(fields, _) = self.layout.kind else { panic!("not a variant") };
    fields
  }
}

/// A field of a struct, see [`Layout::structure`].
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
/// // SAFETY: `Pair` is sized.
/// const PAIR: Layout = unsafe { Layout::structure::<Pair>(core::ptr::null(), &FIELDS, &[]) };
///
/// assert_eq!((PAIR.size(), PAIR.align()), (size_of::<Pair>(), align_of::<Pair>()));
/// ```
#[derive(Copy, Clone, Debug)]
pub struct Field {
  pub(crate) offset: usize,
  pub(crate) layout: &'static Layout,
  /// Declaration index.
  pub(crate) index: usize,
  /// Number of constrained scalars declared before this field in the struct.
  pub(crate) leaf: usize,
  /// The layout is a scalar that accepts less than the type of the field does.
  pub(crate) restricted: bool,
}

impl Field {
  /// A field at `offset` in its struct (in its enum, for the field of a variant), of the layout
  /// `layout`.
  ///
  /// ```
  /// use puns::layout::{Field, Layout, Spec};
  ///
  /// #[repr(C)]
  /// struct Wrapper(u32);
  ///
  /// const VALUE: Layout = Layout::scalar::<u32>(Spec::any::<u32>());
  /// const FIELDS: [Field; 1] = [Field::new(0, &VALUE)];
  /// // SAFETY: `Wrapper` is sized.
  /// const WRAPPER: Layout =
  ///   unsafe { Layout::structure::<Wrapper>(core::ptr::null(), &FIELDS, &[]) };
  ///
  /// // Every byte is the field's: plain data, which nothing is checked of.
  /// assert!(WRAPPER.is_plain() && WRAPPER.is_free());
  /// ```
  #[inline]
  #[must_use]
  pub const fn new(offset: usize, layout: &'static Layout) -> Self {
    Self { offset, layout, index: 0, leaf: 0, restricted: false }
  }

  /// A scalar field that only accepts some of the values of its type, see
  /// [`Layout::restricted`]: what `#[pun(valid(..))]` makes of a field.
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
  /// // SAFETY: `Version` is sized.
  /// const VERSION: Layout =
  ///   unsafe { Layout::structure::<Version>(core::ptr::null(), &FIELDS, &[]) };
  ///
  /// // Its byte is checked where a `Version` is made.
  /// assert!(!VERSION.is_free());
  /// ```
  ///
  /// # Panics
  /// If `layout` is not a scalar.
  #[inline]
  #[must_use]
  pub const fn restricted(offset: usize, layout: &'static Layout) -> Self {
    assert!(matches!(layout.kind, Kind::Scalar(_)), "only scalar fields can be restricted");
    Self { offset, layout, index: 0, leaf: 0, restricted: true }
  }

  /// The specification of a restricted field.
  #[doc(hidden)]
  #[must_use]
  pub const fn restriction(&self) -> Option<Spec> {
    match self.layout.kind {
      Kind::Scalar(spec) if self.restricted => Some(spec),
      _ => None,
    }
  }

  #[doc(hidden)]
  #[must_use]
  pub const fn offset(&self) -> usize {
    self.offset
  }

  #[doc(hidden)]
  #[must_use]
  pub const fn leaf(&self) -> usize {
    self.leaf
  }
}

/// Memory layout and valid bit patterns of a type.
///
/// Every [`Pun`](crate::Pun) type has one, which `#[derive(Pun)]` makes with the functions
/// below; the analysis of the crate compares them at compile time.
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
  pub(crate) kind: Kind,
  /// Size of the type if it is `Sized`, otherwise the offset of the first element of its tail
  /// slice.
  pub(crate) size: usize,
  pub(crate) align: usize,
  /// Size of an element of the tail slice, `0` if the type is `Sized`.
  pub(crate) tail: usize,
  pub(crate) unsized_: bool,
  /// Number of things that validation may have to check: constrained scalars, patterns, and
  /// structs with rules (array and slice elements are counted once).
  pub(crate) leaves: usize,
  pub(crate) uniform: Uniform,
  /// Whatever a struct of the layout has rules.
  pub(crate) has_rules: bool,
  /// What it keeps in every layout that Rust allows for its type.
  pub(crate) stable: Stable,
  /// The identity of its type.
  pub(crate) id: Id,
}

/// What [`validate`](crate::validate) has to do for a type.
#[derive(Copy, Clone)]
pub(crate) enum Shape {
  Scalar(Spec),
  Array(usize),
  Slice,
  Struct,
  Pattern(&'static Dfa),
  Enum(&'static Enum),
  Pointer(Pointer),
}

/// Round `value` up to a multiple of `align` (a power of two).
#[inline]
pub(crate) const fn round_up(value: usize, align: usize) -> Option<usize> {
  match value.checked_add(align - 1) {
    Some(v) => Some(v & !(align - 1)),
    None => None,
  }
}

/// Offset of the last field of a `repr(C)` struct.
///
/// - `end` is the end offset of the field that precedes it (`0` if there is none),
/// - `align` is the alignment of its type,
/// - `pack` is the `N` of `repr(packed(N))`, or `usize::MAX`.
#[doc(hidden)]
#[must_use]
pub const fn tail_offset(end: usize, align: usize, pack: usize) -> usize {
  match round_up(end, if align < pack { align } else { pack }) {
    Some(offset) => offset,
    None => panic!("struct is too large"),
  }
}

/// Whatever the type `P`, the offset of a self-relative pointer, has no value whose bytes are all
/// zero: `Option<P>` has the size of `P` (a niche in it), with `None` all zeros.
///
/// # Panics
/// If `Option<P>` has the size of `P` with a `None` that is not zeros: a niche that is not zero,
/// which tells of values of the offset that are no pointer, and which is not supported.
const fn excludes_zero<P>() -> bool {
  if size_of::<Option<P>>() != size_of::<P>() {
    return false;
  }
  let none = mem::MaybeUninit::new(None::<P>);
  let mut i = 0;
  while i < size_of::<P>() {
    // SAFETY: in bounds. A byte of `None` that is not initialized fails the build (R2 of
    // `discover.rs`).
    let byte = unsafe { *none.as_ptr().cast::<u8>().add(i) };
    assert!(
      byte == 0,
      "the offset of a self-relative pointer excludes values other than zero (a niche that is not \
       zero): not supported, nothing tells which"
    );
    i += 1;
  }
  true
}

/// The tag of an enum that no value can be.
const NO_TAG: Layout = Layout::scalar::<()>(Spec::any::<()>());

/// What [`Layout::PIN`] is made of.
fn pin<T>(_: T) {}

/// Whether the `unstable` feature is on: `puns` then takes the layout that `rustc` chose, as it is.
///
/// Without it, nothing that `puns` does depends on what Rust lets the compiler choose otherwise
/// (in its next version, or with `-Zrandomize-layout`): the order of the fields of a struct or of
/// a tuple of the `Rust` representation, where the tag of an enum without `repr` is and its values,
/// the niches of `Option` and `Result` but where `core` documents them. A pun compiles only if
/// what it does, what it checks and which values it accepts are the same in every layout that
/// Rust allows. With it, what compiles without it compiles and does the same, and more compiles.
/// See "What Rust keeps of a layout" in `DESIGN.md`.
pub const UNSTABLE: bool = cfg!(feature = "unstable");

/// What a function of [`Visit`](crate::Visit) says, at compile time, of a type of which something
/// that it would check, or a pointer, is at a place that Rust does not keep.
pub(crate) const VISIT_UNSPECIFIED: &str = "the type has a part whose layout Rust does not keep \
  from one compilation to the next (a struct or a tuple of the `Rust` representation, an enum \
  without `repr`), and something to check or a pointer in it, which would be at another place \
  with another compiler.\nAdd `#[repr(C)]` (or `#[repr(u8)]` to an enum), or enable the \
  `unstable` feature of `puns`";

impl Layout {
  /// The types that the code that `#[derive(Pun)]` generates names by their path in the crate
  /// it is told of (`#[pun(crate = ..)]`), as they must be: the functions that it calls are
  /// associated functions of these types, or of `Layout` and `Spec`, which the traits that it
  /// implements name. A path that leads to other types, with functions of the same name, does
  /// not compile (`fn` types are not paths, nothing can stand for them).
  #[doc(hidden)]
  #[expect(
    clippy::type_complexity,
    reason = "the derive writes this tuple of `fn` types out, with the paths it is told"
  )]
  pub const PIN: (fn(Rule), fn(Op), fn(Field), fn(Enum), fn(Variant)) =
    (pin::<Rule>, pin::<Op>, pin::<Field>, pin::<Enum>, pin::<Variant>);

  /// An uninitialized `T`, whose fields `#[derive(Pun)]` finds the offsets of.
  #[doc(hidden)]
  #[must_use]
  pub const fn uninit<T>() -> mem::MaybeUninit<T> {
    mem::MaybeUninit::uninit()
  }

  /// The layout of the enum `E` without `repr`, found by experiments on a sample of each of its
  /// `V` variants (anything for a variant that has no value) and its `fields`, observed on the
  /// sample: see `discover.rs`. For `#[derive(Pun)]`.
  #[doc(hidden)]
  #[must_use]
  pub const fn discover<E, const V: usize>(
    samples: &[mem::MaybeUninit<E>; V],
    fields: &[&[Field]; V],
  ) -> Desc<V> {
    crate::discover::discover(samples, fields)
  }

  /// A valid `T`, made of nothing but its layout (every scalar the least of its values), for the
  /// samples of the variants of an enum without `repr`.
  #[doc(hidden)]
  #[must_use]
  pub const fn sample<T: crate::Pun>() -> mem::MaybeUninit<T> {
    crate::discover::sample::<T>()
  }

  /// `value`, as a sample (see [`Layout::discover`]).
  #[doc(hidden)]
  #[must_use]
  pub const fn wrap<T>(value: T) -> mem::MaybeUninit<T> {
    mem::MaybeUninit::new(value)
  }

  /// Whatever a value with fields of these layouts can be made.
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

  /// The layout of a `repr(C)` struct whose fields have the given sizes and alignments, after
  /// what `start` (an end offset and an alignment) says is before them: the offsets of the
  /// fields, and the size and alignment of the struct. For the variants of enums with fields,
  /// whose layout RFC 2195 specifies in these terms.
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

  /// Size and alignment of `T`, for `#[derive(Pun)]`.
  #[doc(hidden)]
  #[must_use]
  pub const fn size_align<T>() -> (usize, usize) {
    (size_of::<T>(), align_of::<T>())
  }

  /// [`tail_offset`] in a struct that is not packed, for `#[derive(Pun)]`.
  #[doc(hidden)]
  #[must_use]
  pub const fn tail_offset(end: usize, align: usize) -> usize {
    tail_offset(end, align, usize::MAX)
  }

  /// [`tail_offset`] in a `repr(packed(pack))` struct, for `#[derive(Pun)]`.
  #[doc(hidden)]
  #[must_use]
  pub const fn tail_offset_packed(end: usize, align: usize, pack: usize) -> usize {
    tail_offset(end, align, pack)
  }

  /// Layout of a scalar.
  ///
  /// ```
  /// use puns::layout::{Endian, Layout, Spec};
  ///
  /// const HALF: Layout =
  ///   Layout::scalar::<u16>(Spec::ranges::<u16>(Endian::NATIVE, &[(0, 0x7fff)]));
  /// assert_eq!((HALF.size(), HALF.align()), (size_of::<u16>(), align_of::<u16>()));
  /// // Some values are not one: they are checked.
  /// assert!(!HALF.is_free());
  /// ```
  ///
  /// # Panics
  /// If `spec` does not have the size of `T`.
  #[must_use]
  pub const fn scalar<T>(spec: Spec) -> Self {
    assert!(spec.size == size_of::<T>(), "`Spec` size mismatch");
    Self {
      kind: Kind::Scalar(spec),
      size: spec.size,
      align: align_of::<T>(),
      tail: 0,
      unsized_: false,
      leaves: spec.is_constrained() as usize,
      uniform: match spec.valid {
        Valid::Uninit => Uniform::Pad,
        Valid::Any => Uniform::Any,
        Valid::In(_) => Uniform::No,
      },
      has_rules: false,
      stable: Stable::FIXED,
      id: Id::NONE,
    }
  }

  /// Layout of the tag of an enum that `discover.rs` found: a scalar of no type of its own, so of
  /// no alignment.
  pub(crate) const fn tag(spec: Spec) -> Self {
    Self {
      kind: Kind::Scalar(spec),
      size: spec.size,
      align: 1,
      tail: 0,
      unsized_: false,
      leaves: spec.is_constrained() as usize,
      uniform: match spec.valid {
        Valid::Uninit => Uniform::Pad,
        Valid::Any => Uniform::Any,
        Valid::In(_) => Uniform::No,
      },
      has_rules: false,
      stable: Stable::FIXED,
      id: Id::NONE,
    }
  }

  /// Same scalar, that only accepts the values of `ranges` (see [`Spec::ranges`]), which must
  /// all be valid for the scalar.
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
  ///
  /// # Panics
  /// If `self` is not an initialized scalar, or if `ranges` are not valid values of it (see
  /// [`Spec::with_ranges`]).
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

  /// Layout of an unsized type made of bytes that `dfa` accepts, such as `str`.
  ///
  /// # Safety
  /// `probe` must be a pointer to a type whose pointers have a slice length, which is
  /// `PROBE_LEN` (see `Pun::PROBE`).
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
  /// // SAFETY: the probe of a slice of bytes, as a pointer to `Digits`, which is one.
  /// const DIGITS: Layout =
  ///   unsafe { Layout::pattern::<Digits>(<[u8] as Pun>::PROBE as *const Digits, Dfa::DIGITS) };
  ///
  /// // Unsized: its size is where its bytes start.
  /// assert_eq!((DIGITS.size(), DIGITS.align()), (0, 1));
  /// assert!(!DIGITS.is_free());
  /// ```
  ///
  /// # Panics
  /// If `probe` is not a pointer to bytes, or if `dfa` is not well formed.
  pub const unsafe fn pattern<T: ?Sized>(probe: *const T, dfa: &'static Dfa) -> Self {
    // SAFETY: the caller gives the probe of a type whose pointers have a slice length.
    unsafe { ptr::verify::<T>(probe) };
    // SAFETY: the metadata of `probe` is a small slice length.
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
      has_rules: false,
      stable: Stable::FIXED,
      id: Id::NONE,
    }
  }

  /// Layout of a type the size and alignment of `T` whose bytes are all padding.
  ///
  /// This is the layout of `MaybeUninit<T>`.
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
      has_rules: false,
      stable: Stable::FIXED,
      id: Id::NONE,
    }
  }

  /// Layout of the self-relative pointer `P`, which is the offset of `pointer` and nothing else,
  /// to a `T` (an element of a sequence, `()` for text), for [`self_relative!`](crate::self_relative).
  ///
  /// # Panics
  /// If `P` does not have the size of the offset, or if `pointer` was not made for a `T`.
  #[doc(hidden)]
  #[must_use]
  pub const fn pointer<P, T>(pointer: Pointer) -> Self {
    assert!(size_of::<P>() == pointer.width, "a self-relative pointer is its offset");
    // Whatever the type has no value of zeros: `Option` of it has its size, `None` being zeros (a
    // `NonZero`). A niche elsewhere would tell of values of the offset that are no pointer, which
    // nothing here can name: refused.
    let nonzero = excludes_zero::<P>();
    assert!(
      !(nonzero && pointer.nullable),
      "a self-relative pointer that may be null has a value of zeros: its offset is no `NonZero`"
    );
    let pointer = Pointer { nonzero, ..pointer };
    assert!(
      size_of::<T>() == pointer.size && align_of::<T>() == pointer.align,
      "the `POINTER` of a `SelfRelative` type is made for its `Target`"
    );
    Self {
      kind: Kind::Pointer(pointer),
      size: pointer.width,
      align: align_of::<P>(),
      tail: 0,
      unsized_: false,
      // A leaf: what validation may have to check of it is that it is not null.
      leaves: 1,
      uniform: Uniform::No,
      has_rules: false,
      stable: Stable::FIXED,
      id: Id::NONE,
    }
  }

  /// The layout of `Option<P>`, where `P` is a self-relative pointer whose `POINTER` is `pointer`
  /// and which aims at `T`: the same pointer, that may be null. [`self_relative!`](crate::self_relative)
  /// gives it as `Pun::OPTION`, by which `Option<P>` is that pointer.
  ///
  /// `Option<P>` is that when it has the size of `P` and `None` is all zero bytes, which is
  /// checked here. Then `rustc` puts `None` in a niche of the bytes of `P` (all of them are of
  /// its offset, initialized: S1 of [`SelfRelative`](crate::SelfRelative)), which is zeros, and
  /// leaves the bytes of `Some(p)` the ones of `p` (R1 of `discover.rs`), which are not all zero:
  /// a value of all zero bytes aims at nothing, any other is a pointer as `P` is (S5).
  ///
  /// ```
  /// use core::num::NonZero;
  ///
  /// use puns::layout::{Endian, Layout, Pointer};
  ///
  /// // An offset that is never zero, to a `u32`.
  /// const POINTER: Pointer = Pointer::one::<u32>("my crate", 4, Endian::NATIVE);
  /// const OPTION: Layout = Layout::nullable::<NonZero<i32>, u32>(POINTER);
  ///
  /// assert!(OPTION.as_pointer().unwrap().is_nullable());
  /// assert_eq!(OPTION.size(), size_of::<Option<NonZero<i32>>>());
  /// ```
  ///
  /// ```should_panic
  /// use puns::layout::{Endian, Layout, Pointer};
  ///
  /// // An `i32` has no niche: `Option<i32>` is larger.
  /// let pointer = Pointer::one::<u32>("my crate", 4, Endian::NATIVE);
  /// let _ = Layout::nullable::<i32, u32>(pointer);
  /// ```
  ///
  /// # Panics
  /// If `pointer` may be null already, if `Option<P>` is larger than `P` (an offset without a
  /// niche at zero, such as a byte array, has a tag of its own in `Option`; a `NonZero` has
  /// one), or if `None` is not zeros.
  #[must_use]
  pub const fn nullable<P, T>(pointer: Pointer) -> Self {
    assert!(!pointer.is_nullable(), "`Option` of a pointer that may be null is not a pointer");
    assert!(
      size_of::<Option<P>>() == size_of::<P>(),
      "`Option` of this self-relative pointer is larger than it: its offset has no niche at zero \
       (a `NonZero` has one), so `None` is not the pointer of zeros"
    );
    let none = mem::MaybeUninit::new(None::<P>);
    let mut i = 0;
    while i < size_of::<P>() {
      // SAFETY: in bounds. A byte of `None` that is not initialized fails the build (R2).
      let byte = unsafe { *none.as_ptr().cast::<u8>().add(i) };
      assert!(byte == 0, "`None` of this self-relative pointer is not zeros");
      i += 1;
    }
    Self::pointer::<Option<P>, T>(pointer.or_null())
  }

  /// The pointer that a value of the layout is, if it is a self-relative pointer (see
  /// [`Pointer`]); `None` for anything else, a struct that holds one included.
  ///
  /// ```
  /// use core::num::NonZero;
  ///
  /// use puns::layout::{Endian, Layout, Pointer, Spec};
  ///
  /// const POINTER: Pointer = Pointer::one::<u32>("my crate", 4, Endian::NATIVE);
  /// const OPTION: Layout = Layout::nullable::<NonZero<i32>, u32>(POINTER);
  ///
  /// assert_eq!(OPTION.as_pointer().map(|pointer| pointer.scheme()), Some("my crate"));
  /// assert!(Layout::scalar::<i32>(Spec::any::<i32>()).as_pointer().is_none());
  /// assert!(Layout::array(&OPTION, 2).as_pointer().is_none());
  /// ```
  #[must_use]
  pub const fn as_pointer(&self) -> Option<Pointer> {
    match self.kind {
      Kind::Pointer(pointer) => Some(pointer),
      _ => None,
    }
  }

  /// Whatever validation has nothing to check of a value of the layout: every bit pattern of its
  /// bytes (initialized) is a value, it holds no pointer, no enum and no rule. A value of it is
  /// made of any initialized bytes; [`validate_in`](crate::validate_in) reads nothing of it.
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

  /// Whatever a value of the layout is all its bytes: it holds no padding (no byte that may be
  /// uninitialized), and no pointer. Such a value can be copied as bytes, and its bytes are
  /// initialized. An enum with fields is never one (which bytes are padding depends on its tag),
  /// nor a slice or a pattern (unsized).
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
  /// // SAFETY: `Padded` is sized.
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
          // Sorted by offset: a field after the bytes covered so far leaves a hole.
          if fields[i].offset != covered || !fields[i].layout.is_plain() {
            return false;
          }
          covered += fields[i].layout.size;
          i += 1;
        }
        !self.unsized_ && covered == self.size
      }
      Kind::Slice(_) | Kind::Pattern(_) | Kind::Enum(_) | Kind::Pointer(_) => false,
    }
  }

  /// Whatever values of the layout hold a self-relative pointer (see [`Pointer`]), not counting
  /// what the pointers aim at.
  ///
  /// ```
  /// use core::num::NonZero;
  ///
  /// use puns::layout::{Endian, Layout, Pointer, Spec};
  ///
  /// const POINTER: Pointer = Pointer::one::<u32>("my crate", 4, Endian::NATIVE);
  /// const OPTION: Layout = Layout::nullable::<NonZero<i32>, u32>(POINTER);
  ///
  /// assert!(OPTION.has_pointers() && Layout::array(&OPTION, 2).has_pointers());
  /// assert!(!Layout::scalar::<i32>(Spec::any::<i32>()).has_pointers());
  /// ```
  #[must_use]
  pub const fn has_pointers(&self) -> bool {
    // A pointer is a leaf.
    if self.leaves == 0 {
      return false;
    }
    match self.kind {
      Kind::Pointer(_) => true,
      Kind::Scalar(_) | Kind::Pattern(_) => false,
      Kind::Array(elem, _) | Kind::Slice(elem) => elem.has_pointers(),
      Kind::Struct(fields, _) => {
        let mut i = 0;
        while i < fields.len() {
          if fields[i].layout.has_pointers() {
            return true;
          }
          i += 1;
        }
        false
      }
      Kind::Enum(layout) => {
        let mut i = 0;
        while i < layout.variants.len() {
          if layout.variants[i].layout.has_pointers() {
            return true;
          }
          i += 1;
        }
        false
      }
    }
  }

  /// Layout of `[T; len]`.
  ///
  /// ```
  /// use puns::layout::{Layout, Spec};
  ///
  /// const WORD: Layout = Layout::scalar::<u32>(Spec::any::<u32>());
  /// const WORDS: Layout = Layout::array(&WORD, 3);
  /// assert_eq!((WORDS.size(), WORDS.align()), (size_of::<[u32; 3]>(), align_of::<u32>()));
  /// ```
  ///
  /// # Panics
  /// If `elem` is unsized, or if the array is larger than `usize::MAX` bytes.
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
      has_rules: elem.has_rules,
      // Its elements are at multiples of their size.
      stable: Stable::of_part(elem),
      id: Id::NONE,
    }
  }

  /// Layout of `[T]`.
  ///
  /// ```
  /// use puns::{
  ///   Pun,
  ///   layout::{Layout, Spec},
  /// };
  ///
  /// const WORD: Layout = Layout::scalar::<u16>(Spec::any::<u16>());
  /// // SAFETY: the probe of a slice.
  /// const WORDS: Layout = unsafe { Layout::slice::<u16>(<[u16] as Pun>::PROBE, &WORD) };
  ///
  /// // Unsized: its size is where its elements start.
  /// assert_eq!((WORDS.size(), WORDS.align()), (0, align_of::<u16>()));
  /// assert!(!WORDS.is_plain());
  /// ```
  ///
  /// # Safety
  /// The length of `probe` must be `PROBE_LEN` (see `Pun::PROBE`).
  ///
  /// # Panics
  /// If `elem` is unsized or does not have the size of `T`.
  pub const unsafe fn slice<T>(probe: *const [T], elem: &'static Layout) -> Self {
    assert!(!elem.unsized_ && elem.size == size_of::<T>());
    // SAFETY: `[T]` is a slice.
    unsafe { ptr::verify::<[T]>(probe) };
    Self {
      kind: Kind::Slice(elem),
      size: 0,
      align: elem.align,
      tail: elem.size,
      unsized_: true,
      leaves: elem.leaves,
      uniform: elem.uniform,
      has_rules: elem.has_rules,
      stable: Stable::of_part(elem),
      id: Id::NONE,
    }
  }

  /// Layout of the struct `T`.
  ///
  /// - `probe` is `T::PROBE`,
  /// - `fields` are the fields of `T` as returned by [`Layout::sort_fields`],
  /// - `rules` are what values of `T` must satisfy beyond the validity of their fields.
  ///
  /// The size and alignment of `T` come from the compiler. If the last field is unsized, so is
  /// `T`, and the way the compiler sizes `T` is checked against the model used by this crate.
  ///
  /// # Safety
  /// If `T` is unsized, its tail must be a slice, and the slice length of `probe` must be
  /// `PROBE_LEN` (see `Pun::PROBE`).
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
  /// // SAFETY: `Range` is sized.
  /// const RANGE: Layout =
  ///   unsafe { Layout::structure::<Range>(core::ptr::null(), &FIELDS, &RULES) };
  ///
  /// assert_eq!(RANGE.size(), size_of::<Range>());
  /// // Any bytes are its fields, not its rule: it is checked.
  /// assert!(!RANGE.is_free());
  /// ```
  ///
  /// # Panics
  /// If the fields overlap, are not sorted or are out of the bounds of `T`, if a rule is
  /// malformed, or if `T` is not laid out as this crate models it.
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
        // SAFETY: `T` is `Sized` (none of its fields is unsized): `probe` is not even looked at.
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
        // SAFETY: the tail of `T` is a slice (the tail of its last field is).
        unsafe { ptr::verify::<T>(probe) };
        // SAFETY: the metadata of `probe` is a small slice length.
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
          has_rules,
          stable: Stable::FIXED,
          id: Id::NONE,
        };
        let this = Self { stable: this.specified_stable(), ..this };
        // Check the size model against the compiler.
        let mut len = 0;
        while len <= 8 {
          // SAFETY: `T` has a slice tail of `len` elements, and its size is tiny.
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

  /// Layout of a variant of the enum `T`: its `fields`, as returned by [`Layout::sort_fields`],
  /// with their offsets from the start of the enum.
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
  /// // As large as the enum, whose bytes the fields do not cover are padding.
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
      has_rules,
      stable: Stable::of_parts(fields),
      id: Id::NONE,
    }
  }

  /// Layout of the enum with fields `T`, whose representation must be one that Rust specifies
  /// (`repr(C)`, `repr(u8)`...): a tag at `tag.offset`, then the fields of a variant.
  ///
  /// What is known of such an enum is its tag, and which of its bytes are initialized; what it
  /// requires is checked as a whole, at run time, unless the source is an enum whose variants
  /// each prove the variant of the same tag.
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
  ///
  /// # Panics
  /// If `T` is not laid out as its representation specifies, or if the variants do not fit it:
  /// a tag that is not a value of the tag or that two variants share, a field out of bounds.
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
      has_rules: false,
      stable: Stable::of_variants(layout.variants),
      id: Id::NONE,
    }
  }

  // -----------------------------------------------------------------------------------------------
  // What every layout that Rust allows has (section 12 of the design)

  /// The same layout, of the type whose identity is `id` (see [`Id`]).
  #[doc(hidden)]
  #[must_use]
  pub const fn identified(self, id: Id) -> Self {
    Self { id, ..self }
  }

  /// The same layout, of a type of which Rust specifies nothing but, if it is 0, the size: an
  /// enum without `repr` (with fields or not), `Option` and `Result` but where `core` documents
  /// them.
  #[doc(hidden)]
  #[must_use]
  pub const fn unspecified(self) -> Self {
    // An enum without variants, or of one variant whose fields have no size, has none.
    Self { stable: Stable::opaque(self.size == 0, true), ..self }
  }

  /// The same layout, of the size of the type of `like`, which Rust keeps where it keeps the
  /// size of `like`: `MaybeUninit<T>`.
  #[doc(hidden)]
  #[must_use]
  pub const fn sized_like(self, like: &Layout) -> Self {
    let size = like.stable.size;
    Self { stable: Stable { fixed: size, ordered: size, size, deep: size, init: false }, ..self }
  }

  /// What is kept of a struct whose offsets are the ones that its representation specifies
  /// (`repr(C)`, `repr(transparent)`): they are computed from the sizes and the alignments of its
  /// fields, so they are kept if these are. A rule that reads bytes of it by offset reads the
  /// same thing in every layout only if its fields are kept too, at any depth.
  const fn specified_stable(&self) -> Stable {
    let Kind::Struct(fields, _) = self.kind else { panic!("not a struct") };
    let stable = Stable::of_parts(fields);
    if self.rules_read_bytes() && !stable.deep {
      return Stable::opaque(stable.size, false);
    }
    stable
  }

  /// Of a struct: whatever the size of every field is a multiple of the greatest alignment of a
  /// field (as the struct lowers it, if it is `packed(N)`), so that the fields of a size are one
  /// right after the other in every order, without padding between them.
  pub(crate) const fn tiled(&self) -> bool {
    let Kind::Struct(fields, _) = self.kind else { panic!("not a struct") };
    let (mut align, mut i) = (1, 0);
    while i < fields.len() {
      // The alignment of the struct is at most `N`: the one of a field, lowered to it.
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

  /// The same layout, of a struct or a tuple of the `Rust` representation, packed to `pack`
  /// (`usize::MAX`: not packed).
  ///
  /// Rust only says of it that each field is at a multiple of its alignment and that the fields
  /// do not overlap. `rustc` places them one after the other, each at the first offset that its
  /// alignment allows, in an order that it chooses: the one that `-Zrandomize-layout` draws at
  /// random. What is the same in every order is kept:
  ///
  /// - the whole layout, if every order makes the same bytes: one field has a size (it is at 0,
  ///   and no rule reads bytes by offset); or the fields that have one are all multiples of the
  ///   alignment of every field (no padding between them, whatever their order), and are all
  ///   bytes of any value without a rule in them, or all the same layout (`(u8, [u8; 3])`,
  ///   `(bool, bool)`), and the struct has no rule (a rule names fields: `a < b` holds of other
  ///   bytes in another order, and a rule of a field holds of the bytes where the field is);
  /// - else its size, if every order makes the one that `rustc` chose, and whatever its bytes
  ///   are all initialized in every order: the struct is then *opaque* (see [`Stable`]). Orders
  ///   are tried for up to 6 fields that matter (a size, or an alignment greater than 1); past
  ///   that, unless the fields need no padding in any order, the size is not kept.
  ///
  /// # Panics
  /// If `self` is not the layout of a sized struct.
  #[doc(hidden)]
  #[must_use]
  pub const fn rust(self, pack: usize) -> Self {
    let Kind::Struct(fields, rules) = self.kind else { panic!("not a struct") };
    assert!(!self.unsized_, "a struct of the `Rust` representation is sized");
    let parts = Stable::of_parts(fields);
    if !parts.size {
      // A field whose size is not kept: nor is the size of the struct.
      return Self { stable: Stable::opaque(false, false), ..self };
    }
    // The fields that matter to the order: of a size, or aligned to more than 1.
    let (mut sized, mut sum, mut init, mut i) = (0, 0, true, 0);
    let (mut all_any, mut all_same) = (true, true);
    let mut first: Option<&Layout> = None;
    while i < fields.len() {
      let field = fields[i].layout;
      if field.size > 0 {
        sized += 1;
        sum += field.size;
        init &= field.always_init();
        // Of no rule either: one in a field is checked where the field is, which is its order.
        all_any &= matches!(field.uniform, Uniform::Any) && !field.has_rules;
        match first {
          None => {
            all_same &= field.stable.deep;
            first = Some(field);
          }
          // The same layout, which no other build may arrange otherwise.
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
        // At 0: nothing of a size is before it.
        1 => {
          let mut i = 0;
          while fields[i].layout.size == 0 {
            i += 1;
          }
          fields[i].offset == 0 && tiled_size == self.size
        }
        // A rule names fields: which bytes it holds of depends on their order.
        _ => rules.is_empty() && tiled && (all_any || all_same) && tiled_size == self.size,
      };
    if same_bytes {
      return Self {
        // One part of a size is at 0 in every layout; more are in some order.
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
    Self { stable: Stable::opaque(size, init && !padded), ..self }
  }

  /// Of a struct of the `Rust` representation whose `fields` are packed to `pack`: whatever every
  /// order of its fields makes its size, and whatever one of them leaves padding.
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

  /// Of the fields `items` placed in `order`, each at the first offset that its alignment allows:
  /// whatever the struct has the size of `self`, and whatever there is padding.
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

  /// Whatever the analysis does not look into the layout, without the `unstable` feature: it is
  /// not the same in every layout that Rust allows. But for bytes that are all of any value, or
  /// all padding, in every layout of it (its size is kept, and it has no rule, which would name
  /// fields): what they are does not depend on where its fields are.
  pub(crate) const fn is_opaque(&self) -> bool {
    let same_bytes =
      self.stable.size && !self.has_rules && matches!(self.uniform, Uniform::Any | Uniform::Pad);
    !UNSTABLE && !self.stable.fixed && !same_bytes
  }

  /// Whatever every byte of a value is initialized, in every layout that Rust allows: as
  /// [`Layout::is_plain`], where what is not kept counts for what it is in every layout.
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
      Kind::Slice(_) | Kind::Pattern(_) | Kind::Enum(_) | Kind::Pointer(_) => false,
    }
  }

  /// Whatever the layout keeps everything in every layout that Rust allows, at any depth: what
  /// [`validate_in`](crate::validate_in) checks of it is then the same in all.
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

  /// Whatever nothing that is not kept in every layout that Rust allows, at any depth, has
  /// something to check or a pointer: what [`validate_in`](crate::validate_in) and the other
  /// functions of [`Visit`](crate::Visit) do with a value of the layout is then the same in all.
  pub(crate) const fn checks_kept(&self) -> bool {
    if UNSTABLE {
      return true;
    }
    if self.stable.deep {
      return true;
    }
    if !self.stable.fixed {
      return self.leaves == 0 && !self.has_pointers();
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
      Kind::Scalar(_) | Kind::Pattern(_) | Kind::Pointer(_) => true,
    }
  }

  /// The expression of the `rule`th rule, if it has one (else none).
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

  /// The `index`th rule, if any.
  pub(crate) const fn rule(&self, index: usize) -> Option<Rule> {
    let rules = self.rules();
    if index < rules.len() { Some(rules[index]) } else { None }
  }

  /// The tag of the `variant`th variant of an enum.
  #[doc(hidden)]
  #[must_use]
  pub const fn variant_tag(&self, variant: usize) -> u128 {
    let Kind::Enum(layout) = self.kind else { panic!("not an enum") };
    layout.variants[variant].tag
  }

  /// The values of the tag of the `variant`th variant of an enum, if they are not
  /// [`Layout::variant_tag`] alone.
  #[doc(hidden)]
  #[must_use]
  pub const fn variant_ranges(&self, variant: usize) -> Option<&'static [(u128, u128)]> {
    let Kind::Enum(layout) = self.kind else { panic!("not an enum") };
    layout.variants[variant].ranges
  }

  /// Layout of a type that has no value, such as `Infallible` or an enum without variants.
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
      has_rules: false,
      stable: Stable::FIXED,
      id: Id::NONE,
    }
  }

  /// Whatever a value of the layout can exist: one of a type without values is not. (A slice
  /// can be empty, an array of no element too.)
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
      Kind::Scalar(_) | Kind::Pattern(_) | Kind::Pointer(_) | Kind::Slice(_) => true,
      Kind::Array(elem, len) => len == 0 || elem.is_inhabited(),
      Kind::Struct(fields, _) => {
        let mut i = 0;
        while i < fields.len() {
          if !fields[i].layout.unsized_ && !fields[i].layout.is_inhabited() {
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

  /// Layout of the `repr(C)` union `T`, whose fields are the variants of `layout` (see
  /// [`Enum::union`]).
  ///
  /// The validity of unions is not settled by the language. What is taken here is what every rule
  /// that is considered allows: as a target, bytes are a union if they are one of its fields
  /// (where no rule requires less, this requires more); as a source, nothing is known of them, not
  /// even that they are initialized (a field may be written over another, and some rules let a
  /// union hold any bytes).
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
  /// As a source, nothing is known of it:
  ///
  /// ```compile_fail
  /// # use puns::{Pun, num::u8in};
  /// # #[derive(Pun, Clone, Copy)]
  /// # #[repr(C)]
  /// # union Choice { flag: bool, small: u8in<0, 3> }
  /// let _: u8 = puns::pun(Choice { flag: true });
  /// ```
  ///
  /// # Panics
  /// If a field is not at offset 0 or out of the union, or holds a self-relative pointer.
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
      assert!(!variant.layout.has_pointers(), "a union holds no self-relative pointer");
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
      has_rules: false,
      stable: Stable::of_variants(layout.variants),
      id: Id::NONE,
    }
  }

  /// The `index`th field, as declared, of the `variant`th variant of an enum.
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

  /// Whatever the bytes from `offset` are initialized, and where bytes that may be otherwise
  /// start: in every value of the layout (`always`), or in some.
  ///
  /// This does not look into arrays whose elements are not all one or the other: their bytes
  /// are not known to be initialized (`always`), or are all required to be (not `always`).
  ///
  /// With `checked` (and not `always`), the padding of a struct whose rules read the bytes of its
  /// value is required to be initialized, as these rules may be checked.
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
      // What a value holds (`always`) is what is known of it; what is required of one, what is
      // valid.
      Kind::Scalar(spec) => {
        (!matches!(if always { spec.known } else { spec.valid }, Valid::Uninit), self.size)
      }
      // The bytes of a pointer are initialized.
      Kind::Pointer(_) => (true, self.size),
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
      // Nothing is known of what a union holds; a target may need what any field needs.
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
          // A variant that no value can be has nothing to tell.
          if !layout.variants[i].is_present() {
            i += 1;
            continue;
          }
          let (init, end) = layout.variants[i].layout.segment(offset, always, checked);
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

  /// Number the fields of a struct (given in declaration order) and sort them by offset.
  ///
  /// ```
  /// use core::mem::offset_of;
  ///
  /// use puns::layout::{Field, Layout, Spec};
  ///
  /// // Without `repr`, `rustc` chooses the order of the fields.
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
  /// // SAFETY: `Mixed` is sized.
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

  /// Alignment of the type.
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

  /// Size of the type if it is `Sized`, otherwise the offset of its tail slice.
  ///
  /// ```
  /// use puns::{
  ///   Pun,
  ///   layout::{Layout, Spec},
  /// };
  ///
  /// const BYTE: Layout = Layout::scalar::<u8>(Spec::any::<u8>());
  /// assert_eq!(Layout::array(&BYTE, 5).size(), 5);
  /// // SAFETY: the probe of a slice.
  /// const BYTES: Layout = unsafe { Layout::slice::<u8>(<[u8] as Pun>::PROBE, &BYTE) };
  /// assert_eq!(BYTES.size(), 0);
  /// ```
  #[inline]
  #[must_use]
  pub const fn size(&self) -> usize {
    self.size
  }

  #[inline]
  pub(crate) const fn is_unsized(&self) -> bool {
    self.unsized_
  }

  /// Whatever the size of values of the type depends on a slice length.
  #[inline]
  pub(crate) const fn is_dynamic(&self) -> bool {
    self.unsized_ && self.tail > 0
  }

  /// Size of an element of the tail slice if the type [is dynamic](Self::is_dynamic), else `0`.
  #[inline]
  pub(crate) const fn stride(&self) -> usize {
    if self.unsized_ { self.tail } else { 0 }
  }

  /// Size of a value whose tail slice has `len` elements, trailing padding included.
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

  /// Offset of the field declared at `index`.
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

  #[doc(hidden)]
  #[must_use]
  pub const fn field_offset(&self, index: usize) -> usize {
    self.field(index).offset
  }

  #[doc(hidden)]
  #[must_use]
  pub const fn field_leaf(&self, index: usize) -> usize {
    self.field(index).leaf
  }

  #[inline]
  pub(crate) const fn shape(&self) -> Shape {
    match self.kind {
      Kind::Scalar(spec) => Shape::Scalar(spec),
      Kind::Array(_, len) => Shape::Array(len),
      Kind::Slice(_) => Shape::Slice,
      Kind::Struct(..) => Shape::Struct,
      Kind::Pattern(dfa) => Shape::Pattern(dfa),
      Kind::Enum(layout) => Shape::Enum(layout),
      Kind::Pointer(pointer) => Shape::Pointer(pointer),
    }
  }

  /// Whatever a rule of the struct reads bytes of its value other than its fields (`le`, `be`,
  /// `crc32`, `sum16`): its padding, if it has any, is then read when the rule is checked.
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

  /// The first scalar of the sized part of a value of this layout that only some values are
  /// valid for (the EtherType of a frame, the version of a header), by offset: its offset and
  /// what it requires. Whatever bytes a value of this layout is made from, it is checked there.
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

  /// How many elements of this layout the validation of a slice checks at a time, without
  /// stopping between them (see `validate::validate_elems`): about 64 bytes of them, if checking
  /// one is a few comparisons, else 1.
  pub(crate) const fn block(&self) -> usize {
    if self.size == 0 || self.size > 16 || self.unsized_ || !self.is_cheap() {
      return 1;
    }
    64 / self.size
  }

  /// Whatever checking a value of this layout is a few comparisons: scalars, and arrays and
  /// structs of them, whose rules only compute on their fields (no loop, no division).
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
      Kind::Slice(_) | Kind::Pattern(_) | Kind::Enum(_) | Kind::Pointer(_) => false,
    }
  }

  /// Rules of a struct.
  #[inline]
  pub(crate) const fn rules(&self) -> &'static [Rule] {
    match self.kind {
      Kind::Struct(_, rules) => rules,
      _ => &[],
    }
  }

  /// Fields of a struct, by offset (none if it is not one).
  pub(crate) const fn struct_fields(&self) -> &'static [Field] {
    match self.kind {
      Kind::Struct(fields, _) => fields,
      _ => &[],
    }
  }

  /// Whatever the last rule of a struct is a region without an expression, which takes what
  /// remains of the tail.
  pub(crate) const fn has_last_region(&self) -> bool {
    matches!(self.rules().last(), Some(Rule::Region(None)))
  }

  /// The rule that tells how long a value is from its first bytes, if any: the first `Size` or
  /// `Len` rule of the type, or else of its last field, and so on.
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

  /// The specification of the unsigned integer that the type is, if it is one: an initialized
  /// scalar of 1 to 16 bytes whose bits are its value, or a struct whose only field (but for the
  /// ones without size) is one.
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

  /// The tail of a struct, as rules see it: its slice if it is unsized, else its last field.
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

  /// Offset of the first byte of the tail slice (the size of a `Sized` type).
  #[inline]
  pub(crate) const fn head(&self) -> usize {
    self.size
  }

  /// Whatever the all-zeros bit pattern is valid.
  pub(crate) const fn zeroable(&self) -> bool {
    // Whatever zeros are of an enum without `repr` (or of `Option<bool>`) is not kept: which
    // values its tag has is not specified. The fields of a struct are zeros whatever their order.
    if !UNSTABLE && !self.stable.fixed && !matches!(self.kind, Kind::Struct(..)) {
      return false;
    }
    match self.kind {
      Kind::Scalar(spec) => spec.contains(0),
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
      // No pointer.
      Kind::Pointer(pointer) => pointer.nullable,
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

  /// Whatever two structs have rules that are the same function of the bytes of each: the same
  /// rules, in the same order ([`Rule::equals`]), over a struct of the same extent. A rule is a
  /// function of the bytes of its struct, of its size (`size_of_val(self)`, a `size` rule), of
  /// the number of elements of its tail (a `len` rule) and of the bytes of its tail (regions):
  /// so the same sized part, and the same tail (where it starts, the size of an element, and for
  /// a sized struct its bytes and the elements of the array it ends with), whatever the fields.
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

  /// Whatever two layouts describe the same thing.
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
      // What a scalar requires, and what it is known to be: a layout that only requires what
      // another guarantees (`valid(..)` on a `u8`, and a `bool`) is not the same.
      //
      // A scalar that may be uninitialized is equal to none: what it is lies between what it
      // requires and what it is known to be, which two types can have the same of while one has
      // padding where the other does not.
      (Kind::Scalar(a), Kind::Scalar(b)) => {
        !a.may_be_uninit()
          && a.size == b.size
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
      // As far as the pointers themselves go: what they aim at is related through the types.
      (Kind::Pointer(a), Kind::Pointer(b)) => a.same(&b),
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

/// Whatever two sets of valid values of scalars of the same size are the same.
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

/// A deterministic finite automaton over bytes: the language of an unsized type like `str`.
///
/// The first state is the initial one. At most 64 states.
///
/// A rule of `#[derive(Pun)]` can require bytes of a struct to be accepted by one,
/// `matches(DFA, start, end)`, of this crate or of yours:
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
  /// A hint: two automata of different `id`s are taken to be different, which saves comparing
  /// their states. Nothing else is taken of it: two automata are the same only if their states
  /// and edges are (where pattern types are compared, see [`Layout::pattern`], and where a rule
  /// is), and bytes are checked as UTF-8 (by `str::from_utf8`) only by [`Dfa::UTF8`] itself, the
  /// same states and edges. Those of this crate are 1 to 6.
  pub id: u32,
  /// The states: the first is the initial one. An edge leads to the index of a state in it.
  pub states: &'static [DfaState],
}

/// A state of a [`Dfa`]: whatever a value may end in it, and where each byte leads from it.
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
  /// Whatever a value can end in this state.
  pub accept: bool,
  /// `(first byte, last byte, next state)`, sorted and disjoint.
  pub edges: &'static [(u8, u8, u8)],
}

impl Dfa {
  /// UTF-8, as of the Unicode standard (table 3-7, "well-formed UTF-8 byte sequences").
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

  /// ASCII: bytes below `0x80`.
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

  /// Whatever two automata have the same states and edges (what their `id`s do not prove).
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

  /// Whatever the automaton accepts `bytes`, by its states and edges alone.
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

  const fn check(&self) {
    assert!(!self.states.is_empty() && self.states.len() <= 64, "a DFA has 1 to 64 states");
    let mut i = 0;
    while i < self.states.len() {
      let (edges, mut j) = (self.states[i].edges, 0);
      while j < edges.len() {
        assert!(edges[j].0 <= edges[j].1 && (edges[j].2 as usize) < self.states.len());
        assert!(j == 0 || edges[j - 1].1 < edges[j].0, "DFA edges must be sorted and disjoint");
        j += 1;
      }
      i += 1;
    }
  }

  /// States reached from the states of `set` (a bit for each) with a byte of `lo..=hi`, and
  /// whatever every such byte leads somewhere from every state of `set`.
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

  /// Whatever a value can end in every state of `set`.
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

  /// Every state that can be reached from the initial one.
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

  /// Whatever the automaton is [`Dfa::UTF8`]: its states and edges, whatever its `id` says. To
  /// be asked in a `const`, where it costs nothing at run time.
  pub(crate) const fn is_utf8(&self) -> bool {
    self.id == Self::UTF8.id && self.same(Self::UTF8)
  }

  /// Whatever the automaton accepts the `len` bytes at `ptr`, by its states and edges.
  ///
  /// # Safety
  /// `ptr` must be valid for reads of `len` initialized bytes.
  #[inline]
  pub(crate) const unsafe fn matches(&self, ptr: *const u8, len: usize) -> bool {
    // SAFETY: guaranteed by the caller.
    self.accepts_bytes(unsafe { core::slice::from_raw_parts(ptr, len) })
  }

  /// Whatever the `len` bytes at `ptr` are UTF-8, by `str::from_utf8`: what [`Dfa::UTF8`]
  /// accepts, faster than its states.
  ///
  /// # Safety
  /// `ptr` must be valid for reads of `len` initialized bytes.
  #[inline]
  pub(crate) const unsafe fn matches_utf8(ptr: *const u8, len: usize) -> bool {
    // SAFETY: guaranteed by the caller.
    core::str::from_utf8(unsafe { core::slice::from_raw_parts(ptr, len) }).is_ok()
  }
}

// -------------------------------------------------------------------------------------------------
// `Rule`

/// An operation of a postfix expression over the scalar fields of a struct, see [`Rule`].
///
/// Expressions are evaluated on `u128` (and computed in a register where every value is known
/// to fit one, with the same results). What cannot be computed (an overflow, an underflow, a
/// division by zero, a shift by 128 or more, a read out of the value) makes the rule that holds
/// the expression fail. What Rust does not compute is not computed: the right side of a `&&` or
/// of a `||` that the left side decides, the branch of an `if` that is not taken
/// ([`Op::Shortcut`], [`Op::Then`], [`Op::Else`]). So `kind != 7 || be(40, 2) == 0` holds for a
/// value that is not of kind 7, however short, and `be(40, 2) == 0 || kind != 7` fails for one
/// that is too short. As nothing that is computed can then be left out of the result, the first
/// failure is the one of the expression, and its evaluation stops there. `<<` drops the bits that
/// it shifts out, as in Rust.
///
/// A binary operation replaces the two values at the top of the stack, the one pushed first on
/// its left side, by its result; a comparison by `1` if it holds, else `0`.
///
/// `#[derive(Pun)]` compiles the expressions of its rules to them:
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
  /// Push a constant.
  Lit(u128),
  /// Push the unsigned integer of `size` bytes at `offset` in the struct.
  Load {
    /// Where the integer is, in bytes from the start of the struct.
    offset: usize,
    /// Its size in bytes.
    size: u8,
    /// `true` if it is big-endian, `false` if little-endian.
    big: bool,
  },
  /// Push the size in bytes of the struct (not in `size` and `len` rules, which define it).
  Size,
  /// Replace an offset by the unsigned integer of `size` bytes that is there in the value.
  At {
    /// The size in bytes of the integer.
    size: u8,
    /// `true` if it is big-endian, `false` if little-endian.
    big: bool,
  },
  /// Replace an offset by the variable-length integer of the `form` that is there in the value, or
  /// with `length`, by its number of bytes.
  Varint {
    /// The form of the integer.
    form: Varint,
    /// `true` for its number of bytes, `false` for its value.
    length: bool,
  },
  /// Replace two offsets by the CRC-32 (the one of Ethernet) of the bytes between them.
  Crc32,
  /// Replace two offsets by the ones' complement sum of the big-endian 16 bits words between
  /// them (the checksum of IP, before it is complemented).
  Sum16,
  /// Replace two offsets by the CRC of the bytes between them.
  Crc(&'static Crc),
  /// Replace four offsets by the CRC of the bytes between the first two, the ones between the
  /// last two taken as zeros (a CRC that covers the field that holds it).
  CrcZeros(&'static Crc),
  /// Replace two offsets by a checksum of the bytes between them.
  Checksum(Checksum),
  /// Replace two offsets by whatever the automaton accepts the bytes between them: a pattern
  /// that a struct requires of them (which proves nothing of them, as any rule).
  Matches(&'static Dfa),
  /// `+`: fails if the sum overflows.
  Add,
  /// `-`: fails if the difference is below zero.
  Sub,
  /// `*`: fails if the product overflows.
  Mul,
  /// `/`, rounded down: fails if the right side is zero.
  Div,
  /// `%`: fails if the right side is zero.
  Rem,
  /// `&`, bitwise.
  And,
  /// `|`, bitwise.
  Or,
  /// `^`, bitwise.
  Xor,
  /// `<<`, which drops the bits that it shifts out: fails if the right side is 128 or more.
  Shl,
  /// `>>`: fails if the right side is 128 or more.
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
  /// `&&`, after its [`Op::Shortcut`] and its right side: replace the left side, which did not
  /// decide, and the right side by the right side.
  Both,
  /// `||`, as [`Op::Both`].
  Either,
  /// After the left side of a `&&` (`either: false`) or of a `||` (`either: true`): if that side
  /// decides (it is `either`), it is the result, and the `len` operations that follow, which are
  /// the right side and the [`Op::Both`] or [`Op::Either`], are not computed.
  Shortcut {
    /// `true` after the left side of a `||`, `false` after the one of a `&&`: the value of the
    /// left side that decides.
    either: bool,
    /// The number of operations that follow and are not computed if the left side decides.
    len: u16,
  },
  /// `!`, on a boolean.
  Not,
  /// After the condition of an `if`: if it is false, the `len` operations that follow, which are
  /// the branch of the `if` and its [`Op::Else`], are not computed (the value of the branch is
  /// then a placeholder, which [`Op::Select`] does not take).
  Then(u16),
  /// After the branch of an `if` that is taken when its condition is true: the `len` operations
  /// that follow, which are the `else` branch, are not computed (its value is a placeholder).
  Else(u16),
  /// After an [`Op::Then`] and an [`Op::Else`], and their branches: replace a condition and two
  /// values by the first value if the condition is true, by the second if not. Only that one
  /// was computed.
  Select,
  /// Take a value off the stack: it is the `index`th variable. What a `let` binds is computed,
  /// as in Rust, whether it is used or not.
  Set(u8),
  /// Push the `index`th variable.
  Get(u8),
  /// Replace a value by what it becomes when, for as long as the expression `cond` is true of
  /// it, it is replaced by the expression `next`. In both, the value is the variable `var`.
  ///
  /// This is how what is at an offset that depends on what is before is reached: options, a
  /// chain of headers. A walk takes at most `8 * size + 128` steps, where `size` is the size of
  /// the value in bytes: a step for each bit of the value, and for each bit of a `u128`.
  Walk {
    /// The variable that is the value, in `cond` and in `next`.
    var: u8,
    /// What must be true of the value for the walk to take a step.
    cond: &'static [Op],
    /// What the value becomes at each step.
    next: &'static [Op],
  },
}

/// The types of the constants of rules: unsigned integers, which they compute on.
#[doc(hidden)]
#[diagnostic::on_unimplemented(
  message = "a constant of `#[pun(..)]` is an unsigned integer: `{Self}` is not one",
  label = "`#[pun(..)]` computes on unsigned integers",
  note = "an expression of `#[pun(..)]` means what it means in Rust, where every integer is a `u128`"
)]
pub trait Unsigned: Copy + seal::Unsigned {}

/// Implemented by this crate only.
mod seal {
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
  /// [`Op::Matches`], of an automaton that is checked to be one.
  #[doc(hidden)]
  #[must_use]
  pub const fn matches(dfa: &'static Dfa) -> Self {
    dfa.check();
    Op::Matches(dfa)
  }

  /// What `as usize` keeps of a value.
  #[doc(hidden)]
  pub const USIZE_MAX: u128 = usize::MAX as u128;

  /// The value of a constant of a rule.
  #[doc(hidden)]
  #[must_use]
  pub const fn constant<T: Unsigned>(value: &T) -> u128 {
    // SAFETY: an unsigned integer is initialized, and has no padding.
    unsafe { load(core::ptr::from_ref::<T>(value).cast(), size_of::<T>(), Endian::NATIVE) }
  }

  /// Push the value of the field of type `T` at `offset`: an integer, or a struct that only
  /// holds one.
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
  ///
  /// # Panics
  /// If `T` is not an integer.
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

  const fn code(self) -> u8 {
    // SAFETY-free: the discriminant of a variant without data, by exhaustion.
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
      Op::Varint { .. } => 30,
      Op::Crc(_) => 31,
      Op::CrcZeros(_) => 32,
      Op::Checksum(_) => 33,
      Op::Matches(_) => 34,
      Op::Shortcut { .. } => 29,
      Op::Then(_) => 35,
      Op::Else(_) => 36,
    }
  }
}

/// A CRC of at most 64 bits, by the parameters of the catalogue of CRCs.
///
/// They are its `width`, its polynomial `poly` (without its high bit), the value `init` that the
/// register starts with, whatever its bits are `reflect`ed (the bytes taken least significant bit
/// first, and the result read so), and `xorout`, what the result is combined with by XOR.
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
  /// `tables[k][b]`: the register after the byte `b` and `k` zero bytes, from a register of
  /// zeros: in its low bits if reflected, else in its high bits. Sixteen bytes are then sixteen
  /// lookups that do not wait for each other (slicing by 16), rather than a chain of sixteen.
  tables: Tables,
}

/// The tables of a [`Crc`] ([`CRC_TABLES`] of them): of `u32`s for a CRC of at most 32 bits (16
/// KiB, half of what the cache holds of them otherwise), else of `u64`s.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[expect(
  clippy::large_enum_variant,
  reason = "a `Crc` is a constant of 32 KiB, as it was with tables of `u64`s only: nothing to box"
)]
enum Tables {
  /// The register in the low bits of a `u32` if reflected, else in its high bits.
  Narrow([[u32; 256]; CRC_TABLES]),
  /// The register in the low bits of a `u64` if reflected, else in its high bits.
  Wide([[u64; 256]; CRC_TABLES]),
}

/// The number of tables of a [`Crc`]: 16 (32 KiB, sixteen bytes at a time), but 1 (2 KiB, a byte
/// at a time) on a 16-bit target, where no value is larger than `isize::MAX` (32 KiB minus 1)
/// bytes. The CRC is the same either way.
pub(crate) const CRC_TABLES: usize = if cfg!(target_pointer_width = "16") { 1 } else { 16 };

// The tables of a `Crc` are a value, which is at most `isize::MAX` bytes.
const _: () = assert!(size_of::<[[u64; 256]; CRC_TABLES]>() <= isize::MAX as usize);

impl Crc {
  /// The CRC-32 of Ethernet, zlib, PNG: what `crc32(..)` is.
  pub const CRC32: Self = Self::new(32, 0x04c1_1db7, 0xffff_ffff, true, 0xffff_ffff);

  /// The CRC of these parameters, see [`Crc`].
  ///
  /// Its tables are 32 KiB (16 KiB of them are used for a CRC of up to 32 bits): a `Crc` is
  /// meant to be a constant, made in const evaluation.
  ///
  /// ```
  /// use puns::layout::Crc;
  ///
  /// // CRC-16/MODBUS, of its check value.
  /// const MODBUS: Crc = Crc::new(16, 0x8005, 0xffff, true, 0);
  /// assert_eq!(MODBUS.check(b"123456789"), 0x4b37);
  /// ```
  ///
  /// # Panics
  /// If `width` is not 1 to 64, or a parameter has more bits than that.
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

  /// The CRC of `bytes`.
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

  /// The CRC of `bytes`, the ones from `zeros.0` to `zeros.1` taken as zeros.
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

  /// The register after `bytes`.
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

  /// [`Crc::update`] with tables of `u32`s: as the CRC-32 of Ethernet ([`crc32`]).
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

  /// [`Crc::update`] with tables of `u64`s.
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

  /// The CRC of `bytes` with the first table only, a byte at a time: what a target of one table
  /// computes, for the tests.
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

  const fn equals(&self, other: &Crc) -> bool {
    self.width == other.width
      && self.poly == other.poly
      && self.init == other.init
      && self.reflect == other.reflect
      && self.xorout == other.xorout
  }
}

/// A checksum of bytes, see [`Op::Checksum`].
///
/// `sum(start, end)`, `fletcher16(start, end)` and `adler32(start, end)` in the rules of
/// `#[derive(Pun)]`:
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
  /// The sum of the bytes (not reduced: `% 256` makes it the one of a byte).
  Sum,
  /// Fletcher-16: two sums modulo 255, the second of the first, as `second << 8 | first`. It is
  /// `0` over data that holds its ISO 8473 checksum (OSPF, IS-IS).
  Fletcher16,
  /// Adler-32 (zlib): two sums modulo 65521, the first starting at 1, as `second << 16 | first`.
  Adler32,
}

impl Checksum {
  /// The checksum of `bytes`.
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

/// A form of variable-length integer, see [`Op::Varint`].
///
/// `leb128(offset)`, `quic(offset)` and `vlq(offset)` in the rules of `#[derive(Pun)]`, and with
/// `_len`, their number of bytes:
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
  /// Unsigned LEB128 (Protocol Buffers, WebAssembly, MQTT, DWARF): 7 bits a byte, the least
  /// significant first, each byte but the last with its high bit set.
  Leb128,
  /// The variable-length integer of QUIC (RFC 9000): the two high bits of the first byte tell
  /// its length (1, 2, 4 or 8 bytes), the rest is the value, big-endian.
  Quic,
  /// 7 bits a byte, the most significant first, each byte but the last with its high bit set
  /// (the tag numbers and the arcs of object identifiers of ASN.1, MIDI).
  Vlq,
}

impl Varint {
  /// The value of the integer at `ptr`, and its number of bytes, of at most `size`: `None` if
  /// it is not over by then, or does not fit in a `u128`.
  ///
  /// # Safety
  /// `ptr` must be valid for reads of `size` initialized bytes.
  #[inline]
  const unsafe fn read(self, ptr: *const u8, size: usize) -> Option<(u128, usize)> {
    // SAFETY: guaranteed by the caller.
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

/// Whatever an expression only computes on fields with a few instructions: it does not read the
/// value at an offset that it computes, pass over its bytes, walk, or divide.
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

/// Whatever two expressions are the same.
pub(crate) const fn same_ops(a: &[Op], b: &[Op]) -> bool {
  let mut i = 0;
  while i < a.len() && i < b.len() && a[i].equals(&b[i]) {
    i += 1;
  }
  a.len() == b.len() && i == a.len()
}

/// Something that values of a struct must satisfy, beyond the validity of their fields.
///
/// Rules are checked when a value is made from something else (`try_pun*`, `from_head`), and
/// tell `from_head` how long a value is. They are not invariants that unsafe code may rely on: safe
/// code can write to the fields of a struct.
///
/// `#[derive(Pun)]` makes them of `#[pun(size = ..)]` and `#[pun(assert(..))]` on a struct, and
/// of `#[pun(len = ..)]` and `#[pun(regions(..))]` on its last field:
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
  /// The size in bytes of the value is the result of the expression.
  Size(&'static [Op]),
  /// The number of elements of the tail slice of the value is the result of the expression.
  Len(&'static [Op]),
  /// The expression is true.
  Assert(&'static [Op]),
  /// The tail of the value starts with a region of as many bytes as the result of the
  /// expression (`None`: what remains). Regions follow each other, in the order of the rules.
  Region(Option<&'static [Op]>),
}

/// Whatever an expression reads bytes of the value that are not those of its integer fields.
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
  /// Its expression, if it has one.
  pub(crate) const fn ops(&self) -> Option<&'static [Op]> {
    match *self {
      Rule::Size(ops) | Rule::Len(ops) | Rule::Assert(ops) | Rule::Region(Some(ops)) => Some(ops),
      Rule::Region(None) => None,
    }
  }

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

/// See [`Layout::tail_shape`].
#[derive(Copy, Clone)]
pub(crate) struct TailShape {
  /// Offset of the tail.
  pub(crate) start: usize,
  /// Size of the tail, if the struct is `Sized`.
  pub(crate) bytes: usize,
  /// Number of elements of the array that the tail ends with, if the struct is `Sized`.
  pub(crate) len: usize,
  pub(crate) array: bool,
}

/// See [`Layout::length_rule`].
#[derive(Copy, Clone)]
pub(crate) struct LengthRule {
  /// Offset of the struct that has the rule.
  pub(crate) base: usize,
  /// The expression is a number of elements of the tail slice, rather than the size in bytes of
  /// the struct.
  pub(crate) elements: bool,
  pub(crate) ops: &'static [Op],
}

/// Depth of the stack of the evaluator ([`Machine`]).
pub(crate) const STACK: usize = 16;
/// Number of variables of an expression.
pub(crate) const VARIABLES: usize = 8;
/// The most operations of an expression, and the most walks that it nests (a walk in a walk is
/// two): the validation applies each operation in an instance of a function of its own, reached
/// by recursions over a tree of the operations (`validate::steps`), which these keep short.
/// `#[derive(Pun)]` has the same limits, and says where an expression goes beyond them.
pub(crate) const MAX_OPS: usize = 256;
pub(crate) const MAX_WALKS: usize = 4;

/// Check that the expressions of `rules` are well formed, and only read the first `head` bytes.
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

/// The `index`th operation of `ops`, if any.
pub(crate) const fn op_at(ops: &[Op], index: usize) -> Option<Op> {
  if index < ops.len() { Some(ops[index]) } else { None }
}

/// Check an expression, in which the variables of `defined` are (a bit for each), and which is in
/// `walks` walks. With `sizes`, it is a size or a length, which cannot be computed from the size
/// of the value.
///
/// What an [`Op::Shortcut`], an [`Op::Then`] or an [`Op::Else`] does not compute is a *part*: the
/// right side of a `&&` or `||`, a branch of an `if`. A part must end with what it is a part of
/// (the `Both`, `Either`, `Else` or `Select`), be within the part that holds it, and push one
/// value without taking any of what is below: then the stack is as deep after a part, whether it
/// was computed or not (the evaluator pushes a placeholder for a branch that is not). What a part
/// defines is not defined after it. Every `&&`, `||` and `if` has its parts: what is computed is
/// what Rust computes, and no failure can be left out of the value of the expression.
pub(crate) const fn check_ops(ops: &[Op], head: usize, sizes: bool, mut defined: u8, walks: usize) {
  assert!(ops.len() <= MAX_OPS, "an expression is too long");
  let (mut depth, mut i) = (0usize, 0);
  // The parts that are open, innermost last: the index of the operation that ends each, the depth
  // of the stack before it, the variables defined before it, and the operation that opened it.
  let (mut parts, mut open) = ([(0usize, 0usize, 0u8, Op::Size); STACK], 0);
  while i < ops.len() {
    let op = ops[i];
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
    // but for the part of an `Else`, which is followed by the `Select` (which it does not skip).
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

/// What an operation takes off the stack, and what it puts on it, where it is computed (see
/// [`check_ops`] for the parts that are not).
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

/// What evaluates an expression: a stack, and variables.
///
/// It is given one operation at a time, each a constant of the instance of the function that
/// applies it (`validate::steps`): there is no interpreter. This is written so that what remains
/// once it is inlined there is the arithmetic of the expression, without relying on much of the
/// optimizer: the stack is not an array but a variable for each of its slots, what is not
/// computed is told by an index (`skip`), which is a constant on each path through the code, and
/// a failure, which is the one of the expression (nothing that is computed is left out of its
/// value), returns at once.
#[derive(Copy, Clone)]
pub(crate) struct Machine {
  stack: Stack,
  variables: Variables,
  depth: usize,
  /// The operations before this index are not computed: the part that an [`Op::Shortcut`], an
  /// [`Op::Then`] or an [`Op::Else`] skips.
  skip: usize,
}

/// `slots!(Name: [0 1 2 ..] last)`: values in fields, one for each index, rather than in an array: so
/// that the optimizer makes a variable of each, whatever the index asked for (see [`Machine`]).
macro_rules! slots {
  ($($name:ident: [$($index:tt)+] $last:tt)+) => {$(
    #[derive(Copy, Clone)]
    struct $name($(slots!(@u128 $index),)+ u128);

    impl $name {
      const NEW: Self = Self($(slots!(@zero $index),)+ 0);

      // The last one is the one of any other index, which is not asked for: no other case.
      #[inline(always)]
      const fn get(&self, index: usize) -> u128 {
        match index {
          $($index => self.$index,)+
          _ => self.$last,
        }
      }

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
  pub(crate) const NEW: Self =
    Self { stack: Stack::NEW, variables: Variables::NEW, depth: 0, skip: 0 };

  #[inline(always)]
  const fn get(&self, slot: usize) -> u128 {
    self.stack.get(slot)
  }

  #[inline(always)]
  const fn set(&mut self, slot: usize, value: u128) {
    self.stack.set(slot, value);
  }

  #[inline(always)]
  const fn push(&mut self, value: u128) {
    self.stack.set(self.depth, value);
    self.depth += 1;
  }

  /// Whatever the operation at the index `at` of the expression is computed.
  #[inline(always)]
  pub(crate) const fn computes(&self, at: usize) -> bool {
    at >= self.skip
  }

  /// The value of the expression, once all of its operations are applied without a failure.
  #[inline(always)]
  pub(crate) const fn result(&self) -> u128 {
    self.get(0)
  }

  /// The value at the top of the stack.
  #[inline(always)]
  pub(crate) const fn top(&self) -> u128 {
    self.get(self.depth - 1)
  }

  /// Replace the value at the top of the stack: whatever there is one (else the expression
  /// fails).
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

  /// A machine to evaluate another expression, in which the variable `index` is `value` and the
  /// other ones are what they are here.
  #[inline(always)]
  pub(crate) const fn enter(&self, index: u8, value: u128) -> Self {
    let mut inner = Self { depth: 0, skip: 0, ..*self };
    inner.variables.set(index as usize, value);
    inner
  }

  /// Apply the operation `op`, at the index `at` of an expression, computed as `how` says (see
  /// `narrow`), over the struct at `base`, whose size is `size`: `false` if it fails, which is
  /// then the failure of the expression.
  ///
  /// # Safety
  /// `op` must be an operation of an expression of a rule of the layout of a struct, and `base`
  /// must be valid for reads of the sized fields of such a struct, and of `size` bytes,
  /// initialized. `size` is the size of the struct, or for a `size` or `len` rule, which tells it,
  /// what can be read.
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
        // SAFETY: `check_rules` made sure that the field is in the sized part of the struct.
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
          // SAFETY: in the bounds of the value.
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
          // SAFETY: in the bounds of the value.
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
        // The range, and the one taken as zeros (none but for `CrcZeros`).
        let operands = if matches!(op, Op::CrcZeros(_)) { 4 } else { 2 };
        let (start, end) = (self.get(depth - operands), self.get(depth - operands + 1));
        let (zeros, to) =
          if operands == 4 { (self.get(depth - 2), self.get(depth - 1)) } else { (0, 0) };
        self.depth -= operands - 1;
        if start <= end && end <= size as u128 && zeros <= to && to <= size as u128 {
          // SAFETY: in the bounds of the value.
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

/// So that a walk ends: a step for each bit of a value of `size` bytes, and for each bit of a
/// `u128`. (`size` is at most `isize::MAX`: adding 16 does not overflow.)
#[inline(always)]
pub(crate) const fn walk_limit(size: usize) -> usize {
  (size + 16).saturating_mul(8)
}

/// `op` of `a` and `b` on `u128`, `None` where Rust would panic: what an expression means.
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

/// The CRC-32 of Ethernet (and zlib, PNG...): the frame check sequence of a frame is the CRC of
/// what is before it, least significant byte first.
///
/// ```
/// assert_eq!(puns::crc32(b"123456789"), 0xcbf43926);
/// ```
#[must_use]
pub const fn crc32(bytes: &[u8]) -> u32 {
  crc32_inline(bytes)
}

/// [`crc32`], inlined where a rule computes it.
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

/// The ones' complement sum of the big-endian 16 bits words of `bytes`.
///
/// A last byte alone is the high byte of a word. The checksums of IP, UDP and TCP are the
/// complement of this sum: over data that holds its own checksum, it is `0xffff`.
///
/// ```
/// assert_eq!(puns::sum16(&[0x00, 0x01, 0xf2, 0x03, 0xf4, 0xf5, 0xf6, 0xf7]), 0xddf2);
/// ```
#[must_use]
pub const fn sum16(bytes: &[u8]) -> u16 {
  sum16_inline(bytes)
}

/// The blocks of 8 bytes that `sum16` adds before it folds: 2^20, or all of them where a `usize`
/// does not count that far (a 16-bit target: at most 2^12 blocks, far from a lost carry).
const SUM16_FOLD: usize = if usize::BITS > 20 { 1 << (20 % usize::BITS) } else { usize::MAX };

// A block adds less than 2^33 to the sum: `SUM16_FOLD` of them never carry out of a `u64`.
const _: () = assert!((SUM16_FOLD as u128) << 33 < 1 << 64);

/// [`sum16`], inlined where a rule computes it.
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

/// A sum of 16 bits words, folded into 16 bits with end-around carries.
#[inline(always)]
const fn fold(mut sum: u64) -> u16 {
  while sum > 0xffff {
    sum = (sum & 0xffff) + (sum >> 16);
  }
  sum as u16
}

/// Bounds of a range of valid values of an integer of the size of `T`, for `#[pun(valid(..))]`.
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

/// Sort and merge inclusive ranges; returns the ranges and their count.
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

/// The bits of a value of at most 16 bytes, as a native-endian unsigned integer.
///
/// # Safety
/// Every byte of `value` must be initialized.
#[doc(hidden)]
pub const unsafe fn bits_of<T>(value: &T) -> u128 {
  assert!(size_of::<T>() <= 16, "a fieldless enum cannot be larger than 16 bytes");
  // SAFETY: guaranteed by the caller.
  unsafe { load(core::ptr::from_ref::<T>(value).cast(), size_of::<T>(), Endian::NATIVE) }
}
