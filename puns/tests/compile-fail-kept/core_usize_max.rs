//! `::core` names this crate, whose `usize::MAX` is 1. The derive gives a struct of the `Rust`
//! representation its packing, `usize::MAX` where it has none, and a packing of 1 would make every
//! order of the fields of `Three` 4 bytes, as `rustc` makes it here, where `(u8, u16, u8)` would
//! make it 6. The derive writes that value without a path, so the pun depends on the order that
//! `rustc` chose, and is refused as it is without `extern crate self as core;`.
extern crate self as core;

pub use std::{marker, mem, option, ptr, result};

pub mod primitive {
  pub use std::primitive::{bool, u8, u16, u128};

  #[expect(non_camel_case_types, reason = "it stands for the `usize` of `::core`")]
  pub struct usize;

  impl usize {
    pub const MAX: std::primitive::usize = 1;
  }
}

#[derive(puns::Pun, Clone, Copy)]
struct Three {
  a: u8,
  b: u16,
  c: u8,
}

fn main() {
  let _: u32 = puns::pun(Three { a: 1, b: 2, c: 3 });
}
