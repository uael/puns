//! The structs of `try_union_proven_rule_loads`, as the elements of an array in the first field of
//! a union that a later field proves. The source has elements of the same size at the same place,
//! or other parts, and the array is short, or longer than the elements that the analysis looks at
//! one by one. Each check of the union would load `x` of an element from a byte that may be
//! uninitialized in the source.
use core::mem::MaybeUninit;

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
#[repr(C)]
#[pun(assert(x < 3))]
struct Small {
  x: u8,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Short {
  small: [Small; 2],
  byte: Byte,
}

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Shorts {
  V(Short),
  X,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Long {
  small: [Small; 66],
  raw: [MaybeUninit<u8>; 66],
}

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Longs {
  V(Long),
  X,
}

/// The same elements: `same_elements`.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Same {
  tag: u8,
  bytes: [Byte; 2],
}

/// Other parts: each element on its own.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Fields {
  tag: u8,
  pair: Pair,
}

/// 66 bytes of other parts, without an array, whose periods would cut the union.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Eight {
  a: Pair,
  b: Pair,
  c: Pair,
  d: Pair,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct ThirtyTwo {
  a: Eight,
  b: Eight,
  c: Eight,
  d: Eight,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Many {
  tag: u8,
  a: ThirtyTwo,
  b: ThirtyTwo,
  c: Pair,
}

fn main() {
  let none = Byte { none: () };
  let pair = Pair { x: none, y: none };
  let eight = Eight { a: pair, b: pair, c: pair, d: pair };
  let many = ThirtyTwo { a: eight, b: eight, c: eight, d: eight };
  let _ = puns::try_pun::<Same, Shorts>(Same { tag: 0, bytes: [none; 2] });
  let _ = puns::try_pun::<Fields, Shorts>(Fields { tag: 0, pair });
  let _ = puns::try_pun::<Many, Longs>(Many { tag: 0, a: many, b: many, c: pair });
}
