//! Without the `unstable` feature, the pointee of an eager aim is not checked where its layout is
//! one that Rust leaves to the compiler and something in it is checked: its places could differ.
#[allow(dead_code)]
#[path = "../support/aims.rs"]
mod aims;

#[derive(puns::Pun)]
struct Rusty {
  flag: bool,
  word: u32,
}

fn main() {
  let _ = puns::from_head::<aims::Near<Rusty>>(&[0; 16]);
}
