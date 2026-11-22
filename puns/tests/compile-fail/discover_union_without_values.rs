// An enum of the `Rust` representation with a variant that holds a union none of whose fields has
// values cannot be discovered: a value of that variant may exist, but no sample of it can be made
// to find its tag.
use core::convert::Infallible;

/// A union none of whose fields has values. Unsafe code can make one under the rule for unions
/// that lets a union hold any bytes.
#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
union Hollow {
  a: Infallible,
}

#[derive(puns::Pun, Clone, Copy)]
enum R3 {
  A(Hollow),
  B(u8),
  C,
}

fn main() {
  let _ = puns::pun_ref::<R3, R3>(&R3::C);
}
