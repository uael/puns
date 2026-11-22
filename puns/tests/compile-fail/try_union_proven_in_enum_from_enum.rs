//! A union that the same union proves, in a variant of an enum that is related to another enum,
//! variant by variant. The `bool` after the union is not proven, so the enum is checked as a
//! whole, and that check would read the union, of which nothing is known.
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Flags {
  both: [bool; 2],
  first: bool,
}

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Loose {
  A(Flags, u8),
}

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Strict {
  A(Flags, bool),
}

fn main() {
  let _ = puns::try_pun::<Loose, Strict>(Loose::A(Flags { first: true }, 1));
}
