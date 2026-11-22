//! A model check of the analysis of `puns`, against oracles that do not use it.
//!
//! For each type of the model, [`Oracle::check`] decides whether some bytes are a [valid] value of
//! that type. Each oracle is written from what the language and the documentation of the type say.
//! It takes the offsets of the fields from the compiler, through `offset_of!` or through the
//! addresses of the fields of a value. An uninitialized byte is `None`.
//!
//! [`values`] enumerates the valid values of a type over an alphabet of bytes ([`alphabet`]).
//! [`check_pair`] compares every verdict of the library on a pair of types with what these values
//! imply.
//!
//! [`check_part`] checks the parts of a value in the same way, for every target that is not larger
//! than the source:
//!
//! - The first bytes (`head`, `head_ref`) and the last bytes (`tail`, `tail_ref`) are checked
//!   against the oracle of the target at their offset.
//! - The bytes and the address that these functions return at run time are compared with those of
//!   the source.
//! - A `&mut` to a part (`head_mut`, `tail_mut`) is checked by writing every value of the target
//!   into every value of the source. The result must be a valid source. If the source met its
//!   requirements and the value written meets those of the target, the result must still meet the
//!   requirements of the source.
//!
//! # Requirements
//!
//! A value that safe code holds need not meet what `#[pun(valid(..))]` or a rule requires, because
//! safe code can write any value of its type to a field. The oracles therefore have two modes
//! ([`Mode::required`]): without it, an oracle decides only what the type system guarantees, and
//! with it, the requirements count too.
//!
//! # Unions
//!
//! The language has not settled the validity of unions, and `puns` must be sound under every rule.
//! An oracle decides under either end ([`Unions`]): a union is one of its fields, or any bytes,
//! initialized or not, as Miri implements. The values of a source are its values under either
//! rule. A pun must make a valid target of each value under the first rule under which that value
//! is a source ([`rule_of`]): a value that is a source only if a union may hold any bytes is judged
//! with the unions of the target holding any bytes too. A check must accept a value that is a valid
//! target with every union one of its fields, since that is how `puns` checks a union. So a union
//! without values may hold a value in a source, and a union of the source is read as nothing, as
//! the documentation of `puns` says, while the same union of the target accepts it.
//!
//! What a check reads is not an oracle's: an oracle decides validity. The values that the model
//! makes leave uninitialized every byte that may be (padding, `MaybeUninit`, the bytes of a union
//! that may hold anything, a field of a union of size 0), and under Miri the checks run on them, so
//! a check that reads such a byte is reported there. Natively, only a decision that needs such a
//! byte is found unsound.
//!
//! The library may leave a requirement of the target unchecked only if the requirements of the
//! source imply it. A source that meets its own requirements then meets it, and the library
//! promises nothing of a source that does not. The model finds these implications by itself, one
//! requirement at a time ([`require`]). A requirement of the target is implied if every value of
//! the source that meets the requirements of the source meets it.
//!
//! The library proves such an implication only from the same rules of the source, in a struct of
//! the same extent at the same place. The model does not use that test. It reports every
//! requirement that the library leaves unchecked and that the requirements of the source do not
//! imply.
//!
//! # Verdicts
//!
//! The model goes through the test-only `verdict` feature of `puns` (`puns::hook`), which says
//! what each function would do without failing to compile. It runs without the `unstable` feature
//! of `puns` and with it, through a feature `unstable` of its own. Without it, a refusal for what
//! Rust does not specify counts as imprecise, by design (`hook::Verdict::unspecified`). The model
//! reports three kinds of verdicts:
//!
//! - A verdict is **unsound** if it lets through a value that the type system does not allow, or
//!   reads a byte of padding.
//! - A verdict is **incorrect** if it rejects a valid value at run time, or leaves unchecked a
//!   requirement of the target that the source does not imply.
//! - A verdict is **imprecise** otherwise, where it refuses or checks what it could prove. These
//!   are counted.
//!
//! A defect of `puns` that is known and not fixed yet is listed in [`KNOWN`], with its pair and the
//! finding that names it. Its unsound or incorrect verdicts are printed as known and not counted,
//! and a listed pair that is checked and no longer reported fails the check, so the entry goes
//! with the fix. A test that reaches a known read of an uninitialized byte is ignored under Miri,
//! which stops at it, with the finding as the reason.
//!
//! # Types
//!
//! The model has about 150 types of 1 to 8 bytes in the full run ([`types`]). It checks every
//! ordered pair of types of the same size, and every value of the source over an alphabet, which is
//! every value up to 2 bytes. The types of the shapes below are not in that product, which would
//! grow by half: each is paired with the sources and the controls that matter for it, in
//! `tests/shapes.rs`. It checks slices and structs that end with a slice on every pattern of up to 6
//! bytes, among them a slice of elements without values, a struct that ends with a struct without
//! values, and a string type whose automaton refuses the empty string, alone and after a field.
//!
//! Nothing of the test of the library (the same rules, the same extent) is in the oracle. So a rule
//! of the target wrongly taken as proven by another rule is reported. The types include the same
//! rules over other fields, another rule, and another extent. They put these rules in a struct, an
//! enum of a primitive representation and of the `Rust` representation, an array, a union, and a
//! struct with a byte that may be uninitialized. The model reports a rule proven by another (a
//! `same_rules` that compares nothing) and a `&mut` part over a rule that the target does not have
//! (a `cut` without its test). Both faults were tried.
//!
//! The types include enums of the `Rust` representation: a niche in a `bool`, in a `NonZero` and in
//! a `char`, a niche shared by two variants next to a field of a third, a direct tag of one byte
//! and one that `rustc` widened, one variant, and a variant without values that `rustc` gives a
//! tag. Their oracles are written from how `rustc` lays out enums. They are checked against the
//! bytes of values natively, and by Miri on the values that the oracle makes. The offsets of the
//! fields are taken from values, never from the library. With the `unstable` feature, a niche in
//! an array is in the model, in an `Option` and in a derived enum. The types also include unions,
//! `MaybeUninit`, and a scalar that may be uninitialized (`Zero`). Against `Zero`, reading such a
//! scalar as initialized is found unsound.
//!
//! Parts without values are in the model, of size 0 and above: in a struct, in an array, in a
//! variant next to a field that has values, and as a field of a union. So is a union without
//! values, in a struct and in a variant of a source. Their oracles read nothing of a part without
//! values. The shapes of the model put a part that a check need not read next to a part that it
//! checks: the same union in an array, a union in an enum checked as a whole, an array of small
//! unions under an enum of the target whose first run accepts every value of 2 bytes, and a tuple,
//! whose layout Rust leaves to the compiler, in a field of a union and in a variant next to a
//! scalar that a pun never makes, a struct with a rule that loads its field, in a union in an
//! enum, and a union whose first field the source proves only through a requirement, which a
//! value of the source may break: the values of a source are what the type guarantees, not what
//! it requires. `tests/shapes.rs` has a sample of each, and runs under Miri.
//!
//! A scalar that a pun never makes (`Opaque`, made with `Spec::never`) is in the model alone, in a
//! struct, and in a variant of an enum. Its oracle reads an initialized byte as a source, and as a
//! target requires what no byte meets, without reading it. A check that accepts it is found
//! incorrect, which was tried. A check that rejects its variant need not read its byte, which may
//! be uninitialized in the source (`ByteZero -> MaybeOpaque`): only the value of a decision that
//! needs an uninitialized byte in both modes is unsound to check (`check_pair`), and Miri runs the
//! pair. `OpaqueNz` is such a byte with a niche at zero, and `Option<OpaqueNz>` is such a scalar
//! too, alone and in a struct (`HoldsOptOpaque`). That the same type does not prove it is not a
//! verdict that the oracle can judge, since a source that does not meet its own requirements
//! implies any requirement: the compile-fail case `never_same_type` of `puns` holds it.
//!
//! # Parts
//!
//! Parts are checked over the same types, for about four thousand pairs whose target is not larger
//! than the source ([`check_part`]). The checks catch a wrong shift, in the analysis or in the
//! pointer, and a `mutable_part` without its test that nothing outside the part depends on it. A
//! rule of the source over the part, or a part of an enum, would break the requirements of the
//! source.
//!
//! # Views
//!
//! View enums are checked against what they mean ([`views`]). Every ordered pair of the types of 1
//! and of 2 bytes is a view of two variants (`Two<A, B>`, a generic view enum, in which `A` is
//! decided from `B` where `B` takes every `A`). It is compared on every pattern of bytes up to one
//! more than its size, at an aligned and a misaligned address. A sample runs on a push
//! (`tests/views.rs`). It has views of three variants, a chain of bases, a base shared by two
//! variants, and views of unsized types, whose extents `from_head` takes from a rule or as many
//! elements as fit. Under Miri, a view runs for each unsafe path of `view.rs`. Every pair runs with
//! the `full` feature. The checks report each of the following faults, which were tried: a variant
//! checked for nothing from its base, a base chosen without the test that it takes the variant, a
//! base for `from_head` that takes other bytes, extents compared with their trailing padding, and a
//! variant rebuilt from its state with another length.
//!
//! # What runs where
//!
//! Every check that CI runs on a push takes at most a minute. So `cargo test` runs a sample of the
//! model check, and Miri runs a selection of pairs chosen for the unsafe paths that they reach, not
//! for the verdicts (`tests/miri_sample.rs` lists what each pair is for). Miri also runs the pairs
//! of `tests/shapes.rs`, for what a check of them reads. Miri is there to find undefined behavior.
//! The examples of `puns` other than `ethernet` do not run under Miri. They are
//! declarations, and the unsafe code that they reach is that of the library, which its own tests
//! and `ethernet` run there.
//!
//! The exhaustive run, every pair and every part of every type, and every mask of one bit, is
//! behind the `full` feature, which `cargo test` does not build. It runs with
//! `cargo test --profile model -p puns-model --features full`, optimized and without LTO. It takes
//! minutes to build and a few minutes to run. The workflow `model.yml` runs it without and with the
//! `unstable` feature, every night and on demand.
//!
//! [valid]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity

pub mod types;
pub mod views;

use std::{collections::BTreeMap, fmt::Write as _, mem::MaybeUninit, sync::Mutex};

use puns::{
  Pun,
  hook::{self, Parts, Verdict},
};

/// The bytes of a value, with `None` for each uninitialized byte.
pub type Bytes = [Option<u8>];

/// Whether some bytes are a valid value, or [`Uninit`] if the decision reads an uninitialized
/// byte.
pub type Valid = Result<bool, Uninit>;

/// The error of an oracle whose decision reads an uninitialized byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Uninit;

/// The rule for the validity of unions under which an oracle decides.
///
/// The language has not settled it. `puns` must be sound under every rule, so the model decides
/// under the two ends: a union whose bytes are one of its fields, which the strictest rules
/// require, and a union of any bytes, initialized or not, which Miri implements. Every type that
/// holds no union has the same values under both.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Unions {
  /// A union is valid if one of its fields is.
  Field,
  /// A union is valid whatever its bytes, and its oracle reads none of them.
  Bytes,
}

/// How an oracle decides: under which rule for unions, and whether requirements count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mode {
  /// Whether the requirements of the type count (`valid(..)`, rules), as for a target. Without
  /// them, only what the type system guarantees counts.
  pub required: bool,
  /// The rule for unions.
  pub unions: Unions,
}

impl Mode {
  /// What the type system guarantees under `unions`.
  #[must_use]
  pub const fn guaranteed(unions: Unions) -> Self {
    Self { required: false, unions }
  }

  /// What the type requires under `unions`.
  #[must_use]
  pub const fn requires(unions: Unions) -> Self {
    Self { required: true, unions }
  }
}

/// An oracle of the valid values of a type, written without the analysis of `puns`.
pub trait Oracle: Pun + Copy + 'static {
  /// Returns whether `b[off..off + size_of::<Self>()]` is a valid value of `Self`, decided as
  /// `mode` says.
  ///
  /// The oracle sets `reads[i]` for every byte `b[i]` that it looks at, and leaves the other
  /// elements of `reads` unchanged.
  ///
  /// # Errors
  ///
  /// Returns [`Uninit`] if the decision depends on an uninitialized byte.
  fn check(b: &Bytes, off: usize, reads: &mut [bool], mode: Mode) -> Valid;
}

/// Returns the rule for unions under which `p` is a value of `T`: [`Unions::Field`] if it is one
/// under that rule, [`Unions::Bytes`] otherwise.
///
/// A value under the first rule is one under the second too, and a target valid under the first
/// is valid under the second. So a pun must make a valid target of a value under the first rule
/// under which that value is a source, which is the one that this function returns.
fn rule_of<T: Oracle>(p: &Bytes) -> Unions {
  let mut reads = vec![false; p.len()];
  if T::check(p, 0, &mut reads, Mode::guaranteed(Unions::Field)) == Ok(true) {
    Unions::Field
  } else {
    Unions::Bytes
  }
}

/// Reads the unsigned integer of `size` bytes at `off`, least significant byte first if `little`.
///
/// It marks the `size` bytes as read in `reads`, whether they are initialized or not.
///
/// # Errors
///
/// Returns [`Uninit`] if one of the bytes is uninitialized.
///
/// # Panics
///
/// Panics if `off + size` is greater than the length of `b` or of `reads`.
pub fn int(
  b: &Bytes,
  off: usize,
  size: usize,
  little: bool,
  reads: &mut [bool],
) -> Result<u128, Uninit> {
  let mut value = 0u128;
  let mut uninit = false;
  for i in 0..size {
    reads[off + i] = true;
    let byte = if little { b[off + size - 1 - i] } else { b[off + i] };
    match byte {
      Some(byte) => value = (value << 8) | u128::from(byte),
      None => uninit = true,
    }
  }
  if uninit { Err(Uninit) } else { Ok(value) }
}

/// Whether the target is little-endian, the byte order in which the oracles read native integers.
pub const NATIVE_LITTLE: bool = cfg!(target_endian = "little");

/// A requirement of a value: its offset in the value, its name, and whether it holds.
type Requirement = (usize, &'static str, Valid);

thread_local! {
  /// The requirements that [`require`] records, or `None` when nothing records them.
  ///
  /// [`requirements`] sets it to `Some` while it runs an oracle.
  static REQUIRED: std::cell::RefCell<Option<Vec<Requirement>>> =
    const { std::cell::RefCell::new(None) };
}

/// Records a requirement of a value that the type system does not guarantee, and returns `holds`.
///
/// A requirement is a `valid(..)`, a rule, or what a target requires of a scalar that may be
/// uninitialized. `off` is its offset in the value, and `what` names it, which must be the same for
/// the same requirement at every offset. The requirement is recorded only while `requirements`
/// collects them.
///
/// # Errors
///
/// Returns [`Uninit`] if `holds` is [`Uninit`].
pub fn require(off: usize, what: &'static str, holds: Valid) -> Valid {
  REQUIRED.with(|required| {
    if let Some(required) = required.borrow_mut().as_mut() {
      required.push((off, what, holds));
    }
  });
  holds
}

/// Runs `holds`, and records its result as one requirement named `what` at `off`.
///
/// The requirements that `holds` records itself are discarded, because none of them is a
/// requirement of the value on its own. For example, a union requires that one of its fields is
/// valid, not that each field meets its requirements.
///
/// # Errors
///
/// Returns [`Uninit`] if `holds` returns [`Uninit`].
pub fn require_whole(off: usize, what: &'static str, holds: impl FnOnce() -> Valid) -> Valid {
  let outer = REQUIRED.with(|required| required.borrow_mut().take());
  let valid = holds();
  REQUIRED.with(|required| *required.borrow_mut() = outer);
  require(off, what, valid)
}

/// Returns the requirements of `p[off..off + size_of::<D>()]` as a `D`, each with its offset in the
/// `D`.
///
/// A requirement in a variant that the bytes are not is not returned, nor one of a union under
/// [`Unions::Bytes`].
fn requirements<D: Oracle>(p: &Bytes, off: usize, unions: Unions) -> Vec<Requirement> {
  let p = &p[off..off + size_of::<D>()];
  let mut reads = vec![false; p.len()];
  let outer = REQUIRED.with(|required| required.borrow_mut().replace(vec![]));
  let _ = D::check(p, 0, &mut reads, Mode::requires(unions));
  REQUIRED
    .with(|required| std::mem::replace(&mut *required.borrow_mut(), outer))
    .unwrap_or_default()
}

/// Returns, for each value of `source`, whether the requirements of the `D` at `off` that the value
/// does not meet are all implied by the requirements of `S`.
///
/// A requirement is identified by its offset and its name. It is implied if every value of
/// `source` that meets the requirements of `S` meets it. A value that does not have the
/// requirement, because it is another variant, meets it. The library may leave an implied
/// requirement unchecked, because a source that does not meet it does not meet its own
/// requirements. Each value is judged under its rule for unions ([`rule_of`]).
fn implied<S: Oracle, D: Oracle>(source: &[Vec<Option<u8>>], off: usize) -> Vec<bool> {
  let rules: Vec<Unions> = source.iter().map(|p| rule_of::<S>(p)).collect();
  let required: Vec<Vec<Requirement>> =
    source.iter().zip(&rules).map(|(p, rule)| requirements::<D>(p, off, *rule)).collect();
  let mut broken = std::collections::BTreeSet::<(usize, &'static str)>::new();
  for ((p, required), rule) in source.iter().zip(&required).zip(&rules) {
    if truth::<S>(p, *rule).soft == Ok(true) {
      for (at, what, holds) in required {
        if *holds != Ok(true) {
          broken.insert((*at, *what));
        }
      }
    }
  }
  required
    .iter()
    .map(|required| {
      required.iter().all(|(at, what, holds)| *holds == Ok(true) || !broken.contains(&(*at, *what)))
    })
    .collect()
}

/// Combines the results of the fields of a struct: valid if every field is valid.
///
/// # Errors
///
/// Returns [`Uninit`] if one of `results` is [`Uninit`], even if another field is invalid.
pub fn all(results: &[Valid]) -> Valid {
  if results.iter().any(std::result::Result::is_err) {
    return Err(Uninit);
  }
  Ok(results.iter().all(|r| *r == Ok(true)))
}

macro_rules! scalar_oracle {
  ($($ty:ty => |$v:ident| $pred:expr, $little:expr;)+) => {$(
    impl Oracle for $ty {
      fn check(b: &Bytes, off: usize, reads: &mut [bool], _: Mode) -> Valid {
        let $v = int(b, off, size_of::<$ty>(), $little, reads)?;
        Ok($pred)
      }
    }
  )+};
}

use core::num::NonZero;

use puns::num::{
  u8c, u8in, u16be, u16bec, u16le, u16lec, u24be, u24bec, u24le, u24lec, u32be, u32bec, u32le,
  u32lec,
};

scalar_oracle! {
  u8 => |_v| true, true;
  i8 => |_v| true, true;
  u16 => |_v| true, true;
  i16 => |_v| true, true;
  u32 => |_v| true, true;
  u64 => |_v| true, true;
  i32 => |_v| true, true;
  f32 => |_v| true, true;
  bool => |v| v <= 1, true;
  char => |v| v <= 0xd7ff || (0xe000..=0x10_ffff).contains(&v), NATIVE_LITTLE;
  NonZero<u8> => |v| v != 0, true;
  NonZero<u16> => |v| v != 0, true;
  NonZero<u32> => |v| v != 0, true;
  NonZero<i8> => |v| v != 0, true;
  Option<NonZero<u8>> => |_v| true, true;
  Option<NonZero<u16>> => |_v| true, true;
  Option<NonZero<u32>> => |_v| true, true;
  u16le => |_v| true, true;
  u16be => |_v| true, true;
  u24le => |_v| true, true;
  u24be => |_v| true, true;
  u32le => |_v| true, true;
  u32be => |_v| true, true;
}

/// The oracle of two niches in one byte.
///
/// `Some(None)` is 2, the first value after those of the `bool`. `rustc` chooses the byte of
/// `None`, so the oracle reads it from a value of `None`. The current `rustc` uses 255, the value
/// below 0 when the byte wraps.
impl Oracle for Option<Option<bool>> {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], _: Mode) -> Valid {
    let v = int(b, off, 1, true, reads)?;
    let none = None::<Option<bool>>;
    const { assert!(size_of::<Option<Option<bool>>>() == 1, "the niches are in one byte") };
    // SAFETY: `none` is 1 byte, which is asserted above, so the pointer is valid for reads of a
    // `u8`, aligned for it. That byte holds the niche of `None`, which `rustc` writes, so it is
    // initialized. Rust does not document this, and Miri reports a read of an uninitialized byte.
    let none = u128::from(unsafe { *core::ptr::from_ref(&none).cast::<u8>() });
    Ok(v <= 2 || v == none)
  }
}

/// The oracle of a niche in a `bool`, where `None` is 2.
impl Oracle for Option<bool> {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], _: Mode) -> Valid {
    Ok(int(b, off, 1, true, reads)? <= 2)
  }
}

impl<const V: u8> Oracle for u8c<V> {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], _: Mode) -> Valid {
    Ok(int(b, off, 1, true, reads)? == u128::from(V))
  }
}

impl<const MIN: u8, const MAX: u8> Oracle for u8in<MIN, MAX> {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], _: Mode) -> Valid {
    let v = int(b, off, 1, true, reads)?;
    Ok(u128::from(MIN) <= v && v <= u128::from(MAX))
  }
}

macro_rules! literal_oracle {
  ($($ty:ident: $raw:ty, $size:literal, $little:literal;)+) => {$(
    impl<const V: $raw> Oracle for $ty<V> {
      fn check(b: &Bytes, off: usize, reads: &mut [bool], _: Mode) -> Valid {
        Ok(int(b, off, $size, $little, reads)? == u128::from(V))
      }
    }
  )+};
}

literal_oracle! {
  u16lec: u16, 2, true;
  u16bec: u16, 2, false;
  u24lec: u32, 3, true;
  u24bec: u32, 3, false;
  u32lec: u32, 4, true;
  u32bec: u32, 4, false;
}

impl Oracle for () {
  fn check(_: &Bytes, _: usize, _: &mut [bool], _: Mode) -> Valid {
    Ok(true)
  }
}

impl<T: ?Sized + 'static> Oracle for core::marker::PhantomData<T> {
  fn check(_: &Bytes, _: usize, _: &mut [bool], _: Mode) -> Valid {
    Ok(true)
  }
}

impl<T: Oracle, const N: usize> Oracle for [T; N] {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], mode: Mode) -> Valid {
    let results: Vec<Valid> =
      (0..N).map(|i| T::check(b, off + i * size_of::<T>(), reads, mode)).collect();
    all(&results)
  }
}

impl<T: Oracle> Oracle for core::mem::ManuallyDrop<T> {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], mode: Mode) -> Valid {
    T::check(b, off, reads, mode)
  }
}

impl<A: Oracle> Oracle for (A,) {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], mode: Mode) -> Valid {
    A::check(b, off + core::mem::offset_of!(Self, 0), reads, mode)
  }
}

impl<A: Oracle, B: Oracle> Oracle for (A, B) {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], mode: Mode) -> Valid {
    all(&[
      A::check(b, off + core::mem::offset_of!(Self, 0), reads, mode),
      B::check(b, off + core::mem::offset_of!(Self, 1), reads, mode),
    ])
  }
}

impl<A: Oracle, B: Oracle, C: Oracle> Oracle for (A, B, C) {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], mode: Mode) -> Valid {
    all(&[
      A::check(b, off + core::mem::offset_of!(Self, 0), reads, mode),
      B::check(b, off + core::mem::offset_of!(Self, 1), reads, mode),
      C::check(b, off + core::mem::offset_of!(Self, 2), reads, mode),
    ])
  }
}

impl<A: Oracle, B: Oracle, C: Oracle, D: Oracle> Oracle for (A, B, C, D) {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], mode: Mode) -> Valid {
    all(&[
      A::check(b, off + core::mem::offset_of!(Self, 0), reads, mode),
      B::check(b, off + core::mem::offset_of!(Self, 1), reads, mode),
      C::check(b, off + core::mem::offset_of!(Self, 2), reads, mode),
      D::check(b, off + core::mem::offset_of!(Self, 3), reads, mode),
    ])
  }
}

/// Returns the values that each byte takes in the enumeration of the values of a type of `size`
/// bytes.
///
/// Natively, the alphabet is every byte for a type of up to 2 bytes. For a larger type, it is the
/// bounds of the ranges of the types of the model. Under Miri, it is a few bytes that tell the
/// valid values of the pairs of its sample from the invalid ones. The alphabet always ends with
/// `None`, an uninitialized byte.
pub fn alphabet(size: usize) -> Vec<Option<u8>> {
  let bytes: Vec<u8> = match size {
    // Miri executes the code (an interpreter is some thousand times slower): just what tells the
    // valid from the invalid values of the pairs of its sample, `0`, `1`, the high bit, all ones.
    0..=1 if cfg!(miri) => vec![0, 1, 2, 7, 0x80, 0xff],
    2 if cfg!(miri) => vec![0, 1, 0x80, 0xff],
    3.. if cfg!(miri) => vec![0, 1, 0xff],
    0..=2 => (0..=255).collect(),
    3 => vec![
      0, 1, 2, 3, 5, 7, 0x10, 0x11, 0x12, 0x34, 0x56, 0x7f, 0x80, 0xc2, 0xd7, 0xd8, 0xdf, 0xe0,
      0xfe, 0xff,
    ],
    4 => vec![0, 1, 2, 7, 0x10, 0x11, 0x34, 0x7f, 0x80, 0xd7, 0xd8, 0xe0, 0xfe, 0xff],
    _ => vec![0, 1, 2, 0xff],
  };
  let mut alphabet: Vec<Option<u8>> = bytes.into_iter().map(Some).collect();
  alphabet.push(None);
  alphabet
}

/// Returns every pattern of `size` bytes over the alphabet of a type of `size` bytes.
#[must_use]
pub fn patterns(size: usize) -> Vec<Vec<Option<u8>>> {
  patterns_over(size, size)
}

/// Returns every pattern of `size` bytes over the alphabet of a type of `of` bytes.
#[must_use]
pub fn patterns_over(size: usize, of: usize) -> Vec<Vec<Option<u8>>> {
  let alphabet = alphabet(of);
  let mut out = vec![vec![]];
  for _ in 0..size {
    let mut next = Vec::with_capacity(out.len() * alphabet.len());
    for prefix in &out {
      for &byte in &alphabet {
        let mut pattern = prefix.clone();
        pattern.push(byte);
        next.push(pattern);
      }
    }
    out = next;
  }
  out
}

/// Returns the values of `T` as a source, over the alphabet of a type of its size.
///
/// The values of a source are what may exist under some rule for unions, so requirements do not
/// count. A value is a pattern that the oracle of `T` accepts under one of the two rules
/// ([`Unions`]), without requirements. Its bytes that the oracle does not read under that rule are
/// padding, or bytes of a union that may be anything, and are uninitialized in the pattern.
#[must_use]
pub fn values<T: Oracle>() -> Vec<Vec<Option<u8>>> {
  values_over::<T>(size_of::<T>())
}

/// Returns the values of `T` as a source, over the alphabet of a type of `of` bytes.
#[must_use]
pub fn values_over<T: Oracle>(of: usize) -> Vec<Vec<Option<u8>>> {
  let size = size_of::<T>();
  patterns_over(size, of)
    .into_iter()
    .filter(|p| {
      [Unions::Field, Unions::Bytes].into_iter().any(|unions| {
        let mut reads = vec![false; size];
        T::check(p, 0, &mut reads, Mode::guaranteed(unions)) == Ok(true)
          && (0..size).all(|i| reads[i] || p[i].is_none())
      })
    })
    .collect()
}

/// Returns a `T` made of `bytes`, leaving the `None` bytes uninitialized.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `bytes` has at most `size_of::<T>()` elements.
/// - `bytes`, with uninitialized bytes where it holds `None`, is a valid `T`. A value of
///   [`values`] is one if the oracle of `T` is right. A wrong oracle makes this function return
///   an invalid `T`, which Miri reports.
#[must_use]
pub unsafe fn make<T: Oracle>(bytes: &Bytes) -> T {
  let mut value = MaybeUninit::<T>::uninit();
  let ptr = value.as_mut_ptr().cast::<u8>();
  for (i, byte) in bytes.iter().enumerate() {
    if let Some(byte) = byte {
      // SAFETY: `i < bytes.len() <= size_of::<T>()`, by contract on the caller. So `add(i)` stays
      // in `value`, and the byte there is valid for writes. A `u8` is aligned at any address.
      unsafe { ptr.add(i).write(*byte) };
    }
  }
  // SAFETY: The bytes of `value` are `bytes`, uninitialized where it holds `None`, which is a
  // valid `T` by contract on the caller, as `assume_init` requires.
  unsafe { value.assume_init() }
}

/// Returns the bytes of `value` that the oracle of `T` reads, with `None` for every other byte.
///
/// Which bytes the oracle reads depends on the value, for example on the tag of an enum. The
/// function therefore reads the bytes of `value` in rounds, only those that the oracle has read so
/// far, until a round reads no new byte. The oracle decides under [`Unions::Bytes`], under which
/// it reads no byte of a union, since a union may hold uninitialized bytes.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The oracle of `T` reads no byte that may be uninitialized in a value of `T`, which holds if
///   the oracle is right: a wrong oracle makes this function read an uninitialized byte, which Miri
///   reports.
pub unsafe fn bytes_of<T: Oracle>(value: &T) -> Vec<Option<u8>> {
  let size = size_of::<T>();
  let mut reads = vec![false; size];
  // Which bytes are looked at depends on the value: find them with the bytes of the value,
  // reading only what the oracle reads (the others may be padding).
  let ptr = std::ptr::from_ref::<T>(value).cast::<u8>();
  let mut known: Vec<Option<u8>> = vec![None; size];
  loop {
    let before = reads.clone();
    let _ = T::check(&known, 0, &mut reads, Mode::guaranteed(Unions::Bytes));
    for i in 0..size {
      if reads[i] {
        // SAFETY: `i < size_of::<T>()`, so `add(i)` stays in `value`, which is valid for reads. A
        // `u8` is aligned at any address. The oracle read the byte, so it is initialized in
        // `value`, by contract on the caller.
        known[i] = Some(unsafe { ptr.add(i).read() });
      }
    }
    if reads == before {
      return known;
    }
  }
}

/// The findings of a model check: what the library did wrong, and what it did less well than it
/// could.
#[derive(Default)]
pub struct Report {
  /// The number of pairs of types whose verdicts were checked ([`check_pair`]).
  pub pairs: usize,
  /// The number of pairs whose parts were checked (`head`, `tail` and their `_ref` and `_mut`
  /// forms, [`check_part`]).
  pub parts: usize,
  /// The number of values of sources that were enumerated.
  pub values: usize,
  /// The number of conversions run at run time.
  pub conversions: usize,
  /// The unsound verdicts. Each lets safe code make an invalid value or read padding.
  pub unsound: Vec<String>,
  /// The verdicts that contradict the oracle without being unsound. For example, a run-time check
  /// rejects a valid value, or a requirement of the target that the source does not imply is left
  /// unchecked.
  pub incorrect: Vec<String>,
  /// The imprecise verdicts, by cause. Each is a valid conversion that is refused, or that is
  /// checked at run time while it could be proven.
  pub imprecise: BTreeMap<String, Vec<String>>,
  /// The number of pairs in each class of verdict.
  pub classes: BTreeMap<String, usize>,
  /// The number of view enums checked ([`views::check_view`]).
  pub views: usize,
  /// The number of variants of these view enums that have a base.
  pub view_bases: usize,
  /// The pairs checked by [`check_pair`] or [`check_part`], by which [`finish`] tells a known
  /// defect that is no longer reported.
  pub checked: std::collections::BTreeSet<String>,
}

/// A verdict of `puns` that the model reports, and that is a known defect not fixed yet.
///
/// [`finish`] prints its findings as known, and does not count them. It fails if the pair was
/// checked and nothing was reported of it: the entry is then removed, with the fix. A defect that
/// only Miri reports, a read of an uninitialized byte, is not here, since Miri stops at it: the
/// test that reaches it is ignored under Miri, with the finding as the reason.
pub struct Known {
  /// The pair, as the report names it: `source -> target`.
  pub pair: &'static str,
  /// Whether it is reported without the `unstable` feature of `puns`.
  pub stable: bool,
  /// Whether it is reported with the `unstable` feature of `puns`.
  pub unstable: bool,
  /// What the defect is, as the audit names it.
  pub finding: &'static str,
}

/// The known defects of `puns` that the model reports.
pub const KNOWN: &[Known] = &[
  Known {
    pair: "PadEnd -> str",
    stable: true,
    unstable: true,
    finding: "finding 12, the check of a string reads past what is related",
  },
  Known {
    pair: "PadMid -> str",
    stable: true,
    unstable: true,
    finding: "finding 12, the check of a string reads past what is related",
  },
  Known {
    pair: "TagUnit -> str",
    stable: true,
    unstable: true,
    finding: "finding 12, the check of a string reads past what is related",
  },
  Known {
    pair: "ByteZero -> str",
    stable: true,
    unstable: true,
    finding: "finding 12, the check of a string reads past what is related",
  },
];

/// Returns the known defect of `pair` that this build of `puns` has, if any.
#[must_use]
pub fn known(pair: &str) -> Option<&'static Known> {
  let unstable = cfg!(feature = "unstable");
  KNOWN
    .iter()
    .find(|known| known.pair == pair && if unstable { known.unstable } else { known.stable })
}

/// The report of the running process, which every check of every test adds to.
///
/// [`finish`] takes it. Tests that run in parallel share it.
pub static REPORT: Mutex<Option<Report>> = Mutex::new(None);

/// Adds to [`REPORT`] with `f`, creating it if it is `None`.
///
/// # Panics
///
/// Panics if [`REPORT`] is poisoned.
fn record(f: impl FnOnce(&mut Report)) {
  let mut report = REPORT.lock().unwrap();
  f(report.get_or_insert_with(Report::default));
}

/// Formats bytes in hexadecimal, with `__` for an uninitialized byte.
fn show(p: &Bytes) -> String {
  let mut s = String::from("[");
  for (i, b) in p.iter().enumerate() {
    if i > 0 {
      s.push(' ');
    }
    match b {
      Some(b) => write!(s, "{b:02x}").unwrap(),
      None => s.push_str("__"),
    }
  }
  s.push(']');
  s
}

/// Returns the name of `T` without the paths of the crates that the model uses.
pub(crate) fn name<T: ?Sized>() -> String {
  std::any::type_name::<T>()
    .replace("puns_model::", "")
    .replace("puns::num::", "")
    .replace("core::num::nonzero::", "")
    .replace("core::option::", "")
    .replace("core::mem::manually_drop::", "")
    .replace("core::marker::", "")
}

/// What the oracle of a type says of a pattern, in its two modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Truth {
  /// Whether the pattern is valid for what the type system guarantees.
  hard: Valid,
  /// Whether the pattern is also valid for what the type requires (`valid(..)`, rules).
  soft: Valid,
}

/// Returns what the oracle of `D` says of the first `size_of::<D>()` bytes of `p`, under the rule
/// for unions `unions`.
fn truth<D: Oracle>(p: &Bytes, unions: Unions) -> Truth {
  truth_at::<D>(p, 0, unions)
}

/// Returns what the oracle of `D` says of `p[off..off + size_of::<D>()]`, under the rule for
/// unions `unions`.
fn truth_at<D: Oracle>(p: &Bytes, off: usize, unions: Unions) -> Truth {
  let p = &p[off..off + size_of::<D>()];
  let mut reads = vec![false; p.len()];
  let hard = D::check(p, 0, &mut reads, Mode::guaranteed(unions));
  let soft = D::check(p, 0, &mut reads, Mode::requires(unions));
  Truth { hard, soft }
}

/// Checks the verdict of the library on `S -> D` against the oracles, and records the findings in
/// [`REPORT`].
///
/// At most `limit` values of the source are converted at run time. With `usize::MAX`, all of them
/// are.
///
/// A verdict is unsound if it lets through a value that the type system does not allow, or if it
/// reads padding. A requirement of the target (`valid(..)`, a rule) that the library leaves
/// unchecked is not unsound. It is correct if the requirements of the source imply it, and
/// incorrect otherwise.
///
/// # Panics
///
/// Panics if a conversion that the verdict says compiles does not compile, or if [`REPORT`] is
/// poisoned.
pub fn check_pair<S: Oracle, D: Oracle>(limit: usize) {
  let v: Verdict = const { hook::verdict::<S, D>() };
  let pair = format!("{} -> {}", name::<S>(), name::<D>());
  let (ss, ds) = (size_of::<S>(), size_of::<D>());
  let source = values::<S>();
  let rules: Vec<Unions> = source.iter().map(|p| rule_of::<S>(p)).collect();
  let truths: Vec<Truth> = if ds <= ss {
    source.iter().zip(&rules).map(|(p, rule)| truth::<D>(p, *rule)).collect()
  } else {
    vec![]
  };
  // Whatever the requirements of the target that a value does not meet are implied by those of
  // the source.
  let claims: Vec<bool> = if ds <= ss { implied::<S, D>(&source, 0) } else { vec![] };
  // A value whose decision needs an uninitialized byte: what the type system guarantees reads one,
  // and so does what the target requires, which is what a check decides. Every oracle reads in
  // target mode what it reads without it, but that of a scalar that a pun never makes, whose
  // requirement holds of no byte and reads none: a check rejects its variant unread.
  let any_uninit = truths.iter().any(|t| t.hard.is_err() && t.soft.is_err());
  // A requirement that reads uninitialized bytes is unsound to check: only one that the source
  // implies may be read so, as it is then not checked.
  let soft_uninit = truths.iter().zip(&claims).any(|(t, c)| t.soft.is_err() && !c);
  let all_hard = ds <= ss && truths.iter().all(|t| t.hard == Ok(true));
  let all_valid = ds <= ss && truths.iter().all(|t| t.soft == Ok(true));
  let none_valid = truths.iter().all(|t| t.soft != Ok(true));
  let class = match () {
    () if v.pun_ref => "pun_ref",
    () if v.pun => "pun (not by reference: alignment)",
    () if v.try_pun_ref => "try_pun_ref",
    () if v.try_pun => "try_pun (not by reference)",
    () if v.head || v.head_ref => "head only",
    () => "rejected",
  };
  let mut unsound = vec![];
  let mut incorrect = vec![];
  let mut imprecise: Vec<(String, String)> = vec![];
  let mut conversions = 0;
  let example = |pred: &dyn Fn(&Truth) -> bool| {
    source
      .iter()
      .zip(&truths)
      .find(|(_, t)| pred(t))
      .map_or_default(|(p, t)| format!("{} ({t:?})", show(p)))
  };

  // Sizes and alignment.
  if (v.pun || v.try_pun) && ss != ds {
    unsound.push(format!("{pair}: by value between sizes {ss} and {ds}"));
  }
  if v.head && ds > ss {
    unsound.push(format!("{pair}: head to a larger type"));
  }
  if (v.pun_ref || v.pun_mut || v.head_ref) && align_of::<D>() > align_of::<S>() {
    unsound.push(format!("{pair}: reference to a more aligned type"));
  }
  if ds <= ss {
    let infallible = if ss == ds { v.pun || v.pun_ref } else { v.head || v.head_ref };
    let fallible = ss == ds && (v.try_pun || v.try_pun_ref);
    // (a) infallible implies inclusion, without padding.
    if infallible && !all_hard {
      unsound.push(format!(
        "{pair}: compiles without a check, but {}",
        example(&|t| t.hard != Ok(true))
      ));
    } else if infallible && !all_valid {
      let unproven =
        source.iter().zip(&truths).zip(&claims).any(|((_, t), c)| t.soft != Ok(true) && !c);
      if unproven {
        incorrect.push(format!(
          "{pair}: a requirement is not checked: {}",
          example(&|t| t.soft != Ok(true))
        ));
      }
    }
    if ss == ds && !v.try_pun && !v.try_pun_ref {
      // (c) never implies emptiness; uninit is only for what reads padding.
      let why = v.why();
      if why.contains("no value of the source") && !none_valid {
        imprecise.push((
          "rejected as never valid, but some values are".into(),
          format!("{pair}: {}", example(&|t| t.soft == Ok(true))),
        ));
      } else if why.contains("is padding") && !any_uninit && !soft_uninit {
        imprecise.push(("rejected for padding that no valid target reads".into(), pair.clone()));
      } else if v.unspecified() {
        imprecise
          .push(("rejected: Rust does not keep the layout (`unstable`)".into(), pair.clone()));
      } else if !why.contains("no value") && !why.contains("padding") {
        incorrect.push(format!("{pair}: try_pun_ref rejected: {}", why.trim()));
      }
    }
    if fallible && any_uninit {
      unsound.push(format!("{pair}: try compiles, but {}", example(&|t| t.hard.is_err())));
    }
    if fallible && soft_uninit {
      unsound.push(format!(
        "{pair}: try compiles, and a requirement reads padding: {}",
        example(&|t| t.soft.is_err())
      ));
    }
    if fallible && !infallible && all_valid && !source.is_empty() {
      imprecise.push(("demoted to a run-time check".into(), pair.clone()));
    }
    // (b) the run-time check accepts exactly the valid values.
    // A conversion is not run once the verdict lets through a value that is not valid: it would
    // make that value, and the model would hold an invalid value (`static_unsound`).
    let static_unsound = !unsound.is_empty();
    if v.try_pun && !any_uninit && !soft_uninit && !static_unsound {
      for (((p, t), claim), rule) in source.iter().zip(&truths).zip(&claims).zip(&rules).take(limit)
      {
        // A check must accept a value that is a valid target under every rule for unions. Under
        // `Unions::Bytes` alone, a union of the target over other bytes of the source is valid
        // whatever they are, but `puns` checks it as one of its fields, by design.
        let firm = if *rule == Unions::Field { *t } else { truth::<D>(p, Unions::Field) };
        // SAFETY: `p` is a value of `values::<S>()`, of `size_of::<S>()` bytes. The model check
        // relies on the oracles being right, which Miri checks. These are the requirements of
        // `make`.
        let value = unsafe { make::<S>(p) };
        let result = hook::try_pun::<S, D>(value).expect("try_pun compiles");
        // By reference, an address that is not aligned for the target is an error of its own
        // (Miri puts values anywhere their alignment allows, natively they are often more aligned).
        let result_ref = if v.try_pun_ref {
          let result = hook::try_pun_ref::<S, D>(&value).expect("try_pun_ref compiles");
          let misaligned = !(&raw const value).addr().is_multiple_of(align_of::<D>());
          if misaligned && result.err() != Some(puns::Error::Alignment) {
            unsound.push(format!(
              "{pair}: try_pun_ref of a misaligned {} is not an alignment error",
              show(p)
            ));
          }
          (!misaligned).then(|| result.is_ok())
        } else {
          None
        };
        conversions += 1;
        let accepted = result.is_ok();
        if let Ok(d) = result {
          // SAFETY: The model check relies on the oracle of `D` being right, as `bytes_of`
          // requires.
          let got = unsafe { bytes_of(&d) };
          let want: Vec<Option<u8>> =
            (0..ds).map(|i| if got[i].is_some() { p[i] } else { None }).collect();
          if got != want {
            unsound.push(format!("{pair}: {} became {}", show(p), show(&got)));
          }
        }
        for (what, accepted) in [("try_pun", Some(accepted)), ("try_pun_ref", result_ref)] {
          let Some(accepted) = accepted else { continue };
          if accepted {
            match (t.hard, t.soft) {
              (Ok(true), Ok(true)) => {}
              (Ok(true), _) if *claim => {}
              (Ok(true), _) => incorrect.push(format!(
                "{pair}: {what} accepts {} ({t:?}), a requirement is not checked",
                show(p)
              )),
              _ => unsound.push(format!("{pair}: {what} accepts {} ({t:?})", show(p))),
            }
          } else if firm.hard == Ok(true) && firm.soft == Ok(true) {
            incorrect.push(format!("{pair}: {what} rejects the valid {} ({firm:?})", show(p)));
          }
        }
        if unsound.len() + incorrect.len() > 3 {
          break;
        }
      }
    }
    // (d) a `&mut` needs the inclusion both ways.
    if ss == ds && (v.pun_mut || v.try_pun_mut) {
      // Under each rule for unions, every target is a source.
      for q in values::<D>() {
        let mut reads = vec![false; ss];
        let back = S::check(&q, 0, &mut reads, Mode::guaranteed(rule_of::<D>(&q)));
        if back != Ok(true) {
          unsound
            .push(format!("{pair}: &mut, and the target {} is not a source ({back:?})", show(&q)));
          break;
        }
      }
    }
    // Truncation: the first bytes of the source, unchanged.
    if ds < ss && v.head && !static_unsound {
      for p in source.iter().take(limit) {
        // SAFETY: `p` is a value of `values::<S>()`, of `size_of::<S>()` bytes. The model check
        // relies on the oracles being right, which Miri checks. These are the requirements of
        // `make`.
        let d = hook::head::<S, D>(unsafe { make::<S>(p) }).expect("head compiles");
        // SAFETY: The model check relies on the oracle of `D` being right, as `bytes_of`
        // requires.
        let got = unsafe { bytes_of(&d) };
        let want: Vec<Option<u8>> =
          (0..ds).map(|i| if got[i].is_some() { p[i] } else { None }).collect();
        conversions += 1;
        if got != want {
          unsound.push(format!("{pair}: {} truncated to {}", show(p), show(&got)));
          break;
        }
      }
    }
  }
  record(|report| {
    report.pairs += 1;
    report.checked.insert(pair.clone());
    report.values += source.len();
    report.conversions += conversions;
    *report.classes.entry(class.into()).or_default() += 1;
    report.unsound.extend(unsound);
    report.incorrect.extend(incorrect);
    for (cause, pair) in imprecise {
      report.imprecise.entry(cause).or_default().push(pair);
    }
  });
}

/// What can go wrong when a value is written through a `&mut D` to the bytes `off..` of a source,
/// with an example of each failure.
struct Writes {
  /// A write that leaves a source that is not valid.
  hard: Option<String>,
  /// A write of a `D` that meets its requirements, to a source that met its own, that leaves a
  /// source that does not meet them. A rule or a `valid(..)` of the source that depends on the
  /// bytes written is such a requirement.
  soft: Option<String>,
}

/// Writes every value of `D` into the bytes `off..` of every value of `source`, and judges each
/// result with the oracle of `S`.
///
/// The values of `D` are those over the alphabet of the source. Sources that differ only in the
/// bytes written, and in nothing else that matters here, are tried once. A result is judged under
/// the rule for unions under which both the source and the value written exist: [`Unions::Bytes`]
/// if one of them is a value under that rule only.
fn writes<S: Oracle, D: Oracle>(source: &[Vec<Option<u8>>], off: usize) -> Writes {
  let (ss, ds) = (size_of::<S>(), size_of::<D>());
  // What a source is outside of the bytes, under which rule for unions, and whatever one of those
  // sources meets its requirements.
  let mut outsides: BTreeMap<(Vec<Option<u8>>, Unions), bool> = BTreeMap::new();
  for p in source {
    let mut outside = p.clone();
    outside[off..off + ds].fill(None);
    let rule = rule_of::<S>(p);
    *outsides.entry((outside, rule)).or_default() |= truth::<S>(p, rule).soft == Ok(true);
  }
  let (mut hard, mut soft) = (None, None);
  // With whatever the target meets its own requirements: one that does not breaks those of the
  // source as surely as a `&mut` to the source itself does.
  let targets: Vec<(Vec<Option<u8>>, Unions, bool)> = values_over::<D>(ss)
    .into_iter()
    .map(|q| {
      let rule = rule_of::<D>(&q);
      let met = truth::<D>(&q, rule).soft == Ok(true);
      (q, rule, met)
    })
    .collect();
  for ((outside, rule), requirements) in &outsides {
    for (q, rule_q, met) in &targets {
      let unions = (*rule).max(*rule_q);
      let mut r = outside.clone();
      r[off..off + ds].copy_from_slice(q);
      let mut reads = vec![false; ss];
      let valid = S::check(&r, 0, &mut reads, Mode::guaranteed(unions));
      if valid != Ok(true) && hard.is_none() {
        hard = Some(format!("{} written at {off} leaves {} ({valid:?})", show(q), show(&r)));
      }
      let required = S::check(&r, 0, &mut reads, Mode::requires(unions));
      if *requirements && *met && required != Ok(true) && soft.is_none() {
        soft = Some(format!("{} written at {off} leaves {} ({required:?})", show(q), show(&r)));
      }
    }
    if hard.is_some() && soft.is_some() {
      break;
    }
  }
  Writes { hard, soft }
}

/// Returns the offset of `part` in `whole`.
///
/// # Panics
///
/// Panics if `part` starts before `whole` and overflow checks are enabled.
fn offset_in<S, D: ?Sized>(whole: &S, part: &D) -> usize {
  std::ptr::from_ref(part).cast::<u8>().addr() - std::ptr::from_ref(whole).cast::<u8>().addr()
}

/// Checks the verdicts of the library on the parts of `S` as a `D` against the oracles, and records
/// the findings in [`REPORT`].
///
/// The parts are the first bytes of `S` (`head`, `head_ref`, `head_mut`) and its last bytes
/// (`tail`, `tail_ref`, `tail_mut`). At most `limit` values of the source are converted at run
/// time.
///
/// A part may be proven only if every source holds a valid `D` there. A `&mut` to the part is
/// sound only if every `D` written there leaves a valid source. It must also keep the requirements
/// (rules, `valid(..)`) of a source that met them, which `pun_mut` does not require. So nothing of
/// the source outside the part may depend on the bytes of the part.
///
/// # Panics
///
/// Panics if [`REPORT`] is poisoned.
pub fn check_part<S: Oracle, D: Oracle>(limit: usize) {
  let v: Parts = const { hook::parts::<S, D>() };
  let pair = format!("{} -> {}", name::<S>(), name::<D>());
  let (ss, ds) = (size_of::<S>(), size_of::<D>());
  let (mut unsound, mut incorrect, mut imprecise) = (vec![], vec![], vec![]);
  let mut conversions = 0;
  let parts = [
    ("head", 0, v.head, v.head_ref, v.head_mut),
    ("tail", ss.saturating_sub(ds), v.tail, v.tail_ref, v.tail_mut),
  ];
  let source = if ds <= ss { values::<S>() } else { vec![] };
  for (part, off, by_value, by_ref, by_mut) in parts {
    if ds > ss {
      if by_value || by_ref || by_mut {
        unsound.push(format!("{pair}: {part} of a larger type"));
      }
      continue;
    }
    if (by_ref || by_mut) && (align_of::<D>() > align_of::<S>() || off % align_of::<D>() != 0) {
      unsound.push(format!("{pair}: {part}_ref to a misaligned target"));
    }
    if (by_ref && !by_value) || (by_mut && !by_ref) {
      incorrect.push(format!("{pair}: {part}: {by_value}, by reference {by_ref}, mut {by_mut}"));
    }
    // The bytes of the part, and whatever the requirements of the part that a value does not meet
    // are implied by those of the source.
    let truths: Vec<Truth> =
      source.iter().map(|p| truth_at::<D>(p, off, rule_of::<S>(p))).collect();
    let claims = if ds <= ss { implied::<S, D>(&source, off) } else { vec![] };
    let example = |pred: &dyn Fn(&Truth) -> bool| {
      source
        .iter()
        .zip(&truths)
        .find(|(_, t)| pred(t))
        .map_or_default(|(p, t)| format!("{} ({t:?})", show(p)))
    };
    let all_hard = truths.iter().all(|t| t.hard == Ok(true));
    let all_valid = truths.iter().all(|t| t.soft == Ok(true));
    if by_value && !all_hard {
      unsound.push(format!("{pair}: {part} compiles, but {}", example(&|t| t.hard != Ok(true))));
    } else if by_value && !all_valid {
      let unproven =
        source.iter().zip(&truths).zip(&claims).any(|((_, t), c)| t.soft != Ok(true) && !c);
      if unproven {
        incorrect.push(format!(
          "{pair}: {part}: a requirement is not checked: {}",
          example(&|t| t.soft != Ok(true))
        ));
      }
    }
    if !by_value && all_valid && !source.is_empty() {
      imprecise.push((format!("{part} refused, but every part is a target"), pair.clone()));
    }
    // At run time: the very bytes, at the very place. Not once the verdict lets through a value
    // that is not valid, which the model would then hold.
    let limit = if unsound.is_empty() { limit } else { 0 };
    let want = |p: &Bytes, got: &[Option<u8>]| -> Vec<Option<u8>> {
      (0..ds).map(|i| if got[i].is_some() { p[off + i] } else { None }).collect()
    };
    for p in source.iter().take(limit) {
      // SAFETY: `p` is a value of `values::<S>()`, of `size_of::<S>()` bytes. The model check
      // relies on the oracles being right, which Miri checks. These are the requirements of
      // `make`.
      let value = unsafe { make::<S>(p) };
      let got = if part == "head" { hook::head::<S, D>(value) } else { hook::tail::<S, D>(value) };
      if let Some(d) = got {
        conversions += 1;
        // SAFETY: The model check relies on the oracle of `D` being right, as `bytes_of`
        // requires.
        let got = unsafe { bytes_of(&d) };
        if got != want(p, &got) {
          unsound.push(format!("{pair}: {part} of {} is {}", show(p), show(&got)));
          break;
        }
      }
      let got = if part == "head" {
        hook::head_ref::<S, D>(&value)
      } else {
        hook::tail_ref::<S, D>(&value)
      };
      if let Some(d) = got {
        conversions += 1;
        // SAFETY: The model check relies on the oracle of `D` being right, as `bytes_of` requires.
        let (at, got) = (offset_in(&value, d), unsafe { bytes_of(d) });
        if at != off || got != want(p, &got) {
          unsound.push(format!("{pair}: {part}_ref of {} is {} at {at}", show(p), show(&got)));
          break;
        }
      }
      let mut value = value;
      let base = std::ptr::from_ref(&value).cast::<u8>().addr();
      let got = if part == "head" {
        hook::head_mut::<S, D>(&mut value)
      } else {
        hook::tail_mut::<S, D>(&mut value)
      };
      if let Some(d) = got {
        conversions += 1;
        let at = std::ptr::from_mut(d).cast::<u8>().addr() - base;
        if at != off {
          unsound.push(format!("{pair}: {part}_mut of {} is at {at}", show(p)));
          break;
        }
      }
    }
    // What can be written through a `&mut` to the part.
    if by_ref && !source.is_empty() {
      let writes = writes::<S, D>(&source, off);
      match (&writes.hard, &writes.soft) {
        (Some(why), _) if by_mut => unsound.push(format!("{pair}: {part}_mut, but {why}")),
        (None, Some(why)) if by_mut => {
          unsound.push(format!("{pair}: {part}_mut, but a requirement breaks: {why}"));
        }
        (None, None) if !by_mut => {
          imprecise.push((format!("{part}_mut refused, but every write is valid"), pair.clone()));
        }
        _ => {}
      }
    }
  }
  record(|report| {
    report.parts += 1;
    report.checked.insert(pair.clone());
    report.conversions += conversions;
    report.unsound.extend(unsound);
    report.incorrect.extend(incorrect);
    for (cause, pair) in imprecise {
      report.imprecise.entry(cause).or_default().push(pair);
    }
  });
}

/// Takes [`REPORT`] and prints it under `title`.
///
/// # Panics
///
/// Panics if the report holds an unsound or an incorrect verdict that is not [`KNOWN`], if a known
/// pair was checked and nothing was reported of it, or if [`REPORT`] is poisoned.
pub fn finish(title: &str) {
  let mut report = REPORT.lock().unwrap().take().unwrap_or_default();
  // The findings of known defects, apart, and the known pairs checked of which none is reported.
  let of = |line: &String| line.split(": ").next().and_then(known);
  let mut reported = std::collections::BTreeSet::new();
  let mut known_lines = vec![];
  for lines in [&mut report.unsound, &mut report.incorrect] {
    lines.retain(|line| {
      let Some(known) = of(line) else { return true };
      reported.insert(known.pair);
      known_lines.push(format!("{}: {line}", known.finding));
      false
    });
  }
  let stale: Vec<&str> = report
    .checked
    .iter()
    .filter_map(|pair| known(pair))
    .filter(|known| !reported.contains(known.pair))
    .map(|known| known.pair)
    .collect();
  println!(
    "== {title}: {} pairs, {} pairs of parts, {} source values, {} conversions at run time",
    report.pairs, report.parts, report.values, report.conversions
  );
  for (class, count) in &report.classes {
    println!("   {count:5} {class}");
  }
  if report.views > 0 {
    println!("   {} views, {} variants with a base", report.views, report.view_bases);
  }
  for (cause, pairs) in &report.imprecise {
    println!("   imprecise, {cause}: {}", pairs.len());
    for pair in pairs.iter().take(12) {
      println!("      {pair}");
    }
  }
  for line in &report.incorrect {
    println!("   INCORRECT {line}");
  }
  for line in &report.unsound {
    println!("   UNSOUND {line}");
  }
  for line in &known_lines {
    println!("   KNOWN {line}");
  }
  for pair in &stale {
    println!("   NO LONGER REPORTED, remove it from `KNOWN`: {pair}");
  }
  assert!(
    report.unsound.is_empty() && report.incorrect.is_empty() && stale.is_empty(),
    "{title}: {} unsound, {} incorrect, {} known pairs no longer reported",
    report.unsound.len(),
    report.incorrect.len(),
    stale.len()
  );
}

/// Checks every ordered pair of the types, a type with itself included, with [`check_pair`] and
/// [`check_part`].
///
/// The first argument is the `limit` of both functions.
#[macro_export]
macro_rules! all_pairs {
  ($limit:expr; $($ty:ty),+ $(,)?) => {
    $crate::all_pairs!(@outer $limit; [$($ty),+]; $($ty),+);
  };
  (@outer $limit:expr; $all:tt; $($s:ty),+) => {
    $( $crate::all_pairs!(@inner $limit; $s; $all); )+
  };
  (@inner $limit:expr; $s:ty; [$($d:ty),+]) => {
    $( $crate::check_pair::<$s, $d>($limit); $crate::check_part::<$s, $d>($limit); )+
  };
}

/// Checks the given pairs of types with `$check`, which is `check_pair` or `check_part`.
///
/// The pairs are a selection rather than a product, for Miri.
#[macro_export]
macro_rules! pairs {
  ($check:ident, $limit:expr; $(($s:ty, $d:ty)),+ $(,)?) => {
    $( $crate::$check::<$s, $d>($limit); )+
  };
}

/// Checks the parts of every source of the first list as every target of the second, with
/// [`check_part`].
///
/// The targets are meant to be smaller than the sources.
#[macro_export]
macro_rules! part_pairs {
  ($limit:expr; [$($s:ty),+ $(,)?]; $targets:tt) => {
    $( $crate::part_pairs!(@inner $limit; $s; $targets); )+
  };
  (@inner $limit:expr; $s:ty; [$($d:ty),+ $(,)?]) => {
    $( $crate::check_part::<$s, $d>($limit); )+
  };
}
