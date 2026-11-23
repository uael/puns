//! A value that holds an opaque scalar is not made from bytes.
#[path = "../support/opaque.rs"]
mod opaque;

fn main() {
  let _ = puns::from_bytes::<opaque::Node>(&[0; 8]);
  let _ = puns::try_pun::<u32, opaque::Off>(5);
}
