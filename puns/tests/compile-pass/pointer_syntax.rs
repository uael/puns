//! Every form of `pointer(..)` and `regions(..)` parses: a pointer with a name or with a type only,
//! whatever the type starts with, after `shared` or not, `eager` or `lazy` before a region, a name
//! that is a keyword of the options, and a pointer without an expression, whose field says where its
//! pointee is. `AIMS` names each eager aim with a name. The pointees are within the struct, which
//! only a shared pointee, or one that ends with the tail of the struct, may be.

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
  #[pun(pointer(shared [u8] = a as u128, size = 2))]
  a: InPlace<u8>,
  #[pun(pointer(shared <u8 as Pointed>::Pointee = b as u128, len = 2))]
  b: InPlace<u8>,
  #[pun(pointer(shared lazy: Thin<str, u8> = c as u128))]
  c: InPlace<u8>,
  #[pun(pointer(first: [u8] = d as u128, size = ..))]
  d: InPlace<u8>,
  #[pun(pointer(second: [u8], size = ..))]
  e: InPlace<u8>,
  #[pun(pointer(shared third: u8))]
  f: InPlace<u8>,
  #[pun(regions(eager: [u8] = 1, eager last: u8, rest))]
  tail: InPlace<[u8]>,
}

#[derive(Pun)]
#[repr(C)]
struct Names {
  #[pun(regions(lazy = 1, eager eager: u8, rest))]
  tail: InPlace<[u8]>,
}

fn main() {
  // Pointees small enough for the budget of a check, twice the bytes.
  let bytes = [0, 0, 0, 9, 9, 0, 1, 2, 3, 4];
  let forms = puns::from_bytes::<Forms>(&bytes).unwrap();
  let _ = (forms.lazy(), forms.first(), forms.second(), forms.third());
  let _ = (forms.eager(), forms.last(), forms.rest());
  let _ = (Forms::AIMS.lazy, Forms::AIMS.first, Forms::AIMS.second, Forms::AIMS.third);
  let names = puns::from_bytes::<Names>(&bytes).unwrap();
  let _ = (names.lazy(), names.eager(), names.rest(), Names::AIMS.eager);
}
