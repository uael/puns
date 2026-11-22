//! A union whose first field is a struct of the `Rust` representation, which the source does not
//! prove, and whose second field proves the union, in a variant of an enum that is checked as a
//! whole. The check of the union tries the struct first, where `rustc` put its fields, which would
//! read a byte that may be uninitialized in the source.
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Byte {
  any: u8,
  none: (),
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Pair {
  x: Byte,
  y: Byte,
}

#[derive(Pun, Clone, Copy)]
struct Loose {
  flag: bool,
  byte: u8,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Either {
  loose: Loose,
  pair: Pair,
}

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Target {
  V(Either),
  X,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Source {
  tag: u8,
  x: Byte,
  y: Byte,
}

fn main() {
  let source = Source { tag: 0, x: Byte { none: () }, y: Byte { none: () } };
  let _ = puns::try_pun::<Source, Target>(source);
}
