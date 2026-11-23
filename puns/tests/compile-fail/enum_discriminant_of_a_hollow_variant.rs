//! A variant that holds a union none of whose fields has values has no sample, but a value of it
//! may exist in a source, so its tag must be known. A discriminant that a macro writes is not, and
//! the enum is refused. With a literal or a constant, it compiles (`Source5` in `tests/uninit.rs`).
use core::convert::Infallible;

#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
union Hollow {
  a: Infallible,
}

macro_rules! two {
  () => {
    2
  };
}

#[derive(puns::Pun, Clone, Copy)]
#[repr(u8)]
#[allow(dead_code)]
enum E {
  A(Hollow) = two!(),
  B(u8),
}

fn main() {
  let _ = puns::try_pun::<[u8; 2], E>([3, 7]);
}
