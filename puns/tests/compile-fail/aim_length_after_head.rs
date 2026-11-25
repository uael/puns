//! The pointee of an eager aim stores its length in its sized fields: a `len` rule that reads a
//! byte by offset could read past them, so the check refuses it, and the read never meets a length
//! that it cannot read.
use puns::{InPlace, Pun};

/// A list whose length is its first element, after a byte.
#[derive(Pun)]
#[repr(C)]
struct Counted {
  first: u8,
  #[pun(len = le(1, 1))]
  data: [u8],
}

/// A pointer to a `Counted`.
#[derive(Pun)]
#[repr(transparent)]
struct ToCounted {
  #[pun(pointer(Counted = off as u128))]
  off: InPlace<i8>,
}

fn main() {
  let _ = puns::from_head::<ToCounted>(&[1, 0, 2, 5]);
}
