//! A tuple of 12 elements is refused as a smaller one is: Rust lets the compiler put its `bool`
//! at any byte, so which byte must be 0 or 1 depends on the layout. The `unstable` feature checks
//! it where `rustc` put it (`a_tuple_of_twelve` in `tests/api.rs`).
type First = (bool, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8);

fn main() {
  let _ = puns::try_pun::<[u8; 12], First>([1; 12]);
}
