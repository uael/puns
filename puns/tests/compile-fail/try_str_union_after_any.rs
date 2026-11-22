//! A `str` from a byte of any value, then a union that may hold no value. The string is checked at
//! run time, and the check reads the byte of the union.
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
  let source = AnyThenHole { text: b'a', hole: Byte { none: () } };
  let _ = puns::try_pun_ref::<AnyThenHole, str>(&source);
}
