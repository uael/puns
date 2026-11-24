//! No code but `puns` makes an `InPlace`, and `puns` makes none, so no safe code makes a value of a
//! struct with an eager aim: neither a constructor, `Clone`, `Default`, nor a pun.

use puns::InPlace;

#[derive(puns::Pun, Clone, Default)]
#[repr(transparent)]
struct Near {
  #[pun(pointer(u32 = off as u128))]
  off: InPlace<i32>,
}

fn main() {
  let _ = Near { off: InPlace(1000) };
  let _: InPlace<i32> = puns::pun(1000);
}
