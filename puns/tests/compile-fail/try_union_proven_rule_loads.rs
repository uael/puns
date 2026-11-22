//! A union that its second field proves, since that field takes a byte of a union, in a variant of
//! an enum that is checked as a whole. The check of the union tries its first field first, whose
//! rule loads `x`, a byte that may be uninitialized in the source, though it is of any value. The
//! same with the load in a walk.
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Byte {
  any: u8,
  none: (),
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(x < 3))]
struct Small {
  x: u8,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Rule {
  small: Small,
  byte: Byte,
}

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Target {
  V(Rule),
  X,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(walk(0, |i| i < x, |i| i + 1) < 3))]
struct Walked {
  x: u8,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Walk {
  walked: Walked,
  byte: Byte,
}

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Walks {
  V(Walk),
  X,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Source {
  tag: u8,
  byte: Byte,
}

fn main() {
  let source = Source { tag: 0, byte: Byte { none: () } };
  let _ = puns::try_pun::<Source, Target>(source);
  let _ = puns::try_pun::<Source, Walks>(source);
}
