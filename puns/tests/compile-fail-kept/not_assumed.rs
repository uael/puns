//! Each pun depends on the layout of an enum of the `Rust` representation that `puns` does not
//! assume, because `rustc` may lay it out otherwise in another layout of its fields.
use core::num::NonZero;

use puns::{
  Pun, Scalar,
  layout::{Endian, Spec},
};

/// Two fields with a niche each, with as many free values: `rustc` chooses which one holds the tag.
#[derive(Pun, Clone, Copy)]
enum Twins {
  Both(bool, bool),
  Neither,
}

/// Fields of different alignments: the tag has the alignment of the one that `rustc` puts first.
#[derive(Pun, Clone, Copy)]
enum Uneven {
  Both(u16, [u8; 2]),
  Neither,
}

/// A scalar declared by hand, whose code keeps its values: Rust allows every byte.
#[derive(Clone, Copy)]
#[repr(transparent)]
struct Percent(u8);

// SAFETY: `Percent` is `repr(transparent)` over a `u8`, so it has its size. No value of it is
// made in this test, and every value that `SPEC` accepts is a valid `u8`. It holds no
// `UnsafeCell`.
unsafe impl Scalar for Percent {
  const SPEC: Spec = Spec::ranges::<Percent>(Endian::NATIVE, &[(0, 100)]);
}

/// A `u32` stored little-endian, never zero, whose `Spec` does not declare the type that its bytes
/// are (`Spec::stored_as`).
#[derive(Clone, Copy)]
#[repr(transparent)]
struct U32le(NonZero<u32>);

// SAFETY: `U32le` is `repr(transparent)` over a `NonZero<u32>`, so it has its size, and its values
// are every 4 bytes but zeros, which `SPEC` accepts, read little-endian. It holds no `UnsafeCell`.
unsafe impl Scalar for U32le {
  const SPEC: Spec = Spec::ranges::<U32le>(Endian::Little, &[(1, u32::MAX as u128)]);
}

fn main() {
  // A niche in a tuple, whose `bool` is either field.
  let _ = puns::try_pun::<[u8; 4], Option<(bool, u16)>>([0; 4]);
  let _ = puns::try_pun::<[u8; 2], Twins>([0; 2]);
  let _ = puns::try_pun::<[u16; 3], Uneven>([0; 3]);
  // Whether `rustc` gives `Percent` a niche is not in its `Spec`.
  let _ = puns::try_pun::<[u8; 2], Option<Percent>>([0; 2]);
  let _ = puns::pun::<[u8; 4], Option<U32le>>([0; 4]);
}
