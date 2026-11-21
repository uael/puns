//! What is proven of values that hold pointers, next to what is refused in `compile-fail`.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;

use puns::{Pun, num::i16le};
use relative::{One, Opt, Seq, Text};

#[derive(Pun)]
#[repr(u8)]
enum Old {
  Leaf(bool),
  Node(One<Old>),
}

#[derive(Pun)]
#[repr(u8)]
#[allow(dead_code)]
enum New {
  Leaf(u8),
  Node(One<New>),
  More(Seq<New>),
}

#[derive(Pun)]
#[repr(C)]
struct Named {
  name: Text<i16le>,
  // As aligned as the declarer of `Seq` says: the larger of 4 and the alignment of an element.
  words: Seq<[u8; 4]>,
  next: Opt<Named>,
}

#[derive(Pun)]
#[repr(C)]
struct Numbered {
  name: Text<i16le>,
  words: Seq<u32>,
  next: Opt<Numbered>,
}

fn view(old: &Old) -> &New {
  puns::view_ref(old)
}

fn named(named: &Named) -> &Numbered {
  puns::view_ref(named)
}

fn main() {
  let _: fn(&Old) -> &New = view;
  let _: fn(&Named) -> &Numbered = named;
  // A pointer is reinterpreted as bytes; a null one as a null one.
  let none: Opt<u32> = Opt::NONE;
  assert_eq!(puns::as_bytes(&none), [0; 4]);
  let _: &Opt<[u8; 4]> = puns::view_ref(&none);
}
