//! `le` and `be` read 1 to 16 bytes. The derive refuses a literal size where it is written, and
//! the layout a constant one, where the type is used.
use puns::Pun;

const SIZE: u8 = 17;

#[derive(Pun)]
#[repr(C)]
#[pun(assert(be(0, SIZE) == 0))]
struct Wide {
  a: [u8; 17],
}

fn main() {
  let _ = puns::from_bytes::<Wide>(&[0; 17]);
}
