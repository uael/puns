//! Checks the sample of the model of self-relative pointers that runs on every push.
//!
//! `puns_model::pointers` says what is checked. `pointers_full.rs` checks every pair of every
//! type.

use puns_model::pointers::{report, sound, types::*};

/// Checks that the oracle alone gives the known answer on pairs whose answer is known.
///
/// It must find the values that are not a target, in the corpora that have them.
#[test]
#[cfg_attr(miri, ignore = "a check of the oracle, which runs no unsafe code of the library")]
fn the_oracle_tells() {
  // More values, a pointer that may be null, more variants.
  assert!(sound::<Flags, Bytes>() && sound::<ToFlag, ToByte>() && sound::<ToByte, MaybeByte>());
  assert!(sound::<Tree, Wood>() && sound::<FlagList, ByteList>() && sound::<ToFlags, ToBytes>());
  // The first bytes of what a pointer aims at, and a pointer as its bytes.
  assert!(sound::<ToWord, ToByte>() && sound::<ToWord, ToPair>() && sound::<ToByte, Raw>());
  // Fewer values, a null pointer, another byte order, a larger target, another kind of target,
  // bytes as a pointer, fewer variants, text that is not.
  assert!(!sound::<Bytes, Flags>() && !sound::<ToByte, ToFlag>() && !sound::<MaybeByte, ToByte>());
  assert!(!sound::<ToByte, ToByteBe>() && !sound::<ToByte, ToWord>() && !sound::<Raw, ToByte>());
  assert!(!sound::<ToByte, ToBytes>() && !sound::<Wood, Tree>() && !sound::<ByteList, FlagList>());
  assert!(
    !sound::<ToBytes, ToText>() && !sound::<Flags, FlagList>() && !sound::<ToBytes, ToFlags>()
  );
}

/// Checks that `view_ref` and `try_view_ref` themselves compile and agree, for pairs that
/// `hook::views` accepts.
///
/// `hook::views` asks the code that decides whether the functions compile. This test calls the
/// functions.
#[test]
fn the_functions_themselves() {
  fn view<S: Model, D: Model>() {
    assert!(const { puns::hook::views::<S, D>(FUEL) });
    for region in S::corpus() {
      let view: &D = puns::view_ref(&*region);
      let tried = puns::try_view_ref::<S, D>(&region).expect("what is proven");
      assert_eq!(view.show(&region), tried.show(&region));
    }
  }
  use puns_model::pointers::{FUEL, Model};
  view::<Flags, Bytes>();
  view::<Tree, Wood>();
  view::<FlagList, ByteList>();
  // Under Miri, a recursive struct, an enum and sequences are what there is to see.
  if !cfg!(miri) {
    view::<ToFlags, ToBytes>();
    view::<ToWord, ToByte>();
    view::<ToByte, MaybeByte>();
    view::<ToText, ToText>();
    view::<ToByteBe, Raw>();
  }
}

/// Checks every pair of a selection of the types against the oracle.
///
/// The selection has a recursive struct, an enum, a pointer that may be null, and bytes. Under
/// Miri, the test checks two pairs that are checked at run time, for a pointer that may be null
/// and for a variant. Their views are then read.
#[test]
fn pairs() {
  #[cfg(not(miri))]
  puns_model::pointer_pairs!(check_pair; Flags, Bytes, ToFlag, ToByte, MaybeByte, Raw, Tree, Wood);
  #[cfg(miri)]
  {
    use puns_model::pointers::check_pair;
    check_pair::<MaybeByte, ToByte>();
    check_pair::<Wood, Tree>();
  }
  report();
}

/// Checks that `puns::validate_in`, with a visitor that follows the pointers through the buffer,
/// accepts exactly what the oracle accepts.
///
/// The images are the corpus and every copy with one byte changed, for a sample of the types
/// (`pointers_full.rs` has every type). Under Miri, the test checks a pointer that may be null,
/// with one mask, since the unsafe code that `validate_in` reaches is the same for every type.
#[test]
fn validate_in_is_the_oracle() {
  use puns_model::pointers::check_validate;
  let masks: &[u8] = if cfg!(miri) { &[0xff] } else { &[0x01, 0x80, 0xff] };
  let mut compared = check_validate::<MaybeByte>(masks);
  if !cfg!(miri) {
    compared += check_validate::<Wood>(masks);
    compared += check_validate::<Flags>(masks) + check_validate::<FlagList>(masks);
    compared += check_validate::<ToText>(masks) + check_validate::<ToByteBe>(masks);
  }
  println!("{compared} images compared");
  assert!(compared > if cfg!(miri) { 100 } else { 10_000 });
}

/// Checks that `puns::pointers_in` hands over the pointers that `validate_in` hands over.
///
/// The images are those that `validate_in` accepts, following the pointers through the buffer, for
/// a sample of the types (`pointers_full.rs` has every type). Under Miri, the test checks a pointer
/// that may be null, with one mask.
#[test]
fn pointers_in_is_validate_in() {
  use puns_model::pointers::check_pointers_in;
  let masks: &[u8] = if cfg!(miri) { &[0xff] } else { &[0x01, 0x80, 0xff] };
  let mut compared = check_pointers_in::<MaybeByte>(masks);
  if !cfg!(miri) {
    compared += check_pointers_in::<Wood>(masks) + check_pointers_in::<FlagList>(masks);
    compared += check_pointers_in::<Flags>(masks) + check_pointers_in::<ToText>(masks);
  }
  println!("{compared} images compared");
  assert!(compared > if cfg!(miri) { 20 } else { 1_000 });
}

/// Checks that `puns::copy_in` copies the bytes that the oracle says a value holds, and no other.
///
/// The images are those that `validate_in` accepts, for a sample of the types (`pointers_full.rs`
/// has every type). Under Miri, the test checks an enum whose variants hold different bytes, with
/// one mask.
#[test]
fn copy_in_is_the_oracle() {
  use puns_model::pointers::check_copy_in;
  let masks: &[u8] = if cfg!(miri) { &[0xff] } else { &[0x01, 0x80, 0xff] };
  let mut compared = check_copy_in::<Tree>(masks);
  if !cfg!(miri) {
    compared += check_copy_in::<MaybeByte>(masks) + check_copy_in::<Wood>(masks);
    compared += check_copy_in::<Flags>(masks) + check_copy_in::<ToByteBe>(masks);
  }
  println!("{compared} images compared");
  assert!(compared > if cfg!(miri) { 20 } else { 1_000 });
}
