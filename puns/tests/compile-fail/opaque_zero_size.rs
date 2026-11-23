//! An opaque scalar of size 0: a pun relates bytes, so it would make one from no bytes at all, as
//! `pun::<(), Token>(())` would.
#[derive(Clone, Copy)]
struct Token(());

// SAFETY: `Token` has no byte, and no `UnsafeCell`. `Spec::opaque` refuses it.
unsafe impl puns::Scalar for Token {
  const SPEC: puns::layout::Spec = puns::layout::Spec::any::<Token>().opaque();
}

fn main() {
  let _: Token = puns::pun(());
}
