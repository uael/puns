//! `#[derive(Pun)]` in a crate where `::core` is another crate (`extern crate .. as core;`, safe
//! code): what it generates must not depend on what that crate says.

extern crate fakecore as core;

use puns::{Error, Pun};

/// The default representation: the offsets of the fields were `offset_of!`. Which byte is `b` is
/// the layout that `rustc` chose: the `unstable` feature.
#[cfg(feature = "unstable")]
#[derive(Pun, Debug)]
struct Pair {
  a: u8,
  b: bool,
}

/// The layout of a variant was measured on a struct that held a `PhantomData` (not found
/// exploitable: the size of the enum is checked, but nothing relies on it any more).
#[derive(Pun, Debug, Clone, Copy, PartialEq)]
#[repr(C, align(16))]
enum Aligned {
  A(bool),
}

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
