//! The `M` of a `Thin<T, M>` ends with the length of the `T`: an integer, with no padding after it.

use puns::{Pun, Thin};

#[derive(Pun)]
#[repr(C)]
struct Flagged {
  len: u16,
  flag: bool,
}

#[derive(Pun)]
#[repr(C)]
struct Text {
  #[pun(pointer(lazy text: Thin<str, Flagged> = at as u128))]
  at: u8,
}

fn main() {
  let _ = puns::from_head::<Text>(&[0; 8]);
}
