// A struct that ends with a slice of elements without values is valid only with no element. Three
// bytes give it one after its first field.
use core::convert::Infallible;

#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
struct Void1 {
  n: Infallible,
  x: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
struct Tail {
  h: u8,
  items: [Void1],
}

fn main() {
  let _ = puns::pun_ref::<[u8; 3], Tail>(&[1, 2, 3]);
}
