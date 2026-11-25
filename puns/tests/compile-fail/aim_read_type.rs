//! The read of the pointee of an aim names the type of the pointee, which must be the one of the
//! aim: the derive names it, and a direct call is checked.
use puns::{InPlace, Pun};

#[derive(Pun)]
#[repr(transparent)]
struct Next {
  #[pun(pointer(u16 = off as u128))]
  off: InPlace<i16>,
}

fn main() {
  let bytes = [2u16, 7];
  let next = puns::from_head::<Next>(puns::as_bytes(&bytes)).unwrap().0;
  // SAFETY: Not called: the instance does not compile.
  let _ = unsafe {
    puns::aim::<Next, puns::RuleAt<0, puns::steps::One<0>, (), u32>>(next)
  };
}
