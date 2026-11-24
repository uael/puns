//! Each pun relates two types that are not the same, though they are made alike, at a part whose
//! layout Rust leaves to the compiler.
use core::mem::ManuallyDrop;

use puns::Pun;

/// The `Rust` representation: its `bool` is the first byte, or the second.
#[derive(Pun, Clone, Copy)]
struct Mixed {
  a: u8,
  b: bool,
}

fn main() {
  let mixed = Mixed { a: 1, b: true };
  // A `ManuallyDrop<Mixed>` is laid out as a `Mixed`, but a type made of it is another type.
  let _ = puns::pun::<(ManuallyDrop<Mixed>, u8), (Mixed, u8)>((ManuallyDrop::new(mixed), 2));
}
