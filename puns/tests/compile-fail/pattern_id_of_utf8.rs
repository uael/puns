//! An automaton of the user with the `id` of UTF-8 (a field that safe code writes) is not UTF-8:
//! two patterns are the same by their states, and bytes that any bytes are, are not a `str`.
use puns::{
  Pun,
  layout::{Dfa, DfaState, Layout},
};

/// Any bytes, with the `id` of UTF-8.
const ANY: &Dfa =
  &Dfa { id: Dfa::UTF8.id, states: &[DfaState { accept: true, edges: &[(0, 0xff, 0)] }] };

#[repr(transparent)]
struct Lax([u8]);

// SAFETY: `Layout::pattern` is a tail of bytes at offset 0, with alignment 1, whose values are the
// bytes that `ANY` accepts, which are any bytes. `Lax` is `repr(transparent)` over `[u8]`, so it
// has that layout, and any bytes are a valid `Lax`. The lists are `()`, as for `str`, and a `Lax`
// holds no `UnsafeCell`. A test implements `Pun` by hand, which the `# Safety` of `Pun` reserves
// to `puns`, to check what `puns` does with an automaton that safe code made.
unsafe impl Pun for Lax {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Rules = ();
  type Metadata = usize;
  const PROBE: *const Self = <[u8] as Pun>::PROBE as *const Self;
  // SAFETY: `Lax` ends with a slice, and its probe is `<[u8] as Pun>::PROBE`, of 3 bytes.
  const LAYOUT: Layout = unsafe { Layout::pattern::<Self>(Self::PROBE, ANY) };
}

fn main() {
  let lax: &Lax = puns::from_bytes(&[0xff]).unwrap();
  let _: &str = puns::pun_ref(lax);
}
