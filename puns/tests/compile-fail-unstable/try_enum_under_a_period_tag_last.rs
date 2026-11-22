//! A union that its second field proves, in a variant of an enum that is checked as a whole. Its
//! first field is an `Option` whose tag is the niche of its last byte, after two bytes of any
//! value, over an array of the source of elements of one byte. The check of the union tries the
//! first field first, which would read its tag from a byte that may be uninitialized in the
//! source.
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
struct Triple {
  x: Byte,
  y: Byte,
  z: Byte,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Late {
  bytes: [u8; 2],
  last: NonZero<u8>,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Either {
  late: Option<Late>,
  triple: Triple,
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
  bytes: [Byte; 3],
}

fn main() {
  let source = Source { tag: 0, bytes: [Byte { none: () }; 3] };
  let _ = puns::try_pun::<Source, Target>(source);
}
