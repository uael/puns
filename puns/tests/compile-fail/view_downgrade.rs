//! A value is not reinterpreted as a type that has fewer values: here a variant less.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::{One, Seq};

#[derive(puns::Pun)]
#[repr(u8)]
enum New {
  Leaf(u32),
  Node(One<New>),
  More(Seq<New>),
}

#[derive(puns::Pun)]
#[repr(u8)]
enum Old {
  Leaf(u32),
  Node(One<Old>),
}

fn view(new: &New) -> &Old {
  puns::view_ref(new)
}

fn main() {
  let _: fn(&New) -> &Old = view;
}
