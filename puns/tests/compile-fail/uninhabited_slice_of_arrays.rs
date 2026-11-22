// A slice of arrays of elements without values has one value, the empty one.
use core::convert::Infallible;

#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
struct Void1 {
  n: Infallible,
  x: u8,
}

fn main() {
  let _ = puns::from_bytes::<[[Void1; 2]]>(&[1, 2]);
}
