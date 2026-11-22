//! An enum that the same enum proves, in an array whose other element is checked. The elements
//! share their check, which would check the union that the first one holds, of which nothing is
//! known.
use core::mem::MaybeUninit;

use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Maybe {
  flag: bool,
  anything: MaybeUninit<u8>,
}

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Holds {
  A(Maybe),
  B(bool),
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Source {
  first: Holds,
  second: [u8; 2],
}

fn main() {
  let source = Source { first: Holds::A(Maybe { anything: MaybeUninit::uninit() }), second: [1, 1] };
  let _ = puns::try_pun_ref::<Source, [Holds; 2]>(&source);
}
