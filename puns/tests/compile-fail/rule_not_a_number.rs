//! A rule reads unsigned integer fields, and signed ones only under a cast, since a negative value
//! is not a `u128`. The bits of a floating-point number, of a `bool` or of an enum are not an
//! integer.

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(x + 1 > 0))]
struct Signed {
  x: i8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(x > 0))]
struct Float {
  x: f32,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(x == 1))]
struct Bool {
  x: bool,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(x as u128 > 0))]
struct CastFloat {
  x: f32,
}

fn main() {}
