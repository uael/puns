// `A` has no value, and `rustc` lays it out all the same, since its `u8` has a size. `B`, the only
// variant that has values, has no field, so nothing tells where the tag ends: the two bytes of
// the enum are read as the tag in the sample of `B`. The second one is padding there, and const
// evaluation fails the build. If this ever compiles, padding would pass for a tag.
use core::convert::Infallible;

#[derive(puns::Pun, Clone, Copy)]
enum Shape {
  A(u8, Infallible),
  B,
}

fn main() {}
