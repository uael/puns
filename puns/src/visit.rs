//! Validation of a value in a buffer that holds self-relative pointers, for the crate that has
//! them: [`validate_in`], [`validate_elems_in`], and what they hand each pointer to, [`Visit`];
//! the walk of the pointers of a valid value, which checks nothing: [`pointers_in`],
//! [`pointers_elems_in`]; and the copy of the initialized bytes of a valid value, [`copy_in`].
//!
//! The functions recurse over types, as `validate` does: each call is an instance in which the
//! layout is a constant, and what it says cannot be invalid is not compiled. A part of a value
//! that holds no pointer is validated by `validate` itself, the same code as `try_pun_ref`; only
//! where it fails is it walked again, to tell where.

use core::fmt;

use crate::{
  Fields, Pun, Root, Variants,
  layout::{self, Enum, Kind, Shape, load},
  ptr,
  validate::{All, check_rules, validate},
};

/// What a validation of the bytes of a buffer does with each self-relative pointer it meets.
///
/// [`validate_in`] and [`validate_elems_in`] hand each pointer of the value they validate to the
/// visitor: the crate that has the pointers follows them, as its scheme says, and validates what
/// they aim at in turn (or queues it). [`pointers_in`] and [`pointers_elems_in`] hand it the same
/// pointers of a value known to be valid, which they do not check: to copy what they aim at, or
/// to re-aim them when the value moves.
///
/// `pointer::<P>(at)` is called once for each pointer that the value holds where it is (in its
/// fields, in the variant that its tag tells, in the elements of its arrays), in the order of
/// their positions, interleaved with the checks of the rest: `P` is the type of the pointer (its
/// `LAYOUT` is a pointer: [`Layout::as_pointer`](layout::Layout::as_pointer)), `at` its position
/// in the bytes. Bytes of a pointer that are all zero are not handed over if the pointer may be
/// null (no pointer) or if its type has no value of zeros (its offset is a `NonZero`: not valid
/// there); a pointer whose offset may be zero is handed over with it. An error returned by
/// `pointer` ends the validation, with that error.
///
/// ```
/// use puns::{Invalid, Pun, Visit};
///
/// /// Refuses every pointer: for values that hold none.
/// struct NoPointers;
///
/// impl Visit for NoPointers {
///   type Error = Invalid;
///   fn pointer<P: ?Sized + Pun>(&mut self, at: usize) -> Result<(), Invalid> {
///     Err(Invalid::at(at))
///   }
/// }
///
/// #[derive(Pun)]
/// #[repr(C)]
/// struct Flags {
///   on: bool,
///   #[pun(valid(..=3))]
///   level: u8,
/// }
///
/// let bytes = [9, 1, 2, 1, 4];
/// assert_eq!(puns::validate_in::<Flags, _>(&bytes, 1, &mut NoPointers), Ok(()));
/// assert_eq!(puns::validate_in::<Flags, _>(&bytes, 3, &mut NoPointers), Err(Invalid::at(4)));
/// assert_eq!(puns::validate_in::<Flags, _>(&bytes, 4, &mut NoPointers), Err(Invalid::at(4)));
/// ```
pub trait Visit {
  /// What ends a validation: [`Invalid`], or an error of the visitor's own.
  type Error: From<Invalid>;

  /// The pointer of type `P` at the position `at` of the bytes.
  ///
  /// It is called for pointers only, but instantiated for every type that a validated value
  /// holds (the compiler makes the instances of every branch on a constant): a `const` in it
  /// must not fail for a type that is no pointer.
  ///
  /// # Errors
  /// Whatever the visitor finds wrong with it; the validation ends with that error.
  fn pointer<P: ?Sized + Pun>(&mut self, at: usize) -> Result<(), Self::Error>;
}

/// Where bytes stop being a valid value ([`validate_in`]): a position in the bytes.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Invalid {
  /// The position of the scalar that is not valid, of the tag of an enum that is no variant's,
  /// of the union that is none of its fields, or of the struct of which a rule does not hold; of
  /// the value if its bytes are not all there.
  pub at: usize,
}

impl Invalid {
  /// Bytes that stop being a valid value at `at`.
  #[must_use]
  pub const fn at(at: usize) -> Self {
    Self { at }
  }
}

impl fmt::Debug for Invalid {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "Invalid {{ at: {} }}", self.at)
  }
}

impl fmt::Display for Invalid {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "invalid bytes at {}", self.at)
  }
}

impl core::error::Error for Invalid {}

/// What a self-relative pointer type `P` aims at: one value, or an element of a sequence; `()`
/// for text. What a [`Visit`] validates behind a pointer.
pub type Pointee<P> = <P as Pun>::Pointee;

/// Validates the `T` at the position `at` of `bytes`, everything of it but what its pointers aim
/// at, and hands each of its pointers to `visit` ([`Visit`]).
///
/// It is safe, for any bytes and any `at`: it reads nothing out of `bytes`, does not panic, and
/// does not recurse through pointers (the visitor does, or does not). The bytes may be at any
/// address.
///
/// # The contract
///
/// After `Ok(())`, unsafe code may rely on this, and on nothing more:
///
/// 1. **In bounds**: `at + size_of::<T>()` is at most `bytes.len()`.
/// 2. **Valid but for what pointers aim at**: the `size_of::<T>()` bytes from `at` are a valid
///    `T` as its layout says ([`Pun`]), provided that every pointer handed to `visit` is a valid
///    pointer of its type (S1 to S7 of [`SelfRelative`](crate::SelfRelative)): every scalar is
///    one of its valid values (`bool`, `char`, `NonZero`, enums without fields, `valid(..)` of a
///    field), the tag of every enum is the tag of a variant (or in its niche) whose fields are
///    valid, every union is one of its fields, and every rule of every struct holds (`assert`,
///    `size`, and any other), evaluated on the bytes of that struct only.
/// 3. **Every pointer handed over**: each self-relative pointer of the value, where it is, was
///    handed to `visit` once, with its type and its position, and `visit` returned `Ok`; but
///    bytes of a pointer that are all zero where the pointer may be null (no pointer, then). Bytes
///    of zeros of a pointer whose type has no such value (S1 of `SelfRelative`: its offset is a
///    `NonZero`) are not valid; those of any other are a pointer, handed over.
///
/// It says nothing of what the pointers aim at: whether they aim in the bytes, at what, whether
/// two aim at the same place. That is the visitor's business, as is the scheme of the pointers.
/// And it says nothing of addresses: it knows positions in `bytes` only. A `&T` made at
/// `bytes.as_ptr() + at` must be at an address aligned for `T` (`align_of::<T>()`), which is the
/// caller's duty: the buffer at an address so aligned, and `at` a multiple of it. Likewise for
/// what the pointers aim at, which their scheme places.
///
/// A `T` without size has no bytes to check: it is valid if it has a value. `T` is `Sized`: a
/// value whose size its bytes tell (a slice tail, `str`) is not one that a buffer of pointers
/// holds by value, and does not compile here.
///
/// # Errors
/// [`Invalid`] where the bytes stop being a valid `T` (the first such place, in the order of the
/// positions of what is checked; the error of the visitor if it comes first).
#[inline]
pub fn validate_in<T: Pun, V: Visit>(
  bytes: &[u8],
  at: usize,
  visit: &mut V,
) -> Result<(), V::Error> {
  const { assert!(T::LAYOUT.checks_kept(), "{}", layout::VISIT_UNSPECIFIED) };
  let size = const { T::LAYOUT.size() };
  match bytes.get(at..).and_then(|rest| rest.get(..size)) {
    // SAFETY: the `size` bytes of a `T` at `at`, initialized, as `value` requires.
    Some(_) => unsafe { value::<T, V>(bytes.as_ptr(), at, visit) },
    None => Err(Invalid::at(at).into()),
  }
}

/// Validates the `count` values of `T` from the position `at` of `bytes`, `size_of::<T>()`
/// apart, in order, as [`validate_in`] validates one.
///
/// Elements that hold no pointer are validated as `try_pun_ref` does a slice, by blocks where it
/// can (then only the first invalid one is walked again, to tell where); elements that hold
/// pointers one by one, each pointer handed to `visit` in order. Elements of which every value is
/// valid are not looked at.
///
/// The contract is that of [`validate_in`], for each of the `count` values; in bounds means that
/// `at + count * size_of::<T>()` does not overflow and is at most `bytes.len()`.
///
/// # Errors
/// As [`validate_in`]: at the first element that is not valid, or if they are not all there (at
/// `at`).
#[inline]
pub fn validate_elems_in<T: Pun, V: Visit>(
  bytes: &[u8],
  at: usize,
  count: usize,
  visit: &mut V,
) -> Result<(), V::Error> {
  const { assert!(T::LAYOUT.checks_kept(), "{}", layout::VISIT_UNSPECIFIED) };
  let size = const { T::LAYOUT.size() };
  let Some(elems) =
    (size.checked_mul(count)).and_then(|len| bytes.get(at..).and_then(|rest| rest.get(..len)))
  else {
    return Err(Invalid::at(at).into());
  };
  if const { T::LAYOUT.leaves == 0 } {
    return Ok(());
  }
  if size == 0 {
    // No bytes: one is as valid as any other.
    return if count == 0 { Ok(()) } else { validate_in::<T, V>(bytes, at, visit) };
  }
  if const { !T::LAYOUT.has_pointers() } {
    // SAFETY: `count` values of `T`, in bounds and initialized. `validate` reads them at any
    // alignment.
    let valid = unsafe { validate_elems::<T>(elems.as_ptr(), count) };
    if valid {
      return Ok(());
    }
  }
  let mut i = 0;
  while i < count {
    // SAFETY: the element `i`, in bounds (above).
    unsafe { value::<T, V>(bytes.as_ptr(), at + i * size, visit)? };
    i += 1;
  }
  // The validation of the elements said that one is not valid, and none was found so: the
  // verdict is the validation's.
  if const { !T::LAYOUT.has_pointers() } {
    return Err(Invalid::at(at).into());
  }
  Ok(())
}

/// Hands `visit` each self-relative pointer of the valid `T` at the position `at` from `base`,
/// as [`validate_in`] would, and checks nothing.
///
/// It is for a value already known to be valid, such as one being copied or moved in its buffer.
///
/// `visit.pointer::<P>(at)` is called once for each pointer that the value holds where it is (in
/// its fields, in the variant that its tag tells, in the elements of its arrays), with its type
/// and its position, in the order of their positions; bytes of a pointer that are all zero are not
/// handed over if the pointer may be null (no pointer), and are handed over otherwise. These are
/// the calls that [`validate_in`] makes on the same bytes, in the same order (the model check
/// compares them). It reads the tags of enums (and their niches) and the offsets of pointers that
/// may be null, nothing else, at any alignment; it does not recurse through pointers. An error
/// returned by `pointer` ends the walk, with that error.
///
/// # Safety
///
/// `base + at` is valid for reads of `size_of::<T>()` bytes, at any alignment, that hold a `T` as
/// its types say, whatever its rules say:
///
/// - every scalar is initialized and a valid value of its type (`bool`, `char`, `NonZero`, an
///   enum without fields, the offset of a pointer);
/// - the tag of every enum (or its niche) is initialized and tells one of its variants, and the
///   bytes that this variant holds are so in turn.
///
/// The rules of its structs need not hold (`assert`, `size`, `len`, regions), nor the `valid(..)`
/// of a field: safe code can break them, and nothing here reads them (a restricted field is a
/// scalar, which holds no pointer). Padding, and what the variant of an enum does not hold, may be
/// anything, uninitialized too: it is not read. Nothing is required of what the pointers aim at,
/// which is not read either. A valid `T` (a `&T`, whatever values safe code wrote to its fields;
/// the bytes of a buffer that [`validate_in`] accepted) is such.
///
/// # Errors
/// The first error of the visitor.
#[inline]
pub unsafe fn pointers_in<T: Pun, V: Visit>(
  base: *const u8,
  at: usize,
  visit: &mut V,
) -> Result<(), V::Error> {
  const { assert!(T::LAYOUT.checks_kept(), "{}", layout::VISIT_UNSPECIFIED) };
  // SAFETY: per the caller.
  unsafe { follow::<T, V>(base, at, visit) }
}

/// [`pointers_in`] for the `count` values of `T` from the position `at` of `base`,
/// `size_of::<T>()` apart, in order. Elements that hold no pointer are not looked at.
///
/// # Safety
/// As [`pointers_in`], for each of the `count` values.
///
/// # Errors
/// The first error of the visitor.
#[inline]
pub unsafe fn pointers_elems_in<T: Pun, V: Visit>(
  base: *const u8,
  at: usize,
  count: usize,
  visit: &mut V,
) -> Result<(), V::Error> {
  const { assert!(T::LAYOUT.checks_kept(), "{}", layout::VISIT_UNSPECIFIED) };
  if const { !T::LAYOUT.has_pointers() } {
    return Ok(());
  }
  let size = const { T::LAYOUT.size() };
  let mut i = 0;
  while i < count {
    // SAFETY: per the caller, the element `i`.
    unsafe { follow::<T, V>(base, at + i * size, visit)? };
    i += 1;
  }
  Ok(())
}

// The walks of `pointers_in` and `copy_in` are inlined into one function per type where code is
// optimized, but not where it is built for debugging (`debug_assertions`): there the compiler
// gives every inlined body its own stack slots, and a crate that recurses through pointers with
// these walks (a deep copy) took kilobytes of stack per level of nesting.

/// The pointers of the valid `T` at `at`, handed over in order ([`pointers_in`]).
///
/// # Safety
/// As [`pointers_in`].
#[cfg_attr(not(debug_assertions), inline(always))]
unsafe fn follow<T: ?Sized + Pun, V: Visit>(
  base: *const u8,
  at: usize,
  visit: &mut V,
) -> Result<(), V::Error> {
  if const { !T::LAYOUT.has_pointers() } {
    return Ok(());
  }
  const {
    assert!(
      !(matches!(T::LAYOUT.kind, Kind::Enum(layout) if layout.union) && T::LAYOUT.has_pointers()),
      "a union that holds a self-relative pointer: which field it is cannot be told, so neither \
       can the pointer"
    );
  };
  // The walk goes by the lists of types of `T`, as `validate_in` does.
  const { assert!(listed::<T>(), "{}", LISTS) };
  // SAFETY: per the caller; the layout of `T` tells what is at `at`.
  unsafe {
    match const { T::LAYOUT.shape() } {
      Shape::Array(len) => {
        let size = const { <T::Elem as Pun>::LAYOUT.size() };
        let mut i = 0;
        while i < len {
          follow::<T::Elem, V>(base, at + i * size, visit)?;
          i += 1;
        }
        Ok(())
      }
      Shape::Struct => follow_fields::<T::Fields, V>(base, at, visit),
      Shape::Enum(layout) => {
        // A tag that is not a scalar has no value: no valid `T` is given.
        let Kind::Scalar(spec) = layout.tag.layout.kind else { return Ok(()) };
        let tag = load(base.add(at + layout.tag.offset), spec.size, spec.endian);
        follow_variants::<T::Variants, V>(base, at, tag, visit).unwrap_or(Ok(()))
      }
      // All zeros, where the pointer may be null: no pointer.
      Shape::Pointer(pointer) => {
        if pointer.nullable && load(base.add(at), pointer.width, pointer.endian) == 0 {
          Ok(())
        } else {
          visit.pointer::<T>(at)
        }
      }
      // No pointer there.
      Shape::Scalar(_) | Shape::Slice | Shape::Pattern(_) => Ok(()),
    }
  }
}

/// The pointers of the field `F` of the struct (or of the variant) at `at`, and of the ones after
/// it, in the order of their declarations.
///
/// # Safety
/// As [`pointers_in`], for the struct (or the enum) that `F` is a field of.
#[cfg_attr(not(debug_assertions), inline(always))]
unsafe fn follow_fields<F: Fields, V: Visit>(
  base: *const u8,
  at: usize,
  visit: &mut V,
) -> Result<(), V::Error> {
  let Some(field) = (const { F::FIELD }) else { return Ok(()) };
  // SAFETY: per the caller; the field is where its layout says. A restricted field is a scalar,
  // which holds no pointer.
  unsafe {
    if field.restriction().is_none() {
      follow::<F::Type, V>(base, at + field.offset(), visit)?;
    }
    follow_fields::<F::Next, V>(base, at, visit)
  }
}

/// The pointers of the variant, among `W` and the ones after it, whose tag is `tag`: `None` if
/// there is none.
///
/// # Safety
/// As [`pointers_in`], for the enum at `at`.
#[cfg_attr(not(debug_assertions), inline(always))]
unsafe fn follow_variants<W: Variants, V: Visit>(
  base: *const u8,
  at: usize,
  tag: u128,
  visit: &mut V,
) -> Option<Result<(), V::Error>> {
  let expected = const { W::TAG }?;
  // As `variants`: the variant of a niche, or one that no value is, has ranges of tags.
  let this = (const { W::RANGES }).map_or(tag == expected, |ranges| layout::contains(ranges, tag));
  // SAFETY: per the caller.
  unsafe {
    if this {
      Some(follow_fields::<W::Fields, V>(base, at, visit))
    } else {
      follow_variants::<W::Next, V>(base, at, tag, visit)
    }
  }
}

/// Copies the valid `T` at `src` to `dst`: every byte that a valid `T` holds initialized, but
/// those of its pointers, which the caller writes.
///
/// That is each scalar of it (a scalar that may be uninitialized, such as `MaybeUninit`, is
/// none), the tag of each enum and the fields of the variant that it tells. The bytes of a
/// self-relative pointer are copied if it is no pointer (one that may be null whose bytes are all
/// zero), and not written otherwise: those are the pointers that [`pointers_in`] hands over, in
/// the same order, whose offsets the caller writes, re-aimed for where the copy is, or aimed at a
/// copy of their targets. It is for a crate that writes a value into a buffer of initialized
/// bytes, which a copy of the whole value would not keep so: a typed copy leaves the padding of
/// the destination uninitialized, and a copy of the bytes copies the padding of the source, which
/// may be uninitialized.
///
/// Bytes of a valid `T` that may be uninitialized (padding, the bytes of the variants that the
/// value is not, fields that may be uninitialized) are never read from `src` nor written to `dst`:
/// those bytes of `dst` keep what they held. A value without such bytes and without pointers
/// ([`Layout::is_plain`]) is copied whole, at once.
///
/// It reads the tags of enums (their niches included, as [`validate_in`] does) to tell the
/// variant; it does not recurse through pointers. A union does not compile there: which of its
/// fields it is, and so which of its bytes are initialized, cannot be told.
///
/// [`Layout::is_plain`]: layout::Layout::is_plain
///
/// # Safety
///
/// `src` is valid for reads of the `size_of::<T>()` bytes of a `T` as its types say, whatever its
/// rules say, at any alignment, as for [`pointers_in`]: every scalar initialized and valid for its
/// type, the tag of every enum (or its niche) initialized and telling one of its variants, whose
/// bytes are so in turn. The rules of its structs and the `valid(..)` of its fields need not hold:
/// none is read, and a restricted field is copied as the scalar of its type that it is. The bytes
/// that may be uninitialized may be so. `dst` is valid for writes of `size_of::<T>()` bytes, at
/// any alignment, and the two do not overlap.
#[inline]
pub unsafe fn copy_in<T: Pun>(src: *const u8, dst: *mut u8) {
  // SAFETY: per the caller.
  unsafe { copy::<T>(src, dst) }
}

/// Whatever every byte of every value of the layout is initialized, and no pointer is there:
/// [`Layout::is_plain`].
///
/// [`Layout::is_plain`]: layout::Layout::is_plain
const fn whole(layout: &layout::Layout) -> bool {
  match layout.kind {
    Kind::Scalar(spec) => !spec.may_be_uninit(),
    Kind::Array(elem, _) => whole(elem),
    Kind::Struct(fields, _) => {
      let (mut i, mut covered) = (0, 0);
      while i < fields.len() {
        // Sorted by offset: a field after the bytes covered so far leaves a hole.
        if fields[i].offset != covered || !whole(fields[i].layout) {
          return false;
        }
        covered += fields[i].layout.size;
        i += 1;
      }
      !layout.unsized_ && covered == layout.size
    }
    Kind::Pointer(_) | Kind::Slice(_) | Kind::Pattern(_) | Kind::Enum(_) => false,
  }
}

/// The copy of the valid `T` at `src` to `dst` ([`copy_in`]).
///
/// # Safety
/// As [`copy_in`].
#[cfg_attr(not(debug_assertions), inline(always))]
unsafe fn copy<T: ?Sized + Pun>(src: *const u8, dst: *mut u8) {
  let size = const { T::LAYOUT.size() };
  if const { whole(&T::LAYOUT) } {
    // SAFETY: per the caller, `size` initialized bytes, which do not overlap.
    unsafe { core::ptr::copy_nonoverlapping(src, dst, size) };
    return;
  }
  const {
    assert!(
      !matches!(T::LAYOUT.kind, Kind::Enum(layout) if layout.union),
      "`copy_in` of a union: which of its fields it is, and so which of its bytes are \
       initialized, cannot be told"
    );
  };
  // The walk goes by the lists of types of `T`, as `validate_in` does.
  const { assert!(listed::<T>(), "{}", LISTS) };
  // SAFETY: per the caller; the layout of `T` tells what is where.
  unsafe {
    match const { T::LAYOUT.shape() } {
      Shape::Array(len) => {
        let size = const { <T::Elem as Pun>::LAYOUT.size() };
        let mut i = 0;
        while i < len {
          copy::<T::Elem>(src.add(i * size), dst.add(i * size));
          i += 1;
        }
      }
      Shape::Struct => copy_fields::<T::Fields>(src, dst),
      Shape::Enum(layout) => {
        // A tag that is not a scalar has no value: no valid `T` is given.
        let Kind::Scalar(spec) = layout.tag.layout.kind else { return };
        let place = layout.tag.offset;
        let tag = load(src.add(place), spec.size, spec.endian);
        core::ptr::copy_nonoverlapping(src.add(place), dst.add(place), spec.size);
        copy_variants::<T::Variants>(src, dst, tag);
      }
      // All zeros, where the pointer may be null: no pointer, whose bytes are copied; a pointer
      // is left to the caller.
      Shape::Pointer(pointer) => {
        if pointer.nullable && load(src, pointer.width, pointer.endian) == 0 {
          core::ptr::copy_nonoverlapping(src, dst, size);
        }
      }
      // A scalar that may be uninitialized is not copied, and a sized value holds neither of the
      // others.
      Shape::Scalar(_) | Shape::Slice | Shape::Pattern(_) => {}
    }
  }
}

/// The copy of the field `F` of the struct (or of the variant) at `src`, and of the ones after it.
///
/// # Safety
/// As [`copy_in`], for the struct (or the enum) that `F` is a field of.
#[cfg_attr(not(debug_assertions), inline(always))]
unsafe fn copy_fields<F: Fields>(src: *const u8, dst: *mut u8) {
  let Some(field) = (const { F::FIELD }) else { return };
  let off = field.offset();
  // SAFETY: per the caller; the field is where its layout says. A restricted field is a scalar
  // of its type, whose bytes are those of the type.
  unsafe {
    copy::<F::Type>(src.add(off), dst.add(off));
    copy_fields::<F::Next>(src, dst);
  }
}

/// The copy of the fields of the variant, among `W` and the ones after it, whose tag is `tag`;
/// nothing if there is none.
///
/// # Safety
/// As [`copy_in`], for the enum at `src`.
#[cfg_attr(not(debug_assertions), inline(always))]
unsafe fn copy_variants<W: Variants>(src: *const u8, dst: *mut u8, tag: u128) {
  let Some(expected) = (const { W::TAG }) else { return };
  // As `variants`: the variant of a niche, or one that no value is, has ranges of tags.
  let this = (const { W::RANGES }).map_or(tag == expected, |ranges| layout::contains(ranges, tag));
  // SAFETY: per the caller.
  unsafe {
    if this {
      copy_fields::<W::Fields>(src, dst);
    } else {
      copy_variants::<W::Next>(src, dst, tag);
    }
  }
}

/// Whatever the `count` values of `T` at `base` are valid: the slice validation of `validate`.
///
/// # Safety
/// `base` is valid for reads of `count` initialized `T`s, at any alignment.
#[inline(always)]
const unsafe fn validate_elems<T: Pun>(base: *const u8, count: usize) -> bool {
  // SAFETY: per the caller: a slice of `count` `T`s, which `validate` reads at any alignment.
  unsafe { validate::<All, Root, [T]>(ptr::from_raw_parts::<[T]>(base, count)) }
}

/// The `T` at the position `at` from `base`.
///
/// # Safety
/// `base + at` is valid for reads of the `size_of::<T>()` bytes of a `T`, all initialized (any
/// alignment).
#[inline(always)]
unsafe fn value<T: ?Sized + Pun, V: Visit>(
  base: *const u8,
  at: usize,
  visit: &mut V,
) -> Result<(), V::Error> {
  if const { T::LAYOUT.leaves == 0 } {
    return Ok(());
  }
  const {
    assert!(
      !(matches!(T::LAYOUT.kind, Kind::Enum(layout) if layout.union) && T::LAYOUT.has_pointers()),
      "a union that holds a self-relative pointer: which field it is cannot be told, so neither \
       can the pointer"
    );
  };
  // The walk goes by the lists of types of `T`, which the derive writes: they must be its layout.
  const { assert!(listed::<T>(), "{}", LISTS) };
  // SAFETY: per the caller.
  let here = unsafe { base.add(at) };
  if const { !T::LAYOUT.has_pointers() } {
    // The code of `try_pun_ref` is the verdict; where it fails, the walk tells where, and the
    // value itself if the walk finds nothing.
    // SAFETY: per the caller; a sized `T` (`Sized` at the root, so are its parts).
    if unsafe { validate::<All, Root, T>(ptr::from_raw_parts::<T>(here, 0)) } {
      return Ok(());
    }
    // SAFETY: per the caller.
    return match unsafe { walk::<T, V>(base, at, here, visit) } {
      Ok(()) => Err(Invalid::at(at).into()),
      error => error,
    };
  }
  // SAFETY: per the caller.
  unsafe { walk::<T, V>(base, at, here, visit) }
}

/// The walk of the `T` at `at` (at `here`): where it is not valid, its pointers handed over.
///
/// # Safety
/// As [`value`]; `here` is `base + at`.
#[inline(always)]
unsafe fn walk<T: ?Sized + Pun, V: Visit>(
  base: *const u8,
  at: usize,
  here: *const u8,
  visit: &mut V,
) -> Result<(), V::Error> {
  // SAFETY: per the caller; the layout of `T` tells what is at `here`.
  unsafe {
    match const { T::LAYOUT.shape() } {
      Shape::Scalar(spec) => {
        if spec.matches(here) {
          Ok(())
        } else {
          Err(Invalid::at(at).into())
        }
      }
      Shape::Array(len) => {
        let size = const { <T::Elem as Pun>::LAYOUT.size() };
        let mut i = 0;
        while i < len {
          value::<T::Elem, V>(base, at + i * size, visit)?;
          i += 1;
        }
        Ok(())
      }
      Shape::Struct => {
        fields::<T::Fields, V>(base, at, visit)?;
        // The rules of `T`, each operation applied by an instance of its own (`T::Rules`).
        if const { T::LAYOUT.rules().is_empty() } || check_rules::<T>(ptr::from_raw_parts(here, 0))
        {
          Ok(())
        } else {
          Err(Invalid::at(at).into())
        }
      }
      // Not valid: `validate` said so above (a union holds no pointer, see below).
      Shape::Enum(layout) if layout.union => Err(Invalid::at(at).into()),
      Shape::Enum(layout) => enumeration::<T, V>(base, at, layout, visit),
      // All zeros: no pointer if it may be null; not a value if its type has none of zeros (a
      // `NonZero` offset); else a pointer whose offset is zero, which its scheme reads.
      Shape::Pointer(pointer) => match load(here, pointer.width, pointer.endian) {
        0 if pointer.nullable => Ok(()),
        0 if pointer.nonzero => Err(Invalid::at(at).into()),
        _ => visit.pointer::<T>(at),
      },
      // A sized value holds neither (the root is `Sized`, and so are its parts).
      Shape::Slice | Shape::Pattern(_) => Err(Invalid::at(at).into()),
    }
  }
}

/// What the walk is refused for: lists of types that are not the layout.
const LISTS: &str = "an implementation of `Pun` lists fields, variants or elements that are not \
                     those of its layout: `validate_in` goes by the lists, and would not check \
                     what they leave out";

/// Whatever the lists of types of `T` (`Fields`, `Variants`, `Elem`, `Pointee`), by which the
/// walk goes, are what its layout holds: every field, by its declaration index, at its offset, of
/// its layout (restricted or not), of a type whose layout it is; every variant, its tag, its tags
/// and its fields; the element; the size and the alignment of what a pointer aims at.
const fn listed<T: ?Sized + Pun>() -> bool {
  match T::LAYOUT.kind {
    Kind::Struct(fields, _) => fields_listed::<T::Fields>(fields, 0),
    Kind::Enum(layout) => variants_listed::<T::Variants>(layout.variants, 0),
    Kind::Array(elem, _) | Kind::Slice(elem) => <T::Elem as Pun>::LAYOUT.equals(elem),
    Kind::Pointer(pointer) => {
      let target = <T::Pointee as Pun>::LAYOUT;
      target.size() == pointer.size && target.align() == pointer.align
    }
    Kind::Scalar(_) | Kind::Pattern(_) => true,
  }
}

/// The list `F` is the `fields`, from the one declared at `index` on, in that order.
const fn fields_listed<F: Fields>(fields: &[layout::Field], index: usize) -> bool {
  let Some(field) = F::FIELD else { return index == fields.len() };
  let mut i = 0;
  let mut found = false;
  while i < fields.len() {
    let other = &fields[i];
    found |= other.index == index
      && other.offset == field.offset
      && other.restricted == field.restricted
      && other.layout.equals(field.layout);
    i += 1;
  }
  found
    && field.index == index
    && (field.restricted || <F::Type as Pun>::LAYOUT.equals(field.layout))
    && fields_listed::<F::Next>(fields, index + 1)
}

/// The list `W` is the `variants`, from the one at `index` on: the same tags, the same tags of a
/// niche, the same fields.
const fn variants_listed<W: Variants>(variants: &[layout::Variant], index: usize) -> bool {
  let Some(tag) = W::TAG else { return index == variants.len() };
  index < variants.len()
    && variants[index].tag == tag
    && same_ranges(W::RANGES, variants[index].ranges)
    && fields_listed::<W::Fields>(variants[index].fields(), 0)
    && variants_listed::<W::Next>(variants, index + 1)
}

/// Whatever two lists of ranges of tags are the same.
const fn same_ranges(a: Option<&[(u128, u128)]>, b: Option<&[(u128, u128)]>) -> bool {
  match (a, b) {
    (None, None) => true,
    (Some(a), Some(b)) => {
      if a.len() != b.len() {
        return false;
      }
      let mut i = 0;
      while i < a.len() {
        if a[i].0 != b[i].0 || a[i].1 != b[i].1 {
          return false;
        }
        i += 1;
      }
      true
    }
    _ => false,
  }
}

/// The field `F` of the struct (or of the variant) at `at`, and the ones after it, in the order
/// of their declarations.
///
/// # Safety
/// As [`value`], for the struct (or the enum) that `F` is a field of.
#[inline(always)]
unsafe fn fields<F: Fields, V: Visit>(
  base: *const u8,
  at: usize,
  visit: &mut V,
) -> Result<(), V::Error> {
  let Some(field) = (const { F::FIELD }) else { return Ok(()) };
  let place = at + field.offset();
  // SAFETY: per the caller; the field is where its layout says.
  unsafe {
    match field.restriction() {
      // A scalar that accepts less than its type does.
      Some(spec) => {
        if !spec.matches(base.add(place)) {
          return Err(Invalid::at(place).into());
        }
      }
      None => value::<F::Type, V>(base, place, visit)?,
    }
    fields::<F::Next, V>(base, at, visit)
  }
}

/// The enum at `at`: its tag, and the fields of the variant that it tells.
///
/// # Safety
/// As [`value`]; `layout` is the one of `T`.
#[inline(always)]
unsafe fn enumeration<T: ?Sized + Pun, V: Visit>(
  base: *const u8,
  at: usize,
  layout: &Enum,
  visit: &mut V,
) -> Result<(), V::Error> {
  let Kind::Scalar(spec) = layout.tag.layout.kind else { return Err(Invalid::at(at).into()) };
  let place = at + layout.tag.offset;
  // SAFETY: the tag is in the bytes of the enum.
  let tag = unsafe { load(base.add(place), spec.size, spec.endian) };
  // SAFETY: per the caller.
  unsafe { variants::<T::Variants, V>(base, at, tag, visit) }
    .unwrap_or_else(|| Err(Invalid::at(place).into()))
}

/// The fields of the variant, among `W` and the ones after it, whose tag is `tag`: `None` if there
/// is none.
///
/// # Safety
/// As [`value`], for the enum at `at`.
#[inline(always)]
unsafe fn variants<W: Variants, V: Visit>(
  base: *const u8,
  at: usize,
  tag: u128,
  visit: &mut V,
) -> Option<Result<(), V::Error>> {
  let expected = const { W::TAG }?;
  // The variant of a niche, or one that no value is (no range), has ranges of tags.
  let this = (const { W::RANGES }).map_or(tag == expected, |ranges| layout::contains(ranges, tag));
  // SAFETY: per the caller.
  unsafe {
    if this {
      Some(fields::<W::Fields, V>(base, at, visit))
    } else {
      variants::<W::Next, V>(base, at, tag, visit)
    }
  }
}
