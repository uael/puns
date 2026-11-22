// A struct of a source that holds a union none of whose fields has values may exist, and no value
// of it is a value of a target without values.
use core::convert::Infallible;

/// A union none of whose fields has values. Unsafe code can make one under the rule for unions
/// that lets a union hold any bytes.
#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
union Hollow {
  a: Infallible,
}

#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
struct HS {
  u: Hollow,
  x: u8,
}

#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
struct Void1 {
  n: Infallible,
  x: u8,
}

fn convert(hs: &HS) -> &Void1 {
  puns::pun_ref(hs)
}

fn main() {
  let _: fn(&HS) -> &Void1 = convert;
}
