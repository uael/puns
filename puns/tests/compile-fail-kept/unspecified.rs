//! Each pun depends on where `rustc` put something that Rust lets it put elsewhere.
use puns::Pun;

/// The `Rust` representation: its `bool` is the first byte, or the second.
#[derive(Pun, Clone, Copy)]
struct Mixed {
  a: u8,
  b: bool,
}

/// The `Rust` representation, with a rule: which byte is `a`.
#[derive(Pun, Clone, Copy)]
#[pun(assert(a < b))]
struct Ordered {
  a: u8,
  b: u8,
}

fn main() {
  // Which byte must be 0 or 1.
  let _ = puns::from_bytes::<Mixed>(&[1, 1]);
  let _ = puns::try_pun::<[u8; 2], (bool, u8)>([1, 1]);
  // Which bytes a rule holds of.
  let _ = puns::from_bytes::<Ordered>(&[1, 2]);
  // Where the padding is: the second byte, or the last.
  let _ = puns::head::<(u8, u16), [u8; 2]>((1, 2));
  // The size: 4 or 6 bytes.
  let _ = puns::as_bytes(&(1u8, 2u16, 3u8));
  // What a `&mut` lets be written.
  let _ = puns::pun_mut::<(bool, u8), [u8; 2]>(&mut (true, 1));
}
