//! The description of a pointer is made for what it aims at: here for a `u8`, while it aims at
//! a `u32`, which the proof would then take for a value of one byte.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use core::marker::PhantomData;

use puns::{
  SelfRelative,
  layout::{Endian, Pointer},
};

#[repr(transparent)]
struct Mislabeled(i32, PhantomData<u32>);

// SAFETY: Not met, on purpose: `POINTER` is not made for `Target`, which must be refused.
unsafe impl SelfRelative for Mislabeled {
  type Target = u32;
  const POINTER: Pointer = Pointer::one::<u8>("mislabeled", 4, Endian::NATIVE);
}

puns::self_relative!(Mislabeled);

fn view(pointer: &Mislabeled) -> &Mislabeled {
  puns::view_ref(pointer)
}

fn main() {
  let _: fn(&Mislabeled) -> &Mislabeled = view;
}
