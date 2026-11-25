//! A pointer is eager: its pointee is checked with the struct, so `pointer(..)` takes no `lazy`.

use puns::Pun;

#[derive(Pun)]
#[repr(C)]
struct Lazy {
  #[pun(pointer(lazy next: u8 = at as u128))]
  at: u8,
}

fn main() {}
