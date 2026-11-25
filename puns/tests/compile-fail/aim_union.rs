//! A union does not hold an eager aim: which of its fields its bytes are would depend on pointees
//! that a check with a worklist checks after it.

use core::mem::ManuallyDrop;

use puns::{InPlace, Pun};

#[derive(Pun)]
#[repr(transparent)]
struct Next {
  #[pun(pointer(u8 = off as u128))]
  off: InPlace<u8>,
}

#[derive(Pun)]
#[repr(C)]
union Either {
  next: ManuallyDrop<Next>,
  byte: u8,
}

fn main() {
  let _ = puns::from_head::<Either>(&[1, 2]);
}
