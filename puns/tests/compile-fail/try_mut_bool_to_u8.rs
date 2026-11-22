//! A check does not make the `&mut` valid: a `u8` that is not a `bool` could be written back.
fn main() {
  let _ = puns::try_pun_mut::<bool, u8>(&mut true);
}
