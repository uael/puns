//! A sequence is reinterpreted as a sequence of elements of the same size: its length is a number
//! of elements.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::Seq;

fn view(source: &Seq<u32>) -> &Seq<u16> {
  puns::view_ref(source)
}

fn main() {
  let _: fn(&Seq<u32>) -> &Seq<u16> = view;
}
