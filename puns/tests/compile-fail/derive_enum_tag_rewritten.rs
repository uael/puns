//! An attribute below `#[derive(Pun)]` is expanded after the derive has read the item, and here it
//! makes the tag a `u16` where the derive read a `u8`. The fields are at 2 either way, and the
//! first byte of each tag is the one that the derive declares, so bytes `[0, 1, 0, 0]` would have
//! been the variant `A` for the derive and an invalid tag for the enum. The derive sets the byte
//! after the tag that it read in a sample of each variant, and the variant of that value must not
//! change: here the tag is invalid, and the derive does not compile.
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[rewrite::repr(u16)]
#[repr(u8)]
enum Widened {
  A(u16),
  B(u16),
}

fn main() {
  let _ = puns::try_pun::<[u8; 4], Widened>([0, 1, 0, 0]);
}
