//! A value that holds an opaque scalar is not made from the head of bytes either.
#[path = "../support/opaque.rs"]
mod opaque;

fn main() {
  let _ = puns::from_head::<opaque::Node>(&[0; 8]);
}
