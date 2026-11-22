// A slice of elements without values has one value, the empty one, and is not made from bytes,
// which may give it elements. This is stricter than needed: empty bytes would be valid.
use core::convert::Infallible;

#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
struct Void1 {
  n: Infallible,
  x: u8,
}

fn main() {
  let _ = puns::from_bytes::<[Void1]>(&[1, 2]);
}
