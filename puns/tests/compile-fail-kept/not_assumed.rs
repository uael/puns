//! Each pun depends on the layout of an enum of the `Rust` representation that `puns` does not
//! assume, because `rustc` may lay it out otherwise in another layout of its fields.
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

fn main() {
  // A niche in a tuple, whose `bool` is either field.
  let _ = puns::try_pun::<[u8; 4], Option<(bool, u16)>>([0; 4]);
  let _ = puns::try_pun::<[u8; 2], Twins>([0; 2]);
  let _ = puns::try_pun::<[u16; 3], Uneven>([0; 3]);
  // Whether `rustc` gives `Percent` a niche is not in its `Spec`.
  let _ = puns::try_pun::<[u8; 2], Option<Percent>>([0; 2]);
}
