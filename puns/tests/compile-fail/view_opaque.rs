//! A view does not make a value that holds an opaque scalar, from bytes.
#[path = "../support/opaque.rs"]
mod opaque;

#[derive(puns::Pun)]
enum View<'a> {
  Node(&'a opaque::Node),
  Bytes(&'a [u8]),
}

fn main() {
  let _ = View::from_bytes(&[0; 8]);
}
