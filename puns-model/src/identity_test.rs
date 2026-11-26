//! Tests that a type of the library and a type of an executable of the same package, declared by
//! the same tokens of the same file, are two types (`puns_model::identity`).
//!
//! This test is that executable: its name is the one of the library, and it is in `src/`, so that
//! it includes `identity.rs` at the path where the library does.

use puns::{Error, hook};

/// The most that the `Bounded` of this executable allows.
const BOUNDED_MAX: u8 = 1;

#[path = "identity.rs"]
mod identity;

/// Checks that the eager aim of `Holds` of the library does not prove the one of this executable,
/// whose pointee allows less, and that each proves itself.
#[test]
fn a_type_of_the_library_is_not_one_of_the_executable() {
  use puns_model::identity::Holds as Lax;

  use crate::identity::Holds as Strict;
  assert!(!hook::verdict::<Lax, Strict>().pun_ref);
  assert!(hook::verdict::<Lax, Lax>().pun_ref);
  assert!(hook::verdict::<Strict, Strict>().pun_ref);
  // The pointee is the second byte.
  let bytes = [1, 2];
  let lax = puns::from_bytes::<Lax>(&bytes).unwrap();
  assert!(matches!(puns::from_bytes::<Strict>(&bytes), Err(Error::Validity { .. })));
  assert!(hook::pun_ref::<Lax, Strict>(lax).is_none());
  assert!(puns::from_bytes::<Strict>(&[1, 1]).is_ok());
}
