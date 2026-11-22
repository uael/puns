//! A string type whose automaton does not accept the empty string, from a slice that may have no
//! element. The string is checked at run time, and the check reads the second byte of each element,
//! a union that may hold no value.
#[path = "../support/non_empty.rs"]
mod non_empty;

use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Byte {
  any: u8,
  none: (),
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct AnyThenHole {
  text: u8,
  hole: Byte,
}

fn main() {
  let source = [AnyThenHole { text: b'a', hole: Byte { none: () } }];
  let _ = puns::try_pun_ref::<[AnyThenHole], non_empty::NonEmpty>(&source);
}
