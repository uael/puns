// A slice of tuples without values has one value, the empty one, and is not made from bytes.
fn main() {
  let _ = puns::from_bytes::<[(core::convert::Infallible, u8)]>(&[1, 2]);
}
