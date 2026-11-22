//! A struct that ends with a `str`, from a byte of any value under the string, then a union that
//! may hold no value. The string is checked at run time, and the check reads the byte of the union.
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Byte {
  any: u8,
  none: (),
}

#[derive(Pun)]
#[repr(C)]
struct Msg {
  kind: u8,
  text: str,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct KindAnyHole {
  kind: u8,
  text: u8,
  hole: Byte,
}

fn main() {
  let source = KindAnyHole { kind: 1, text: b'a', hole: Byte { none: () } };
  let _ = puns::try_pun_ref::<KindAnyHole, Msg>(&source);
}
