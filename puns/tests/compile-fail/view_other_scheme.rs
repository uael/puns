//! A pointer is reinterpreted as a pointer of the same scheme: another crate may find its target
//! another way, from the same bytes.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use core::marker::PhantomData;

use puns::{
  Pun, SelfRelative,
  layout::{Endian, Pointer},
  num::i32le,
};
use relative::One;

/// A pointer to a `T`, of another scheme than the pointers of `relative`, with the same offset.
#[repr(transparent)]
struct Other<T>(i32le, PhantomData<T>);

// SAFETY: The type is its `i32le` offset, of a scheme of its own, and no value of it is made.
unsafe impl<T: Pun> SelfRelative for Other<T> {
  type Target = T;
  const POINTER: Pointer = Pointer::one::<T>("another scheme", 4, Endian::Little);
}

puns::self_relative!(impl[T: Pun] Other<T>);

fn view(source: &One<u32>) -> &Other<u32> {
  puns::view_ref(source)
}

fn main() {
  let _: fn(&One<u32>) -> &Other<u32> = view;
}
