//! Test-only access to the verdicts of the analysis, for the model check of
//! `puns-model`: the same checks as the public functions, but answering `None` instead of
//! failing to compile.
//!
//! Not part of the API: only compiled with the `verdict` feature, which nothing but tests may
//! enable. It changes nothing else.

use crate::{Error, Need, Plan, Pun, Root, pun_unchecked, validate::validate};

/// What each public function would do with a pun from `S` to `D`: whatever it
/// compiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "one for each function of the API")]
pub struct Verdict {
  pub pun: bool,
  pub head: bool,
  pub try_pun: bool,
  pub pun_ref: bool,
  pub head_ref: bool,
  pub try_pun_ref: bool,
  pub pun_mut: bool,
  pub try_pun_mut: bool,
  /// Why `try_pun_ref` does not compile, if it does not.
  why: ([u8; 512], usize),
}

impl Verdict {
  /// Why `try_pun_ref` does not compile, if it does not (empty if it does).
  #[must_use]
  pub fn why(&self) -> &str {
    core::str::from_utf8(self.why.0.split_at(self.why.1).0).unwrap_or("(not UTF-8)")
  }

  /// Whatever `try_pun_ref` does not compile because the pun depends on what Rust does not keep
  /// of a layout: without the `unstable` feature, by design.
  #[must_use]
  pub fn unspecified(&self) -> bool {
    self.why().contains(crate::UNSPECIFIED)
  }
}

/// Whatever the pun from `S` to `D` meets the `need`.
const fn ok<S: ?Sized + Pun, D: ?Sized + Pun>(need: Need) -> bool {
  Plan::<S, D>::check(need).is_none()
}

/// What the analysis says of a pun from `S` to `D`.
#[must_use]
pub const fn verdict<S: ?Sized + Pun, D: ?Sized + Pun>() -> Verdict {
  let mutable = Plan::<S, D>::MUTABLE;
  let why = match Plan::<S, D>::check(Need::TRY_REF) {
    None => ([0; 512], 0),
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

/// Whatever [`view_ref`](crate::view_ref) compiles for `S` and `D`.
///
/// This is its plan, and what the pointers of `D` aim at, followed `fuel` times. The function
/// itself follows them through instances of functions, without a bound: `fuel` must be the
/// number of pairs of types that can be met for the answer to be the same.
#[must_use]
pub const fn views<S: ?Sized + Pun, D: ?Sized + Pun>(fuel: usize) -> bool {
  ok::<S, D>(Need::VIEW) && crate::deep::local::<S, D>(fuel).is_none()
}

/// Whatever [`try_view_ref`](crate::try_view_ref) compiles for `S` and `D`, as [`views`].
#[must_use]
pub const fn tries_view<S: ?Sized + Pun, D: ?Sized + Pun>(fuel: usize) -> bool {
  ok::<S, D>(Need::TRY_VIEW) && crate::deep::local::<S, D>(fuel).is_none()
}

/// [`view_ref`](crate::view_ref), if it compiles (see [`views`] for `FUEL`).
#[must_use]
pub fn view_ref<S: ?Sized + Pun, D: ?Sized + Pun, const FUEL: usize>(value: &S) -> Option<&D> {
  if const { !views::<S, D>(FUEL) } {
    return None;
  }
  // SAFETY: as in `view_ref`, which would compile: `FUEL` follows the pointers as far as the
  // types of the model go.
  Some(unsafe { &*Plan::<S, D>::cast(value) })
}

/// [`try_view_ref`](crate::try_view_ref), if it compiles (see [`views`] for `FUEL`).
#[must_use]
pub fn try_view_ref<S: ?Sized + Pun, D: ?Sized + Pun, const FUEL: usize>(
  value: &S,
) -> Option<Result<&D, Error>> {
  if const { !tries_view::<S, D>(FUEL) } {
    return None;
  }
  // SAFETY: as in `try_view_ref`, which would compile.
  Some(Plan::<S, D>::try_cast(value).map(|target| unsafe { &*target }))
}

/// What each function that takes a part of a value would do with a pun from `S` to `D`:
/// whatever it compiles.
///
/// Apart from [`Verdict`], so that what only looks at the whole does not pay for the analyses of
/// the end of the source and of the `&mut` to a part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(clippy::struct_excessive_bools, reason = "one for each function of the API")]
pub struct Parts {
  pub head: bool,
  pub head_ref: bool,
  pub head_mut: bool,
  pub tail: bool,
  pub tail_ref: bool,
  pub tail_mut: bool,
}

/// What the analysis says of the parts of `S` as a `D`.
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

/// [`try_pun`](crate::try_pun), if it compiles.
#[must_use]
pub const fn try_pun<S: Pun + Copy, D: Pun + Copy>(value: S) -> Option<Result<D, Error>> {
  if const { !verdict::<S, D>().try_pun } {
    return None;
  }
  // SAFETY: as in `try_pun`, which would compile.
  if !unsafe { validate::<Plan<S, D>, Root, D>((&raw const value).cast()) } {
    return Some(Err(Error::Validity));
  }
  // SAFETY: just validated.
  Some(Ok(unsafe { pun_unchecked(value) }))
}

/// [`pun`](crate::pun), if it compiles.
#[must_use]
pub const fn pun<S: Pun + Copy, D: Pun + Copy>(value: S) -> Option<D> {
  if const { !verdict::<S, D>().pun } {
    return None;
  }
  // SAFETY: as in `pun`, which would compile.
  Some(unsafe { pun_unchecked(value) })
}

/// [`head`](crate::head), if it compiles.
#[must_use]
pub const fn head<S: Pun + Copy, D: Pun + Copy>(value: S) -> Option<D> {
  if const { !verdict::<S, D>().head } {
    return None;
  }
  // SAFETY: as in `head`, which would compile.
  Some(unsafe { pun_unchecked(value) })
}

/// [`tail`](crate::tail), if it compiles.
#[must_use]
pub const fn tail<S: Pun + Copy, D: Pun + Copy>(value: S) -> Option<D> {
  if const { !parts::<S, D>().tail } {
    return None;
  }
  let shift = const { Plan::<S, D>::TAIL_ANALYSIS.shift };
  // SAFETY: as in `tail`, which would compile.
  Some(unsafe { (&raw const value).cast::<u8>().add(shift).cast::<D>().read_unaligned() })
}

/// [`tail_ref`](crate::tail_ref), if it compiles.
#[must_use]
pub const fn tail_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> Option<&D> {
  if const { !parts::<S, D>().tail_ref } {
    return None;
  }
  // SAFETY: as in `tail_ref`, which would compile.
  Some(unsafe { &*Plan::<S, D>::tail_cast(value) })
}

/// [`try_pun_ref`](crate::try_pun_ref), if it compiles.
#[must_use]
pub fn try_pun_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> Option<Result<&D, Error>> {
  if const { !verdict::<S, D>().try_pun_ref } {
    return None;
  }
  // SAFETY: as in `try_pun_ref`, which would compile.
  Some(Plan::<S, D>::try_cast(value).map(|target| unsafe { &*target }))
}

/// [`try_pun_mut`](crate::try_pun_mut), if it compiles.
#[must_use]
pub fn try_pun_mut<S: ?Sized + Pun, D: ?Sized + Pun>(
  value: &mut S,
) -> Option<Result<&mut D, Error>> {
  if const { !verdict::<S, D>().try_pun_mut } {
    return None;
  }
  let source: *mut S = value;
  // SAFETY: as in `try_pun_mut`, which would compile.
  Some(Plan::<S, D>::try_cast(source).map(|target| unsafe { &mut *target.cast_mut() }))
}

/// [`pun_ref`](crate::pun_ref), if it compiles.
#[must_use]
pub const fn pun_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> Option<&D> {
  if const { !verdict::<S, D>().pun_ref } {
    return None;
  }
  // SAFETY: as in `pun_ref`, which would compile.
  Some(unsafe { &*Plan::<S, D>::cast(value) })
}

/// [`head_ref`](crate::head_ref), if it compiles.
#[must_use]
pub const fn head_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> Option<&D> {
  if const { !verdict::<S, D>().head_ref } {
    return None;
  }
  // SAFETY: as in `head_ref`, which would compile.
  Some(unsafe { &*Plan::<S, D>::cast(value) })
}

/// [`head_mut`](crate::head_mut), if it compiles.
#[must_use]
pub const fn head_mut<S: ?Sized + Pun, D: ?Sized + Pun>(value: &mut S) -> Option<&mut D> {
  if const { !parts::<S, D>().head_mut } {
    return None;
  }
  let source: *mut S = value;
  // SAFETY: as in `head_mut`, which would compile.
  Some(unsafe { &mut *Plan::<S, D>::cast(source).cast_mut() })
}

/// [`tail_mut`](crate::tail_mut), if it compiles.
#[must_use]
pub const fn tail_mut<S: ?Sized + Pun, D: ?Sized + Pun>(value: &mut S) -> Option<&mut D> {
  if const { !parts::<S, D>().tail_mut } {
    return None;
  }
  let source: *mut S = value;
  // SAFETY: as in `tail_mut`, which would compile.
  Some(unsafe { &mut *Plan::<S, D>::tail_cast(source).cast_mut() })
}

/// [`from_head`](crate::from_head), if it compiles.
#[must_use]
pub const fn from_head<D: ?Sized + Pun>(bytes: &[u8]) -> Option<Result<(&D, &[u8]), Error>> {
  if const { !verdict::<[u8], D>().try_pun_ref } {
    return None;
  }
  // SAFETY: bytes.
  Some(match unsafe { Plan::<[u8], D>::front(bytes.as_ptr(), bytes.len()) } {
    // SAFETY: as in `from_head`, which would compile.
    Ok((target, size)) => Ok((unsafe { &*target }, bytes.split_at(size).1)),
    Err(error) => Err(error),
  })
}

/// [`from_tail`](crate::from_tail), if it compiles.
#[must_use]
pub const fn from_tail<D: ?Sized + Pun>(bytes: &[u8]) -> Option<Result<(&[u8], &D), Error>> {
  if const { Plan::<[u8], D>::check_from_tail().is_some() } {
    return None;
  }
  // SAFETY: bytes.
  Some(match unsafe { Plan::<[u8], D>::back(bytes.as_ptr(), bytes.len()) } {
    // SAFETY: as in `from_tail`, which would compile.
    Ok((start, target)) => Ok((bytes.split_at(start).0, unsafe { &*target })),
    Err(error) => Err(error),
  })
}
