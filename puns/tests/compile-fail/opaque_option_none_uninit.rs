//! An `Option` of an opaque scalar, of the size of the scalar, has no layout if `None` leaves a
//! byte uninitialized: here the second byte, beside the niche of the first.
use core::num::NonZero;

use puns::{
  Deferred, Scalar,
  layout::{Endian, Spec},
};

#[derive(Clone, Copy)]
#[repr(C)]
struct Gap(NonZero<u8>, u8);

// SAFETY: `Gap` is two initialized bytes, without padding or `UnsafeCell`. Read big-endian, its
// values are those whose first byte is not zero, 256 and above, each a valid `Gap`.
unsafe impl Scalar for Gap {
  const SPEC: Spec = Spec::ranges::<Gap>(Endian::Big, &[(256, 0xffff)]).opaque();
}

fn main() {
  let _ = puns::from_bytes::<Deferred<Option<Gap>>>(&[1, 0]);
}
