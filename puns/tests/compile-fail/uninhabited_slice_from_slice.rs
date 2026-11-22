// A slice of elements without values has one value, the empty one. A slice of bytes may give it
// elements.
use core::convert::Infallible;

#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
struct Void1 {
  n: Infallible,
  x: u8,
}

fn main() {
  let _ = puns::pun_ref::<[u8], [Void1]>(&[1, 2]);
}
