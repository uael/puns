//! A `&mut` to bytes that may be uninitialized, over a tuple: Rust leaves the layout of the tuple
//! to the compiler, so what a write would change of it is not known.
use core::mem::MaybeUninit;

#[derive(puns::Pun)]
#[repr(C)]
struct Record {
  pair: (u8, u16),
  last: u8,
}

fn main() {
  let mut record = Record { pair: (1, 2), last: 3 };
  let _ = puns::head_mut::<Record, [MaybeUninit<u8>; 4]>(&mut record);
}
