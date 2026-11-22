//! A pattern is a string of bytes, which an automaton reads one by one: a slice of `u16`s is not.
use puns::{
  Pun,
  layout::{Dfa, Layout},
};

// SAFETY: `[u16]` is a slice, the probe is `<[u8] as Pun>::PROBE` cast to `*const [u16]` with an
// `as` cast, and its 3 elements fit in an `isize`. Its elements are not bytes, which
// `Layout::pattern` refuses.
const _: Layout =
  unsafe { Layout::pattern::<[u16]>(<[u8] as Pun>::PROBE as *const [u16], Dfa::DIGITS) };

fn main() {}
