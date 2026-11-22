// A struct that ends with a struct that ends with a struct without values has no value either.
use core::convert::Infallible;

#[derive(puns::Pun)]
#[repr(C)]
struct Inner {
  v: Infallible,
  tail: [u8],
}

#[derive(puns::Pun)]
#[repr(C)]
struct Outer {
  h: u8,
  inner: Inner,
}

#[derive(puns::Pun)]
#[repr(C)]
struct Deep {
  g: u8,
  outer: Outer,
}

fn main() {
  let _ = puns::pun_ref::<[u8; 4], Deep>(&[1, 2, 3, 4]);
}
