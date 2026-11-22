//! A union whose first field the source proves through a rule, in a variant of an enum that is
//! checked as a whole. Safe code may write any `u8` to `x`. The check of the first field then
//! fails, and the check tries the second field, whose `bool`s would read `rest`, a byte that may
//! be uninitialized in the source.
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Byte {
  any: u8,
  none: (),
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(x > 1))]
struct Ruled {
  x: u8,
  rest: Byte,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Either {
  ruled: Ruled,
  flags: [bool; 2],
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
  ruled: Ruled,
}

fn main() {
  let source = Source { tag: 0, ruled: Ruled { x: 1, rest: Byte { none: () } } };
  let _ = puns::try_pun::<Source, Target>(source);
}
