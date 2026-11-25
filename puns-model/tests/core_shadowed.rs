//! Tests that the code that `#[derive(Pun)]` generates does not depend on what `::core` names.
//!
//! In this crate, `::core` is the crate `fakecore`, through `extern crate fakecore as core;`, which
//! is safe code. Its `offset_of!` swaps the fields named `a` and `b`, its `PhantomData` is aligned
//! to 16 bytes, and its `concat!`, `stringify!`, `module_path!`, `file!`, `line!` and `column!` give
//! one value wherever they are.

extern crate fakecore as core;

use puns::{Error, InPlace, Pun, hook};

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
  assert!(matches!(puns::try_pun_ref::<[u8; 2], Pair>(&bytes), Err(Error::Validity { .. })));
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
  assert!(matches!(puns::try_pun::<[u8; 16], Aligned>(bytes), Err(Error::Validity { .. })));
}

/// A byte that may be anything.
#[derive(Pun)]
#[repr(C)]
struct Lax {
  value: u8,
}

/// A byte that is 0 or 1, of the size and the alignment of `Lax`.
#[derive(Pun)]
#[repr(C)]
struct Strict {
  #[pun(valid(0..=1))]
  value: u8,
}

/// A `Lax` at the offset that the first byte holds, in the struct, which only a shared pointee may
/// be.
#[derive(Pun)]
#[repr(C)]
struct HoldsLax {
  #[pun(pointer(shared Lax = off as u128))]
  off: InPlace<u8>,
  value: u8,
}

/// The same aim, at a `Strict`.
#[derive(Pun)]
#[repr(C)]
struct HoldsStrict {
  #[pun(pointer(shared Strict = off as u128))]
  off: InPlace<u8>,
  value: u8,
}

/// Checks that the identity of a type is not made of what `::core` names: were it made of the
/// macros of `fakecore`, `Lax` and `Strict` would have one, the eager aim of `HoldsLax` would prove
/// the one of `HoldsStrict`, and a `HoldsStrict` would point at a `Strict` that holds 2.
///
/// It guards against a change of the derive. It passes on the derive that named a type through
/// `__site!` too, whose macros were those of the real `core`, through `puns`.
#[test]
fn identities_are_not_made_with_core() {
  assert!(!hook::verdict::<HoldsLax, HoldsStrict>().pun_ref);
  assert!(hook::verdict::<HoldsLax, HoldsLax>().pun_ref);
  let bytes = [1, 2];
  let lax = puns::from_bytes::<HoldsLax>(&bytes).unwrap();
  assert!(hook::pun_ref::<HoldsLax, HoldsStrict>(lax).is_none());
  assert!(matches!(puns::from_bytes::<HoldsStrict>(&bytes), Err(Error::Validity { .. })));
}
