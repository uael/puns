//! A tuple of 13 elements is not a `Pun` type: `puns` implements `Pun` for tuples of up to 12, as
//! `core` implements its own traits.
type Thirteen<T> = (T, T, T, T, T, T, T, T, T, T, T, T, T);

fn main() {
  let _ = puns::pun::<[u8; 13], Thirteen<u8>>([1; 13]);
}
