//! An `Option` of a scalar that a pun never makes, larger than the scalar, has no layout: `None`
//! may leave bytes uninitialized, and `Some` has a tag.
use puns::{Scalar, layout::Spec};

#[derive(Clone, Copy)]
#[repr(transparent)]
struct Wide(u32);

// SAFETY: `Wide` is `repr(transparent)` over a `u32`: 4 initialized bytes, no `UnsafeCell`.
unsafe impl Scalar for Wide {
  const SPEC: Spec = Spec::never::<Wide>();
}

fn main() {
  let _ = puns::validate_but_never::<Option<Wide>>(&[0; 8]);
}
