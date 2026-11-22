//! The value of an enum without fields is at most 16 bytes, which `#[derive(Pun)]` reads as an
//! integer: `align(32)` makes this one 32.

#[derive(puns::Pun, Clone, Copy)]
#[repr(u8, align(32))]
enum Aligned {
  A,
  B,
}

fn main() {
  let _ = puns::try_pun::<_, Aligned>([0u8; 32]);
}
