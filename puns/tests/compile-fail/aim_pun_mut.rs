//! No `&mut` is made from or to a value that holds an eager aim, even of the same type, nor to a
//! part of one.
#[allow(dead_code)]
#[path = "../support/aims.rs"]
mod aims;

use aims::{Near, Pair};

fn same(pair: &mut Pair) -> &mut Pair {
  puns::pun_mut(pair)
}

fn bytes(pair: &mut Pair) -> &mut [u8; 12] {
  puns::pun_mut(pair)
}

fn part(pair: &mut Pair) -> &mut Near<str> {
  puns::head_mut(pair)
}

fn main() {
  let functions: (
    fn(&mut Pair) -> &mut Pair,
    fn(&mut Pair) -> &mut [u8; 12],
    fn(&mut Pair) -> &mut Near<str>,
  ) = (same, bytes, part);
  core::hint::black_box(functions);
}
