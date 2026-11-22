// A variant of a source that holds a union none of whose fields has values may exist, and leaves
// the byte after its tag uninitialized, which the target requires.
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
enum Source6 {
  A(Hollow) = 0,
  B(u8) = 1,
}

fn main() {
  let _ = puns::pun::<Source6, [u8; 2]>(Source6::B(7));
}
