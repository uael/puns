//! A string type whose automaton does not accept the empty string: bytes, at least one.

use puns::{
  Pun,
  layout::{Dfa, DfaState, Layout},
};

/// At least one byte, of any value.
pub const NON_EMPTY: &Dfa = &Dfa {
  id: u32::from_be_bytes(*b"nonE"),
  states: &[
    DfaState { accept: false, edges: &[(0, 255, 1)] },
    DfaState { accept: true, edges: &[(0, 255, 1)] },
  ],
};

/// A type that the user defines, whose values are the bytes that `NON_EMPTY` accepts.
#[repr(transparent)]
pub struct NonEmpty(pub [u8]);

// SAFETY: `Pun` is not to be implemented by hand, and the tests do so to check a string type whose
// automaton does not accept the empty string. `NonEmpty` is `repr(transparent)` over `[u8]`, and
// the tests make a `NonEmpty` only through `puns`. So the layout of `Layout::pattern` with
// `NON_EMPTY`, a tail of bytes at offset 0 that the automaton accepts, is its layout. The lists are
// `()`, and it holds no `UnsafeCell`.
unsafe impl Pun for NonEmpty {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Rules = ();
  type Metadata = usize;
  const PROBE: *const Self = <[u8] as Pun>::PROBE as *const Self;
  // SAFETY: `NonEmpty` is a struct whose last field is a slice. `Self::PROBE` is
  // `<[u8] as Pun>::PROBE` cast with an `as` cast, and its 3 bytes fit in an `isize`. These are the
  // requirements of `Layout::pattern`.
  const LAYOUT: Layout = unsafe { Layout::pattern::<Self>(Self::PROBE, NON_EMPTY) };
}

/// A kind, then a body of at least one byte.
#[derive(Pun)]
#[repr(C)]
pub struct Msg {
  pub kind: u8,
  pub body: NonEmpty,
}
