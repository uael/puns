//! Model check of the analysis of `puns` against an oracle that does not use it.
//!
//! For each type, [`Oracle::check`] tells whatever some bytes (`None`: uninitialized) are a valid
//! value, written by hand from what the language and the documentation of the type say, with the
//! offsets that the compiler gives (`offset_of!`, addresses of the fields of a value). The valid
//! values of a type are found by enumeration ([`values`]), and every verdict of the library on a
//! pair of types is compared with what they imply ([`check_pair`]).
//!
//! The parts of a value are checked the same way ([`check_part`]), for targets of any size not
//! greater than the source: the first bytes (`head`, `head_ref`) and the last (`tail`,
//! `tail_ref`) against the oracle of the target at that offset, the bytes and the address that
//! the functions give at run time, and a `&mut` to a part (`head_mut`, `tail_mut`) by writing
//! every value of the target into every value of the source: what results must be a valid
//! source, and must still meet the requirements of the source if it did and if what was written
//! meets those of the target.
//!
//! What a value of a struct is known to be does not include what `#[pun(valid(..))]` or a rule
//! requires: safe code can write anything to a plain field. The oracle has two modes for that.
//!
//! A requirement of the target that the library does not check is right only where the
//! requirements of the source imply it: then a source that met its own meets it (and one that did
//! not is not something the library promised anything of). The oracle finds this by itself, one
//! requirement at a time ([`require`]): a requirement of the target is implied if every value of
//! the source that meets the requirements of the source meets it. This is what the library proves
//! from the same rules of the source in a struct of the same extent at the same place, decided
//! without that rule: any other requirement left unchecked is reported.

pub mod pointers;
// The self-relative pointers of the tests of `puns`, declared through its public API only.
#[path = "../../puns/tests/support/relative.rs"]
pub mod relative;
pub mod types;
pub mod views;

use std::{collections::BTreeMap, fmt::Write as _, mem::MaybeUninit, sync::Mutex};

use puns::{
  Pun,
  hook::{self, Parts, Verdict},
};

/// Bytes of a value, `None` where they are uninitialized.
pub type Bytes = [Option<u8>];

/// Whatever the value was decided without reading an uninitialized byte.
pub type Valid = Result<bool, Uninit>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Uninit;

/// What the bytes of a type must be.
pub trait Oracle: Pun + Copy + 'static {
  /// Whatever `b[off..off + size_of::<Self>()]` is a valid value. With `target`, requirements
  /// (`valid(..)`, rules) count; without, only what the type system guarantees. Every byte that is
  /// looked at is set in `reads`.
  ///
  /// # Errors
  /// [`Uninit`] if what decides is an uninitialized byte.
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid;
}

/// Read the integer of `size` bytes at `off`.
///
/// # Errors
/// [`Uninit`] if one of them is uninitialized.
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

pub const NATIVE_LITTLE: bool = cfg!(target_endian = "little");

/// A requirement of a value: where it is in the value, what it is, and whatever it holds.
type Requirement = (usize, &'static str, Valid);

thread_local! {
  /// The requirements that [`require`] records, while [`requirements`] asks for them.
  static REQUIRED: std::cell::RefCell<Option<Vec<Requirement>>> =
    const { std::cell::RefCell::new(None) };
}

/// A requirement of a value that the type system does not guarantee: `holds`, which is returned.
///
/// It is a `valid(..)`, a rule, what a scalar that may be uninitialized must be. It is recorded
/// with where it is (`off`) and what it is (`what`, the same for the same requirement wherever it
/// is).
///
/// # Errors
/// `holds`, if it is [`Uninit`].
pub fn require(off: usize, what: &'static str, holds: Valid) -> Valid {
  REQUIRED.with(|required| {
    if let Some(required) = required.borrow_mut().as_mut() {
      required.push((off, what, holds));
    }
  });
  holds
}

/// What `holds` is, as one requirement (`off`, `what`), whatever it requires within.
///
/// The requirements that it records are not recorded, as they are not requirements of the value
/// on their own (the fields of a union, of which one must be valid).
///
/// # Errors
/// What `holds` returns, if it is [`Uninit`].
pub fn require_whole(off: usize, what: &'static str, holds: impl FnOnce() -> Valid) -> Valid {
  let outer = REQUIRED.with(|required| required.borrow_mut().take());
  let valid = holds();
  REQUIRED.with(|required| *required.borrow_mut() = outer);
  require(off, what, valid)
}

/// The requirements of `p[off..off + size_of::<D>()]` as a `D`, each with where it is in the `D`.
fn requirements<D: Oracle>(p: &Bytes, off: usize) -> Vec<Requirement> {
  let p = &p[off..off + size_of::<D>()];
  let mut reads = vec![false; p.len()];
  let outer = REQUIRED.with(|required| required.borrow_mut().replace(vec![]));
  let _ = D::check(p, 0, &mut reads, true);
  REQUIRED
    .with(|required| std::mem::replace(&mut *required.borrow_mut(), outer))
    .unwrap_or_default()
}

/// For each value of the source, whatever every requirement of the `D` at `off` that it does not
/// meet is implied by the requirements of the source: met by every value of the source that meets
/// those (a requirement that is not there in a value, in a variant that it is not, is met). Then
/// the library may leave them unchecked: a source that does not meet them did not meet its own.
fn implied<S: Oracle, D: Oracle>(source: &[Vec<Option<u8>>], off: usize) -> Vec<bool> {
  let required: Vec<Vec<Requirement>> = source.iter().map(|p| requirements::<D>(p, off)).collect();
  let mut broken = std::collections::BTreeSet::<(usize, &'static str)>::new();
  for (p, required) in source.iter().zip(&required) {
    if truth::<S>(p).soft == Ok(true) {
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

/// All the fields of a struct: valid if each is, an uninitialized byte under any is an error.
///
/// # Errors
/// [`Uninit`] if one of `results` is.
pub fn all(results: &[Valid]) -> Valid {
  if results.iter().any(std::result::Result::is_err) {
    return Err(Uninit);
  }
  Ok(results.iter().all(|r| *r == Ok(true)))
}

macro_rules! scalar_oracle {
  ($($ty:ty => |$v:ident| $pred:expr, $little:expr;)+) => {$(
    impl Oracle for $ty {
      fn check(b: &Bytes, off: usize, reads: &mut [bool], _: bool) -> Valid {
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

/// Two niches in one byte, after the values of the `bool` (`Some(None)`, 2) and where `rustc`
/// puts the other, read from a value of it (below the valid values today: 255).
impl Oracle for Option<Option<bool>> {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], _: bool) -> Valid {
    let v = int(b, off, 1, true, reads)?;
    let none = None::<Option<bool>>;
    // SAFETY: a byte of a niche, which `rustc` writes (`None` is no padding).
    let none = u128::from(unsafe { *core::ptr::from_ref(&none).cast::<u8>() });
    Ok(v <= 2 || v == none)
  }
}

/// A niche in a `bool`: `None` is 2.
impl Oracle for Option<bool> {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], _: bool) -> Valid {
    Ok(int(b, off, 1, true, reads)? <= 2)
  }
}

impl<const V: u8> Oracle for u8c<V> {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], _: bool) -> Valid {
    Ok(int(b, off, 1, true, reads)? == u128::from(V))
  }
}

impl<const MIN: u8, const MAX: u8> Oracle for u8in<MIN, MAX> {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], _: bool) -> Valid {
    let v = int(b, off, 1, true, reads)?;
    Ok(u128::from(MIN) <= v && v <= u128::from(MAX))
  }
}

macro_rules! literal_oracle {
  ($($ty:ident: $raw:ty, $size:literal, $little:literal;)+) => {$(
    impl<const V: $raw> Oracle for $ty<V> {
      fn check(b: &Bytes, off: usize, reads: &mut [bool], _: bool) -> Valid {
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
  fn check(_: &Bytes, _: usize, _: &mut [bool], _: bool) -> Valid {
    Ok(true)
  }
}

impl<T: ?Sized + 'static> Oracle for core::marker::PhantomData<T> {
  fn check(_: &Bytes, _: usize, _: &mut [bool], _: bool) -> Valid {
    Ok(true)
  }
}

impl<T: Oracle, const N: usize> Oracle for [T; N] {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    let results: Vec<Valid> =
      (0..N).map(|i| T::check(b, off + i * size_of::<T>(), reads, target)).collect();
    all(&results)
  }
}

impl<T: Oracle> Oracle for core::mem::ManuallyDrop<T> {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    T::check(b, off, reads, target)
  }
}

impl<A: Oracle> Oracle for (A,) {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    A::check(b, off + core::mem::offset_of!(Self, 0), reads, target)
  }
}

impl<A: Oracle, B: Oracle> Oracle for (A, B) {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    all(&[
      A::check(b, off + core::mem::offset_of!(Self, 0), reads, target),
      B::check(b, off + core::mem::offset_of!(Self, 1), reads, target),
    ])
  }
}

impl<A: Oracle, B: Oracle, C: Oracle> Oracle for (A, B, C) {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    all(&[
      A::check(b, off + core::mem::offset_of!(Self, 0), reads, target),
      B::check(b, off + core::mem::offset_of!(Self, 1), reads, target),
      C::check(b, off + core::mem::offset_of!(Self, 2), reads, target),
    ])
  }
}

impl<A: Oracle, B: Oracle, C: Oracle, D: Oracle> Oracle for (A, B, C, D) {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    all(&[
      A::check(b, off + core::mem::offset_of!(Self, 0), reads, target),
      B::check(b, off + core::mem::offset_of!(Self, 1), reads, target),
      C::check(b, off + core::mem::offset_of!(Self, 2), reads, target),
      D::check(b, off + core::mem::offset_of!(Self, 3), reads, target),
    ])
  }
}

/// The byte values that the enumeration of the values of a type of `size` bytes takes for each
/// byte: all of them up to 2 bytes, else the bounds of the ranges of the types of the model.
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

/// Every pattern of `size` bytes over the alphabet.
#[must_use]
pub fn patterns(size: usize) -> Vec<Vec<Option<u8>>> {
  patterns_over(size, size)
}

/// Every pattern of `size` bytes over the alphabet of values of `of` bytes.
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

/// The values of `T` (as a source: what safe code may hold), over the alphabet: the patterns that
/// are valid and that are uninitialized wherever they are not looked at (padding).
#[must_use]
pub fn values<T: Oracle>() -> Vec<Vec<Option<u8>>> {
  values_over::<T>(size_of::<T>())
}

/// The values of `T` over the alphabet of values of `of` bytes.
#[must_use]
pub fn values_over<T: Oracle>(of: usize) -> Vec<Vec<Option<u8>>> {
  let size = size_of::<T>();
  patterns_over(size, of)
    .into_iter()
    .filter(|p| {
      let mut reads = vec![false; size];
      T::check(p, 0, &mut reads, false) == Ok(true) && (0..size).all(|i| reads[i] || p[i].is_none())
    })
    .collect()
}

/// A `T` made of `bytes`, which the oracle says are a valid `T`.
#[must_use]
pub fn make<T: Oracle>(bytes: &Bytes) -> T {
  let mut value = MaybeUninit::<T>::uninit();
  let ptr = value.as_mut_ptr().cast::<u8>();
  for (i, byte) in bytes.iter().enumerate() {
    if let Some(byte) = byte {
      // SAFETY: in bounds.
      unsafe { ptr.add(i).write(*byte) };
    }
  }
  // SAFETY: the oracle says that these bytes are a `T`. If it is wrong, Miri says so.
  unsafe { value.assume_init() }
}

/// The bytes of a value, as the oracle sees them.
pub fn bytes_of<T: Oracle>(value: &T) -> Vec<Option<u8>> {
  let size = size_of::<T>();
  let mut reads = vec![false; size];
  // Which bytes are looked at depends on the value: find them with the bytes of the value,
  // reading only what the oracle reads (the others may be padding).
  let ptr = std::ptr::from_ref::<T>(value).cast::<u8>();
  let mut known: Vec<Option<u8>> = vec![None; size];
  loop {
    let before = reads.clone();
    let _ = T::check(&known, 0, &mut reads, false);
    for i in 0..size {
      if reads[i] {
        // SAFETY: a byte that the oracle reads is not padding.
        known[i] = Some(unsafe { ptr.add(i).read() });
      }
    }
    if reads == before {
      return known;
    }
  }
}

/// What the library did wrong, or did not do as well as it could.
#[derive(Default)]
pub struct Report {
  pub pairs: usize,
  /// Pairs whose parts (`head`, `tail` and their `_ref` and `_mut` forms) were checked.
  pub parts: usize,
  pub values: usize,
  pub conversions: usize,
  /// Soundness: a verdict that lets an invalid value through.
  pub unsound: Vec<String>,
  /// A run-time check that rejects a valid value, or a verdict that contradicts the truth in a
  /// way that is not unsound.
  pub incorrect: Vec<String>,
  /// Valid conversions rejected, or demoted to run-time checks, by cause.
  pub imprecise: BTreeMap<String, Vec<String>>,
  /// How the library classified the pairs.
  pub classes: BTreeMap<String, usize>,
  /// View enums checked (`views::check_view`), and how many of their variants have a base.
  pub views: usize,
  pub view_bases: usize,
}

pub static REPORT: Mutex<Option<Report>> = Mutex::new(None);

fn record(f: impl FnOnce(&mut Report)) {
  let mut report = REPORT.lock().unwrap();
  f(report.get_or_insert_with(Report::default));
}

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

pub(crate) fn name<T: ?Sized>() -> String {
  std::any::type_name::<T>()
    .replace("puns_model::", "")
    .replace("puns::num::", "")
    .replace("core::num::nonzero::", "")
    .replace("core::option::", "")
    .replace("core::mem::manually_drop::", "")
    .replace("core::marker::", "")
}

/// What a pattern is for a type: valid for what the type system guarantees (`hard`), and for what
/// it requires on top (`soft`: `valid(..)`, rules).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Truth {
  hard: Valid,
  soft: Valid,
}

fn truth<D: Oracle>(p: &Bytes) -> Truth {
  truth_at::<D>(p, 0)
}

/// What `p[off..off + size_of::<D>()]` is for `D`.
fn truth_at<D: Oracle>(p: &Bytes, off: usize) -> Truth {
  let p = &p[off..off + size_of::<D>()];
  let mut reads = vec![false; p.len()];
  let hard = D::check(p, 0, &mut reads, false);
  let soft = D::check(p, 0, &mut reads, true);
  Truth { hard, soft }
}

/// Compare what the library says of `S -> D` with the truth. `limit` bounds the number of values
/// of the source that are converted at run time (all of them with `usize::MAX`).
///
/// A requirement of the target (`valid(..)`, a rule) that the library does not check is not
/// unsound, and it is by design when the source has the same requirement: it is then only
/// counted. Anything that lets a value through that the type system does not allow is unsound.
///
/// # Panics
/// If a conversion that the verdict says compiles does not, or if the report is poisoned.
pub fn check_pair<S: Oracle, D: Oracle>(limit: usize) {
  let v: Verdict = const { hook::verdict::<S, D>() };
  let pair = format!("{} -> {}", name::<S>(), name::<D>());
  let (ss, ds) = (size_of::<S>(), size_of::<D>());
  let source = values::<S>();
  let truths: Vec<Truth> =
    if ds <= ss { source.iter().map(|p| truth::<D>(p)).collect() } else { vec![] };
  // Whatever the requirements of the target that a value does not meet are implied by those of
  // the source.
  let claims: Vec<bool> = if ds <= ss { implied::<S, D>(&source, 0) } else { vec![] };
  let any_uninit = truths.iter().any(|t| t.hard.is_err());
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
    if v.try_pun && !any_uninit && !soft_uninit {
      for ((p, t), claim) in source.iter().zip(&truths).zip(&claims).take(limit) {
        let value = make::<S>(p);
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
          let got = bytes_of(&d);
          let want: Vec<Option<u8>> =
            (0..ds).map(|i| if got[i].is_some() { p[i] } else { None }).collect();
          if got != want {
            unsound.push(format!("{pair}: {} became {}", show(p), show(&got)));
          }
        }
        for (what, accepted) in [("try_pun", Some(accepted)), ("try_pun_ref", result_ref)] {
          let Some(accepted) = accepted else { continue };
          match (accepted, t.hard, t.soft) {
            (true, Ok(true), Ok(true)) | (false, _, Ok(false) | Err(_)) | (false, Ok(false), _) => {
            }
            (true, Ok(true), _) if *claim => {}
            (true, Ok(true), _) => incorrect.push(format!(
              "{pair}: {what} accepts {} ({t:?}), a requirement is not checked",
              show(p)
            )),
            (true, _, _) => unsound.push(format!("{pair}: {what} accepts {} ({t:?})", show(p))),
            (false, _, _) => {
              incorrect.push(format!("{pair}: {what} rejects the valid {} ({t:?})", show(p)));
            }
          }
        }
        if unsound.len() + incorrect.len() > 3 {
          break;
        }
      }
    }
    // (d) a `&mut` needs the inclusion both ways.
    if ss == ds && (v.pun_mut || v.try_pun_mut) {
      for q in values::<D>() {
        let mut reads = vec![false; ss];
        let back = S::check(&q, 0, &mut reads, false);
        if back != Ok(true) {
          unsound
            .push(format!("{pair}: &mut, and the target {} is not a source ({back:?})", show(&q)));
          break;
        }
      }
    }
    // Truncation: the first bytes of the source, unchanged.
    if ds < ss && v.head {
      for p in source.iter().take(limit) {
        let d = hook::head::<S, D>(make::<S>(p)).expect("head compiles");
        let got = bytes_of(&d);
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

/// What may go wrong when anything is written through a `&mut D` to the bytes `off..` of a
/// source: a source that is not valid any more (`hard`), or one that met its requirements and
/// does not any more although what was written meets those of `D` (`soft`: a rule or a
/// `valid(..)` of the source that depends on the bytes), with an example of each.
struct Writes {
  hard: Option<String>,
  soft: Option<String>,
}

/// Write every value of `D` (over the alphabet of the source) into the bytes `off..` of every
/// value of `S` (what is outside of those bytes, once each), and judge the result with the
/// oracle of `S`.
fn writes<S: Oracle, D: Oracle>(source: &[Vec<Option<u8>>], off: usize) -> Writes {
  let (ss, ds) = (size_of::<S>(), size_of::<D>());
  // What a source is outside of the bytes, and whatever one of those sources meets its
  // requirements.
  let mut outsides: BTreeMap<Vec<Option<u8>>, bool> = BTreeMap::new();
  for p in source {
    let mut outside = p.clone();
    outside[off..off + ds].fill(None);
    *outsides.entry(outside).or_default() |= truth::<S>(p).soft == Ok(true);
  }
  let (mut hard, mut soft) = (None, None);
  // With whatever the target meets its own requirements: one that does not breaks those of the
  // source as surely as a `&mut` to the source itself does.
  let targets: Vec<(Vec<Option<u8>>, bool)> = values_over::<D>(ss)
    .into_iter()
    .map(|q| (q.clone(), truth::<D>(&q).soft == Ok(true)))
    .collect();
  for (outside, requirements) in &outsides {
    for (q, met) in &targets {
      let mut r = outside.clone();
      r[off..off + ds].copy_from_slice(q);
      let mut reads = vec![false; ss];
      let valid = S::check(&r, 0, &mut reads, false);
      if valid != Ok(true) && hard.is_none() {
        hard = Some(format!("{} written at {off} leaves {} ({valid:?})", show(q), show(&r)));
      }
      let required = S::check(&r, 0, &mut reads, true);
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

/// The offset of `part` in `whole`.
fn offset_in<S, D: ?Sized>(whole: &S, part: &D) -> usize {
  std::ptr::from_ref(part).cast::<u8>().addr() - std::ptr::from_ref(whole).cast::<u8>().addr()
}

/// Compare what the library says of the parts of `S` as a `D` with the truth.
///
/// The parts are its first bytes (`head`, `head_ref`, `head_mut`) and its last (`tail`,
/// `tail_ref`, `tail_mut`). `limit` bounds the number of values of the source that are converted
/// at run time.
///
/// A part is proven if every source holds a valid `D` there. A `&mut` to it is sound if every
/// `D` written there leaves a valid source, and, on top of what `pun_mut` requires, if it leaves
/// a source that met its requirements (rules, `valid(..)`) meeting them: nothing of the source
/// outside the part may depend on it.
///
/// # Panics
/// If a conversion that the verdict says compiles does not, or if the report is poisoned.
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
    let truths: Vec<Truth> = source.iter().map(|p| truth_at::<D>(p, off)).collect();
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
    // At run time: the very bytes, at the very place.
    let want = |p: &Bytes, got: &[Option<u8>]| -> Vec<Option<u8>> {
      (0..ds).map(|i| if got[i].is_some() { p[off + i] } else { None }).collect()
    };
    for p in source.iter().take(limit) {
      let value = make::<S>(p);
      let got = if part == "head" { hook::head::<S, D>(value) } else { hook::tail::<S, D>(value) };
      if let Some(d) = got {
        conversions += 1;
        let got = bytes_of(&d);
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
        let (at, got) = (offset_in(&value, d), bytes_of(d));
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
    report.conversions += conversions;
    report.unsound.extend(unsound);
    report.incorrect.extend(incorrect);
    for (cause, pair) in imprecise {
      report.imprecise.entry(cause).or_default().push(pair);
    }
  });
}

/// Print the report, and fail if anything is unsound or incorrect.
///
/// # Panics
/// If anything is unsound or incorrect.
pub fn finish(title: &str) {
  let report = REPORT.lock().unwrap().take().unwrap_or_default();
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
  assert!(
    report.unsound.is_empty() && report.incorrect.is_empty(),
    "{title}: {} unsound, {} incorrect",
    report.unsound.len(),
    report.incorrect.len()
  );
}

/// Every ordered pair of the types, through `check_pair`.
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

/// The given pairs, through `check_pair` or `check_part` (`$check`): a selection rather than a
/// product, for Miri.
#[macro_export]
macro_rules! pairs {
  ($check:ident, $limit:expr; $(($s:ty, $d:ty)),+ $(,)?) => {
    $( $crate::$check::<$s, $d>($limit); )+
  };
}

/// Every pair of a source of the first list and a target of the second, through `check_part`:
/// for targets smaller than their sources.
#[macro_export]
macro_rules! part_pairs {
  ($limit:expr; [$($s:ty),+ $(,)?]; $targets:tt) => {
    $( $crate::part_pairs!(@inner $limit; $s; $targets); )+
  };
  (@inner $limit:expr; $s:ty; [$($d:ty),+ $(,)?]) => {
    $( $crate::check_part::<$s, $d>($limit); )+
  };
}
