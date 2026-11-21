//! The verdicts of the analysis, for the model check of `puns-model`.
//!
//! Each function of this module makes the same checks as the public function of the same name.
//! Where the public function does not compile, this one returns `None` instead.
//!
//! This module is not part of the API. It is compiled only with the `verdict` feature, which only
//! tests may enable, and which changes nothing else.

use crate::{Error, Need, Plan, Pun, Root, pun_unchecked, validate::validate};

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
  why: ([u8; 512], usize),
}

impl Verdict {
  /// Returns the message of the compile error of `try_pun_ref`, or `""` if it compiles.
  ///
  /// The message is cut to 512 bytes. Returns `"(not UTF-8)"` if the cut is not UTF-8.
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

/// Returns whether [`view_ref`](crate::view_ref) compiles for `S` and `D`.
///
/// It checks the plan of `view_ref`, then what the pointers of `D` aim at, followed `fuel`
/// times. `view_ref` itself follows pointers through instances of functions, without a bound.
/// The answer is that of `view_ref` only if `fuel` is at least the number of pairs of types
/// that can be met.
#[must_use]
pub const fn views<S: ?Sized + Pun, D: ?Sized + Pun>(fuel: usize) -> bool {
  ok::<S, D>(Need::VIEW) && crate::deep::local::<S, D>(fuel).is_none()
}

/// Returns whether [`try_view_ref`](crate::try_view_ref) compiles for `S` and `D`.
///
/// `fuel` is as for [`views`].
#[must_use]
pub const fn tries_view<S: ?Sized + Pun, D: ?Sized + Pun>(fuel: usize) -> bool {
  ok::<S, D>(Need::TRY_VIEW) && crate::deep::local::<S, D>(fuel).is_none()
}

/// Returns what [`view_ref`](crate::view_ref) returns, or `None` if it does not compile.
///
/// `FUEL` is as for [`views`].
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `FUEL` is at least the number of pairs of types that can be met from `S` and `D`, so that
///   [`views`] is `true` only if `view_ref` compiles.
#[must_use]
pub unsafe fn view_ref<S: ?Sized + Pun, D: ?Sized + Pun, const FUEL: usize>(
  value: &S,
) -> Option<&D> {
  if const { !views::<S, D>(FUEL) } {
    return None;
  }
  // SAFETY: `views::<S, D>(FUEL)` is `true`, and `FUEL` is at least the number of pairs of types
  // that can be met, by contract on the caller. So `crate::view_ref::<S, D>` compiles: `Plan::<S,
  // D>::VIEW` and `deep::proof::<S, D>` do. As in `pun_ref` below, the pointer that `Plan::cast`
  // returns is not null, is aligned for `D`, and is in the allocation of `value`, and the bytes
  // there are a valid `D` when its pointers are taken as plain bytes. What the pointers of `D` aim
  // at is proven as well:
  // - The relation records no `pointer`, so each pointer of `D` is, in `S`, a pointer of the same
  //   scheme, offset and kind of aim, which may be null only if that of `D` may. It aims where the
  //   pointer of `S` aims (`SelfRelative`, section Safety).
  // - `deep::proof::<S, D>` compiles, so each value that a pointer of `S` aims at is a valid target
  //   of the pointer of `D` at the same place, and so on behind every pointer (`deep`, module
  //   documentation).
  // - What a pointer of `S` aims at is there and valid, and nothing writes to it, while a shared
  //   reference to the pointer lives (`SelfRelative`, section Safety).
  // The reference has the lifetime of the shared borrow `value`, and neither type holds an
  // `UnsafeCell` (`Pun`, section Safety).
  Some(unsafe { &*Plan::<S, D>::cast(value) })
}

/// Returns what [`try_view_ref`](crate::try_view_ref) returns, or `None` if it does not compile.
///
/// `FUEL` is as for [`views`].
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `FUEL` is at least the number of pairs of types that can be met from `S` and `D`, so that
///   [`tries_view`] is `true` only if `try_view_ref` compiles.
#[must_use]
pub unsafe fn try_view_ref<S: ?Sized + Pun, D: ?Sized + Pun, const FUEL: usize>(
  value: &S,
) -> Option<Result<&D, Error>> {
  if const { !tries_view::<S, D>(FUEL) } {
    return None;
  }
  // SAFETY: `tries_view::<S, D>(FUEL)` is `true`, and `FUEL` is at least the number of pairs of
  // types that can be met, by contract on the caller. So `crate::try_view_ref::<S, D>` compiles:
  // `Plan::<S, D>::TRY_VIEW` and `deep::proof::<S, D>` do. `value` is a reference, so it points to
  // a valid `S`, valid for reads, as `try_cast` requires. If `try_cast` returns `Ok`, `target` is
  // not null, is aligned for `D`, and is in the allocation of `value`, and the bytes there are a
  // valid `D` when its pointers are taken as plain bytes, as in `try_pun_ref` below. `validate`
  // checked that a pointer of `D` that may not be null is not null where that of `S` may be. What
  // the pointers aim at is proven as in `view_ref` above. The reference has the lifetime of the
  // shared borrow `value`.
  Some(unsafe { Plan::<S, D>::try_cast(value) }.map(|target| unsafe { &*target }))
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
  // analysis meets `Need::TRY`, as when `Plan::<S, D>::TRY` compiles. `S` and `D` are `Copy`, so
  // `Sized`, and the fit is `Exact::Always`: they have the same size. So `value` is valid for reads
  // of a `D`, which `validate` does not need aligned. The relation records no byte that may be
  // padding in `S` and that `D` requires to be initialized, or that a check marked by the analysis
  // reads. So every byte that `validate` reads is initialized.
  if !unsafe { validate::<Plan<S, D>, Root, D>((&raw const value).cast()) } {
    return Some(Err(Error::Validity));
  }
  // SAFETY: `S` and `D` have the same size. `validate` found every part of `D` that the analysis
  // marked to be valid in `value`. Every part that it did not mark is valid in every value of `S`,
  // since the relation records no disjoint part, no pointer, and nothing that Rust leaves to the
  // compiler. So `value` is a valid `D`, as `pun_unchecked` requires.
  Some(Ok(unsafe { pun_unchecked(value) }))
}

/// Returns what [`pun`](crate::pun) returns, or `None` if it does not compile.
#[must_use]
pub const fn pun<S: Pun + Copy, D: Pun + Copy>(value: S) -> Option<D> {
  if const { !verdict::<S, D>().pun } {
    return None;
  }
  // SAFETY: `verdict::<S, D>().pun` is `true`, so `check(Need::VALUE)` is `None`, and the analysis
  // meets `Need::VALUE`, as when `Plan::<S, D>::BY_VALUE` compiles. Its fit is `Exact::Always`: `S`
  // and `D` are `Copy`, so `Sized`, and `size_of::<S>() == size_of::<D>()`. Its relation records
  // nothing, so every value of `S` is a valid `D` (`Relation::infallible`). This is what
  // `pun_unchecked` requires.
  Some(unsafe { pun_unchecked(value) })
}

/// Returns what [`head`](crate::head) returns, or `None` if it does not compile.
#[must_use]
pub const fn head<S: Pun + Copy, D: Pun + Copy>(value: S) -> Option<D> {
  if const { !verdict::<S, D>().head } {
    return None;
  }
  // SAFETY: `verdict::<S, D>().head` is `true`, so `check(Need::PART)` is `None`, and the analysis
  // meets `Need::PART`, as when `Plan::<S, D>::HEAD` compiles. Its fit can shrink: `size_of::<S>()
  // >= size_of::<D>()`. Its relation, of the first `size_of::<D>()` bytes of `S`, records nothing.
  // So the first `size_of::<D>()` bytes of every value of `S` are a valid `D`. This is what
  // `pun_unchecked` requires.
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
  // analysis of the last bytes meets `Need::PART`, as when `Plan::<S, D>::TAIL` compiles. Both
  // types are sized, `size_of::<S>() >= size_of::<D>()`, and `shift` is `size_of::<S>() -
  // size_of::<D>()` (`analyze_tail`). The relation records nothing, so the `size_of::<D>()` bytes
  // of every value of `S` from `shift` are a valid `D`. So `add(shift)` stays within `value`, and
  // the pointer is valid for reads of `size_of::<D>()` bytes, which end where `value` ends, and
  // points to a valid `D`. These are the requirements of `add` and of `read_unaligned`, which needs
  // no alignment.
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
  // compiles. Both types are sized, and `shift` is `size_of::<S>() - size_of::<D>()`
  // (`analyze_tail`). `Plan::tail_cast` returns a pointer to the `size_of::<D>()` bytes of `value`
  // from `shift`, which end where `value` ends. The reference is valid:
  // - It is not null, because it points within `value`, a reference.
  // - It is aligned, because the analysis found `D` no more aligned than `S`, and `shift` a
  //   multiple of the alignment of `D` (`aligned`).
  // - It is not dangling, because it is within `value`.
  // - It points to a valid `D`, because the relation of the last bytes records nothing.
  // The reference has the lifetime of the shared borrow `value`, and neither type holds an
  // `UnsafeCell` (`Pun`, section Safety).
  Some(unsafe { &*Plan::<S, D>::tail_cast(value) })
}

/// Returns what [`try_pun_ref`](crate::try_pun_ref) returns, or `None` if it does not compile.
#[must_use]
pub fn try_pun_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> Option<Result<&D, Error>> {
  if const { !verdict::<S, D>().try_pun_ref } {
    return None;
  }
  // SAFETY: `verdict::<S, D>().try_pun_ref` is `true`, so `check(Need::TRY_REF)` is `None`, and the
  // analysis meets `Need::TRY_REF`, as when `Plan::<S, D>::TRY_REF` compiles. `value` is a
  // reference, so it points to a valid `S`, valid for reads, as `try_cast` requires. If `try_cast`
  // returns `Ok`, `target` is the address of `value`, with a tail length that gives it the extent
  // of `value`, and the reference made from it is valid:
  // - It is not null, because `value` is a reference at the same address.
  // - It is aligned. Either the analysis found `D` no more aligned than `S`, or `Plan::try_cast_by`
  //   checked the address.
  // - It is not dangling. Its extent is that of `value`. If `D` is unsized and more aligned than
  //   `S`, `Plan::try_cast_by` checked that its size is at most the size of `value`. Otherwise
  //   either `D` is sized, and its size is its extent, or `D` is unsized and its extent is rounded
  //   up to an alignment that divides that of `S`. Either way its size is at most the size of
  //   `value`.
  // - It points to a valid `D`. `validate` found every part that the analysis marked to be valid.
  //   Every other part is valid in every value of `S`, since the relation records no byte read from
  //   padding, no disjoint part, no pointer, and nothing that Rust leaves to the compiler.
  // The reference has the lifetime of the shared borrow `value`, and neither type holds an
  // `UnsafeCell` (`Pun`, section Safety).
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
  // `source` is made from a reference, so it points to a valid `S`, valid for reads, as `try_cast`
  // requires. If `try_cast` returns `Ok`, `target` is not null, is aligned for `D`, is within
  // `value`, and points to a valid `D`, as in `try_pun_ref` above. It is made from `source`, a
  // pointer that may write. The reference has the lifetime of the exclusive borrow `value`.
  // `Plan::MUTABLE` is `true`, so every value of `D`, padding included, is a valid `S`, and `value`
  // is a valid `S` when the borrow ends.
  Some(unsafe { Plan::<S, D>::try_cast(source) }.map(|target| unsafe { &mut *target.cast_mut() }))
}

/// Returns what [`pun_ref`](crate::pun_ref) returns, or `None` if it does not compile.
#[must_use]
pub const fn pun_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> Option<&D> {
  if const { !verdict::<S, D>().pun_ref } {
    return None;
  }
  // SAFETY: `verdict::<S, D>().pun_ref` is `true`, so `check(Need::REF)` is `None`, and the
  // analysis meets `Need::REF`, as when `Plan::<S, D>::BY_REF` compiles: the fit is
  // `Exact::Always`, `D` is no more aligned than `S`, and the relation is infallible. `Plan::cast`
  // returns a pointer to the address of `value`, with a tail length that gives the target the
  // extent of `value`. The reference made from it is valid:
  // - It is not null, because `value` is a reference at the same address.
  // - It is aligned, because `value` is aligned for `S`, `D` is no more aligned than `S`, and
  //   alignments are powers of two.
  // - It is not dangling. The target has the extent of `value` and no greater alignment, so its
  //   size is at most the size of `value`.
  // - It points to a valid `D`, because the relation is infallible: every value of `S` is a valid
  //   `D`.
  // The reference has the lifetime of the shared borrow `value`, and neither type holds an
  // `UnsafeCell` (`Pun`, section Safety).
  Some(unsafe { &*Plan::<S, D>::cast(value) })
}

/// Returns what [`head_ref`](crate::head_ref) returns, or `None` if it does not compile.
#[must_use]
pub const fn head_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> Option<&D> {
  if const { !verdict::<S, D>().head_ref } {
    return None;
  }
  // SAFETY: `verdict::<S, D>().head_ref` is `true`, so `check(Need::PART_REF)` is `None`, and the
  // analysis meets `Need::PART_REF`, as when `Plan::<S, D>::HEAD_REF` compiles. The fit can shrink:
  // the sized part of `S` is at least that of `D`. So `Plan::cast` returns a pointer to the address
  // of `value` whose extent is at most that of `value`. The reference is valid:
  // - It is not null, because `value` is a reference at the same address.
  // - It is aligned, because `D` is no more aligned than `S` (`aligned`).
  // - It is not dangling, because a smaller extent and no greater alignment make a size at most the
  //   size of `value`.
  // - It points to a valid `D`, because the relation of the bytes of `D` records nothing.
  // The reference has the lifetime of the shared borrow `value`, and neither type holds an
  // `UnsafeCell` (`Pun`, section Safety).
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
  // condition under which `Plan::<S, D>::HEAD_MUT` compiles: the analysis meets `Need::PART_REF`,
  // and `mutable_part` found nothing to refuse. As in `head_ref` above, the pointer that
  // `Plan::cast` returns is not null, is aligned for `D`, is within `value`, and points to a valid
  // `D`. It is made from `source`, a pointer that may write. The reference has the lifetime of the
  // exclusive borrow `value`, so nothing else reads or writes the bytes while it lives.
  // `mutable_part` found that neither type holds a self-relative pointer, that every value of `D`
  // is valid for what `S` has over its bytes, and that nothing of `S` outside them depends on them.
  // So `value` is a valid `S` when the borrow ends, whatever `D` is written through the reference.
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
  // condition under which `Plan::<S, D>::TAIL_MUT` compiles: the analysis of the last bytes meets
  // `Need::PART_REF`, and `mutable_part` found nothing to refuse at `shift`. As in `tail_ref`
  // above, the pointer that `Plan::tail_cast` returns is not null, is aligned for `D`, is within
  // `value`, and points to a valid `D`. It is made from `source`, a pointer that may write. The
  // reference has the lifetime of the exclusive borrow `value`. As in `head_mut` above, any `D`
  // written through it leaves `value` a valid `S`, by what `mutable_part` found of the bytes from
  // `shift`.
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
    // `Need::TRY_REF`, as when `Plan::<[u8], D>::TRY_REF` compiles. `Plan::front` returned `Ok`, so
    // `target` is a `D` at the address of `bytes`, whose size is `size`, at most `bytes.len()`. The
    // reference is valid:
    // - It is not null, because `bytes` is a reference at the same address.
    // - It is aligned. The address was checked if `D` is aligned to more than 1.
    // - It is not dangling, because it is within `bytes`.
    // - It points to a valid `D`. `validate` found every part that the analysis marked to be valid.
    //   The relation records no disjoint part, no pointer, and nothing that Rust leaves to the
    //   compiler, so every other part accepts any initialized bytes.
    // The reference has the lifetime of the shared borrow `bytes`, and `D` holds no `UnsafeCell`
    // (`Pun`, section Safety).
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
    // SAFETY: `check_from_tail()` is `None`, which is the condition under which `Plan::<[u8],
    // D>::FROM_TAIL` compiles. `Plan::back` returned `Ok`, so `target` is the sized `D` of the last
    // `size_of::<D>()` bytes of `bytes`. The reference is valid, as in `from_head` above: it is not
    // null, its address was checked if `D` is aligned to more than 1, it is within `bytes`, and
    // `validate` found every part that the analysis marked to be valid. It has the lifetime of the
    // shared borrow `bytes`, and `D` holds no `UnsafeCell` (`Pun`, section Safety).
    Ok((start, target)) => Ok((bytes.split_at(start).0, unsafe { &*target })),
    Err(error) => Err(error),
  })
}
