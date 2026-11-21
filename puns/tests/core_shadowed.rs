//! Tests that what `#[derive(Pun)]` generates does not depend on what `::core` names.
//!
//! Safe code can make `::core` name another crate, with `extern crate .. as core;`. Here `::core`
//! names this crate. This `core` is the real one, except for two items:
//!
//! - `mem::offset_of!` swaps the fields named `a` and `b`.
//! - `marker::PhantomData` is aligned to 16 bytes.

extern crate self as core;

pub use std::{option, primitive, ptr, result};

use puns::{Error, Pun};

pub mod mem {
  #[expect(
    unused_macros,
    reason = "what the derive used to read the offsets from: it no longer does"
  )]
  macro_rules! offset_of {
    (Self, a) => {
      ::std::mem::offset_of!(Self, b)
    };
    (Self, b) => {
      ::std::mem::offset_of!(Self, a)
    };
    ($($t:tt)*) => {
      ::std::mem::offset_of!($($t)*)
    };
  }

  #[expect(
    unused_imports,
    reason = "what the derive used to read the offsets from: it no longer does"
  )]
  pub(crate) use offset_of;
}

pub mod marker {
  pub use std::marker::Sized;

  #[repr(align(16))]
  pub struct PhantomData<T: ?Sized>(std::marker::PhantomData<T>);
}

/// A struct of the `Rust` representation. The derive must not read the offsets of its fields with
/// `::core::mem::offset_of!`, which swaps them here.
#[cfg(feature = "unstable")]
#[derive(Pun, Debug)]
struct Pair {
  a: u8,
  b: bool,
}

/// An enum with one variant. The derive must not measure the layout of the variant on a struct
/// that holds a `::core::marker::PhantomData`, which is aligned to 16 bytes here.
#[derive(Pun, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C, align(16))]
enum Aligned {
  A(bool),
}

// `Pair` is of the `Rust` representation: which byte is its `bool` is the `unstable` feature.
#[cfg(feature = "unstable")]
#[test]
fn offsets_are_not_read_from_core() {
  let mut bytes = [7u8; 2];
  bytes[std::mem::offset_of!(Pair, b)] = 1;
  // `a` is 7, which is fine; `b` is 1.
  assert!(puns::try_pun_ref::<[u8; 2], Pair>(&bytes).unwrap().b);
  let mut bytes = [1u8; 2];
  bytes[std::mem::offset_of!(Pair, b)] = 7;
  assert_eq!(puns::try_pun_ref::<[u8; 2], Pair>(&bytes).err(), Some(Error::Validity));
}

#[test]
fn variants_are_not_measured_with_core() {
  // The tag (a C `int`), then the `bool`, at 4.
  let mut bytes = [0u8; 16];
  bytes[4] = 1;
  assert_eq!(puns::try_pun::<[u8; 16], Aligned>(bytes), Ok(Aligned::A(true)));
  bytes[4] = 7;
  bytes[8] = 1;
  assert_eq!(puns::try_pun::<[u8; 16], Aligned>(bytes), Err(Error::Validity));
}
