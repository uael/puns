//! Validation, walk and copy of a value in a buffer that holds self-relative pointers, for the
//! crate that declares the pointers.
//!
//! [`validate_in`] and [`validate_elems_in`] validate values and hand each pointer to a [`Visit`]:
//! they call its `pointer` method with the type and the position of the pointer. [`pointers_in`]
//! and [`pointers_elems_in`] hand over the pointers of values known to be valid, and check nothing.
//! [`copy_in`] copies the initialized bytes of a valid value.
//!
//! Like `validate`, the functions recurse over types, so the layout is a constant in each instance
//! and no code is compiled for what cannot be invalid.
//!
//! # The walk
//!
//! What holds no pointer is validated by `validate`, at the speed of `try_pun_ref`, and its answer
//! is the verdict. Where it fails, the walk by position ([`value`], [`fields`], [`enumeration`])
//! goes over the value again to find where. If that walk finds nothing, the error is reported at
//! the value itself, which is less precise but never wrong. The elements of [`validate_elems_in`]
//! are validated the same way, and the test `the_verdict_is_the_one_of_try_pun_ref` checks that the
//! two give the same answers.
//!
//! The walk follows the lists of types that the derive writes (`Fields`, `Variants`, `Elem`,
//! `Pointee`), and checks them against the layout of every type that it goes through ([`listed`]),
//! as [`deep`](crate::deep) does for `view_ref`. It checks each field by its declaration index,
//! offset, layout and type, each variant by its tag, its niche tags and its fields, the element
//! type, and the target of a pointer. Without that check, a list that left a field out would leave
//! the field unchecked (compile-fail test `validate_lists_not_the_layout`). A struct that holds
//! pointers is walked field by field, so that its pointers are handed over in order, between the
//! checks of the rest, and a visitor whose queue is full can stop the walk where it is with an
//! error of its own. The functions recurse over types only, never through pointers, and read
//! nothing outside the bytes, since each read follows a `get` and checked arithmetic, at a position
//! in bounds.
//!
//! The bytes are a `&[u8]`, so they are initialized, and a `MaybeUninit` or padding may hold any
//! bytes. A `MaybeUninit` of a pointer is not a pointer, and nothing in it is handed over. A union
//! that holds a pointer does not compile, because which field the union holds cannot be told, and
//! so neither can whether it holds the pointer.
//!
//! [`pointers_in`] makes the same calls as [`validate_in`] on the same bytes, in the same order,
//! because [`variants`] and [`follow_variants`] choose the variant of an enum by the same test on
//! its tag or niche. Apart from tags, it reads only the offsets of pointers that may be null, and
//! it evaluates no rule. The rules of a value and the `valid(..)` of its fields need not hold,
//! since safe code can break them in a value that it makes, such as `Span { lo: 2, hi: 1 }` for
//! `assert(lo <= hi)`, and `nearest` walks and copies such a value all the same. A type without
//! pointers is not walked at all.
//!
//! [`copy_in`] walks by the same lists of types as `validate_in` and chooses the variant by the
//! same test, so that it compiles to a few moves for each type, rather than copying everything and
//! then writing the pointers a second time. It refuses a union (compile-fail test `copy_in_union`).
//! Its source is what `pointers_in` reads, a value as its types say, whatever its rules say, since
//! it reads no rule, and copies a restricted field as the scalar of its type.
//!
//! # Tests
//!
//! `tests/validate_in.rs` checks the answer and the error positions for each kind of type, and runs
//! random bytes, positions and counts, some of which overflow, under Miri on every target of CI. On
//! the bytes that `validate_in` accepts there, it checks that `pointers_in` and `pointers_elems_in`
//! hand over the same pointers at any alignment, with the bytes that they need not read left
//! uninitialized. It checks that `copy_in` of a value on the stack, whose padding and other variant
//! are uninitialized, into memory that is then read whole as a `&[u8]`, copies the value and
//! nothing else, and that `copy_in` of random accepted bytes gives a value that `validate_in`
//! accepts again. Last, `rules_need_not_hold_for_pointers_in_and_copy_in` breaks the `assert` of a
//! value with pointers and the `valid(..)` of one of its fields, in a buffer. `validate_in` refuses
//! the value, while `pointers_in` hands over its pointers and `copy_in` copies its scalars and its
//! null pointer, as for a value that meets them. The model check compares these functions with an
//! oracle of pointers (`puns-model`, "Self-relative pointers").

use core::fmt;

use crate::{
  Fields, Pun, Root, Variants,
  layout::{self, Enum, Kind, Shape, load},
  ptr,
  validate::{All, check_rules, validate},
};

/// A visitor of the self-relative pointers of a value in a buffer.
///
/// [`validate_in`] and [`validate_elems_in`] hand each pointer of the value they validate to the
/// visitor, which belongs to the crate that declares the pointers. The visitor follows each pointer
/// as its scheme says, and validates the target in turn or queues it. [`pointers_in`] and
/// [`pointers_elems_in`] hand over the same pointers of a value already known to be valid, without
/// checking anything, so that a crate can copy what the pointers aim at, or update them when the
/// value moves.
///
/// The walk calls `pointer::<P>(at)` once for each pointer that the value holds, in its fields, in
/// the variant that the tag of an enum selects, and in the elements of its arrays. `P` is the
/// pointer type, whose `LAYOUT` describes it ([`Layout::as_pointer`]), and `at` is the position of
/// the pointer in the bytes. The calls come in the order of the walk, interleaved with the checks
/// of the rest of the value: the fields of a struct or a variant in declaration order, and the
/// elements of an array in order, which for a `repr(C)` type is the order of their positions.
///
/// A pointer whose bytes are all zero is not handed over if the pointer may be null, since it is
/// then not a pointer, nor if its type has no value of all zero bytes (its offset is a `NonZero`),
/// since it is then not valid. Any other pointer of all zero bytes is handed over. An error
/// returned by `pointer` ends the walk with that error.
///
/// [`Layout::as_pointer`]: layout::Layout::as_pointer
///
/// # Examples
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
  /// The error that ends a walk, either [`Invalid`] or an error of the visitor's own.
  type Error: From<Invalid>;

  /// Visits the pointer of type `P` at the position `at` of the bytes.
  ///
  /// The walk calls this method for pointers only, but the compiler instantiates it for every type
  /// that the walked value holds, since it instantiates every branch on a constant. A `const` block
  /// in an implementation must therefore not fail for a `P` that is not a pointer.
  ///
  /// # Errors
  ///
  /// Returns the error that the visitor finds with the pointer, if any. The walk ends with that
  /// error.
  fn pointer<P: ?Sized + Pun>(&mut self, at: usize) -> Result<(), Self::Error>;
}

/// The position at which bytes stop being a valid value, as [`validate_in`] reports it.
///
/// This type is `#[non_exhaustive]`, so that it may later say why. Make one with [`Invalid::at`].
///
/// # Examples
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
/// // The second `bool`, at position 2, holds 2.
/// let bytes = [0, 1, 2];
/// let error = puns::validate_in::<[bool; 2], _>(&bytes, 1, &mut NoPointers).unwrap_err();
/// assert_eq!(error.at, 2);
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Invalid {
  /// The position in the bytes of what is not valid: a scalar that is not a valid value, the tag
  /// of an enum that selects no variant, a union that is none of its fields, or a struct one of
  /// whose rules does not hold. It is the position of the value itself if its bytes are not all
  /// there.
  pub at: usize,
}

impl Invalid {
  /// Creates an error at the position `at`.
  ///
  /// # Examples
  ///
  /// ```
  /// assert_eq!(puns::Invalid::at(4).at, 4);
  /// ```
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

/// The type that a self-relative pointer type `P` aims at (the target of one value, or an element
/// of a sequence, or `()` for text).
///
/// A [`Visit`] validates what a pointer aims at as this type, with [`validate_in`] or
/// [`validate_elems_in`].
pub type Pointee<P> = <P as Pun>::Pointee;

/// Validates the `T` at the position `at` of `bytes`, except what its pointers aim at, and hands
/// each of its self-relative pointers to `visit`.
///
/// The function is safe for any bytes and any `at`. It reads nothing outside `bytes`, and does not
/// panic unless `visit` does. It does not follow pointers. The visitor decides whether to follow
/// them. The bytes may be at any address.
///
/// After `Ok(())`, unsafe code may rely on the following, and on nothing more:
///
/// 1. **In bounds.** `at + size_of::<T>()` is at most `bytes.len()`.
/// 2. **Valid.** The `size_of::<T>()` bytes from `at` are a [valid] `T`, provided that every
///    pointer handed to `visit` is a valid pointer of its type, as the `# Safety` section of
///    [`SelfRelative`](crate::SelfRelative#safety) requires. Every scalar is one of its valid
///    values: `bool`, `char`, `NonZero`, enums without fields, and the `valid(..)` restriction of a
///    field. The tag of every enum, or its niche, selects a variant whose fields are valid. Every
///    union is one of its fields. Every rule of every struct holds (`assert`, `size`, and the
///    others), evaluated on the bytes of that struct only.
/// 3. **Pointers handed over.** Each self-relative pointer of the value was handed to `visit` once,
///    with its type and its position, and `visit` returned `Ok`. The exception is a pointer that
///    may be null and whose bytes are all zero. It is no pointer, and is not handed over. All zero
///    bytes are not valid for a pointer whose type has no such value (its offset is a `NonZero`,
///    see "The type is its offset" in [`SelfRelative`](crate::SelfRelative#safety)). For any other
///    pointer they are a pointer, handed over.
///
/// Nothing is promised about the targets of the pointers: not that a target is inside `bytes`, not
/// what it holds, and not that two pointers have different targets. That is for the visitor to
/// check, as is the scheme of the pointers. Nothing is promised of addresses either, only of
/// positions in `bytes`. To make a `&T` at `bytes.as_ptr() + at`, the caller must ensure that this
/// address is aligned for `T`, for example by placing the buffer at an address aligned to
/// `align_of::<T>()` and choosing `at` as a multiple of it. The same holds for what the pointers
/// aim at, which their scheme places.
///
/// A `T` of size 0 has no bytes to check, so it is valid if it has a value and its rules hold. `T`
/// is `Sized`, because a value whose size its bytes tell (a slice tail, `str`) is not one that a
/// buffer of pointers holds by value.
///
/// [valid]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity
/// [`SelfRelative`]: crate::SelfRelative
///
/// # Errors
///
/// Returns [`Invalid`] at the first position where the bytes stop being a valid `T`, in the
/// order of the walk (see [`Visit`]), or at `at` if the bytes are not all there. Returns the
/// error of `visit` if it comes first.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - Without the `unstable` feature, the walk does not depend on a layout that Rust leaves to the
///   compiler. No part of `T` whose layout Rust leaves to the compiler (such as a struct, a tuple
///   or an enum of the `Rust` representation) holds a self-relative pointer or anything to check (a
///   scalar with invalid bit patterns, an enum, or a rule). A part is exempt if every order of its
///   fields makes the same bytes.
/// - No union in `T` holds a self-relative pointer, since which field a union holds cannot be
///   told.
/// - Every implementation of `Pun` that the walk goes through lists the fields, variants,
///   elements and pointer targets that its layout describes and, for a struct whose layout has
///   rules, those rules. This holds for every type that derives `Pun` and every type for which
///   `puns` implements it.
///
/// These conditions are checked after monomorphization, as [the crate
/// documentation](crate#compile-time-errors) explains. A generic function that calls `validate_in`
/// compiles until it is instantiated with types that violate them.
///
/// # Examples
///
/// ```
/// use core::{marker::PhantomData, mem::offset_of};
///
/// use puns::{
///   Invalid, Pun, SelfRelative, Visit,
///   layout::{Endian, Pointer},
/// };
///
/// /// A pointer to a `T` in the same buffer: the distance from its own address.
/// #[repr(transparent)]
/// pub struct Rel<T> {
///   offset: i32,
///   target: PhantomData<T>,
/// }
///
/// // SAFETY: Each requirement of `SelfRelative` holds, as its example proves for this type:
/// // `Rel<T>` is `repr(transparent)` over an `i32`, its scheme is its own, and `POINTER` is made
/// // for `Target`. This example makes no `Rel`, only bytes, so no pointer is followed.
/// unsafe impl<T: Pun> SelfRelative for Rel<T> {
///   type Target = T;
///   const POINTER: Pointer = Pointer::one::<T>("rel, an example", 4, Endian::NATIVE);
/// }
///
/// puns::self_relative!(impl[T: Pun] Rel<T>);
///
/// /// Records the position of each pointer.
/// struct Positions(Vec<usize>);
///
/// impl Visit for Positions {
///   type Error = Invalid;
///   fn pointer<P: ?Sized + Pun>(&mut self, at: usize) -> Result<(), Invalid> {
///     self.0.push(at);
///     Ok(())
///   }
/// }
///
/// #[derive(Pun)]
/// #[repr(C)]
/// struct Pair {
///   on: bool,
///   first: Rel<u8>,
///   second: Rel<u8>,
/// }
///
/// // A `Pair` at 0. Its offsets are zero: pointers to themselves, which the visitor ignores.
/// let mut bytes = [0; size_of::<Pair>()];
/// bytes[0] = 1;
/// let mut positions = Positions(Vec::new());
/// assert_eq!(puns::validate_in::<Pair, _>(&bytes, 0, &mut positions), Ok(()));
/// assert_eq!(positions.0, [offset_of!(Pair, first), offset_of!(Pair, second)]);
///
/// // A `bool` of 2 is not valid. The walk stops there, before the pointers.
/// bytes[0] = 2;
/// let mut positions = Positions(Vec::new());
/// assert_eq!(puns::validate_in::<Pair, _>(&bytes, 0, &mut positions), Err(Invalid::at(0)));
/// assert!(positions.0.is_empty());
/// ```
#[inline]
pub fn validate_in<T: Pun, V: Visit>(
  bytes: &[u8],
  at: usize,
  visit: &mut V,
) -> Result<(), V::Error> {
  const { assert!(T::LAYOUT.checks_kept(), "{}", layout::VISIT_UNSPECIFIED) };
  let size = const { T::LAYOUT.size() };
  match bytes.get(at..).and_then(|rest| rest.get(..size)) {
    // SAFETY: `bytes.get(at..)` and `.get(..size)` returned `Some`, so the `at + size` bytes from
    // the start of `bytes` are in `bytes`, one allocation. The `size` bytes from `at` are valid for
    // reads and initialized, since they are `u8`s of a slice. This is what `value` requires.
    Some(_) => unsafe { value::<T, V>(bytes.as_ptr(), at, visit) },
    None => Err(Invalid::at(at).into()),
  }
}

/// Validates the `count` values of `T` from the position `at` of `bytes`, `size_of::<T>()`
/// apart, in order, as [`validate_in`] validates one.
///
/// Elements that hold no pointer are validated as `try_pun_ref` validates a slice, by blocks where
/// it can, and only the first element found not valid is walked again, to find the position of the
/// error. Elements that hold pointers are validated one by one, with each pointer handed to `visit`
/// in order. Elements of a type whose every bit pattern is valid are not read.
///
/// After `Ok(())`, unsafe code may rely on what [`validate_in`] promises, for each of the `count`
/// values. **In bounds** becomes that `at + count * size_of::<T>()` does not overflow and is at
/// most `bytes.len()`.
///
/// # Errors
///
/// Returns [`Invalid`] at `at` if the `count` values are not all in `bytes`. Otherwise, returns
/// the error of [`validate_in`] for the first element that is not valid.
///
/// # Compile-time errors
///
/// A call compiles under the same conditions as one of [`validate_in`], which its
/// [compile-time errors](validate_in#compile-time-errors) list. A generic function that calls
/// `validate_elems_in` compiles until it is instantiated with types that violate them.
///
/// # Examples
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
/// let bytes = [0, 1, 2];
/// assert_eq!(puns::validate_elems_in::<bool, _>(&bytes, 0, 2, &mut NoPointers), Ok(()));
/// assert_eq!(
///   puns::validate_elems_in::<bool, _>(&bytes, 0, 3, &mut NoPointers),
///   Err(Invalid::at(2)),
/// );
/// // Three values from 1 are not all there.
/// assert_eq!(
///   puns::validate_elems_in::<bool, _>(&bytes, 1, 3, &mut NoPointers),
///   Err(Invalid::at(1)),
/// );
/// ```
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
    // SAFETY: `elems` is the `size * count` bytes from `at`, which `bytes.get` returned, so
    // `count` consecutive `T`s, valid for reads and initialized. This is what `validate_elems`
    // requires.
    let valid = unsafe { validate_elems::<T>(elems.as_ptr(), count) };
    if valid {
      return Ok(());
    }
  }
  let mut i = 0;
  while i < count {
    // SAFETY: `i < count`, so the `size` bytes from `at + i * size` are in `elems`, which
    // `bytes.get` returned, and every byte from the start of `bytes` to their end is in `bytes`,
    // one allocation. They are valid for reads and initialized, as `value` requires.
    unsafe { value::<T, V>(bytes.as_ptr(), at + i * size, visit)? };
    i += 1;
  }
  // The validation of the elements found one that is not valid, but the walk found none, so the
  // verdict is that of the validation.
  if const { !T::LAYOUT.has_pointers() } {
    return Err(Invalid::at(at).into());
  }
  Ok(())
}

/// Hands each self-relative pointer of the valid `T` at the position `at` from `base` to
/// `visit`, as [`validate_in`] would, and checks nothing.
///
/// This is for a value already known to be valid, such as one being copied or moved in its buffer,
/// where validating it again would only repeat the check of every scalar. The calls of
/// `visit.pointer::<P>(at)` are those that [`validate_in`] makes on the same bytes, in the same
/// order (see [`Visit`]), so a pointer whose bytes are all zero is not handed over if the pointer
/// may be null, and is handed over otherwise. The function reads only the tags of enums, niches
/// included, and the offsets of pointers that may be null, at any alignment, and does not follow
/// pointers. An error returned by `visit` ends the walk with that error.
///
/// # Errors
///
/// Returns the first error that `visit` returns.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The `at + size_of::<T>()` bytes from `base` are within one allocation, and the
///   `size_of::<T>()` bytes at `base + at` are valid for reads. They need not be aligned.
/// - Those bytes hold a `T` as its types say. Every scalar is initialized and a valid value of its
///   type (`bool`, `char`, `NonZero`, an enum without fields, the offset of a pointer), except a
///   scalar that may be uninitialized, such as a `MaybeUninit`.
/// - The tag of every enum, or its niche, is initialized and selects one of its variants. The
///   bytes that this variant holds meet these requirements in turn.
///
/// The rules of its structs (`assert`, `size`, `len`, regions) and the `valid(..)` restrictions of
/// its fields need not hold, since safe code can break them and the walk does not read them. A
/// restricted field is a scalar, which holds no pointer. Padding and the bytes that the variant of
/// an enum does not hold may be anything, uninitialized included, and nothing is required of what
/// the pointers aim at, since none of it is read. A valid `T` meets these requirements, whatever
/// values safe code wrote to its fields, and so do bytes that [`validate_in`] accepted.
///
/// # Compile-time errors
///
/// A call compiles under the same conditions as one of [`validate_in`], which its
/// [compile-time errors](validate_in#compile-time-errors) list, except that the rules are not
/// compared with their list, since this walk does not read them. A generic function that calls
/// `pointers_in` compiles until it is instantiated with types that violate them.
///
/// # Examples
///
/// ```
/// # use core::{marker::PhantomData, mem::offset_of};
/// #
/// # use puns::{
/// #   Invalid, Pun, SelfRelative, Visit,
/// #   layout::{Endian, Pointer},
/// # };
/// #
/// # /// A pointer to a `T` in the same buffer: the distance from its own address.
/// # #[repr(transparent)]
/// # pub struct Rel<T> {
/// #   offset: i32,
/// #   target: PhantomData<T>,
/// # }
/// #
/// # // SAFETY: Each requirement of `SelfRelative` holds, as its example proves for this type:
/// # // `Rel<T>` is `repr(transparent)` over an `i32`, its scheme is its own, and `POINTER` is made
/// # // for `Target`. This example makes no `Rel`, only bytes, so no pointer is followed.
/// # unsafe impl<T: Pun> SelfRelative for Rel<T> {
/// #   type Target = T;
/// #   const POINTER: Pointer = Pointer::one::<T>("rel, an example", 4, Endian::NATIVE);
/// # }
/// #
/// # puns::self_relative!(impl[T: Pun] Rel<T>);
/// #
/// # /// Records the position of each pointer.
/// # struct Positions(Vec<usize>);
/// #
/// # impl Visit for Positions {
/// #   type Error = Invalid;
/// #   fn pointer<P: ?Sized + Pun>(&mut self, at: usize) -> Result<(), Invalid> {
/// #     self.0.push(at);
/// #     Ok(())
/// #   }
/// # }
/// #
/// # #[derive(Pun)]
/// # #[repr(C)]
/// # struct Pair {
/// #   on: bool,
/// #   first: Rel<u8>,
/// #   second: Rel<u8>,
/// # }
/// let mut bytes = [0; size_of::<Pair>()];
/// bytes[0] = 1;
/// let mut checked = Positions(Vec::new());
/// puns::validate_in::<Pair, _>(&bytes, 0, &mut checked).unwrap();
///
/// let mut walked = Positions(Vec::new());
/// // SAFETY: `bytes` is valid for reads, and `validate_in` accepted it as a `Pair`, which meets
/// // the requirements of `pointers_in`.
/// unsafe { puns::pointers_in::<Pair, _>(bytes.as_ptr(), 0, &mut walked) }.unwrap();
/// assert_eq!(walked.0, checked.0);
/// ```
#[inline]
pub unsafe fn pointers_in<T: Pun, V: Visit>(
  base: *const u8,
  at: usize,
  visit: &mut V,
) -> Result<(), V::Error> {
  const { assert!(T::LAYOUT.checks_kept(), "{}", layout::VISIT_UNSPECIFIED) };
  // SAFETY: The requirements of `follow` are those of this function.
  unsafe { follow::<T, V>(base, at, visit) }
}

/// Hands each self-relative pointer of the `count` valid values of `T` from the position `at` of
/// `base`, `size_of::<T>()` apart, to `visit`, in order, as [`pointers_in`] does for one.
///
/// Elements of a type that holds no pointer are not read.
///
/// # Errors
///
/// Returns the first error that `visit` returns.
///
/// # Safety
///
/// The caller must ensure that each of the `count` values at `base + at`, `size_of::<T>()` apart,
/// meets the requirements of [`pointers_in`].
///
/// If `T` holds no self-relative pointer, nothing is read, and nothing is required of `base`,
/// `at` or `count`.
///
/// # Compile-time errors
///
/// A call compiles under the same conditions as one of [`validate_in`], which its
/// [compile-time errors](validate_in#compile-time-errors) list, except that the rules are not
/// compared with their list, since this walk does not read them. A generic function that calls
/// `pointers_elems_in` compiles until it is instantiated with types that violate them.
///
/// # Examples
///
/// ```
/// # use core::marker::PhantomData;
/// #
/// # use puns::{
/// #   Invalid, Pun, SelfRelative, Visit,
/// #   layout::{Endian, Pointer},
/// # };
/// #
/// # /// A pointer to a `T` in the same buffer: the distance from its own address.
/// # #[repr(transparent)]
/// # pub struct Rel<T> {
/// #   offset: i32,
/// #   target: PhantomData<T>,
/// # }
/// #
/// # // SAFETY: Each requirement of `SelfRelative` holds, as its example proves for this type:
/// # // `Rel<T>` is `repr(transparent)` over an `i32`, its scheme is its own, and `POINTER` is made
/// # // for `Target`. This example makes no `Rel`, only bytes, so no pointer is followed.
/// # unsafe impl<T: Pun> SelfRelative for Rel<T> {
/// #   type Target = T;
/// #   const POINTER: Pointer = Pointer::one::<T>("rel, an example", 4, Endian::NATIVE);
/// # }
/// #
/// # puns::self_relative!(impl[T: Pun] Rel<T>);
/// #
/// # /// Records the position of each pointer.
/// # struct Positions(Vec<usize>);
/// #
/// # impl Visit for Positions {
/// #   type Error = Invalid;
/// #   fn pointer<P: ?Sized + Pun>(&mut self, at: usize) -> Result<(), Invalid> {
/// #     self.0.push(at);
/// #     Ok(())
/// #   }
/// # }
/// #
/// # #[derive(Pun)]
/// # #[repr(C)]
/// # struct Pair {
/// #   on: bool,
/// #   first: Rel<u8>,
/// #   second: Rel<u8>,
/// # }
/// // Two `Pair`s, one after the other. Their offsets are zero: pointers to themselves.
/// let mut bytes = [0; 2 * size_of::<Pair>()];
/// bytes[0] = 1;
/// bytes[size_of::<Pair>()] = 1;
/// let mut checked = Positions(Vec::new());
/// puns::validate_elems_in::<Pair, _>(&bytes, 0, 2, &mut checked).unwrap();
///
/// let mut walked = Positions(Vec::new());
/// // SAFETY: `bytes` is valid for reads, and `validate_elems_in` accepted it as two `Pair`s,
/// // each of which meets the requirements of `pointers_in`.
/// unsafe { puns::pointers_elems_in::<Pair, _>(bytes.as_ptr(), 0, 2, &mut walked) }.unwrap();
/// assert_eq!(walked.0, checked.0);
/// assert_eq!(walked.0.len(), 4);
/// ```
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
    // SAFETY: `i < count`, and each of the `count` values at `base + at`, `size` apart, meets
    // the requirements of `pointers_in`, by contract on the caller. This is what `follow`
    // requires of the element `i`.
    unsafe { follow::<T, V>(base, at + i * size, visit)? };
    i += 1;
  }
  Ok(())
}

// The walks of `pointers_in` and `copy_in` are inlined into one function per type where code is
// optimized, but not where it is built for debugging (`debug_assertions`): there the compiler
// gives every inlined body its own stack slots, and a crate that recurses through pointers with
// these walks (a deep copy) took kilobytes of stack per level of nesting.

/// Hands to `visit` the pointers of the valid `T` at `at` from `base`, in order.
///
/// This is the walk of [`pointers_in`]. A pointer that may be null and whose bytes are all zero
/// is not handed over.
///
/// # Errors
///
/// Returns the first error that `visit` returns.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The requirements of [`pointers_in`] hold for the `T` at `at`.
///
/// # Compile-time errors
///
/// An instance compiles only if `T` is not a union that holds a pointer, and the lists of `T` are
/// what its layout holds ([`listed`]).
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
  // SAFETY: The `T` at `base + at` meets the requirements of `pointers_in`, by contract on the
  // caller. `T::LAYOUT` is its layout, and `listed` asserts that its lists are those of the
  // layout. For each shape:
  // - Each element of an array is a `T::Elem` within the `T`, `size` apart, and meets the same
  //   requirements, as `follow` requires.
  // - The fields of a struct are within the `T`, and `T::Fields` is their list, as
  //   `follow_fields` requires.
  // - The tag of an enum is initialized, by contract on the caller. `Layout::enumeration`
  //   asserts that it is within the enum, and of at most 16 bytes (`Spec::is_integer`), so
  //   `add` stays within the `T`, and the tag meets the requirements of `load`. It selects a
  //   variant, whose fields meet the requirements, and `T::Variants` is the list of the
  //   variants and `tag` the tag, as `follow_variants` requires.
  // - The offset of a pointer is initialized, by contract on the caller, and of at most 16
  //   bytes (`Pointer::new`), as `load` requires.
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
      // All zeros, where the pointer may be null, is no pointer.
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

/// Hands to `visit` the pointers of the field `F` of the struct or variant at `at`, and of the
/// fields after it, in the order of their declarations.
///
/// A field restricted by `valid(..)` is a scalar, which holds no pointer.
///
/// # Errors
///
/// Returns the first error that `visit` returns.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The requirements of [`pointers_in`] hold for the struct or the enum at `at`.
/// - `F` is the list of its fields, or of the fields of the variant that its tag selects, or a
///   list that `Fields::Next` reaches from it.
#[cfg_attr(not(debug_assertions), inline(always))]
unsafe fn follow_fields<F: Fields, V: Visit>(
  base: *const u8,
  at: usize,
  visit: &mut V,
) -> Result<(), V::Error> {
  let Some(field) = (const { F::FIELD }) else { return Ok(()) };
  // SAFETY: `F` is a field of the struct or the variant at `at` (`Fields`, section Safety), whose
  // lists `listed` asserts are its layout. So a `F::Type` is at `at + field.offset()`, within the
  // value, and meets the requirements of `pointers_in`, as `follow` requires. A restricted field
  // is a scalar, which holds no pointer. The requirements of the call for the next fields are those
  // of this call.
  unsafe {
    if field.restriction().is_none() {
      follow::<F::Type, V>(base, at + field.offset(), visit)?;
    }
    follow_fields::<F::Next, V>(base, at, visit)
  }
}

/// Hands to `visit` the pointers of the variant whose tag is `tag`, among `W` and the variants
/// after it.
///
/// Returns `None` if no such variant has the tag `tag`.
///
/// # Errors
///
/// Returns `Some` of the first error that `visit` returns.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The requirements of [`pointers_in`] hold for the enum at `at`.
/// - `W` is the list of its variants, or a list that `Variants::Next` reaches from it.
/// - `tag` is the value of its tag.
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
  // SAFETY: `W` is in the list of the variants of the enum, and `tag` is its tag, by contract on
  // the caller. So if `this`, `W` is the variant that the tag selects, whose fields are `W::Fields`
  // (`Variants`, section Safety). They meet the requirements of `pointers_in`, by contract on the
  // caller, as `follow_fields` requires. Otherwise the requirements of the call for the next
  // variants are those of this call.
  unsafe {
    if this {
      Some(follow_fields::<W::Fields, V>(base, at, visit))
    } else {
      follow_variants::<W::Next, V>(base, at, tag, visit)
    }
  }
}

/// Copies to `dst` every byte that the valid `T` at `src` holds initialized, except the bytes of
/// its self-relative pointers, which the caller writes.
///
/// The bytes copied are those of each scalar, of the tag of each enum, and of the fields of the
/// variant that the tag selects, except a scalar that may be uninitialized, such as a
/// `MaybeUninit`. A pointer that may be null and whose bytes are all zero is copied, since it is
/// not a pointer. The other pointers are not written. They are the ones that [`pointers_in`] hands
/// over, in the same order, and the caller writes their offsets, so that each aims at its target
/// from where the copy is, or at a copy of its target.
///
/// This is for a crate that writes a value into a buffer of initialized bytes, and must keep every
/// byte of it initialized. A typed copy of the whole value leaves the padding of the destination
/// uninitialized, and a copy of its bytes copies the padding of the source, which may be
/// uninitialized.
///
/// The bytes of a valid `T` that may be uninitialized (padding, the bytes of the variants that the
/// value is not, and fields that may be uninitialized) are never read from `src` nor written to
/// `dst`, so those bytes of `dst` keep what they held. A value without such bytes and without
/// pointers ([`Layout::is_plain`]) is copied whole, at once. The function reads the tags of enums,
/// niches included, to find the variant, as [`validate_in`] does, and does not follow pointers.
///
/// [`Layout::is_plain`]: layout::Layout::is_plain
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The `size_of::<T>()` bytes at `src` are valid for reads, and hold a `T` as its types say, as
///   [`pointers_in`] requires. Every scalar is initialized and valid for its type, except a scalar
///   that may be uninitialized, and the tag of every enum, or its niche, is initialized and selects
///   one of its variants, whose bytes meet these requirements in turn. They need not be aligned.
/// - The `size_of::<T>()` bytes at `dst` are valid for writes. They need not be aligned.
/// - The two ranges do not overlap.
///
/// The rules of the structs of `T` and the `valid(..)` restrictions of its fields need not hold,
/// because none is read, and a restricted field is copied as a scalar of its type. The bytes of
/// `src` that a valid `T` may leave uninitialized need not be initialized.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - No part of `T` is a union: not `T` itself, nor a field, an element or a variant at any depth.
///   Which field a union holds, and so which of its bytes are initialized, cannot be told.
/// - Every implementation of `Pun` that the copy goes through lists the fields, variants,
///   elements and pointer targets that its layout describes. This holds for every type that
///   derives `Pun` and every type for which `puns` implements it.
///
/// These conditions are checked after monomorphization, as [the crate
/// documentation](crate#compile-time-errors) explains. A generic function that calls `copy_in`
/// compiles until it is instantiated with types that violate them.
///
/// # Examples
///
/// ```
/// use core::mem::offset_of;
///
/// use puns::Pun;
///
/// #[derive(Pun)]
/// #[repr(C)]
/// struct Entry {
///   kind: u8,
///   value: u32,
/// }
///
/// let entry = Entry { kind: 7, value: 0x0102_0304 };
/// let mut buffer = [0xAA; size_of::<Entry>()];
/// // SAFETY: `entry` is a valid `Entry`, valid for reads. `buffer` is as large, valid for writes,
/// // and does not overlap it.
/// unsafe { puns::copy_in::<Entry>(core::ptr::from_ref(&entry).cast(), buffer.as_mut_ptr()) };
///
/// let at = offset_of!(Entry, value);
/// assert_eq!(buffer[0], 7);
/// assert_eq!(buffer[at..at + 4], 0x0102_0304_u32.to_ne_bytes());
/// // The padding between the fields keeps what the buffer held.
/// assert!(buffer[1..at].iter().all(|&byte| byte == 0xAA));
/// ```
#[inline]
pub unsafe fn copy_in<T: Pun>(src: *const u8, dst: *mut u8) {
  // SAFETY: The requirements of `copy` are those of this function.
  unsafe { copy::<T>(src, dst) }
}

/// Returns whether every byte of every value of `layout` is initialized, and `layout` holds no
/// pointer.
///
/// This is [`Layout::is_plain`].
///
/// [`Layout::is_plain`]: layout::Layout::is_plain
const fn whole(layout: &layout::Layout) -> bool {
  match layout.kind {
    Kind::Scalar(spec) => !spec.may_be_uninit(),
    Kind::Array(elem, _) => whole(elem),
    Kind::Struct(fields, _) => {
      let (mut i, mut covered) = (0, 0);
      while i < fields.len() {
        // The fields are sorted by offset, so a field that does not start where the bytes covered
        // so far end leaves a hole.
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

/// Copies the valid `T` at `src` to `dst`, as [`copy_in`] does.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The requirements of [`copy_in`] hold.
///
/// # Compile-time errors
///
/// An instance compiles only if `T` is not a union, and its lists are what its layout holds
/// ([`listed`]).
#[cfg_attr(not(debug_assertions), inline(always))]
unsafe fn copy<T: ?Sized + Pun>(src: *const u8, dst: *mut u8) {
  let size = const { T::LAYOUT.size() };
  if const { whole(&T::LAYOUT) } {
    // SAFETY: This satisfies the safety contract of `copy_nonoverlapping`. The `size` bytes at
    // `src` are valid for reads, the `size` bytes at `dst` valid for writes, and they do not
    // overlap, by contract on the caller. A `u8` is aligned at any address. `whole` is `true`, so
    // every byte of a valid `T` is initialized, and `dst` is left initialized.
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
  // SAFETY: The `T` at `src` and the bytes at `dst` meet the requirements of `copy_in`, by
  // contract on the caller. `T::LAYOUT` is the layout of `T`, and `listed` asserts that its lists
  // are those of the layout. For each shape:
  // - Each element of an array is within both ranges, `size` apart, and meets the same
  //   requirements, as `copy` requires.
  // - The fields of a struct are within both ranges, and `T::Fields` is their list, as
  //   `copy_fields` requires.
  // - The tag of an enum is initialized, by contract on the caller. `Layout::enumeration` asserts
  //   that it is within the enum, and of at most 16 bytes, so `add` stays within both ranges, and
  //   the tag meets the requirements of `load`. Its `spec.size` bytes are valid for reads at `src`
  //   and writes at `dst`, which do not overlap, and a `u8` is aligned at any address, as
  //   `copy_nonoverlapping` requires. It selects a variant, whose fields meet the requirements of
  //   `copy_variants`, with `T::Variants` the list of the variants and `tag` the tag.
  // - The offset of a pointer is initialized, by contract on the caller, and of at most 16
  //   bytes, as `load` requires. Its `size` bytes are copied as the tag is.
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
      // All zeros, where the pointer may be null, is no pointer, so its bytes are copied. A pointer
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

/// Copies the field `F` of the struct or variant at `src`, and the fields after it, to `dst`.
///
/// A field restricted by `valid(..)` is copied as a scalar of its type.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The requirements of [`copy_in`] hold for the struct or the enum at `src`.
/// - `F` is the list of its fields, or of the fields of the variant that its tag selects, or a
///   list that `Fields::Next` reaches from it.
#[cfg_attr(not(debug_assertions), inline(always))]
unsafe fn copy_fields<F: Fields>(src: *const u8, dst: *mut u8) {
  let Some(field) = (const { F::FIELD }) else { return };
  let off = field.offset();
  // SAFETY: `F` is a field of the struct or the variant at `src` (`Fields`, section Safety),
  // whose lists `listed` asserts are its layout. So a `F::Type` is at `off`, within both ranges,
  // and meets the requirements of `copy_in`, as `copy` requires. A restricted field is a scalar
  // of its type, whose bytes are those of the type. The requirements of the call for the next
  // fields are those of this call.
  unsafe {
    copy::<F::Type>(src.add(off), dst.add(off));
    copy_fields::<F::Next>(src, dst);
  }
}

/// Copies to `dst` the fields of the variant whose tag is `tag`, among `W` and the variants after
/// it.
///
/// Copies nothing if no such variant has the tag `tag`.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The requirements of [`copy_in`] hold for the enum at `src`.
/// - `W` is the list of its variants, or a list that `Variants::Next` reaches from it.
/// - `tag` is the value of its tag.
#[cfg_attr(not(debug_assertions), inline(always))]
unsafe fn copy_variants<W: Variants>(src: *const u8, dst: *mut u8, tag: u128) {
  let Some(expected) = (const { W::TAG }) else { return };
  // As `variants`: the variant of a niche, or one that no value is, has ranges of tags.
  let this = (const { W::RANGES }).map_or(tag == expected, |ranges| layout::contains(ranges, tag));
  // SAFETY: `W` is in the list of the variants of the enum, and `tag` is its tag, by contract on
  // the caller. So if `this`, `W` is the variant that the tag selects, whose fields are `W::Fields`
  // (`Variants`, section Safety). They meet the requirements of `copy_in`, by contract on the
  // caller, as `copy_fields` requires. Otherwise the requirements of the call for the next variants
  // are those of this call.
  unsafe {
    if this {
      copy_fields::<W::Fields>(src, dst);
    } else {
      copy_variants::<W::Next>(src, dst, tag);
    }
  }
}

/// Returns whether the `count` values of `T` at `base` are valid.
///
/// This is the validation of a slice by `validate`.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `base` is valid for reads of `count` consecutive `T`s, all of whose bytes are initialized,
///   padding included. It need not be aligned.
#[inline(always)]
const unsafe fn validate_elems<T: Pun>(base: *const u8, count: usize) -> bool {
  // SAFETY: `ptr::from_raw_parts` makes a pointer to a `[T]` of `count` elements at `base`, valid
  // for reads, by contract on the caller, which `validate` does not need aligned. Every byte is
  // initialized, by contract on the caller, so every byte that a check reads is.
  unsafe { validate::<All, Root, [T]>(ptr::from_raw_parts::<[T]>(base, count)) }
}

/// Validates the `T` at the position `at` from `base`, and hands its pointers to `visit`.
///
/// A `T` without pointers is validated by `validate`, the code of `try_pun_ref`. Only if that
/// fails is it walked ([`walk`]), to find the position of the error. If the walk finds none, the
/// error is at `at`.
///
/// # Errors
///
/// Returns [`Invalid`] at the position where the bytes stop being a valid `T`, or the first error
/// that `visit` returns.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The `at + size_of::<T>()` bytes from `base` are within one allocation, and the
///   `size_of::<T>()` bytes of a `T` at `base + at` are valid for reads, all initialized. They need
///   not be aligned.
///
/// # Compile-time errors
///
/// An instance compiles only if `T` is not a union that holds a pointer, and the lists of `T` are
/// what its layout holds ([`listed`]).
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
  // SAFETY: The `at + size_of::<T>()` bytes from `base` are within one allocation, by contract on
  // the caller, so `add(at)` stays in bounds of it, and `at` fits in an `isize`.
  let here = unsafe { base.add(at) };
  if const { !T::LAYOUT.has_pointers() } {
    // The code of `try_pun_ref` gives the verdict. Where it fails, the walk tells where, or the
    // error is at the value itself if the walk finds nothing.
    //
    // SAFETY: `T` is `Sized`: the root of a walk is, and so are its fields and elements. So
    // `ptr::from_raw_parts` makes a pointer to the `T` at `here`, valid for reads, by contract on
    // the caller, which `validate` does not need aligned. Every byte of it is initialized, by
    // contract on the caller, so every byte that a check reads is.
    if unsafe { validate::<All, Root, T>(ptr::from_raw_parts::<T>(here, 0)) } {
      return Ok(());
    }
    // SAFETY: The requirements of `walk` are those of this function, and `here` is `base + at`.
    return match unsafe { walk::<T, V>(base, at, here, visit) } {
      Ok(()) => Err(Invalid::at(at).into()),
      error => error,
    };
  }
  // SAFETY: The requirements of `walk` are those of this function, and `here` is `base + at`.
  unsafe { walk::<T, V>(base, at, here, visit) }
}

/// Validates the `T` at `at` part by part, and hands its pointers to `visit` in order.
///
/// A union is not valid here, because [`value`] walks a union only after `validate` has found it
/// not valid, since a union that holds a pointer does not compile there.
///
/// # Errors
///
/// Returns [`Invalid`] at the position of the first part that is not valid, or the first error
/// that `visit` returns.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The requirements of [`value`] hold.
/// - `here` is `base + at`.
#[inline(always)]
unsafe fn walk<T: ?Sized + Pun, V: Visit>(
  base: *const u8,
  at: usize,
  here: *const u8,
  visit: &mut V,
) -> Result<(), V::Error> {
  // SAFETY: The `T` at `here` is valid for reads, and its bytes are initialized, by contract on
  // the caller. `T::LAYOUT` is its layout, and `value` asserted that its lists are those of the
  // layout. For each shape:
  // - A scalar is the `spec.size` bytes at `here`, as `Spec::matches` requires.
  // - Each element of an array is within the `T`, `size` apart, as `value` requires.
  // - The fields of a struct are within it, and `T::Fields` is their list, as `fields` requires.
  //   `check_rules` requires a `T` valid for reads, all of whose bytes are initialized, at `here`.
  //   `T` is `Sized`, as in `value`, so `ptr::from_raw_parts` makes a pointer to it.
  // - An enum meets the requirements of `enumeration`, and `layout` is the layout of `T`.
  // - The offset of a pointer is initialized, and of at most 16 bytes (`Pointer::new`), as
  //   `load` requires.
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
      // All zeros is no pointer if the pointer may be null, and not a value if its type has no
      // all-zero value (a `NonZero` offset). Otherwise it is a pointer whose offset is zero, which
      // its scheme reads.
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

/// Why a walk does not compile when the lists of types of `T` are not its layout.
const LISTS: &str = "an implementation of `Pun` lists fields, variants or elements that are not \
                     those of its layout: `validate_in` goes by the lists, and would not check \
                     what they leave out";

/// Returns whether the lists of types of `T`, by which the walk goes, are what its layout holds.
///
/// The lists are `Fields`, `Variants`, `Elem` and `Pointee`. Each field must have its declaration
/// index, its offset, its layout and whether it is restricted, and a type of that layout unless it
/// is restricted. Each variant must have its tag, its tags of a niche, and its fields. The element
/// must have the layout of the elements. What a pointer aims at must have the size and the
/// alignment that the pointer says. Only `T` itself is checked, since the walk checks each type
/// that it goes through.
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

/// Returns whether the list `F` is the `fields`, from the one declared at `index` on, in the
/// order of their declarations.
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

/// Returns whether the list `W` is the `variants`, from the one at `index` on: the same tags, the
/// same tags of a niche, and the same fields.
const fn variants_listed<W: Variants>(variants: &[layout::Variant], index: usize) -> bool {
  let Some(tag) = W::TAG else { return index == variants.len() };
  index < variants.len()
    && variants[index].tag == tag
    && same_ranges(W::RANGES, variants[index].ranges)
    && fields_listed::<W::Fields>(variants[index].fields(), 0)
    && variants_listed::<W::Next>(variants, index + 1)
}

/// Returns whether two lists of ranges of tags are the same.
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

/// Validates the field `F` of the struct or variant at `at`, and the fields after it, in the
/// order of their declarations, and hands their pointers to `visit`.
///
/// A field restricted by `valid(..)` is checked against its restriction.
///
/// # Errors
///
/// Returns [`Invalid`] at the position of the first field that is not valid, or the first error
/// that `visit` returns.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The requirements of [`value`] hold for the struct or the enum at `at`.
/// - `F` is the list of its fields, or of the fields of the variant that its tag selects, or a
///   list that `Fields::Next` reaches from it.
#[inline(always)]
unsafe fn fields<F: Fields, V: Visit>(
  base: *const u8,
  at: usize,
  visit: &mut V,
) -> Result<(), V::Error> {
  let Some(field) = (const { F::FIELD }) else { return Ok(()) };
  let place = at + field.offset();
  // SAFETY: `F` is a field of the struct or the variant at `at` (`Fields`, section Safety), whose
  // lists `value` asserted are its layout. So it is at `place`, within the value, valid for
  // reads, and initialized, by contract on the caller. A restricted field is a scalar of
  // `spec.size` bytes, as `add` and `Spec::matches` require. Any other is a `F::Type`, as
  // `value` requires. The requirements of the call for the next fields are those of this call.
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

/// Validates the tag of the enum at `at`, and the fields of the variant that the tag selects.
///
/// # Errors
///
/// Returns [`Invalid`] at the tag if it selects no variant, or at the first field of the variant
/// that is not valid. Returns the first error that `visit` returns.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The requirements of [`value`] hold.
/// - `layout` is the layout of `T`.
#[inline(always)]
unsafe fn enumeration<T: ?Sized + Pun, V: Visit>(
  base: *const u8,
  at: usize,
  layout: &Enum,
  visit: &mut V,
) -> Result<(), V::Error> {
  let Kind::Scalar(spec) = layout.tag.layout.kind else { return Err(Invalid::at(at).into()) };
  let place = at + layout.tag.offset;
  // SAFETY: `layout` is the layout of `T`, by contract on the caller. `Layout::enumeration`
  // asserts that its tag is within the enum, and of at most 16 bytes (`Spec::is_integer`). The
  // bytes of the enum are valid for reads and initialized, by contract on the caller. So `add`
  // stays within them, and the tag meets the requirements of `load`.
  let tag = unsafe { load(base.add(place), spec.size, spec.endian) };
  // SAFETY: The requirements of `value` hold for the enum at `at`, by contract on the caller.
  // `T::Variants` is the list of its variants, and `tag` its tag, as `variants` requires.
  unsafe { variants::<T::Variants, V>(base, at, tag, visit) }
    .unwrap_or_else(|| Err(Invalid::at(place).into()))
}

/// Validates the fields of the variant whose tag is `tag`, among `W` and the variants after it.
///
/// Returns `None` if no such variant has the tag `tag`.
///
/// # Errors
///
/// Returns `Some` of [`Invalid`] at the first field that is not valid, or of the first error that
/// `visit` returns.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The requirements of [`value`] hold for the enum at `at`.
/// - `W` is the list of its variants, or a list that `Variants::Next` reaches from it.
/// - `tag` is the value of its tag.
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
  // SAFETY: `W` is in the list of the variants of the enum at `at`, and `tag` is its tag, by
  // contract on the caller. So if `this`, `W::Fields` are the fields of the variant `W`
  // (`Variants`, section Safety), whose bytes are valid for reads and initialized, by contract on
  // the caller, as `fields` requires. Otherwise the requirements of the call for the next
  // variants are those of this call.
  unsafe {
    if this {
      Some(fields::<W::Fields, V>(base, at, visit))
    } else {
      variants::<W::Next, V>(base, at, tag, visit)
    }
  }
}
