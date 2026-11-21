//! A sequence is reinterpreted as a sequence, not as a value.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::{One, Seq};

fn view(source: &Seq<u32>) -> &One<u32> {
  puns::view_ref(source)
}

fn main() {
  let _: fn(&Seq<u32>) -> &One<u32> = view;
}
