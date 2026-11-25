//! The model check of aims: every value of small types with eager and lazy aims, at a stored place
//! and at the cursor, whose oracles read the pointee in the bytes, as the documentation says, and
//! the verdicts of the pairs of these types.
//!
//! The oracles of `puns_model` take a value of a `Copy` type, and a type with an eager aim is
//! never one, nor made by value: its field is an `InPlace`. So this file has oracles of its own,
//! over the bytes of a value, which hold its pointees: those of `try_pun_ref` from bytes, and of
//! `check`. Each type is checked over every value of an alphabet of bytes against its oracle, and
//! each pair against the oracle of its target: a pun that is proven must make a valid target of
//! every valid source, and none makes a value with an eager aim, or a `&mut` to one.
//!
//! This file is not built under Miri: it checks verdicts and every value, whose unsafe paths the
//! tests of `puns` on aims (`puns/tests/aims.rs`) reach under Miri.
#![cfg(not(miri))]
// Not for Miri: verdicts and every value, whose unsafe paths `puns/tests/aims.rs` reaches.

use core::num::NonZero;

use puns::{InPlace, Pending, Pun, Thin, hook};

/// A pointer to a `bool` in the struct, at the offset that its first byte holds.
#[derive(Pun)]
#[repr(C)]
struct Hop {
  #[pun(pointer(bool = off as u128))]
  off: InPlace<u8>,
  a: u8,
  b: u8,
}

/// The same aim, at the offset that its first byte holds, plus one.
#[derive(Pun)]
#[repr(C)]
struct HopNext {
  #[pun(pointer(bool = off as u128 + 1))]
  off: InPlace<u8>,
  a: u8,
  b: u8,
}

/// The same place, checked when it is read: it requires nothing of the struct.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct HopLazy {
  #[pun(pointer(lazy bool = off as u128))]
  off: u8,
  a: u8,
  b: u8,
}

/// A region of `n` bytes at the cursor, a `bool`, checked with the struct, then the rest.
#[derive(Pun)]
#[repr(C)]
struct Flag {
  n: u8,
  #[pun(regions(eager flag: bool = n, rest))]
  tail: InPlace<[u8; 2]>,
}

/// The same, checked when it is read: it requires that the region fits.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct FlagLazy {
  n: u8,
  #[pun(regions(flag: bool = n, rest))]
  tail: [u8; 2],
}

/// A list of `bool`s behind its length at the cursor, checked with the struct.
#[derive(Pun)]
#[repr(C)]
struct Bools {
  #[pun(regions(eager list: Thin<[bool], u8>, rest))]
  tail: InPlace<[u8; 3]>,
}

/// A pointer to a list of `bool`s behind its length, in the struct, whose length the struct gives
/// with `len`.
#[derive(Pun)]
#[repr(C)]
struct Wide {
  #[pun(pointer([bool] = off as u128, len = n as u128))]
  off: InPlace<u8>,
  n: u8,
  a: u8,
  b: u8,
}

/// The pointee, a `NonZero<u8>`, at the end of the struct: a size of the rest of the tail.
#[derive(Pun)]
#[repr(C)]
struct Rest {
  #[pun(pointer([NonZero<u8>] = off as u128, size = ..))]
  off: InPlace<u8>,
  a: u8,
  b: u8,
}

/// The bytes of a value of `n` bytes, every one of the alphabet.
fn values(n: usize) -> Vec<Vec<u8>> {
  let alphabet = [0u8, 1, 2, 3, 0xff];
  let mut all = vec![vec![]];
  for _ in 0..n {
    all = all
      .into_iter()
      .flat_map(|value| alphabet.iter().map(move |byte| [value.as_slice(), &[*byte]].concat()))
      .collect();
  }
  all
}

/// Whether `b[at]` is a `bool`, `at` within the bytes.
fn boolean(b: &[u8], at: usize) -> bool {
  b.get(at).is_some_and(|byte| *byte <= 1)
}

/// The oracle of each type: whether the bytes are a valid value, with its pointees in them.
fn hop(b: &[u8]) -> bool {
  boolean(b, usize::from(b[0]))
}

fn hop_next(b: &[u8]) -> bool {
  boolean(b, usize::from(b[0]) + 1)
}

fn flag(b: &[u8]) -> bool {
  b[0] == 1 && boolean(b, 1)
}

const fn flag_lazy(b: &[u8]) -> bool {
  b[0] <= 2
}

fn bools(b: &[u8]) -> bool {
  let len = usize::from(b[0]);
  len <= 2 && (1..=len).all(|i| boolean(b, i))
}

fn wide(b: &[u8]) -> bool {
  let (at, len) = (usize::from(b[0]), usize::from(b[1]));
  at + len <= b.len() && (at..at + len).all(|i| boolean(b, i))
}

fn rest(b: &[u8]) -> bool {
  let at = usize::from(b[0]);
  at <= b.len() && b[at..].iter().all(|byte| *byte != 0)
}

/// Checks every value of `T` with `from_bytes` and `check` against `oracle`, and that `T` is
/// never made by value or reached through a `&mut` if it holds an eager aim.
fn model<T: Pun>(name: &str, oracle: fn(&[u8]) -> bool, eager: bool) {
  let mut wrong = vec![];
  for value in values(size_of::<T>()) {
    let expected = oracle(&value);
    let from = puns::from_bytes::<T>(&value).is_ok();
    let checked = puns::check::<T, 1, _>(&value, &mut [Pending::default(); 4]).is_ok();
    if from != expected || checked != expected {
      wrong.push(format!("{value:?}: from_bytes {from}, check {checked}, oracle {expected}"));
    }
  }
  assert!(wrong.is_empty(), "{name}: {wrong:#?}");
  let v = hook::verdict::<T, T>();
  if eager {
    assert!(
      !(v.pun || v.try_pun || v.pun_mut || v.try_pun_mut),
      "{name}: made by value or by &mut"
    );
  }
}

#[test]
fn every_value_of_each_type() {
  model::<Hop>("Hop", hop, true);
  model::<HopNext>("HopNext", hop_next, true);
  model::<HopLazy>("HopLazy", |_| true, false);
  model::<Flag>("Flag", flag, true);
  model::<FlagLazy>("FlagLazy", flag_lazy, false);
  model::<Bools>("Bools", bools, true);
  model::<Wide>("Wide", wide, true);
  model::<Rest>("Rest", rest, true);
}

/// Checks the pair `S` to `D`: where the pun is proven, every value that is a valid `S` by
/// `source` is a valid `D` by `target`, and where it is checked, the check agrees with `target`.
fn pair<S: Pun, D: Pun>(name: &str, source: fn(&[u8]) -> bool, target: fn(&[u8]) -> bool) {
  let v = hook::verdict::<S, D>();
  let eager = S::LAYOUT.holds_eager_aims() || D::LAYOUT.holds_eager_aims();
  assert!(!(eager && (v.pun_mut || v.try_pun_mut)), "{name}: &mut");
  let mut wrong = vec![];
  for value in values(size_of::<S>()).into_iter().filter(|value| source(value)) {
    let source = puns::from_bytes::<S>(&value).unwrap();
    let expected = target(&value);
    if hook::pun_ref::<S, D>(source).is_some() {
      if !expected {
        wrong.push(format!("{value:?}: proven, not a valid target"));
      }
    } else if let Some(result) = hook::try_pun_ref::<S, D>(source)
      && result.is_ok() != expected
    {
      wrong.push(format!("{value:?}: checked {}, oracle {expected}", result.is_ok()));
    }
  }
  assert!(wrong.is_empty(), "{name}: {wrong:#?}");
}

#[test]
fn pairs() {
  // The same aim proves itself, another place or a lazy aim does not prove an eager one, and an
  // eager aim checks what a lazy one requires.
  pair::<Hop, Hop>("Hop -> Hop", hop, hop);
  pair::<Hop, HopNext>("Hop -> HopNext", hop, hop_next);
  pair::<HopLazy, Hop>("HopLazy -> Hop", |_| true, hop);
  pair::<Hop, HopLazy>("Hop -> HopLazy", hop, |_| true);
  pair::<Flag, FlagLazy>("Flag -> FlagLazy", flag, flag_lazy);
  pair::<FlagLazy, Flag>("FlagLazy -> Flag", flag_lazy, flag);
  pair::<Bools, Bools>("Bools -> Bools", bools, bools);
  pair::<[u8; 4], Wide>("[u8; 4] -> Wide", |_| true, wide);
  assert!(hook::verdict::<Hop, Hop>().pun_ref, "the same aim proves itself");
  assert!(!hook::verdict::<HopLazy, Hop>().pun_ref, "a lazy aim proves no eager one");
}
