//! `try_view_ref` checks the value, not what it points to: behind a pointer, a `u8` is not
//! reinterpreted as a `bool`.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::Seq;

#[derive(puns::Pun)]
#[repr(C)]
struct Bytes {
  first: u8,
  rest: Seq<u8>,
}

#[derive(puns::Pun)]
#[repr(C)]
struct Flags {
  first: bool,
  rest: Seq<bool>,
}

fn check(bytes: &Bytes) -> Result<&Flags, puns::Error> {
  puns::try_view_ref(bytes)
}

fn main() {
  let _: fn(&Bytes) -> Result<&Flags, puns::Error> = check;
}
