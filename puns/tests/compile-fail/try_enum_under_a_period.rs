//! A union that its second field proves, in a variant of an enum that is checked as a whole. Its
//! first field is an enum whose tag accepts every value, over an array of the source of elements
//! of one byte. The check of the union tries the first field first, which would read its tag from
//! bytes that may be uninitialized in the source. The same bytes without the array are refused
//! too.
use core::num::NonZero;

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
union Either {
  opt: Option<NonZero<u16>>,
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
  pad: u8,
  bytes: [Byte; 2],
}

fn main() {
  let source = Source { tag: 0, pad: 0, bytes: [Byte { none: () }; 2] };
  let _ = puns::try_pun::<Source, Target>(source);
}
