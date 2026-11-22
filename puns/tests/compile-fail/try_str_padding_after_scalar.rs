//! A `str` from a `NonZero<u8>`, then a union that may hold no value. Not every value of the
//! scalar is UTF-8, so the string is checked at run time, and the check reads the byte of the union.
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
struct ScalarThenHole {
  first: NonZero<u8>,
  hole: Byte,
}

fn main() {
  let source = ScalarThenHole { first: NonZero::new(b'a').unwrap(), hole: Byte { none: () } };
  let _ = puns::try_pun_ref::<ScalarThenHole, str>(&source);
}
