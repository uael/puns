//! An `Option` of an opaque scalar, larger than the scalar, has no layout: `None` may leave bytes
//! uninitialized, and `Some` has a tag.
use puns::{Deferred, Scalar, layout::Spec};

#[derive(Clone, Copy)]
#[repr(transparent)]
struct Wide(u32);

// SAFETY: `Wide` is `repr(transparent)` over a `u32`: 4 initialized bytes, each pattern a valid
// `Wide`, no `UnsafeCell`.
unsafe impl Scalar for Wide {
  const SPEC: Spec = Spec::any::<Wide>().opaque();
}

fn main() {
  let _ = puns::from_bytes::<Deferred<Option<Wide>>>(&[0; 8]);
}
