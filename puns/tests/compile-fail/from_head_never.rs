//! A value that holds a scalar that a pun never makes is not made from the head of bytes either.
#[path = "../support/never.rs"]
mod never;

fn main() {
  let _ = puns::from_head::<never::Node>(&[0; 8]);
}
