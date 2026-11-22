//! A union that its second field proves, in a variant of an enum that is checked as a whole. Its
//! first field is a union of two bytes, over an array of the source of elements of one byte. The
//! check of the outer union tries the first field first, whose check would read bytes that may be
//! uninitialized in the source.
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
union Flags {
  both: [bool; 2],
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Either {
  flags: Flags,
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
  bytes: [Byte; 2],
}

fn main() {
  let source = Source { tag: 0, bytes: [Byte { none: () }; 2] };
  let _ = puns::try_pun::<Source, Target>(source);
}
