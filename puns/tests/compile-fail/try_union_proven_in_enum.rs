//! A union that its second field proves, since that field takes padding, in a variant of an enum
//! that is checked as a whole. The check of the union tries its first field first, which would
//! read a byte that may be uninitialized in the source.
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
enum Target {
  A(Maybe, bool),
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Source {
  tag: u8,
  anything: MaybeUninit<u8>,
  byte: u8,
}

fn main() {
  let source = Source { tag: 0, anything: MaybeUninit::uninit(), byte: 1 };
  let _ = puns::try_pun::<Source, Target>(source);
}
