// What the discovery of the layout of an enum does with fields that are not those of the enum
// (`puns/src/discover.rs`, "Cases"). Here the fields are said to start at 1, so byte 0 is taken
// for a direct tag. It is a `u8` of both variants, and the niche that tells them is in byte 1.
// Moved from one sample to the other, byte 0 changes no variant. If this ever compiles, a byte
// that tells nothing would pass for a tag.
use core::mem::MaybeUninit;

use puns::{
  Pun,
  layout::{Discover, Field},
};

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Pair(u8, bool);

#[derive(Clone, Copy)]
enum Niche {
  A(Pair),
  B(u8),
}

impl Discover<2> for Niche {
  const FIELDS: [&'static [Field]; 2] = [&[Field::new(1, &<bool as Pun>::LAYOUT)], &[]];
  const SAMPLES: [MaybeUninit<Self>; 2] =
    [MaybeUninit::new(Self::A(Pair(0, false))), MaybeUninit::new(Self::B(0))];
}

fn main() {
  let _ = <Niche as Discover<2>>::DESC;
}
