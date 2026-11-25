//! Every form of `pointer(..)` and `regions(..)` parses: `eager` or `lazy`, then `mut`, before a
//! name or a type, whatever the type starts with, a name that is a keyword of the options, and a
//! pointer without an expression, whose field says where its pointee is.

use puns::{InPlace, Pun, Thin};

trait Pointed {
  type Pointee: ?Sized + Pun;
}

impl Pointed for u8 {
  type Pointee = [u8];
}

#[derive(Pun)]
#[repr(C)]
struct Forms {
  #[pun(pointer(lazy [u8] = a as u128, size = 2))]
  a: u8,
  #[pun(pointer(lazy <u8 as Pointed>::Pointee = b as u128, len = 2))]
  b: u8,
  #[pun(pointer(lazy lazy: Thin<str, u8> = c as u128))]
  c: u8,
  #[pun(pointer(mut first: [u8] = d as u128, size = ..))]
  d: InPlace<u8>,
  #[pun(pointer(eager mut second: [u8], size = ..))]
  e: InPlace<u8>,
  #[pun(pointer(lazy third: u8))]
  f: u8,
  #[pun(regions(eager: [u8] = 1, eager mut last: u8, rest))]
  tail: InPlace<[u8]>,
}

#[derive(Pun)]
#[repr(C)]
struct Names {
  #[pun(regions(lazy = 1, eager mut eager: u8, rest))]
  tail: InPlace<[u8]>,
}

fn main() {
  let bytes = [0, 0, 0, 3, 2, 0, 1, 2, 3, 4];
  let forms = puns::from_bytes::<Forms>(&bytes).unwrap();
  let _ = (forms.lazy(&bytes), forms.first(), forms.second(), forms.third(&bytes));
  let _ = (forms.eager(), forms.last(), forms.rest());
  let names = puns::from_bytes::<Names>(&bytes).unwrap();
  let _ = (names.lazy(), names.eager(), names.rest());
}
