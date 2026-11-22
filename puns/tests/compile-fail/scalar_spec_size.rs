//! The `SPEC` of a scalar has the size of its type: here one byte of a type of two.
use puns::layout::Spec;

#[derive(Clone, Copy)]
#[repr(transparent)]
struct Wide(u16);

// SAFETY: `Wide` is `repr(transparent)` over a `u16`, so any initialized bytes are a valid `Wide`,
// and it holds no `UnsafeCell`. The `Spec` is of one byte, which `Layout::scalar` refuses.
unsafe impl puns::Scalar for Wide {
  const SPEC: Spec = Spec::any::<u8>();
}

fn main() {
  let _: Wide = puns::pun(0u16);
}
