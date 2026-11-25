//! The `M` of a `Thin<T, M>` ends with the length of the `T`: an integer, with no padding after it.

use puns::{InPlace, Pun, Thin};

#[derive(Pun)]
#[repr(C)]
struct Flagged {
  len: u16,
  flag: bool,
}

#[derive(Pun)]
#[repr(C)]
struct Text {
  #[pun(pointer(text: Thin<str, Flagged> = at as u128))]
  at: InPlace<u8>,
}

fn main() {
  let _ = puns::from_head::<Text>(&[0; 8]);
}
