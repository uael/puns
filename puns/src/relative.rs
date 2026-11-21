//! Self-relative pointers, declared by the crate that has them.

use crate::{Pun, layout::Pointer};

/// A self-relative pointer: an offset from its own address to a target in the same buffer, as
/// the `Near<T, O>` of the crate [`nearest`](https://docs.rs/nearest) is.
///
/// A crate declares such a type with an `unsafe impl` of this trait, which says what the pointer
/// is ([`POINTER`](SelfRelative::POINTER)) and what it aims at
/// ([`Target`](SelfRelative::Target)), and with [`self_relative!`](crate::self_relative), which
/// makes it a [`Pun`] type. A value that holds such pointers is then:
///
/// - read as plain data by every function that reads: an offset is initialized bytes;
/// - made by no function from bytes or from another value, but [`view_ref`](crate::view_ref) and
///   [`try_view_ref`](crate::try_view_ref), which read it as another type that holds pointers
///   where it does, with everything they aim at proven at compile time;
/// - never behind a `&mut` that a function of this crate gives.
///
/// How the target is reached (from the address of the pointer, or through the buffer) and how
/// the buffer is built are the business of the declaring crate.
///
/// ```
/// use core::marker::PhantomData;
///
/// use puns::{
///   Pun, SelfRelative,
///   layout::{Endian, Pointer},
/// };
///
/// /// A pointer to a `T` in the same buffer: the distance from its own address, an `i32`.
/// #[repr(transparent)]
/// pub struct Rel<T> {
///   offset: i32,
///   target: PhantomData<T>,
/// }
///
/// // SAFETY: a `Rel<T>` is its `i32`, the distance from its own address to a `T` of the same
/// // buffer, at an address aligned for it. The only one of this example is made where that holds
/// // (below), and nothing writes to the buffer while it is read.
/// unsafe impl<T: Pun> SelfRelative for Rel<T> {
///   type Target = T;
///   const POINTER: Pointer = Pointer::one::<T>("rel, an example", size_of::<i32>(), Endian::NATIVE);
/// }
///
/// puns::self_relative!(impl[T: Pun] Rel<T>);
///
/// #[derive(Pun)]
/// #[repr(C)]
/// struct Old {
///   done: bool,
///   next: Rel<bool>,
/// }
///
/// /// The next version: more states than done or not.
/// #[derive(Pun)]
/// #[repr(C)]
/// struct New {
///   state: u8,
///   next: Rel<u8>,
/// }
///
/// #[repr(C, align(4))]
/// struct Buffer([u8; 12]);
///
/// // An `Old` at 0, whose pointer, at 4, aims at a `bool` at 8.
/// let mut bytes = [0; 12];
/// bytes[0] = 1;
/// bytes[4..8].copy_from_slice(&4i32.to_ne_bytes());
/// bytes[8] = 1;
/// let buffer = Buffer(bytes);
/// // SAFETY: the bytes at 0 are an `Old`, whose pointer aims at a `bool` of the same buffer.
/// let old: &Old = unsafe { &*core::ptr::from_ref(&buffer).cast::<Old>() };
///
/// // Proven at compile time: every `Old`, and what its pointer aims at, is a `New`.
/// let new: &New = puns::view_ref(old);
/// assert!(core::ptr::addr_eq(new, old));
/// assert_eq!(new.state, 1);
/// ```
///
/// Not every `New`, nor what it points to, is an `Old`:
///
/// ```compile_fail
/// # use core::marker::PhantomData;
/// # use puns::{Pun, SelfRelative, layout::{Endian, Pointer}};
/// # #[repr(transparent)]
/// # pub struct Rel<T> { offset: i32, target: PhantomData<T> }
/// # unsafe impl<T: Pun> SelfRelative for Rel<T> {
/// #   type Target = T;
/// #   const POINTER: Pointer = Pointer::one::<T>("rel, an example", 4, Endian::NATIVE);
/// # }
/// # puns::self_relative!(impl[T: Pun] Rel<T>);
/// # #[derive(Pun)] #[repr(C)] struct Old { done: bool, next: Rel<bool> }
/// # #[derive(Pun)] #[repr(C)] struct New { state: u8, next: Rel<u8> }
/// fn back(new: &New) -> &Old {
///   puns::view_ref(new)
/// }
/// # let _: fn(&New) -> &Old = back;
/// ```
///
/// # Safety
///
/// By implementing this trait, a crate promises the following of `Self`. "The pointers of a
/// scheme" are all the types, in any crate, whose `POINTER` has that scheme; a *pointer* is a
/// value of `Self` that the declaring crate may follow to its target (through a shared
/// reference: a `Deref`, a method), and *what it aims at* is what the scheme tells from its
/// address and its bytes.
///
/// 1. **The type is its offset**: `Self` is the integer of `POINTER` (its width, asserted by
///    [`self_relative!`](crate::self_relative), in its byte order) and nothing else: every byte
///    of every value is initialized, and none is in an `UnsafeCell`. A type whose offset has no
///    zero (a `NonZero` in it) is told apart by this crate, which finds it (`Option` of it has its
///    size, `None` all zeros): it is never read from a pointer that may be zero without a check.
///    Any other restriction of the values of the offset is not supported (a niche that is not
///    zero is refused where it is found; one that the compiler does not know, it cannot find).
/// 2. **The scheme is the way to the target**: where a pointer aims, and how many elements or
///    bytes it aims at, depend only on its scheme, the width and the byte order of its offset,
///    the kind of what it aims at ([`Aim`](crate::layout::Aim): one value, a sequence, text),
///    its address, the bytes of its offset, and bytes of its buffer (a length next to the target,
///    say): never on the type that holds it, on its `Target`, on its alignment, or on whatever it
///    is nullable. Two pointers of the same scheme, offset and kind, at the same address with the
///    same bytes, aim at the same place. A scheme is a name that the declaring crate owns (its
///    own name, say): it declares no type of the scheme of another crate unless that crate says
///    so, and changes the way to the target only with the name.
/// 3. **What it aims at is there, valid**, in the same allocation, for as long as a shared
///    reference to the pointer lives: for one value ([`Pointer::one`]), a valid `Target` at an
///    address that is a multiple of its alignment; for a sequence ([`Pointer::sequence`]), that
///    many valid `Target`s, `size_of::<Target>()` bytes apart, the first at an address that is
///    a multiple of the `align` of `POINTER`; for text ([`Pointer::text`]), that many bytes of
///    UTF-8. Every byte of it that `Target` requires to be initialized is.
/// 4. **What it aims at is its own**: while a shared reference to a pointer lives, nothing
///    writes to what it aims at, nor gives a `&mut` to any of it: it is borrowed with the
///    pointer, and read through what `view_ref` makes of it.
/// 5. **Null** (a `POINTER` made with [`Pointer::or_null`]): a value whose bytes are all zero
///    aims at nothing, and is not followed; any other value is a pointer as 2 to 4 say, as one of
///    the same scheme that is not nullable would be.
/// 6. **The target is the type**: `Target` is the type of the value aimed at (of an element of
///    a sequence, `()` for text), and `POINTER` is made with it: `Pointer::one::<Target>`,
///    `Pointer::sequence::<Target>` or `Pointer::text` (sizes and alignments are asserted).
/// 7. **What the crate relies on is this, and no more**: its code that follows a pointer does not
///    rely on anything of a pointer, of its buffer, or of what it aims at, but 1 to 6. In
///    return, this crate makes a value that holds a pointer of `Self` only by
///    [`view_ref`](crate::view_ref) and [`try_view_ref`](crate::try_view_ref), from a value
///    that holds, at the same address, a pointer of the same scheme, offset and kind of aim
///    (nullable only if `Self` is, or not null as checked at run time), of which the target is
///    proven to be a valid target of `Self`: a value as large and as aligned, elements of the
///    same size as aligned, text. It reads the bytes of a pointer, never writes them, and never
///    gives a `&mut` to a value that holds one.
pub unsafe trait SelfRelative: Sized {
  /// The type of the value that the pointer aims at: of an element, if it aims at a sequence;
  /// `()` if it aims at text.
  type Target: Pun;
  /// What the pointer is: its scheme, its offset, what it aims at, whatever it may be null.
  const POINTER: Pointer;
}

/// Makes a [`SelfRelative`] type a [`Pun`] type, and `Option` of it the same pointer that may be
/// null.
///
/// `self_relative!(Type)` for a type without parameters, and `self_relative!(impl[T: Bound, ..]
/// Type<T, ..>)` with the parameters of its implementation of `SelfRelative`. It writes the
/// implementation of `Pun` that the pointer needs, from what `SelfRelative` says of it (see there
/// for an example), and nothing that it does not say: it requires `SelfRelative`, whose
/// implementation is the promise.
///
/// `Option<Type>` is then a `Pun` type too, the pointer of `POINTER.or_null()`, if `None` is the
/// pointer of zeros: if the offset of the pointer has a niche at zero (a `NonZero` in it), as
/// `Layout::nullable` checks where it is used (an offset of no niche makes `Option` larger, and
/// does not compile there).
///
/// That `None` is the zeros is what `rustc` does; Rust documents it of a `repr(transparent)`
/// struct around a `NonZero` ("Representation" in `core::option`), which is how a pointer is
/// declared for `Option` of it to be the same with every compiler. This crate cannot see the
/// `repr` of the type: it takes `Option` of it as the same pointer, with or without the
/// `unstable` feature (see [`layout::UNSTABLE`](crate::layout::UNSTABLE)).
///
/// The items of `Pun` are not written by hand: they are this crate's own, and may change.
///
/// ```
/// use core::{marker::PhantomData, num::NonZero};
///
/// use puns::{
///   Pun, SelfRelative,
///   layout::{Endian, Pointer},
/// };
///
/// /// A pointer to a `T` in the same buffer, never to itself: an offset that is not zero.
/// #[repr(transparent)]
/// pub struct Ptr<T> {
///   offset: NonZero<i32>,
///   target: PhantomData<T>,
/// }
///
/// // SAFETY: a `Ptr<T>` is its offset, the distance from its own address to a `T` of the same
/// // buffer, at an address aligned for it. None is made in this example.
/// unsafe impl<T: Pun> SelfRelative for Ptr<T> {
///   type Target = T;
///   const POINTER: Pointer =
///     Pointer::one::<T>("ptr, an example", size_of::<i32>(), Endian::NATIVE);
/// }
///
/// puns::self_relative!(impl[T: Pun] Ptr<T>);
///
/// #[derive(Pun)]
/// #[repr(C)]
/// struct Maybe {
///   next: Option<Ptr<u8>>,
/// }
///
/// #[derive(Pun)]
/// #[repr(C)]
/// struct Sure {
///   next: Ptr<u8>,
/// }
///
/// // `None` is the pointer of zeros, which aims at nothing: it is no `Ptr`.
/// let none = Maybe { next: None };
/// assert_eq!(size_of::<Maybe>(), size_of::<i32>());
/// assert_eq!(puns::try_view_ref::<Maybe, Sure>(&none).err(), Some(puns::Error::Validity));
/// // Read as plain data, a pointer is the bytes of its offset.
/// assert_eq!(puns::as_bytes(&none), [0; 4]);
/// ```
#[macro_export]
macro_rules! self_relative {
  (impl [$($generics:tt)*] $ty:ty) => {
    // SAFETY: what the implementation of `SelfRelative`, required here, promises of the type:
    // the type is the offset of its `POINTER` (its size is asserted), and it aims at its
    // `Target`, for which `POINTER` is made (asserted).
    unsafe impl<$($generics)*> $crate::Pun for $ty where $ty: $crate::SelfRelative {
      type Elem = ();
      type Fields = ();
      type Variants = ();
      type Rules = ();
      type Pointee = <$ty as $crate::SelfRelative>::Target;
      const PROBE: *const Self = $crate::__sized_probe::<Self>();
      const LAYOUT: $crate::layout::Layout = $crate::layout::Layout::pointer::<
        Self,
        <$ty as $crate::SelfRelative>::Target,
      >(<$ty as $crate::SelfRelative>::POINTER);
      // `Option` of the pointer is the same pointer, that may be null: if `None` is the pointer
      // of zeros, which `nullable` checks where `Option` of it is used (S5).
      const OPTION: ::core::option::Option<$crate::layout::Layout> =
        ::core::option::Option::Some($crate::layout::Layout::nullable::<
          Self,
          <$ty as $crate::SelfRelative>::Target,
        >(<$ty as $crate::SelfRelative>::POINTER));
    }
  };
  ($ty:ty) => {
    $crate::self_relative!(impl[] $ty);
  };
}

/// The `PROBE` of a sized type: a null pointer, made without a path that the code which names
/// it could make lead elsewhere.
#[doc(hidden)]
#[must_use]
pub const fn sized_probe<T>() -> *const T {
  core::ptr::null()
}
