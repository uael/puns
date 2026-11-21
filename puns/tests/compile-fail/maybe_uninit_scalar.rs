// A byte that a target requires, of which a source knows nothing: not a `u8`.
use core::mem::MaybeUninit;

use puns::layout::{Endian, Spec};

#[derive(Clone, Copy)]
#[repr(transparent)]
struct Zero(MaybeUninit<u8>);

// SAFETY: `Zero` is `repr(transparent)` over a `MaybeUninit<u8>`, so `SPEC`, of a `u8`, has its
// size. A `Zero` may hold any byte, uninitialized ones included, as `Spec::maybe_uninit` allows.
// The one value that `SPEC` accepts, 0, is a valid `Zero`, and a `Zero` holds no `UnsafeCell`.
unsafe impl puns::Scalar for Zero {
  const SPEC: Spec = Spec::ranges::<u8>(Endian::Little, &[(0, 0)]).maybe_uninit();
}

fn main() {
  let _ = puns::try_pun::<Zero, bool>(Zero(MaybeUninit::uninit()));
}
