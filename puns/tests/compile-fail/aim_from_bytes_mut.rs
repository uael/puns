//! No `&mut` is given to a value that holds an eager aim: a write could move it away from its
//! target, which is read without a check.
#[allow(dead_code)]
#[path = "../support/aims.rs"]
mod aims;

fn main() {
  let _ = puns::from_bytes_mut::<aims::Pair>(&mut [0; 12]);
}
