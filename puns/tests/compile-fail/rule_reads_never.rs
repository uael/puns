//! A rule reads integers: a scalar that a pun never makes is not one, since what it holds is not
//! known to be a number.
#[path = "../support/never.rs"]
#[allow(dead_code)]
mod never;
use never::Off;

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(next != 0))]
struct Node {
  id: u32,
  next: Off,
}

fn main() {
  let _ = puns::validate_but_never::<Node>(&[0; 8]);
}
