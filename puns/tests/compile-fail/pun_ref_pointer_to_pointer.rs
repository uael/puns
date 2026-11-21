//! A pointer is not reinterpreted as a pointer by `pun_ref`, even one of the same type: only
//! `view_ref` proves what a pointer aims at.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::One;

fn read(pointer: &One<u32>) -> &One<[u8; 4]> {
  puns::pun_ref(pointer)
}

fn main() {
  let _: fn(&One<u32>) -> &One<[u8; 4]> = read;
}
