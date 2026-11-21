//! What a pointer aims at is proven, never checked: bytes reinterpreted as a `(bool, u8)` behind a
//! pointer are not a `bool` wherever `rustc` puts it.
use core::marker::PhantomData;

use puns::{
  Pun, SelfRelative,
  layout::{Endian, Pointer},
};

/// A pointer to a `T`, or none: the distance from its own address, an `i32`, or zero.
#[repr(transparent)]
pub struct Opt<T>(i32, PhantomData<T>);

impl<T> Opt<T> {
  pub const NONE: Self = Self(0, PhantomData);
}

// SAFETY: The type is its `i32`, and the only values of this program are null: none is followed.
unsafe impl<T: Pun> SelfRelative for Opt<T> {
  type Target = T;
  const POINTER: Pointer = Pointer::one::<T>("refused", 4, Endian::NATIVE).or_null();
}

puns::self_relative!(impl[T: Pun] Opt<T>);

#[derive(Pun)]
#[repr(C)]
struct Bytes {
  leaf: Opt<[u8; 2]>,
}

#[derive(Pun)]
#[repr(C)]
struct Flagged {
  leaf: Opt<(bool, u8)>,
}

fn main() {
  let bytes = Bytes { leaf: Opt::NONE };
  let _: &Flagged = puns::view_ref(&bytes);
}
