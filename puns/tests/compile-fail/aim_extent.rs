//! An extent applies to the type of its pointee: a sized pointee has no length, and a `Thin` stores
//! its own.

use puns::{InPlace, Pun, Thin};

#[derive(Pun)]
#[repr(C)]
struct Word {
  #[pun(pointer(u32 = off as u128, len = 2))]
  off: InPlace<u8>,
}

#[derive(Pun)]
#[repr(C)]
struct Given {
  #[pun(pointer(Thin<[u8], u8> = off as u128, size = 4))]
  off: InPlace<u8>,
}

fn main() {
  let _ = puns::from_head::<Word>(&[0; 8]);
  let _ = puns::from_head::<Given>(&[0; 8]);
}
