//! A variant that holds a scalar that a pun never makes has no sample, so its tag is not read from
//! the enum. Its discriminant must then be an integer literal, which is the same value wherever the
//! derive evaluates it again, or none after a variant whose tag is known.
#[path = "../support/never.rs"]
#[allow(dead_code)]
mod never;

const TWO: u8 = 2;

#[derive(puns::Pun, Clone, Copy)]
#[repr(u8)]
#[allow(dead_code)]
enum E {
  A(never::Off) = TWO,
  B(u8),
}

fn main() {
  let _ = puns::try_pun::<[u8; 8], E>([3, 0, 0, 0, 7, 0, 0, 0]);
}
