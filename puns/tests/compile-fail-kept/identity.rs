//! Each pun relates two types that are not the same, though they are made alike, or a type
//! without an identity to itself, at a part whose layout Rust leaves to the compiler.
use core::{marker::PhantomData, mem::ManuallyDrop};

use puns::Pun;

/// The `Rust` representation: its `bool` is the first byte, or the second.
#[derive(Pun, Clone, Copy)]
struct Mixed {
  a: u8,
  b: bool,
}

/// A generic type of the `Rust` representation, whose parameters are bounded by `Pun`.
#[derive(Pun, Clone, Copy)]
struct Three<A: Pun, B: Pun, C: Pun> {
  a: A,
  b: B,
  c: C,
}

/// A parameter that is not bounded by `Pun`: the type has no identity.
#[derive(Pun, Clone, Copy)]
struct Marked<T> {
  a: u8,
  b: bool,
  c: PhantomData<T>,
}

/// A const parameter.
#[derive(Pun, Clone, Copy)]
struct Valued<const N: i32> {
  a: u8,
  b: bool,
}

fn main() {
  let mixed = Mixed { a: 1, b: true };
  // A `ManuallyDrop<Mixed>` is laid out as a `Mixed`, but a type made of it is another type.
  let _ = puns::pun::<(ManuallyDrop<Mixed>, u8), (Mixed, u8)>((ManuallyDrop::new(mixed), 2));
  let three = Three { a: ManuallyDrop::new(1u8), b: 2u16, c: 3u32 };
  let _ = puns::pun::<Three<ManuallyDrop<u8>, u16, u32>, Three<u8, u16, u32>>(three);
  // Another parameter.
  let _ = puns::pun::<Three<u8, u16, u32>, Three<i8, u16, u32>>(Three { a: 1, b: 2, c: 3 });
  let _ = puns::pun::<Valued<-1>, Valued<1>>(Valued { a: 1, b: true });
  // No identity, not even the same type.
  let _ = puns::pun::<Marked<u8>, Marked<u8>>(Marked { a: 1, b: true, c: PhantomData });
}
