//! A pointer is reinterpreted as a pointer whose offset is the same integer: not one of another
//! byte order.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use puns::num::{i32be, i32le};
use relative::One;

fn view(source: &One<u32, i32le>) -> &One<u32, i32be> {
  puns::view_ref(source)
}

fn main() {
  let _: fn(&One<u32, i32le>) -> &One<u32, i32be> = view;
}
