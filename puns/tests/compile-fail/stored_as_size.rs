//! The type that the bytes of a scalar are must have its size and its alignment.
use puns::{Pun, Scalar, layout::Spec};

/// Two bytes, declared as a `u32`: not its size.
#[derive(Clone, Copy)]
#[repr(transparent)]
struct Short(u16);

// SAFETY: `Short` is `repr(transparent)` over a `u16`, so it has its size, and every value of it is
// a valid `Short`, which `SPEC` accepts. It holds no `UnsafeCell`.
unsafe impl Scalar for Short {
  const SPEC: Spec = Spec::any::<Short>().stored_as::<u32>();
}

/// Four bytes aligned to 1, declared as a `u32`, aligned to 4.
#[derive(Clone, Copy)]
#[repr(transparent)]
struct Loose([u8; 4]);

// SAFETY: `Loose` is `repr(transparent)` over a `[u8; 4]`, so it has its size, and every value of
// it is a valid `Loose`, which `SPEC` accepts. It holds no `UnsafeCell`.
unsafe impl Scalar for Loose {
  const SPEC: Spec = Spec::any::<Loose>().stored_as::<u32>();
}

fn main() {
  let _ = <Short as Pun>::LAYOUT;
  let _ = <Loose as Pun>::LAYOUT;
}
