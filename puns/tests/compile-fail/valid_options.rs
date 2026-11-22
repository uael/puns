//! `valid` restricts a scalar to some of its values: not a type that is not a scalar, and not to an
//! empty range or to values that the type does not have.

#[derive(puns::Pun)]
#[repr(C)]
struct EmptyRange {
  #[pun(valid(3..3))]
  a: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
struct EmptyInclusiveRange {
  #[pun(valid(4..=3))]
  a: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
struct NotValuesOfTheType {
  #[pun(valid(0..=2))]
  on: bool,
}

#[derive(puns::Pun)]
#[repr(C)]
struct NotAScalar {
  #[pun(valid(0..=1))]
  pair: [u8; 2],
}

fn main() {}
