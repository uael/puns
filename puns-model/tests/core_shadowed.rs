//! Tests that the code that `#[derive(Pun)]` generates does not depend on what `::core` names.
//!
//! In this crate, `::core` is the crate `fakecore`, through `extern crate fakecore as core;`, which
//! is safe code. Its `offset_of!` swaps the fields named `a` and `b`, and its `PhantomData` is
//! aligned to 16 bytes.

extern crate fakecore as core;

use puns::{Error, Pun};

/// A struct with the default representation, whose fields `a` and `b` the fake `offset_of!` swaps.
///
/// Which byte is `b` is a layout that Rust leaves to the compiler, so the struct needs the
/// `unstable` feature.
#[cfg(feature = "unstable")]
#[derive(Pun, Debug)]
struct Pair {
  a: u8,
  b: bool,
}

/// An enum with one variant, aligned to 16 bytes like the fake `PhantomData`.
///
/// The derive must not measure the layout of a variant on a struct that holds a `PhantomData`
/// named through `::core`.
#[derive(Pun, Debug, Clone, Copy, PartialEq)]
#[repr(C, align(16))]
enum Aligned {
  A(bool),
}

/// Checks that the offsets of the fields of `Pair` are not read from `::core::mem::offset_of!`.
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

/// Checks that the variant of `Aligned` is not measured with `::core::marker::PhantomData`.
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
