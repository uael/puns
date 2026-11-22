// A slice of elements without values is proven by a slice of the same elements only. Here it is
// refused from a slice of other elements without values, which is stricter than needed: both
// slices can only be empty.
use core::convert::Infallible;

#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
struct Void1 {
  n: Infallible,
  x: u8,
}

#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
struct Void2 {
  x: u8,
  n: Infallible,
}

fn main() {
  let _ = puns::pun_ref::<[Void1], [Void2]>(&[]);
}
