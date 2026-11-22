//! A slice of two-byte elements is 4 bytes only for two of them: a reference to an array of 4
//! bytes is made from it only by a check of its length.
fn main() {
  let _: &[u8; 4] = puns::pun_ref::<[u16], [u8; 4]>(&[1, 2][..]);
}
