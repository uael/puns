//! What an aim points at, its pointee ([`Pointee`]): a value of a [`Pun`] type, or a [`Thin<T, M>`],
//! a `T` right after the `M` that ends with its length, the format of a thin pointer to a slice or
//! a `str` that stores its length before the data, as `nearest` stores a list or a text.
//!
//! The metadata of a pointer to a `Pun` type is [`Pun::Metadata`], which [`Metadata`] seals.

use core::marker::PhantomData;

use crate::{
  Pun,
  layout::{Id, Spec, UNSTABLE},
  ptr,
};

/// The seals of [`Metadata`] and [`Pointee`].
mod seal {
  /// Implemented for `()` and `usize` only.
  pub trait Metadata {}

  impl Metadata for () {}
  impl Metadata for usize {}

  /// Implemented for the `Pun` types and for `Thin` only.
  pub trait Pointee {}

  impl<T: ?Sized + crate::Pun> Pointee for T {}
  impl<T: ?Sized, M> Pointee for super::Thin<T, M> {}
}

/// The metadata of a pointer to a [`Pun`] type (`Pun::Metadata`), as in Rust.
///
/// It is `()` for a sized type, and `usize`, the length of its tail, for a slice, a `str` or a
/// struct that ends with one. The trait is sealed, so these are the only two, and a type is sized
/// exactly when its metadata is `()`, which the derive asserts of every struct.
///
/// # Examples
///
/// ```
/// fn metadata<T: ?Sized + puns::Pun>() -> usize {
///   size_of::<T::Metadata>()
/// }
///
/// assert_eq!((metadata::<u32>(), metadata::<[u32]>()), (0, size_of::<usize>()));
/// ```
pub trait Metadata: Copy + seal::Metadata {}

impl Metadata for () {}
impl Metadata for usize {}

/// What an aim points at, its pointee: a value of a [`Pun`] type, at the place where the aim
/// points, or a [`Thin<T, M>`], a `T` after the length that the aim points at.
///
/// The trait is sealed: it is implemented for every `Pun` type and for `Thin`, which is not one.
/// An aim names its pointee by this type, in the item of its rule in the list `Pun::Rules`, and a
/// read of the aim returns a `&Self::Value`.
///
/// # Safety
///
/// Only this crate implements it. The constants say what the type is: the size of the sized part
/// of `Value` and the alignment of the pointee, where the value starts from the place where the
/// aim points, and where its length is.
pub unsafe trait Pointee: seal::Pointee {
  /// The value: what a check checks and a read returns.
  type Value: ?Sized + Pun;
  /// What is stored right before the value, and checked with it: `()` for a `Pun` type, and the
  /// `M` of a `Thin<T, M>`.
  type Prefix: Pun;
  /// The identity of the pointee, which proofs compare.
  #[doc(hidden)]
  const POINTEE_ID: Id;
  /// The size of the value, without the elements of its tail if it is unsized.
  #[doc(hidden)]
  const POINTEE_SIZE: usize;
  /// The alignment of the pointee: of the value, and of what is stored before it.
  #[doc(hidden)]
  const POINTEE_ALIGN: usize;
  /// Where the value starts, in bytes from the place where the aim points: 0 for a `Pun` type,
  /// and the size of the length for a `Thin`, whose aim points at its length.
  #[doc(hidden)]
  const VALUE_AT: usize;
  /// The integer that is the length of the value, at the end of the prefix, if there is one.
  #[doc(hidden)]
  const LENGTH: Option<Spec>;
}

// SAFETY: `T::PROBE` is a probe of `T` (`Pun`, section Safety), as `head_size` and
// `align_of_val_raw` require. The value starts where the aim points, and nothing is before it.
unsafe impl<T: ?Sized + Pun> Pointee for T {
  type Value = T;
  type Prefix = ();
  const POINTEE_ID: Id = T::ID;
  // SAFETY: As above.
  const POINTEE_SIZE: usize = unsafe { ptr::head_size(T::PROBE) };
  // SAFETY: As above.
  const POINTEE_ALIGN: usize = unsafe { core::mem::align_of_val_raw(T::PROBE) };
  const VALUE_AT: usize = 0;
  const LENGTH: Option<Spec> = None;
}

/// The pointee of a thin pointer to an unsized `T`: the `T`, right after an `M` that ends with the
/// length of its tail, an integer of the sign and the byte order of its type. The pointer points at
/// the length.
///
/// The `M` is the length alone, or a struct that ends with it, such as the capacity then the
/// length of a list, which is checked with the value, its rules included. No padding is between
/// the length and the value, as there would be in a `repr(C)` struct of the two where the value is
/// more aligned than the length: the value is aligned for `T`, for `M`, and to the size of the
/// length, rounded up to a power of two, as an integer of the target is aligned to its size. Any
/// padding is before `M`. A value of type `Thin` is never made: the type names the pointee of an
/// aim ([`Pointee`]), whose read returns a `&T`.
///
/// # Examples
///
/// ```
/// use core::num::NonZero;
///
/// use puns::{InPlace, Pun, Thin};
///
/// /// A pointer to a text: an `i16` from its own address to an `i16` length, then the bytes.
/// #[derive(Pun)]
/// #[repr(transparent)]
/// struct Text {
///   #[pun(pointer(text: Thin<str, i16> = off as u128))]
///   off: InPlace<NonZero<i16>>,
/// }
///
/// // The pointer at 0, the length at 2, then the text.
/// let mut words = [2i16, 2, 0];
/// words[2] = i16::from_ne_bytes(*b"hi");
/// let bytes = puns::as_bytes(&words);
/// let text = puns::from_head::<Text>(bytes).unwrap().0;
/// // SAFETY: `puns` checked `text` with its pointee, in `bytes`, from which the pointer is made,
/// // and which are not written.
/// assert_eq!(unsafe { Text::text(bytes.as_ptr().cast()) }, "hi");
/// // A negative length is not one.
/// assert!(puns::from_head::<Text>(puns::as_bytes(&[2i16, -1, 0])).is_err());
/// ```
pub struct Thin<T: ?Sized, M>(PhantomData<M>, PhantomData<T>);

// SAFETY: The value is a `T`, after an `M` whose last part is the length of the tail of `T`, an
// integer at the end of `M` (`Layout::last_integer`), so that the value starts right after it. The
// size is that of the sized part of `T`, from its probe. The alignment is a multiple of those of
// `T` and of `M`, since `M` ends where `T` starts, and its size is a multiple of its alignment, so
// `M` is aligned too. `T` is unsized: its metadata is the length of its tail.
unsafe impl<T: ?Sized + Pun<Metadata = usize>, M: Pun> Pointee for Thin<T, M> {
  type Value = T;
  type Prefix = M;
  const POINTEE_ID: Id = Id::new("Thin", &[T::ID, M::ID]);
  const POINTEE_SIZE: usize = <T as Pointee>::POINTEE_SIZE;
  const POINTEE_ALIGN: usize = {
    let (value, prefix) = (<T as Pointee>::POINTEE_ALIGN, align_of::<M>());
    let length = match Self::LENGTH {
      Some(spec) => spec.size.next_power_of_two(),
      None => 1,
    };
    let align = if value > prefix { value } else { prefix };
    if align > length { align } else { length }
  };
  const VALUE_AT: usize = match Self::LENGTH {
    Some(spec) => spec.size,
    None => 0,
  };
  // Where the length is in `M` must not depend on a layout that Rust leaves to the compiler.
  const LENGTH: Option<Spec> = match M::LAYOUT.last_integer() {
    Some(spec) if UNSTABLE || M::LAYOUT.is_specified() => Some(spec),
    Some(_) => panic!(
      "the `M` of a `Thin<T, M>` ends with the length of the `T` that follows in a layout that Rust \
       leaves to the compiler.\nAdd `#[repr(C)]` to `M`, or enable the `unstable` feature of `puns`"
    ),
    None => panic!(
      "the `M` of a `Thin<T, M>` is an integer, or a sized struct that ends with one, with no \
       padding after it: the length of the `T` that follows"
    ),
  };
}
