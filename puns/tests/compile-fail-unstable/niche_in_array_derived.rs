// An enum of the `Rust` representation whose niche is in the first `bool` of an array. Each byte of
// the source is 0, 1 or 2. Whatever value `rustc` chose for `Unset`, the first `bool` is one in
// `Set`, and the next ones may be 2.
use puns::num::u8in;

#[derive(puns::Pun, Clone, Copy)]
enum Flags {
  Set([bool; 3]),
  Unset,
}

fn main() {
  let two = u8in::<0, 2>::new(2).unwrap();
  let _: Flags = puns::pun([u8in::<0, 2>::new(1).unwrap(), two, two]);
}
