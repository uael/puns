//! A scalar declared by hand whose `Spec` declares falsely that its bytes are a `NonZero<u8>`: they
//! are a `u8`, so `rustc` gives its `Option` a tag, not the niche that `puns` predicts. The
//! prediction is compared with the layout found by experiments, and the build fails.
use core::num::NonZero;

use puns::{Scalar, layout::Spec};

/// A byte, any byte.
#[derive(Clone, Copy)]
#[repr(transparent)]
struct Plain(u8);

// SAFETY: `Plain` is `repr(transparent)` over a `u8`, so it has its size, and every byte is a
// valid `Plain`, which `SPEC` accepts. It holds no `UnsafeCell`. `stored_as` is false here, which
// the `# Safety` of `Scalar` does not forbid.
unsafe impl Scalar for Plain {
  const SPEC: Spec = Spec::any::<Plain>().stored_as::<NonZero<u8>>();
}

fn main() {
  let _ = puns::try_pun::<[u8; 2], Option<Plain>>([0; 2]);
}
