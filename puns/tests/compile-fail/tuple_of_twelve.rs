//! A tuple of 12 elements is refused as a smaller one is: not every `u8` is a `bool`.
type Twelve<T> = (T, T, T, T, T, T, T, T, T, T, T, T);

fn main() {
  let _ = puns::pun::<[u8; 12], Twelve<bool>>([1; 12]);
}
