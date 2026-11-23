//! `zeroed` makes no opaque scalar, not even one of which zero is a value, nor in a `Deferred`.
use puns::{Deferred, Scalar, layout::Spec};

#[derive(Clone, Copy)]
#[repr(transparent)]
struct Handle(u32);

// SAFETY: `Handle` is `repr(transparent)` over a `u32`: 4 initialized bytes, each pattern a valid
// `Handle`, no `UnsafeCell`.
unsafe impl Scalar for Handle {
  const SPEC: Spec = Spec::any::<Handle>().opaque();
}

fn main() {
  let _: Handle = puns::zeroed();
  let _: Deferred<Handle> = puns::zeroed();
}
