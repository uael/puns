//! Two types that the library and the test `puns_model` both declare, from this file: the test of
//! the identity of a type (`Pun::ID`) across the crates of one package.
//!
//! The tokens are the same in both crates, but `crate::BOUNDED_MAX` is not, so neither is
//! `Bounded`: the one of the library allows every byte, the one of the test only 0 and 1. The
//! test, an executable, has the name of the library, as Cargo names the executable of a package,
//! and includes this file at the same path, in a module of the same path, after as many derives.
//! Only the crate as `rustc` tells it apart tells the two `Bounded` apart, and a proof that took
//! them for one type would make a `Bounded` of the test that holds 2.

use puns::{InPlace, Pun};

/// A byte of at most `crate::BOUNDED_MAX`, which each crate that includes this file declares.
#[derive(Pun)]
#[repr(C)]
pub struct Bounded {
  #[pun(valid(0..=crate::BOUNDED_MAX))]
  pub value: u8,
}

/// A `Bounded` at the offset that its first byte holds, in the struct, which only a shared pointee
/// may be.
#[derive(Pun)]
#[repr(C)]
pub struct Holds {
  #[pun(pointer(shared Bounded = off as u128))]
  pub off: InPlace<u8>,
  pub value: u8,
}
