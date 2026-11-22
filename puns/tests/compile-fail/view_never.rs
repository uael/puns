//! A view does not make a value that holds a scalar that a pun never makes, from bytes.
#[path = "../support/never.rs"]
mod never;

#[derive(puns::Pun)]
enum View<'a> {
  Node(&'a never::Node),
  Bytes(&'a [u8]),
}

fn main() {
  let _ = View::from_bytes(&[0; 8]);
}
