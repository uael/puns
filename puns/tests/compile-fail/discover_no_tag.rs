// What the discovery of the layout of an enum does with fields that are not those of the enum
// (`puns/src/discover.rs`, "Cases"). Here the fields are said to start at 0, over the tag, so a
// niche is looked for in them. A `u8` has none, and no tag is found. If this ever compiles, an
// enum would be taken without the place that tells its variants.
use core::mem::MaybeUninit;

use puns::{
  Pun,
  layout::{Discover, Field},
};

#[derive(Clone, Copy)]
enum Direct {
  A(u8),
  B(u8),
}

impl Discover<2> for Direct {
  const FIELDS: [&'static [Field]; 2] =
    [&[Field::new(0, &<u8 as Pun>::LAYOUT)], &[Field::new(0, &<u8 as Pun>::LAYOUT)]];
  const SAMPLES: [MaybeUninit<Self>; 2] =
    [MaybeUninit::new(Self::A(0)), MaybeUninit::new(Self::B(0))];
}

fn main() {
  let _ = <Direct as Discover<2>>::DESC;
}
