//! A value that holds a scalar that a pun never makes is not made from bytes.
#[path = "../support/never.rs"]
mod never;

fn main() {
  let _ = puns::from_bytes::<never::Node>(&[0; 8]);
  let _ = puns::try_pun::<u32, never::Off>(5);
}
