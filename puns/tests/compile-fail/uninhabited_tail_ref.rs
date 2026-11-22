// A struct that ends with a struct without values has no value, so no array of bytes is one.
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

fn main() {
  let _ = puns::pun_ref::<[u8; 3], Outer>(&[1, 2, 3]);
}
