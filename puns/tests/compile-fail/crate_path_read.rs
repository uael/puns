//! `#[pun(crate = ..)]` cannot make the safe read of a pointee call an `unsafe` function: the method
//! that the derive writes calls the read through the path, with no `unsafe` block, so a path that
//! names `core::mem::transmute_copy` there, which would take the offset for an address, does not
//! compile, though every type fits. The struct and its `InPlace` are the ones of `puns`.
mod fake {
  pub use core::mem::transmute_copy as aim_of;
  pub use puns::*;

  /// The item of the read: the type that `aim_of` returns.
  pub type First = &'static u64;

  /// A trait of the name of the one of `puns`, whose `Value` is the type of the method.
  pub trait Pointee {
    type Value: ?Sized;
  }

  impl Pointee for u64 {
    type Value = u64;
  }
}

#[derive(puns::Pun)]
#[pun(crate = fake)]
#[repr(transparent)]
struct Evil {
  #[pun(pointer(pointee: u64 = off as u128))]
  off: puns::InPlace<i64>,
}

fn main() {
  // A valid pointer: its pointee is the `u64` 8 bytes after it.
  let words = [8u64, 7];
  let (evil, _) = puns::from_head::<Evil>(puns::as_bytes(&words)).unwrap();
  let _ = evil.pointee();
}
