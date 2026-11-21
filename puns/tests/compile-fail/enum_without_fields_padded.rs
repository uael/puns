//! An `align` greater than its discriminant gives an enum without fields padding, which the
//! derive refuses: it reads the values of the enum from all of its bytes.
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(u8, align(4))]
enum Padded {
  A = 0,
  B = 1,
}

fn main() {
  let _: Padded = puns::pun(Padded::A);
}
