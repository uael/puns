//! `mut` adds a method that reads an eager pointee to write: on a lazy aim, whose read checks bytes
//! that it is given, or on an aim without a name, it is refused, at either option.
use puns::{InPlace, Pun};

#[derive(Pun)]
#[repr(C)]
struct LazyPointer {
  #[pun(pointer(lazy mut target: u8))]
  off: u8,
}

#[derive(Pun)]
#[repr(C)]
struct Unnamed {
  #[pun(pointer(mut u8))]
  off: InPlace<u8>,
}

#[derive(Pun)]
#[repr(C)]
struct LazyRegion {
  #[pun(regions(mut first: u8, rest))]
  tail: [u8],
}

fn main() {}
