//! An attribute below `#[derive(Pun)]` is expanded after the derive has read the item, and here
//! it swaps `bool` and `u8`: the struct that `rustc` compiles is `{ on: u8, byte: bool }`, and the
//! derive read `{ on: bool, byte: u8 }`, whose layout would have made a `bool` of 2. The types that
//! the derive read are compared with the fields of the item as compiled, and the derive does not
//! compile.
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[rewrite::swap]
#[repr(C)]
struct Swapped {
  on: bool,
  byte: u8,
}

fn main() {
  let _ = puns::try_pun::<[u8; 2], Swapped>([1, 2]);
}
