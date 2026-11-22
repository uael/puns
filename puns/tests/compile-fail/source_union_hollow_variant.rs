// A variant of a source that holds a union none of whose fields has values may exist, since a
// union may hold any bytes. Its tag is none of the target, so the pun is not proven.
use core::convert::Infallible;

/// A union none of whose fields has values. Unsafe code can make one under the rule for unions
/// that lets a union hold any bytes.
#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
union Hollow {
  a: Infallible,
}

#[derive(puns::Pun, Clone, Copy)]
#[repr(u8)]
enum Source5 {
  A(Hollow) = 0,
  B(u8) = 1,
  C = 2,
}

#[derive(puns::Pun, Clone, Copy)]
#[repr(u8)]
enum Target5 {
  B(u8) = 1,
  C = 2,
}

fn main() {
  let _ = puns::pun::<Source5, Target5>(Source5::C);
}
