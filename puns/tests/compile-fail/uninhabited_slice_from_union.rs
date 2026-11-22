// A slice of elements without values is refused from a source that may give it elements, whatever
// the source is known to hold. `HS2` holds a union none of whose fields has values, which no value
// of can be made, but which may hold any bytes under one of the rules for unions.
use core::convert::Infallible;

#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
union Hollow {
  a: Infallible,
}

#[derive(puns::Pun)]
#[repr(C)]
struct HS2 {
  u: Hollow,
  tail: [u8],
}

#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
struct Void1 {
  n: Infallible,
  x: u8,
}

fn convert(hs2: &HS2) -> &[Void1] {
  puns::pun_ref(hs2)
}

fn main() {
  let _: fn(&HS2) -> &[Void1] = convert;
}
