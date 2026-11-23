//! The first two bytes of a `[u8; 4]` are not always where a `u16` can be. The checked functions
//! of a whole value need the same size, so the message suggests `head` and `from_head`.
fn main() {
  let _: &u16 = puns::head_ref(&[0u8; 4]);
  let _: &mut u16 = puns::head_mut(&mut [0u8; 4]);
}
