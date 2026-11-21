//! The layout of an `Option` is found on a sample of its `Some`, which holds the first variant that
//! has values of the enum. That variant holds a scalar that a pun never makes, of which `puns` has
//! no value.
#[path = "../support/never.rs"]
#[allow(dead_code)]
mod never;
use never::Off;

#[derive(puns::Pun, Clone, Copy)]
#[repr(C, u32)]
enum First {
  Next(Off) = 1,
  Value(u32) = 2,
}

fn main() {
  let _: Option<First> = puns::pun(None::<First>);
}
