//! A pointer is always eager: its pointee is checked with the struct, so `pointer(..)` takes
//! neither `eager` nor `lazy`, before a name or before a type.

use puns::{InPlace, Pun};

#[derive(Pun)]
#[repr(C)]
struct Lazy {
  #[pun(pointer(lazy next: u8 = at as u128))]
  at: u8,
}

#[derive(Pun)]
#[repr(C)]
struct Eager {
  #[pun(pointer(eager [u8] = at as u128, size = 1))]
  at: InPlace<u8>,
}

fn main() {}
