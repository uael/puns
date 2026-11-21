//! A self-relative pointer is its offset and nothing else: here 8 bytes, of which the offset
//! is 4.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use core::marker::PhantomData;

use puns::{
  Pun, SelfRelative,
  layout::{Endian, Pointer},
};

#[repr(C)]
struct Wide<T>(i32, u32, PhantomData<T>);

// SAFETY: Not met, on purpose: the type is not its offset, which must be refused.
unsafe impl<T: Pun> SelfRelative for Wide<T> {
  type Target = T;
  const POINTER: Pointer = Pointer::one::<T>("wide", 4, Endian::NATIVE);
}

puns::self_relative!(impl[T: Pun] Wide<T>);

fn view(wide: &Wide<u8>) -> &Wide<u8> {
  puns::view_ref(wide)
}

fn main() {
  let _: fn(&Wide<u8>) -> &Wide<u8> = view;
}
