//! An `Option` of a scalar that a pun never makes, of the size of the scalar, has no layout if
//! `None` leaves a byte uninitialized: here the second byte, beside the niche of the first.
use core::num::NonZero;

use puns::{Scalar, layout::Spec};

#[derive(Clone, Copy)]
#[repr(C)]
struct Gap(NonZero<u8>, u8);

// SAFETY: `Gap` is two initialized bytes, without padding or `UnsafeCell`.
unsafe impl Scalar for Gap {
  const SPEC: Spec = Spec::never::<Gap>();
}

fn main() {
  let _ = puns::validate_but_never::<Option<Gap>>(&[1, 0]);
}
