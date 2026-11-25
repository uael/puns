//! What an aim points at, its pointee ([`Pointee`]): a value of a [`Pun`] type, or a [`Thin<T, M>`],
//! a `T` after the metadata that a pointer to it stores there, as an `M`.
//!
//! The metadata of a pointer to a `Pun` type is [`Pun::Metadata`], which [`Metadata`] seals: what a
//! `Thin<T, M>` stores before its value is a function of it, nothing for a sized `T`.

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
pub trait Metadata: Copy + seal::Metadata {
  /// What a [`Thin<T, M>`] whose `T` has this metadata stores before its value: nothing for `()`,
  /// and the `M` for a length.
  #[doc(hidden)]
  type Stored<M: Pun>: Pun;
}

impl Metadata for () {
  type Stored<M: Pun> = ();
}

impl Metadata for usize {
  type Stored<M: Pun> = M;
}

/// What an aim points at, its pointee: a value of a [`Pun`] type, or a [`Thin<T, M>`], a `T` after
/// the length that the aim points at.
///
/// The trait is sealed: it is implemented for every `Pun` type and for `Thin`, which is not one.
/// An aim names its pointee by this type, in the item of its rule in the list `Pun::Rules`, and a
/// read of the aim returns a `&Self::Value`.
///
/// # Examples
///
/// ```
/// use puns::{Pointee, Thin};
///
/// // A `Thin` of a slice stores its length before its elements, and one of a sized type nothing.
/// fn stored<P: ?Sized + Pointee>() -> usize {
///   size_of::<P::Prefix>()
/// }
///
/// assert_eq!((stored::<Thin<[u8], u16>>(), stored::<Thin<u32, u16>>(), stored::<u32>()), (2, 0, 0));
/// ```
pub trait Pointee: seal::Pointee {
  /// The value: what a check checks and a read returns.
  type Value: ?Sized + Pun;
  /// What is stored right before the value, and checked with it: `()` for a `Pun` type and for a
  /// `Thin` of a sized type, and the `M` of a `Thin<T, M>` of an unsized `T`.
  type Prefix: Pun;
  /// The identity of the pointee, which proofs compare. A `Thin` of a sized `T` is a `T`.
  #[doc(hidden)]
  const POINTEE_ID: Id;
  /// The size of the value, without the elements of its tail if it is unsized.
  #[doc(hidden)]
  const POINTEE_SIZE: usize;
  /// The alignment of the pointee: of the value, and of what is stored before it.
  #[doc(hidden)]
  const POINTEE_ALIGN: usize;
  /// Where the value starts, in bytes from the place where a pointer to it points: 0, and the size
  /// of the length for a `Thin` of an unsized `T`, whose pointer points at its length.
  #[doc(hidden)]
  const VALUE_AT: usize;
  /// The integer that is the length of the value, at the end of the prefix, if there is one.
  #[doc(hidden)]
  const LENGTH: Option<Spec>;
}

impl<T: ?Sized + Pun> Pointee for T {
  type Value = T;
  type Prefix = ();
  const POINTEE_ID: Id = T::ID;
  // SAFETY: `T::PROBE` is a probe of `T` (`Pun`, section Safety), as `head_size` and
  // `align_of_val_raw` require.
  const POINTEE_SIZE: usize = unsafe { ptr::head_size(T::PROBE) };
  // SAFETY: As above.
  const POINTEE_ALIGN: usize = unsafe { core::mem::align_of_val_raw(T::PROBE) };
  const VALUE_AT: usize = 0;
  const LENGTH: Option<Spec> = None;
}

/// The pointee of a thin pointer to a `T`: the metadata of the pointer, stored before the `T`.
///
/// The metadata is the length of the tail of `T` if it is unsized, stored as the integer that ends
/// an `M`, and a pointer to the `Thin` points at the length. For a sized `T`, nothing is stored,
/// and the pointee is the `T`.
///
/// The `M` is the length alone, an integer of the sign and the byte order of its type, or a struct
/// that ends with it, such as the capacity then the length of a list, which is checked with the
/// value, its rules included. No padding is between the length and the value, as there would be in
/// a `repr(C)` struct of the two where the value is more aligned than the length: the value is
/// aligned for `T`, for `M`, and to the size of the length, rounded up to a power of two, so that
/// where it is does not depend on the byte order of the length (an `i32le` has an alignment of 1).
/// Any padding is before `M`.
///
/// A value of type `Thin` is never made: the type names the pointee of an aim ([`Pointee`]),
/// whose read returns a `&T`, as [`Thin::get`] reads it.
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
/// let text = puns::from_head::<Text>(puns::as_bytes(&words)).unwrap().0;
/// assert_eq!(text.text(), "hi");
/// // A negative length is not one.
/// assert!(puns::from_head::<Text>(puns::as_bytes(&[2i16, -1, 0])).is_err());
/// ```
pub struct Thin<T: ?Sized, M>(PhantomData<M>, PhantomData<T>);

impl<T: ?Sized + Pun, M: Pun> Thin<T, M> {
  /// Returns the value of the `Thin` that a pointer points at: at `at` for a sized `T`, and, for an
  /// unsized one, right after the length at `at`, with that length.
  ///
  /// # Safety
  ///
  /// The caller must ensure that a `Thin<T, M>` is at `at`, with the provenance of its bytes: its
  /// value, aligned, is valid as Rust requires of the referent of a `&T`, and the length before it
  /// is the length of its tail, and that these bytes are not written for `'a`, except inside an
  /// `UnsafeCell`. A pointee of an eager aim that `puns` checked is one.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::Thin;
  ///
  /// // A length of 2, then two bytes.
  /// let words = [2u16, u16::from_ne_bytes(*b"ok")];
  /// let bytes = puns::as_bytes(&words);
  /// // SAFETY: `bytes` holds a length of 2 then two bytes of UTF-8, which are not written.
  /// let text = unsafe { Thin::<str, u16>::get(bytes.as_ptr()) };
  /// assert_eq!(text, "ok");
  /// ```
  #[inline(always)]
  #[must_use]
  pub const unsafe fn get<'a>(at: *const u8) -> &'a T {
    let value = at.wrapping_add(<Self as Pointee>::VALUE_AT);
    // SAFETY: The value of a `Thin`, aligned, is at `value`, after its length, by contract on the
    // caller, which is the length of its tail, as `own_length` requires. The pointer has the
    // provenance of its bytes, which are valid and not written for `'a`, so it can be borrowed for
    // `'a`.
    unsafe { &*ptr::from_raw_parts::<T>(value, crate::validate::own_length::<Self>(value)) }
  }
}

impl<T: ?Sized + Pun, M: Pun> Pointee for Thin<T, M> {
  type Value = T;
  type Prefix = <T::Metadata as Metadata>::Stored<M>;
  const POINTEE_ID: Id = match Self::LENGTH {
    None => T::ID,
    Some(_) => Id::new("Thin", &[T::ID, M::ID]),
  };
  const POINTEE_SIZE: usize = <T as Pointee>::POINTEE_SIZE;
  const POINTEE_ALIGN: usize = {
    let (value, prefix) = (<T as Pointee>::POINTEE_ALIGN, align_of::<Self::Prefix>());
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
  // Where the length is in `M` must not depend on a layout that Rust leaves to the compiler. A
  // sized `T` has none.
  const LENGTH: Option<Spec> = match <Self::Prefix as Pun>::LAYOUT.last_integer() {
    _ if size_of::<Self::Prefix>() == 0 => None,
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
