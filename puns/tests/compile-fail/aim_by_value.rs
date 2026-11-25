//! A value that holds an eager aim is not made by value: it would not be where its target was
//! checked. `Near` is not `Copy`, which every function by value requires.
#[allow(dead_code)]
#[path = "../support/aims.rs"]
mod aims;

fn main() {
  let _ = puns::try_pun::<[u8; 4], aims::Near<u32>>([4, 0, 0, 0]);
}
