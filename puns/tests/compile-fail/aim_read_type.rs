//! A read of a pointee names its aim by where it is in the list of the rules of the struct (the
//! hidden `First` and `After`, which the derive counts for `AimOf`), so the pointee that it reads is
//! the one of the struct, which its check found. A list item that the caller writes is not a place
//! in that list: here, the pointee of a region whose type has no identity, of the same size and
//! alignment as a `Thin` that would read 8 bytes before the region.
use core::marker::PhantomData;

use puns::{InPlace, Pun, Thin};

#[derive(Pun)]
#[repr(transparent)]
struct Next {
  #[pun(pointer(u16 = off as u128))]
  off: InPlace<i16>,
}

/// A pointee without an identity: its parameter is not bounded by `Pun`.
#[derive(Pun)]
#[repr(C)]
struct G<T> {
  a: u64,
  _p: PhantomData<T>,
}

#[derive(Pun)]
#[repr(C)]
struct U {
  a: u64,
  tail: [u64],
}

#[derive(Pun)]
#[repr(C)]
struct Regions {
  #[pun(regions(g: G<u8>, rest))]
  tail: [u8],
}

type Fake = puns::RuleAt<0, (), puns::RuleAt<1, (), (), [u8]>, Thin<U, u64>>;

fn main() {
  let bytes = [2u16, 7];
  let next = puns::from_head::<Next>(puns::as_bytes(&bytes)).unwrap().0;
  let _ = puns::aim_of::<Next, puns::RuleAt<0, puns::steps::One<0>, (), u32>>(next);
  let words = [0u64; 2];
  let regions = puns::from_bytes::<Regions>(puns::as_bytes(&words)).unwrap();
  let _ = puns::region_as::<Regions, Fake>(regions);
}
