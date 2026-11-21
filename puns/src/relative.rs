//! Self-relative pointers, declared by the crate that has them.

use crate::{Pun, layout::Pointer};

/// A self-relative pointer, an offset from its own address to a target in the same buffer, as the
/// `Near<T, O>` of the crate [`nearest`](https://docs.rs/nearest) is.
///
/// A crate declares such a type with an `unsafe impl` of this trait, then with
/// [`self_relative!`](crate::self_relative), which implements [`Pun`] for it. The implementation
/// says what the pointer is ([`POINTER`](SelfRelative::POINTER)) and what it aims at
/// ([`Target`](SelfRelative::Target)). Through `puns`, a value that holds such pointers is then:
///
/// - reinterpreted as plain data by every function that reads, since an offset is initialized
///   bytes;
/// - made only from another value that holds pointers where it does, by
///   [`view_ref`](crate::view_ref) and [`try_view_ref`](crate::try_view_ref), which prove at
///   compile time what the pointers aim at, and never from bytes;
/// - never behind a `&mut` that a function of `puns` gives.
///
/// How the target is reached (from the address of the pointer, or through the buffer) and how the
/// buffer is built are decided by the declaring crate.
///
/// # Safety
///
/// An implementation must guarantee all of the following of `Self`. In these items, a type *of a
/// scheme* is any type, in any crate, whose `POINTER` has that scheme. A *pointer* is a value of
/// `Self` that the declaring crate may follow to its target through a shared reference (a `Deref`
/// implementation, a method). *What it aims at* is what the scheme derives from its address and its
/// bytes.
///
/// 1. **The type is its offset.** `Self` is the integer that `POINTER` describes, of its width and
///    in its byte order, and nothing else; [`self_relative!`](crate::self_relative) asserts the
///    width. Every byte of every value is initialized, and none is in an `UnsafeCell`. The offset
///    may be a `NonZero`, which `puns` detects because `Option<Self>` then has the size of `Self`
///    and `None` is all zero bytes, and `puns` then never reinterprets a pointer that may be zero
///    as such a type without a check. The offset has no other restriction of its values. `puns`
///    refuses a niche other than zero where it finds one, but cannot find a restriction that the
///    compiler does not know of.
/// 2. **The scheme is the way to the target.** Where a pointer aims, and how many elements or bytes
///    it aims at, depend only on its scheme, the width and the byte order of its offset, the kind
///    of its aim ([`Aim`](crate::layout::Aim): one value, a sequence, or text), its address, the
///    bytes of its offset, and bytes of its buffer (such as a length next to the target). They
///    never depend on the type that holds the pointer, on its `Target`, on its alignment, or on
///    whether it may be null. Two pointers of the same scheme, offset and kind, at the same address
///    with the same bytes, aim at the same place. A scheme is a name that the declaring crate owns,
///    such as its own name. The crate declares no type of the scheme of another crate unless that
///    crate allows it, and gives a scheme a new name whenever it changes how a pointer of that
///    scheme finds its target.
/// 3. **What it aims at is there, and valid**, in the same allocation, for as long as a shared
///    reference to the pointer lives. For one value ([`Pointer::one`]), it is a [valid] `Target` at
///    an address that is a multiple of the alignment of `Target`. For a sequence
///    ([`Pointer::sequence`]), it is as many valid `Target`s as the scheme says,
///    `size_of::<Target>()` bytes apart, the first at an address that is a multiple of the
///    alignment that `POINTER` declares for the sequence (the `align` of [`Pointer::sequence`]).
///    For text ([`Pointer::text`]), it is as many bytes of UTF-8 as the scheme says. Every byte of
///    it that `Target` requires to be initialized is initialized.
/// 4. **What it aims at is its own**, borrowed with the pointer. While a shared reference to a
///    pointer lives, nothing writes to what it aims at, nor makes a `&mut` to any of it. It is read
///    through what `view_ref` makes of it.
/// 5. **Null.** If `POINTER` is made with [`Pointer::or_null`], a value whose bytes are all zero
///    aims at nothing, and is not followed. Any other value is a pointer, and meets requirements 2,
///    3 and 4 as a pointer of the same scheme that may not be null does.
/// 6. **The target is the type.** `Target` is the type of the value aimed at, of an element for a
///    sequence, and `()` for text. `POINTER` is made for it, by `Pointer::one::<Target>`,
///    `Pointer::sequence::<Target>` or `Pointer::text`. `self_relative!` asserts the size and the
///    alignment.
/// 7. **The declaring crate relies on this, and on nothing more.** Its code that follows a
///    pointer relies on nothing of the pointer, of its buffer, or of what it aims at, except the
///    requirements above.
///
/// If `P: SelfRelative`, then unsafe code may assume these requirements of `P`, except for "The
/// declaring crate relies on this, and on nothing more", which binds the declaring crate. `puns`
/// relies on "The scheme is the way to the target" when it reinterprets a pointer as another of the
/// same scheme, on "What it aims at is there, and valid" and "What it aims at is its own" when it
/// proves what pointers aim at, and on "Null" when [`try_view_ref`](crate::try_view_ref) checks a
/// pointer that may be null.
///
/// In return, `puns` guarantees the declaring crate the following. It makes a value that holds a
/// pointer of `Self` only by [`view_ref`](crate::view_ref) and
/// [`try_view_ref`](crate::try_view_ref), from a value that holds, at the same address, a pointer
/// of the same scheme, offset and kind of aim. That pointer may be null only if `Self` may be, or
/// it is checked at run time not to be null. What it aims at is proven to be a valid target of
/// `Self`: for one value, a value at least as large and as aligned as `Target`; for a sequence,
/// elements of the same size, the first at an alignment at least the one that `POINTER` declares;
/// for text, text. `puns` reads the bytes of a pointer, never writes them, and never gives a `&mut`
/// to a value that holds one.
///
/// This trait is meant to be implemented by hand, by the crate that declares the pointer type.
/// [`Pun`] is not, so implement it with [`self_relative!`](crate::self_relative).
///
/// [valid]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity
///
/// # Examples
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
/// // SAFETY: Each requirement of `SelfRelative` holds:
/// // - The type is its offset: `Rel<T>` is `repr(transparent)` over an `i32`. Every byte of an
/// //   `i32` is initialized, none is in an `UnsafeCell`, and every `i32` is an offset.
/// // - The scheme is the way to the target: a `Rel` aims at its own address plus its offset,
/// //   whatever its `T`. Only this example declares a type of this scheme.
/// // - What it aims at is there, valid, and its own: the only pointer that this example makes is
/// //   the `Rel<bool>` of `old` (below). It aims at a valid `bool` of the same buffer, and a
/// //   `bool` is aligned at any address. Nothing writes to the buffer while `old` is borrowed.
/// // - Null: `POINTER` is not made with `or_null`.
/// // - The target is the type: `POINTER` is made for `Target`, by `Pointer::one::<T>`.
/// // - Nothing in this example follows a `Rel`, so nothing relies on more.
/// unsafe impl<T: Pun> SelfRelative for Rel<T> {
///   type Target = T;
///   const POINTER: Pointer =
///     Pointer::one::<T>("rel, an example", size_of::<i32>(), Endian::NATIVE);
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
/// // SAFETY: The reference is valid:
/// // - It is not null, because it is made from `&buffer`.
/// // - It is aligned, because `Buffer` is aligned to 4, as `Old` is.
/// // - It is not dangling, because an `Old` is 8 bytes, all within the 12 of `buffer`.
/// // - It points to a valid `Old`: a `bool` of 1, padding, and an offset of 4, which aims at the
/// //   `bool` of 1 at 8 of the same buffer.
/// // `buffer` is not written while `old` lives.
/// let old: &Old = unsafe { &*core::ptr::from_ref(&buffer).cast::<Old>() };
///
/// // Proven at compile time: every `Old`, and what its pointer aims at, is a `New`.
/// let new: &New = puns::view_ref(old);
/// assert!(core::ptr::addr_eq(new, old));
/// assert_eq!(new.state, 1);
/// ```
///
/// Not every `New`, nor what its pointer aims at, is an `Old`:
///
/// ```compile_fail,E0080
/// # use core::marker::PhantomData;
/// # use puns::{Pun, SelfRelative, layout::{Endian, Pointer}};
/// # #[repr(transparent)]
/// # pub struct Rel<T> { offset: i32, target: PhantomData<T> }
/// # // SAFETY: Each requirement holds as in the example above. This example makes no `Rel`.
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
pub unsafe trait SelfRelative: Sized {
  /// The type of the value that the pointer aims at.
  ///
  /// It is the type of an element if the pointer aims at a sequence, and `()` if it aims at text.
  type Target: Pun;
  /// The description of the pointer: its scheme, its offset, what it aims at, and whether it may
  /// be null.
  const POINTER: Pointer;
}

/// Implements [`Pun`] for a [`SelfRelative`] type, and makes `Option` of it the same pointer,
/// which may be null.
///
/// Call `self_relative!(Type)` for a type without generic parameters, and `self_relative!(impl[T:
/// Bound, ..] Type<T, ..>)` with the generic parameters of its implementation of `SelfRelative`.
/// The macro is safe to call, since it writes the implementation of `Pun` only from what
/// `SelfRelative` says of the type, and requires `Type: SelfRelative`, whose `unsafe impl` promises
/// it. Do not implement `Pun` for a pointer type by hand, because the items of `Pun` belong to
/// `puns` and may change, while the macro is expanded with the version of `puns` that defines them.
///
/// The macro is not a blanket implementation of `Pun` for every `SelfRelative` type. That
/// implementation would overlap the one for every [`Scalar`](crate::Scalar), because coherence
/// cannot tell that no type is both.
///
/// `Option<Type>` is `Pun` too. It is the pointer `POINTER.or_null()`, whose value of all zero
/// bytes is `None`. This needs an offset with a niche at zero (a `NonZero`). With another offset,
/// `Option<Type>` is larger than `Type`, and a use of it does not compile.
///
/// Where `Option<Type>` is used, `puns` checks that it has the size of `Type`, and that `None`,
/// read in const evaluation, is all zero bytes. The bytes of the pointer are all of its offset, and
/// initialized ("The type is its offset" in [`SelfRelative`]). So `rustc` has put `None` in a niche
/// of the offset, and that niche is zero. `Some(p)` then has the bytes of `p`, which are not all
/// zero, so "Null" holds for `Option<Type>`.
///
/// Rust documents this layout only for a `repr(transparent)` struct around a `NonZero`
/// ("Representation" in [`core::option`]). Declare the pointer type that way, so that every
/// compiler gives it. `puns` cannot see the `repr` of the type, so it relies on the check above,
/// with or without the `unstable` feature (see [`layout::UNSTABLE`](crate::layout::UNSTABLE)).
///
/// The macro states this through a hidden associated constant of `Pun`, the layout of
/// `Option<Self>`. For any other type the constant is `None`, and the layout of its `Option` is
/// found by experiment, as for any enum of the `Rust` representation.
///
/// # Compile-time errors
///
/// A function of `puns` compiles with a type that holds `Type` only if all of the following
/// hold:
///
/// - `Type` is as large as its offset (`size_of::<Type>()` is the width of `POINTER`).
/// - `POINTER` is made for `Target`, with the size and the alignment of `Target`.
/// - If `Option<Type>` has the size of `Type`, then `None` is all zero bytes. That is, the offset
///   has no niche other than zero.
/// - If `POINTER` may be null ([`Pointer::or_null`](crate::layout::Pointer::or_null)), then
///   `Type` has a value of all zero bytes, so its offset is not a `NonZero`.
///
/// It compiles with a type that holds `Option<Type>` only if, in addition:
///
/// - `POINTER` may not be null already.
/// - `Option<Type>` has the size of `Type`, as it has if the offset has a niche at zero.
/// - `None` is all zero bytes.
/// - `Option<Option<Type>>` is larger than `Option<Type>`.
///
/// These conditions are checked after monomorphization, when the `LAYOUT` of `Type` or of
/// `Option<Type>` is evaluated, and not where the macro is called, as [the crate
/// documentation](crate#compile-time-errors) explains. `POINTER` is evaluated at compile time as
/// well, so a panic of [`Pointer::one`](crate::layout::Pointer::one),
/// [`Pointer::sequence`](crate::layout::Pointer::sequence) or
/// [`Pointer::text`](crate::layout::Pointer::text) is also a compile error.
///
/// # Examples
///
/// See [`SelfRelative`] for a pointer that may not be null. This one may be, as `Option` of it:
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
/// // SAFETY: Each requirement of `SelfRelative` holds:
/// // - The type is its offset: `Ptr<T>` is `repr(transparent)` over a `NonZero<i32>`. Every byte
/// //   of it is initialized, none is in an `UnsafeCell`, and zero is the only `i32` that is not
/// //   an offset.
/// // - The scheme is the way to the target: a `Ptr` aims at its own address plus its offset,
/// //   whatever its `T`. Only this example declares a type of this scheme.
/// // - What it aims at is there, valid, and its own: this example makes no `Ptr`, only a `None`,
/// //   which is no pointer.
/// // - Null: `POINTER` is not made with `or_null`.
/// // - The target is the type: `POINTER` is made for `Target`, by `Pointer::one::<T>`.
/// // - Nothing in this example follows a `Ptr`, so nothing relies on more.
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
/// // Reinterpreted as plain data, a pointer is the bytes of its offset.
/// assert_eq!(puns::as_bytes(&none), [0; 4]);
/// ```
#[macro_export]
macro_rules! self_relative {
  (impl [$($generics:tt)*] $ty:ty) => {
    // SAFETY: `$ty: SelfRelative` is required, so its implementation guarantees the items of
    // the `# Safety` of `SelfRelative`. The layout meets the `# Safety` of `Pun`:
    // - `Layout::pointer` has the size of the offset, which it asserts is the size of `$ty`, and
    //   the alignment of `$ty`. `$ty` is that offset and nothing else, every byte of it is
    //   initialized, and none is inside an `UnsafeCell`, by the requirement that the type is its
    //   offset (`SelfRelative`, section Safety).
    // - The layout says only that the bytes are initialized, and not zero if `Option<$ty>` shows
    //   a niche at zero (`excludes_zero`). Every value of `$ty` holds this. A value that the
    //   layout accepts is a valid `$ty`, since an initialized offset is one. What it aims at is
    //   proven only by a pointer of the same scheme, which `SelfRelative` lets `puns` rely on.
    // - `Layout::pointer` asserts that `POINTER` is made for `Target`, of its size and
    //   alignment, and `Pointee` is `Target`. The other lists are `()`.
    // - `OPTION` is `Layout::nullable`, which asserts that `Option<$ty>` has the size of `$ty`
    //   and that `None` is zeros. So `Some(p)` has the bytes of `p`, which are not all zero, and
    //   `Option<$ty>` is the same pointer, which may be null.
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
      // `Option` of the pointer is the same pointer, which may be null, if `None` is the pointer of
      // zeros. `nullable` checks this where `Option` of the pointer is used (the requirement of
      // `SelfRelative` on a pointer that may be null, `SelfRelative`, section Safety).
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

/// Returns the `PROBE` of a sized type, a null pointer.
///
/// [`self_relative!`] names it through `$crate`, so that no path of the crate that calls the macro
/// can make it name something else.
#[doc(hidden)]
#[must_use]
pub const fn sized_probe<T>() -> *const T {
  core::ptr::null()
}
