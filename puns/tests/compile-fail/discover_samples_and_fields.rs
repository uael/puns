// What the discovery of the layout of an enum checks of the samples and the fields that it is
// given (`puns/src/discover.rs`): each sample is its own variant, the discriminant is an `isize`,
// as it is for an enum of the `Rust` representation, and each field is within the enum, apart
// from the other fields of its variant.
use core::mem::MaybeUninit;

use puns::{
  Pun,
  layout::{Discover, Field},
};

const BYTE: Field = Field::new(1, &<u8 as Pun>::LAYOUT);

#[derive(Clone, Copy)]
enum Swapped {
  A(u8),
  B(u8),
}

impl Discover<2> for Swapped {
  const FIELDS: [&'static [Field]; 2] = [&[BYTE], &[BYTE]];
  const SAMPLES: [MaybeUninit<Self>; 2] =
    [MaybeUninit::new(Self::B(0)), MaybeUninit::new(Self::A(0))];
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum Specified {
  A(u8),
  B(u8),
}

impl Discover<2> for Specified {
  const FIELDS: [&'static [Field]; 2] = [&[BYTE], &[BYTE]];
  const SAMPLES: [MaybeUninit<Self>; 2] =
    [MaybeUninit::new(Self::A(0)), MaybeUninit::new(Self::B(0))];
}

#[derive(Clone, Copy)]
enum Outside {
  A(u8),
  B(u8),
}

impl Discover<2> for Outside {
  const FIELDS: [&'static [Field]; 2] = [&[Field::new(2, &<u8 as Pun>::LAYOUT)], &[BYTE]];
  const SAMPLES: [MaybeUninit<Self>; 2] =
    [MaybeUninit::new(Self::A(0)), MaybeUninit::new(Self::B(0))];
}

#[derive(Clone, Copy)]
enum Twice {
  A(u8),
  B(u8),
}

impl Discover<2> for Twice {
  const FIELDS: [&'static [Field]; 2] = [&[BYTE, BYTE], &[BYTE]];
  const SAMPLES: [MaybeUninit<Self>; 2] =
    [MaybeUninit::new(Self::A(0)), MaybeUninit::new(Self::B(0))];
}

fn main() {
  let _ = <Swapped as Discover<2>>::DESC;
  let _ = <Specified as Discover<2>>::DESC;
  let _ = <Outside as Discover<2>>::DESC;
  let _ = <Twice as Discover<2>>::DESC;
}
