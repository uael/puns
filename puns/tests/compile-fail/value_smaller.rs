//! A value is not made from a smaller one: bytes would be missing.
fn main() {
  let _: u16 = puns::pun(1u8);
}
