//! An enum that the same enum proves, in an array whose other element is checked. The elements
//! share their check, which would check the rule of `Padded` in the first one too, and that rule
//! reads the padding of `Padded`.
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(be(0, 4) != 0))]
struct Padded {
  a: u8,
  b: u16,
}

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Holds {
  A(Padded),
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Source {
  first: Holds,
  second: [u16; 3],
}

fn main() {
  let source = Source { first: Holds::A(Padded { a: 1, b: 2 }), second: [0, 1, 1] };
  let _ = puns::try_pun_ref::<Source, [Holds; 2]>(&source);
}
