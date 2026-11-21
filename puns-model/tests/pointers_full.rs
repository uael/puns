//! Checks every pair of every type of the model of self-relative pointers.
//!
//! This file needs the `full` feature, which `cargo test` does not turn on. It runs with
//! `cargo test --profile model -p puns-model --features full`, every night and on demand
//! (`.github/workflows/model.yml`).
//!
//! This file is not built under Miri, which runs the sample of `pointers.rs` instead.

#![cfg(feature = "full")]
#![cfg(not(miri))]

use puns_model::pointers::{report, types::*};

/// Checks every ordered pair of the types against the oracle.
#[test]
fn every_pair() {
  puns_model::pointer_pairs!(check_pair;
    Flags, Bytes, FlagList, ByteList, ToFlag, ToByte, MaybeByte, ToByteBe, ToWord, ToPair, ToBytes,
    ToFlags, ToText, Raw, Tree, Wood
  );
  report();
}

/// Checks `puns::validate_in` against the oracle on every type of the model.
///
/// The images include every change of one byte by every mask of one bit, and by the mask of all
/// bits.
#[test]
fn validate_in_is_the_oracle() {
  use puns_model::pointers::check_validate;
  let masks = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0xff];
  let compared = check_validate::<Flags>(&masks)
    + check_validate::<Bytes>(&masks)
    + check_validate::<FlagList>(&masks)
    + check_validate::<ByteList>(&masks)
    + check_validate::<ToFlag>(&masks)
    + check_validate::<ToByte>(&masks)
    + check_validate::<MaybeByte>(&masks)
    + check_validate::<ToByteBe>(&masks)
    + check_validate::<ToWord>(&masks)
    + check_validate::<ToPair>(&masks)
    + check_validate::<ToBytes>(&masks)
    + check_validate::<ToFlags>(&masks)
    + check_validate::<ToText>(&masks)
    + check_validate::<Raw>(&masks)
    + check_validate::<Tree>(&masks)
    + check_validate::<Wood>(&masks);
  println!("{compared} images compared");
}

/// Checks `puns::pointers_in` against `validate_in` on every type of the model.
///
/// The images include every change of one byte by every mask of one bit, and by the mask of all
/// bits, that leaves the image valid.
#[test]
fn pointers_in_is_validate_in() {
  use puns_model::pointers::check_pointers_in;
  let masks = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0xff];
  let compared = check_pointers_in::<Flags>(&masks)
    + check_pointers_in::<Bytes>(&masks)
    + check_pointers_in::<FlagList>(&masks)
    + check_pointers_in::<ByteList>(&masks)
    + check_pointers_in::<ToFlag>(&masks)
    + check_pointers_in::<ToByte>(&masks)
    + check_pointers_in::<MaybeByte>(&masks)
    + check_pointers_in::<ToByteBe>(&masks)
    + check_pointers_in::<ToWord>(&masks)
    + check_pointers_in::<ToPair>(&masks)
    + check_pointers_in::<ToBytes>(&masks)
    + check_pointers_in::<ToFlags>(&masks)
    + check_pointers_in::<ToText>(&masks)
    + check_pointers_in::<Raw>(&masks)
    + check_pointers_in::<Tree>(&masks)
    + check_pointers_in::<Wood>(&masks);
  println!("{compared} images compared");
}

/// Checks `puns::copy_in` against the bytes that the oracle says a value holds, on every type of
/// the model.
///
/// The images include every change of one byte by every mask of one bit, and by the mask of all
/// bits, that leaves the image valid.
#[test]
fn copy_in_is_the_oracle() {
  use puns_model::pointers::check_copy_in;
  let masks = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0xff];
  let compared = check_copy_in::<Flags>(&masks)
    + check_copy_in::<Bytes>(&masks)
    + check_copy_in::<FlagList>(&masks)
    + check_copy_in::<ByteList>(&masks)
    + check_copy_in::<ToFlag>(&masks)
    + check_copy_in::<ToByte>(&masks)
    + check_copy_in::<MaybeByte>(&masks)
    + check_copy_in::<ToByteBe>(&masks)
    + check_copy_in::<ToWord>(&masks)
    + check_copy_in::<ToPair>(&masks)
    + check_copy_in::<ToBytes>(&masks)
    + check_copy_in::<ToFlags>(&masks)
    + check_copy_in::<ToText>(&masks)
    + check_copy_in::<Raw>(&masks)
    + check_copy_in::<Tree>(&masks)
    + check_copy_in::<Wood>(&masks);
  println!("{compared} images compared");
}
