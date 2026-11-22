//! Three bytes are not a whole number of pairs of bytes.
fn main() {
  let _: &[[u8; 2]] = puns::pun_ref(&[0u8; 3]);
}
