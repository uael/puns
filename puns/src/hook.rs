//! The verdicts of the analysis, for the model check of `puns-model`.
//!
//! Each function of this module makes the same checks as the public function of the same name.
//! Where the public function does not compile, this one returns `None` instead.
//!
//! This module is not part of the API. It is compiled only with the `verdict` feature, which only
//! tests may enable, and which changes nothing else.

use crate::{Error, MESSAGE_LEN, Need, Plan, Pun, Root, pun_unchecked, validate::validate};

/// Whether each of these public functions compiles for a pun from `S` to `D`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "one for each function of the API")]
pub struct Verdict {
  /// Whether [`pun`](crate::pun) compiles.
  pub pun: bool,
  /// Whether [`head`](crate::head) compiles.
  pub head: bool,
  /// Whether [`try_pun`](crate::try_pun) compiles.
  pub try_pun: bool,
  /// Whether [`pun_ref`](crate::pun_ref) compiles.
  pub pun_ref: bool,
  /// Whether [`head_ref`](crate::head_ref) compiles.
  pub head_ref: bool,
  /// Whether [`try_pun_ref`](crate::try_pun_ref) compiles.
  pub try_pun_ref: bool,
  /// Whether [`pun_mut`](crate::pun_mut) compiles.
  pub pun_mut: bool,
  /// Whether [`try_pun_mut`](crate::try_pun_mut) compiles.
  pub try_pun_mut: bool,
  /// The message of the compile error of `try_pun_ref`, and its length. The length is 0 if
  /// `try_pun_ref` compiles.
  why: ([u8; MESSAGE_LEN], usize),
}

impl Verdict {
  /// Returns the message of the compile error of `try_pun_ref`, or `""` if it compiles.
  ///
  /// The message is cut to `MESSAGE_LEN` bytes. Returns `"(not UTF-8)"` if the cut is not UTF-8.
  #[must_use]
  pub fn why(&self) -> &str {
    core::str::from_utf8(self.why.0.split_at(self.why.1).0).unwrap_or("(not UTF-8)")
  }

  /// Returns whether `try_pun_ref` does not compile because the pun depends on a layout that Rust
  /// leaves to the compiler.
  ///
  /// This happens only without the `unstable` feature, by design.
  #[must_use]
  pub fn unspecified(&self) -> bool {
    self.why().contains(crate::UNSPECIFIED)
  }
}

/// Returns whether the plan of a pun from `S` to `D` accepts the `need`.
const fn ok<S: ?Sized + Pun, D: ?Sized + Pun>(need: Need) -> bool {
  Plan::<S, D>::check(need).is_none()
}

/// Returns which of the public functions of [`Verdict`] compile for a pun from `S` to `D`.
#[must_use]
pub const fn verdict<S: ?Sized + Pun, D: ?Sized + Pun>() -> Verdict {
  let mutable = Plan::<S, D>::MUTABLE;
  let why = match Plan::<S, D>::check(Need::TRY_REF) {
    None => ([0; MESSAGE_LEN], 0),
    Some(message) => (message.bytes, message.len),
  };
  Verdict {
    pun: ok::<S, D>(Need::VALUE),
    head: ok::<S, D>(Need::PART),
    try_pun: ok::<S, D>(Need::TRY),
    pun_ref: ok::<S, D>(Need::REF),
    head_ref: ok::<S, D>(Need::PART_REF),
    try_pun_ref: ok::<S, D>(Need::TRY_REF),
    pun_mut: ok::<S, D>(Need::REF) && mutable,
    try_pun_mut: ok::<S, D>(Need::TRY_REF) && mutable,
    why,
  }
}

/// Whether each public function that takes a part of a value compiles for a pun from `S` to `D`.
///
/// It is apart from [`Verdict`], so that a test of whole values does not run the analyses of the
/// end of the source and of a `&mut` to a part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "one for each function of the API")]
pub struct Parts {
  /// Whether [`head`](crate::head) compiles.
  pub head: bool,
  /// Whether [`head_ref`](crate::head_ref) compiles.
  pub head_ref: bool,
  /// Whether [`head_mut`](crate::head_mut) compiles.
  pub head_mut: bool,
  /// Whether [`tail`](crate::tail) compiles.
  pub tail: bool,
  /// Whether [`tail_ref`](crate::tail_ref) compiles.
  pub tail_ref: bool,
  /// Whether [`tail_mut`](crate::tail_mut) compiles.
  pub tail_mut: bool,
}

/// Returns which public functions that take a part of a value compile for a pun from `S` to `D`.
#[must_use]
pub const fn parts<S: ?Sized + Pun, D: ?Sized + Pun>() -> Parts {
  Parts {
    head: ok::<S, D>(Need::PART),
    head_ref: ok::<S, D>(Need::PART_REF),
    head_mut: Plan::<S, D>::check_head_mut().is_none(),
    tail: Plan::<S, D>::check_tail(Need::PART).is_none(),
    tail_ref: Plan::<S, D>::check_tail(Need::PART_REF).is_none(),
    tail_mut: Plan::<S, D>::check_tail_mut().is_none(),
  }
}

/// Returns what [`try_pun`](crate::try_pun) returns, or `None` if it does not compile.
#[must_use]
pub const fn try_pun<S: Pun + Copy, D: Pun + Copy>(value: S) -> Option<Result<D, Error>> {
  if const { !verdict::<S, D>().try_pun } {
    return None;
  }
  // SAFETY: `verdict::<S, D>().try_pun` is `true`, so `check(Need::TRY)` is `None`, and the
  // analysis meets `Need::TRY`, as when `Plan::<S, D>::TRY` compiles. The rest is the proof of the
  // same call in `crate::try_pun`.
  if !unsafe { validate::<Plan<S, D>, Root, D>((&raw const value).cast()) } {
    return Some(Err(Error::Validity));
  }
  // SAFETY: `validate` returned `true`. The rest is the proof of the same call in `crate::try_pun`.
  Some(Ok(unsafe { pun_unchecked(value) }))
}

/// Returns what [`pun`](crate::pun) returns, or `None` if it does not compile.
#[must_use]
pub const fn pun<S: Pun + Copy, D: Pun + Copy>(value: S) -> Option<D> {
  if const { !verdict::<S, D>().pun } {
    return None;
  }
  // SAFETY: `verdict::<S, D>().pun` is `true`, so `check(Need::VALUE)` is `None`, and the analysis
  // meets `Need::VALUE`, as when `Plan::<S, D>::BY_VALUE` compiles. The rest is the proof of the
  // same call in `crate::pun`.
  Some(unsafe { pun_unchecked(value) })
}

/// Returns what [`head`](crate::head) returns, or `None` if it does not compile.
#[must_use]
pub const fn head<S: Pun + Copy, D: Pun + Copy>(value: S) -> Option<D> {
  if const { !verdict::<S, D>().head } {
    return None;
  }
  // SAFETY: `verdict::<S, D>().head` is `true`, so `check(Need::PART)` is `None`, and the analysis
  // meets `Need::PART`, as when `Plan::<S, D>::HEAD` compiles. The rest is the proof of the same
  // call in `crate::head`.
  Some(unsafe { pun_unchecked(value) })
}

/// Returns what [`tail`](crate::tail) returns, or `None` if it does not compile.
#[must_use]
pub const fn tail<S: Pun + Copy, D: Pun + Copy>(value: S) -> Option<D> {
  if const { !parts::<S, D>().tail } {
    return None;
  }
  let shift = const { Plan::<S, D>::TAIL_ANALYSIS.shift };
  // SAFETY: `parts::<S, D>().tail` is `true`, so `check_tail(Need::PART)` is `None`, and the
  // analysis of the last bytes meets `Need::PART`, as when `Plan::<S, D>::TAIL` compiles. The rest
  // is the proof of the same call in `crate::tail`.
  Some(unsafe { (&raw const value).cast::<u8>().add(shift).cast::<D>().read_unaligned() })
}

/// Returns what [`tail_ref`](crate::tail_ref) returns, or `None` if it does not compile.
#[must_use]
pub const fn tail_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> Option<&D> {
  if const { !parts::<S, D>().tail_ref } {
    return None;
  }
  // SAFETY: `parts::<S, D>().tail_ref` is `true`, so `check_tail(Need::PART_REF)` is `None`, and
  // the analysis of the last bytes meets `Need::PART_REF`, as when `Plan::<S, D>::TAIL_REF`
  // compiles. The rest is the proof of the same call in `crate::tail_ref`.
  Some(unsafe { &*Plan::<S, D>::tail_cast(value) })
}

/// Returns what [`try_pun_ref`](crate::try_pun_ref) returns, or `None` if it does not compile.
#[must_use]
pub fn try_pun_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> Option<Result<&D, Error>> {
  if const { !verdict::<S, D>().try_pun_ref } {
    return None;
  }
  // SAFETY: `verdict::<S, D>().try_pun_ref` is `true`, so `check(Need::TRY_REF)` is `None`, and the
  // analysis meets `Need::TRY_REF`, as when `Plan::<S, D>::TRY_REF` compiles. The rest is the proof
  // of the same call in `crate::try_pun_ref`.
  Some(unsafe { Plan::<S, D>::try_cast(value) }.map(|target| unsafe { &*target }))
}

/// Returns what [`try_pun_mut`](crate::try_pun_mut) returns, or `None` if it does not compile.
#[must_use]
pub fn try_pun_mut<S: ?Sized + Pun, D: ?Sized + Pun>(
  value: &mut S,
) -> Option<Result<&mut D, Error>> {
  if const { !verdict::<S, D>().try_pun_mut } {
    return None;
  }
  let source: *mut S = value;
  // SAFETY: `verdict::<S, D>().try_pun_mut` is `true`, so `check(Need::TRY_REF)` is `None` and
  // `Plan::MUTABLE` is `true`, which is the condition under which `Plan::<S, D>::TRY_MUT` compiles.
  // The rest is the proof of the same call in `crate::try_pun_mut`.
  Some(unsafe { Plan::<S, D>::try_cast(source) }.map(|target| unsafe { &mut *target.cast_mut() }))
}

/// Returns what [`pun_ref`](crate::pun_ref) returns, or `None` if it does not compile.
#[must_use]
pub const fn pun_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> Option<&D> {
  if const { !verdict::<S, D>().pun_ref } {
    return None;
  }
  // SAFETY: `verdict::<S, D>().pun_ref` is `true`, so `check(Need::REF)` is `None`, and the
  // analysis meets `Need::REF`, as when `Plan::<S, D>::BY_REF` compiles. The rest is the proof of
  // the same call in `crate::pun_ref`.
  Some(unsafe { &*Plan::<S, D>::cast(value) })
}

/// Returns what [`head_ref`](crate::head_ref) returns, or `None` if it does not compile.
#[must_use]
pub const fn head_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> Option<&D> {
  if const { !verdict::<S, D>().head_ref } {
    return None;
  }
  // SAFETY: `verdict::<S, D>().head_ref` is `true`, so `check(Need::PART_REF)` is `None`, and the
  // analysis meets `Need::PART_REF`, as when `Plan::<S, D>::HEAD_REF` compiles. The rest is the
  // proof of the same call in `crate::head_ref`.
  Some(unsafe { &*Plan::<S, D>::cast(value) })
}

/// Returns what [`head_mut`](crate::head_mut) returns, or `None` if it does not compile.
#[must_use]
pub const fn head_mut<S: ?Sized + Pun, D: ?Sized + Pun>(value: &mut S) -> Option<&mut D> {
  if const { !parts::<S, D>().head_mut } {
    return None;
  }
  let source: *mut S = value;
  // SAFETY: `parts::<S, D>().head_mut` is `true`, so `check_head_mut()` is `None`, which is the
  // condition under which `Plan::<S, D>::HEAD_MUT` compiles. The rest is the proof of the same call
  // in `crate::head_mut`.
  Some(unsafe { &mut *Plan::<S, D>::cast(source).cast_mut() })
}

/// Returns what [`tail_mut`](crate::tail_mut) returns, or `None` if it does not compile.
#[must_use]
pub const fn tail_mut<S: ?Sized + Pun, D: ?Sized + Pun>(value: &mut S) -> Option<&mut D> {
  if const { !parts::<S, D>().tail_mut } {
    return None;
  }
  let source: *mut S = value;
  // SAFETY: `parts::<S, D>().tail_mut` is `true`, so `check_tail_mut()` is `None`, which is the
  // condition under which `Plan::<S, D>::TAIL_MUT` compiles. The rest is the proof of the same call
  // in `crate::tail_mut`.
  Some(unsafe { &mut *Plan::<S, D>::tail_cast(source).cast_mut() })
}

/// Returns what [`from_head`](crate::from_head) returns, or `None` if it does not compile.
#[must_use]
pub const fn from_head<D: ?Sized + Pun>(bytes: &[u8]) -> Option<Result<(&D, &[u8]), Error>> {
  if const { !verdict::<[u8], D>().try_pun_ref } {
    return None;
  }
  // SAFETY: `bytes` is valid for reads of `bytes.len()` initialized bytes, which is what
  // `Plan::front` requires.
  Some(match unsafe { Plan::<[u8], D>::front(bytes.as_ptr(), bytes.len()) } {
    // SAFETY: `verdict::<[u8], D>().try_pun_ref` is `true`, so the analysis of bytes as a `D` meets
    // `Need::TRY_REF`, as when `Plan::<[u8], D>::TRY_REF` compiles. The rest is the proof of the
    // same call in `crate::from_head`.
    Ok((target, size)) => Ok((unsafe { &*target }, bytes.split_at(size).1)),
    Err(error) => Err(error),
  })
}

/// Returns what [`from_tail`](crate::from_tail) returns, or `None` if it does not compile.
#[must_use]
pub const fn from_tail<D: ?Sized + Pun>(bytes: &[u8]) -> Option<Result<(&[u8], &D), Error>> {
  if const { Plan::<[u8], D>::check_from_tail().is_some() } {
    return None;
  }
  // SAFETY: `bytes` is valid for reads of `bytes.len()` initialized bytes. `check_from_tail()` is
  // `None`, and it refuses an unsized `D`. These are the requirements of `Plan::back`.
  Some(match unsafe { Plan::<[u8], D>::back(bytes.as_ptr(), bytes.len()) } {
    // SAFETY: `check_from_tail()` is `None`, which is the condition under which
    // `Plan::<[u8], D>::FROM_TAIL` compiles. The rest is the proof of the same call in
    // `crate::from_tail`.
    Ok((start, target)) => Ok((bytes.split_at(start).0, unsafe { &*target })),
    Err(error) => Err(error),
  })
}
