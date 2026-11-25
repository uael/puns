//! Zero bytes are a `Pair` only by the tag that `rustc` gives `Both`, which the `Rust`
//! representation leaves to the compiler: `puns` does not assume the layout of an enum whose
//! variant holds fields of different alignments.
#[derive(puns::Pun, Clone, Copy)]
enum Pair {
  Both(u8, u16),
  Neither,
}

fn main() {
  let _ = puns::zeroed::<Pair>();
}
