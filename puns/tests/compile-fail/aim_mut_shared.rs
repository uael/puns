//! The pointee of a shared aim is not written: other aims may reach it, and the check claims none
//! of its bytes.
use puns::{InPlace, Pun};

#[derive(Pun)]
#[repr(transparent)]
struct Next {
  #[pun(pointer(shared next: u8 = off as u128))]
  off: InPlace<u8>,
}

fn main() {
  let mut bytes = [1, 9];
  let holder = bytes.as_mut_ptr().cast::<Next>();
  // SAFETY: what `aim_mut` requires, but that the aim is shared, which it refuses.
  let _ = unsafe { puns::aim_mut(holder, Next::AIMS.next) };
}
