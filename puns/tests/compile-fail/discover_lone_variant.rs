// What the discovery of the layout of an enum does with fields that are not those of the enum
// (`puns/src/discover.rs`, "Cases"). Here only `B` is said to have values, and no field is said
// of it. The enum is then taken for a struct of the fields of `B`, whose gaps must leave it `B`.
// They do not: the tag is in them. If this ever compiles, a tag would pass for padding.
use core::{convert::Infallible, mem::MaybeUninit};

use puns::{
  Pun,
  layout::{Desc, Field, Layout},
};

#[derive(Clone, Copy)]
enum Direct {
  A(u8),
  B(u8),
}

impl Direct {
  const FIELDS: [&'static [Field]; 2] = [&[Field::new(0, &<Infallible as Pun>::LAYOUT)], &[]];
  const SAMPLES: [MaybeUninit<Self>; 2] = [MaybeUninit::uninit(), MaybeUninit::new(Self::B(0))];
  // SAFETY: An associated constant is evaluated at compile time, as `discover` requires.
  const DESC: Desc<2> = unsafe { Layout::discover(&Self::SAMPLES, &Self::FIELDS) };
}

fn main() {
  let _ = Direct::DESC;
}
