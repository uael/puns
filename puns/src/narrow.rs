//! The value ranges of rule expressions, computed at compile time.
//!
//! An expression means what it means on `u128`, but most of its values are known to be much
//! smaller. Examples are a field of two bytes, a sum of two such fields, a checksum of 16 bits, or
//! an offset in the value. On a 64-bit target a `u128` is two registers. Its addition carries from
//! one into the other (`adc`), its product is three multiplications, and its division is a call. In
//! a loop, the optimizer does not know that the high half of a state is zero. So this module finds
//! the range of every value of an expression at compile time. Each operation is then computed as
//! narrowly as its range allows, with the same result and the same failure as on `u128`: on the
//! narrowest of `u32`, `u64` and `u128` that holds its values, but not narrower than a register (64
//! bits, or 32 where `usize` is narrower), as [`Range::fits`] says.
//!
//! # Ranges
//!
//! A value is known as two intervals, one for each half of a `u128` ([`Range`]). For example,
//! `(kind << 64) + start` is a kind of `0..=255` in the high half and an offset in the low half. A
//! field, a read of `n` bytes, a CRC-32, an Adler-32, a Fletcher-16 and `sum16` may be any value of
//! their width. A [`Crc`](layout::Crc) of any width is taken to be any value of 64 bits, and a sum
//! of bytes is at most 255 times `isize::MAX`. A literal is itself, and a comparison is `0..=1`.
//! `size_of_val(self)` is at most `isize::MAX` ([`SIZE_MAX`]). A QUIC integer is 62 bits, and the
//! other integers of variable length may be anything. Each operation makes the range of its result
//! from those of its operands, and tells whether it may fail. A sum whose bounds do not overflow
//! cannot fail. Nor can a difference whose left side is never less than its right side.
//!
//! A read that succeeds tells that its offsets are in the value. Examples are `be(at, 1)` and a
//! checksum over `start..end`. After the read, a variable that was read at is at most `isize::MAX`
//! for the rest of the expression. This does not hold after a part that may not have been computed,
//! such as the right side of a `&&` or a branch of an `if`. There the variables are what they were
//! before the part or after it. It does not hold in the `else` branch either, which starts from
//! what was before the first branch. This is what bounds the offset of a walk, whatever the number
//! of its steps. The step of IPv6 reads at `start` before it adds to it, so `start + 8 + length *
//! 8` is less than `2^64` and does not carry.
//!
//! The state of a walk is a fixed point. It is what the walk starts as, joined twice with what a
//! step makes of it. Then each bound that still moves goes as far as it can, in at most four more
//! steps. The limit on the steps of a walk ([Walks](crate::guide::rules#walks)) bounds nothing
//! here: on 64 bits, it is `usize::MAX` steps for a value of `isize::MAX` bytes, so a state that
//! grows at each step could pass `2^64`. The state of a walk three deep or more, inside two others,
//! may be anything, so that the analysis stays a few times the length of the expression.
//!
//! # How an operation is computed
//!
//! How each operation is computed is a [`How`]. It is a constant of the instance of the function
//! that applies the operation, as the operation is (see "No interpreter" in the documentation of
//! [`validate`](mod@crate::validate#no-interpreter)). It says four things:
//!
//! - The width in which the operation is computed, 32, 64 or 128 bits, never narrower than a
//!   register ([`REGISTER`]).
//! - Whether it may fail. An addition, a product or a shift that cannot overflow is not tested.
//! - Whether an addition or a difference of 128 bits is computed on each half alone, which is done
//!   where the low halves cannot carry or borrow.
//! - The width that its result fits. The evaluator ([`Machine`](crate::layout::Machine)) truncates
//!   every value to that width. This changes nothing of the value, and tells the optimizer what it
//!   is where it could not know otherwise. This is the case of the state of a walk at each step,
//!   and of the variables read in its expressions.
//!
//! The plan of an expression is a constant of its type ([`Planned::PLAN`]). Where the expressions
//! of a walk start (the variables, the state) is a constant of theirs (`Ops::ENTRY`). Both are made
//! once at compile time, for each expression.
//!
//! The computation is never narrower than a register. On 64 bits, computing in 32 bits whatever
//! fits there did not reduce the number of instructions, and added some in one case.
//! `40 + payload_length` in 32 bits is not known to be the size that the `size` rule compared in 64
//! bits, so the bounds of what it reads were tested again. For the UDP packet behind 3 headers,
//! that was 217 instructions against 209. A remainder in 32 bits after a sum in 64 bits was worse
//! than either, 243 instructions against 241 for IPv4. These counts are of
//! `valgrind --tool=callgrind` on `benches/checks.rs`, taken when this module was written (commit
//! a1090c5, 2026-11-18), on a machine that was not recorded. The bounds of a read are one
//! comparison of the offset with `size - n`, which does not change in a walk.
//!
//! # Tests
//!
//! `narrow_tests.rs` tests that the narrowed evaluation is the one on `u128`, apart from the rest
//! of the analysis:
//!
//! - Every binary operation, for every pair of ranges at the limits of 32, 64 and 128 bits and of
//!   the halves, over values at their bounds and others, as on a target of 64 bits and of 32 bits.
//!   Each is compared with the operation on `u128`. The result must be in the range that the
//!   analysis gives, and what the analysis says cannot fail must not fail.
//! - Random expressions, with variables, parts, reads and walks, where a walk is random, moves an
//!   offset by what it reads, or carries two numbers in the halves of the state. Each expression is
//!   evaluated over random and boundary bytes, narrowed and not, and both must have the same value
//!   or fail alike.
//!
//! `tests/rules_are_rust.rs` compares rules at the limits of the widths, and walks, with Rust. The
//! tests catch an analysis that does not test a carry, that takes no fixed point, or that does not
//! join what follows a part. They also catch an analysis that is wrong about the range of a
//! product, a remainder or a shift. The tests cannot catch a bound of `isize::MAX` taken too small,
//! because no value of the tests is large enough to tell.

use crate::{
  layout::{self, MAX_OPS, Op, STACK, VARIABLES, Varint},
  steps::Entry,
};

/// The largest size of a value in bytes, which no value exceeds in Rust.
///
/// Once a rule has read at an offset, or over a range, the offset and the end of the range are at
/// most this.
const SIZE_MAX: u128 = isize::MAX as u128;

/// The values that a value may be: `hi << 64 | lo`, for any `hi` in `hi.0..=hi.1` and any `lo` in
/// `lo.0..=lo.1`.
///
/// A range has two halves rather than one interval. The state of a walk may be two numbers in a
/// `u128`, one in each half. What is added to its low half is computed there alone, where it is
/// known not to carry.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Range {
  /// The bounds of the high 64 bits.
  hi: (u64, u64),
  /// The bounds of the low 64 bits.
  lo: (u64, u64),
}

/// The ranges of the variables of an expression.
pub type Vars = [Range; VARIABLES];

/// Returns the lesser of `a` and `b`.
const fn min64(a: u64, b: u64) -> u64 {
  if a < b { a } else { b }
}

/// Returns the greater of `a` and `b`.
const fn max64(a: u64, b: u64) -> u64 {
  if a > b { a } else { b }
}

/// Returns the lesser of `a` and `b`.
const fn min128(a: u128, b: u128) -> u128 {
  if a < b { a } else { b }
}

/// Returns `x` with every bit below its highest set bit set too.
const fn ones(x: u64) -> u64 {
  if x == 0 { 0 } else { u64::MAX >> x.leading_zeros() }
}

impl Range {
  /// The range of any value.
  pub const ANY: Range = Range { hi: (0, u64::MAX), lo: (0, u64::MAX) };

  /// Returns a range that holds the values `min..=max`.
  ///
  /// If `min` and `max` differ in their high halves, the low half may be anything.
  pub const fn new(min: u128, max: u128) -> Range {
    let (a, b) = ((min >> 64) as u64, (max >> 64) as u64);
    if a == b {
      Range { hi: (a, a), lo: (min as u64, max as u64) }
    } else {
      Range { hi: (a, b), lo: (0, u64::MAX) }
    }
  }

  /// Returns the range of the value `value` alone.
  pub const fn exact(value: u128) -> Range {
    Range::new(value, value)
  }

  /// Returns the range of an unsigned integer of `bytes` bytes.
  const fn bytes(bytes: u8) -> Range {
    if bytes >= 16 { Range::ANY } else { Range::new(0, (1 << (8 * bytes as u32)) - 1) }
  }

  /// Returns the least value of the range.
  pub const fn min(self) -> u128 {
    (self.hi.0 as u128) << 64 | self.lo.0 as u128
  }

  /// Returns the greatest value of the range.
  pub const fn max(self) -> u128 {
    (self.hi.1 as u128) << 64 | self.lo.1 as u128
  }

  /// Returns whether `value` is in the range.
  #[cfg(test)]
  pub const fn holds(self, value: u128) -> bool {
    let (hi, lo) = ((value >> 64) as u64, value as u64);
    self.hi.0 <= hi && hi <= self.hi.1 && self.lo.0 <= lo && lo <= self.lo.1
  }

  /// Returns the range of `hi << 64 | lo` for `hi` in `hi.0..=hi.1` and `lo` in `lo.0..=lo.1`.
  #[cfg(test)]
  pub const fn halves(hi: (u64, u64), lo: (u64, u64)) -> Range {
    Range { hi, lo }
  }

  /// Returns a range that holds the values of both ranges.
  const fn join(self, other: Range) -> Range {
    Range {
      hi: (min64(self.hi.0, other.hi.0), max64(self.hi.1, other.hi.1)),
      lo: (min64(self.lo.0, other.lo.0), max64(self.lo.1, other.lo.1)),
    }
  }

  /// Returns whether both ranges have the same bounds.
  const fn same(self, other: Range) -> bool {
    self.hi.0 == other.hi.0
      && self.hi.1 == other.hi.1
      && self.lo.0 == other.lo.0
      && self.lo.1 == other.lo.1
  }

  /// Returns `self` widened toward `to`, where each bound that moved goes as far as it can.
  ///
  /// A fixed point is then reached after a few steps.
  const fn widen(self, to: Range) -> Range {
    Range {
      hi: (
        if to.hi.0 < self.hi.0 { 0 } else { self.hi.0 },
        if to.hi.1 > self.hi.1 { u64::MAX } else { self.hi.1 },
      ),
      lo: (
        if to.lo.0 < self.lo.0 { 0 } else { self.lo.0 },
        if to.lo.1 > self.lo.1 { u64::MAX } else { self.lo.1 },
      ),
    }
  }

  /// Returns the values of the range that are at most `max`.
  ///
  /// Returns `self` if no value is, since what follows is then never computed.
  const fn at_most(self, max: u128) -> Range {
    if self.min() > max {
      return self;
    }
    // Both are true of the value: so is what they share, which holds `self.min()`.
    let cut = Range::new(self.min(), min128(self.max(), max));
    Range {
      hi: (max64(self.hi.0, cut.hi.0), min64(self.hi.1, cut.hi.1)),
      lo: (max64(self.lo.0, cut.lo.0), min64(self.lo.1, cut.lo.1)),
    }
  }

  /// Returns the width in bits in which the value is computed.
  ///
  /// This is the narrowest of `u32`, `u64` and `u128` that holds every value, but not narrower
  /// than a register ([`REGISTER`]).
  pub const fn fits(self) -> u8 {
    let bits = self.bits();
    if bits < REGISTER { REGISTER } else { bits }
  }

  /// Returns the width in bits of the narrowest of `u32`, `u64` and `u128` that holds every
  /// value.
  pub const fn bits(self) -> u8 {
    let max = self.max();
    if max <= u32::MAX as u128 {
      32
    } else if max <= u64::MAX as u128 {
      64
    } else {
      128
    }
  }
}

/// How an operation of an expression is computed, from the ranges of its operands.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct How {
  /// The width in bits (32, 64 or 128) that the result fits.
  ///
  /// The result is truncated to it, so that the optimizer knows its range.
  pub result: u8,
  /// The width in bits (32, 64 or 128) in which a binary operation is computed.
  ///
  /// The operands and the result fit it, and the amount of a shift is less than it.
  pub width: u8,
  /// Whether the operation may fail.
  ///
  /// An addition, a subtraction, a product or a shift that cannot fail is computed without the
  /// test.
  pub fails: bool,
  /// Whether each half of an addition or a subtraction of 128 bits is computed alone.
  ///
  /// It is where the low halves do not carry or borrow, and the high halves do not overflow.
  pub halves: bool,
}

impl How {
  /// On `u128`, with every test: what an expression means.
  pub const WIDE: How = How { result: 128, width: 128, fails: true, halves: false };
}

/// Returns `value` truncated to `bits` bits, which the analysis knows that it fits.
///
/// The value does not change. The truncation tells the optimizer its range.
#[inline(always)]
pub const fn fit(value: u128, bits: u8) -> u128 {
  match bits {
    32 => value as u32 as u128,
    64 => value as u64 as u128,
    _ => value,
  }
}

/// Returns `op` of `a` and `b`, computed as `how` says.
///
/// The result is what [`layout::binary`] makes of them on `u128`, failures included, where the
/// analysis made `how` for the ranges of `a` and `b`.
///
/// # Errors
///
/// Returns `None` if the operation fails.
#[inline(always)]
pub const fn binary(op: Op, how: How, a: u128, b: u128) -> Option<u128> {
  match how.width {
    32 => match narrow32(op, how.fails, a as u32, b as u32) {
      Some(value) => Some(value as u128),
      None => None,
    },
    64 => match narrow64(op, how.fails, a as u64, b as u64) {
      Some(value) => Some(value as u128),
      None => None,
    },
    _ => wide(op, how, a, b),
  }
}

/// Returns `op` of `a` and `b` on `u128`, without the tests that `how` says cannot fail.
///
/// # Errors
///
/// Returns `None` if the operation fails.
#[inline(always)]
const fn wide(op: Op, how: How, a: u128, b: u128) -> Option<u128> {
  match op {
    Op::Add if !how.fails => Some(if how.halves {
      let hi = ((a >> 64) as u64).wrapping_add((b >> 64) as u64);
      (hi as u128) << 64 | (a as u64).wrapping_add(b as u64) as u128
    } else {
      a.wrapping_add(b)
    }),
    Op::Sub if !how.fails => Some(if how.halves {
      let hi = ((a >> 64) as u64).wrapping_sub((b >> 64) as u64);
      (hi as u128) << 64 | (a as u64).wrapping_sub(b as u64) as u128
    } else {
      a.wrapping_sub(b)
    }),
    Op::Mul if !how.fails => Some(a.wrapping_mul(b)),
    // The amount is less than 128.
    Op::Shl if !how.fails => Some(a.wrapping_shl(b as u32)),
    Op::Shr if !how.fails => Some(a.wrapping_shr(b as u32)),
    _ => layout::binary(op, a, b),
  }
}

/// Defines `fn name(op, fails, a, b)` for each `name: T`, a binary operation computed on `T`.
///
/// The operands and the result fit `T`, and a shift is by less than its width, so a sum, a product
/// or a shift does not overflow `T`. A difference, a quotient or a remainder fails where it would
/// on `u128`, and any other operation fails.
macro_rules! narrow {
  ($($name:ident: $t:ty)+) => {$(
    #[inline(always)]
    const fn $name(op: Op, fails: bool, a: $t, b: $t) -> Option<$t> {
      Some(match op {
        Op::Add => a.wrapping_add(b),
        Op::Sub if fails => match a.checked_sub(b) {
          Some(value) => value,
          None => return None,
        },
        Op::Sub => a.wrapping_sub(b),
        Op::Mul => a.wrapping_mul(b),
        Op::Div => match a.checked_div(b) {
          Some(value) => value,
          None => return None,
        },
        Op::Rem => match a.checked_rem(b) {
          Some(value) => value,
          None => return None,
        },
        Op::And => a & b,
        Op::Or => a | b,
        Op::Xor => a ^ b,
        Op::Shl => a.wrapping_shl(b as u32),
        Op::Shr => a.wrapping_shr(b as u32),
        Op::Eq => (a == b) as $t,
        Op::Ne => (a != b) as $t,
        Op::Lt => (a < b) as $t,
        Op::Le => (a <= b) as $t,
        Op::Gt => (a > b) as $t,
        Op::Ge => (a >= b) as $t,
        _ => return None,
      })
    }
  )+};
}

narrow! {
  narrow32: u32
  narrow64: u64
}

/// Returns the range of what `op` makes of values of `a` and `b` where it does not fail, whether
/// it may fail, and whether it can be computed on halves that do not carry or borrow.
///
/// Only an addition or a subtraction is computed on halves.
pub const fn binary_range(op: Op, a: Range, b: Range) -> (Range, bool, bool) {
  match op {
    Op::Add => {
      if let (Some(lo), Some(hi)) = (a.lo.1.checked_add(b.lo.1), a.hi.1.checked_add(b.hi.1)) {
        let range = Range { hi: (a.hi.0 + b.hi.0, hi), lo: (a.lo.0 + b.lo.0, lo) };
        return (range, false, true);
      }
      match a.max().checked_add(b.max()) {
        Some(max) => (Range::new(a.min() + b.min(), max), false, false),
        None => (Range::new(a.min().saturating_add(b.min()), u128::MAX), true, false),
      }
    }
    Op::Sub => {
      if a.lo.0 >= b.lo.1 && a.hi.0 >= b.hi.1 {
        let range =
          Range { hi: (a.hi.0 - b.hi.1, a.hi.1 - b.hi.0), lo: (a.lo.0 - b.lo.1, a.lo.1 - b.lo.0) };
        return (range, false, true);
      }
      let range = Range::new(a.min().saturating_sub(b.max()), a.max().saturating_sub(b.min()));
      (range, a.min() < b.max(), false)
    }
    Op::Mul => match a.max().checked_mul(b.max()) {
      Some(max) => (Range::new(a.min() * b.min(), max), false, false),
      None => (Range::new(a.min().saturating_mul(b.min()), u128::MAX), true, false),
    },
    Op::Div => {
      let least = if b.min() == 0 { 1 } else { b.min() };
      // By zero only: a failure, after which nothing is computed.
      let range = match a.min().checked_div(b.max()) {
        Some(min) => Range::new(min, a.max() / least),
        None => Range::exact(0),
      };
      (range, b.min() == 0, false)
    }
    Op::Rem => {
      let range =
        if b.max() == 0 { Range::exact(0) } else { Range::new(0, min128(a.max(), b.max() - 1)) };
      (range, b.min() == 0, false)
    }
    // Bitwise: each half on its own.
    Op::And => {
      (Range { hi: (0, min64(a.hi.1, b.hi.1)), lo: (0, min64(a.lo.1, b.lo.1)) }, false, false)
    }
    Op::Or | Op::Xor => {
      let range =
        Range { hi: (0, ones(max64(a.hi.1, b.hi.1))), lo: (0, ones(max64(a.lo.1, b.lo.1))) };
      (range, false, false)
    }
    Op::Shl => {
      let fails = b.max() >= 128;
      // The low half becomes the high one.
      if b.min() == 64 && b.max() == 64 {
        return (Range { hi: a.lo, lo: (0, 0) }, false, false);
      }
      // No bit is shifted out.
      let range = if b.max() < 128 && a.max().leading_zeros() as u128 >= b.max() {
        Range::new(a.min() << b.min(), a.max() << b.max())
      } else {
        Range::ANY
      };
      (range, fails, false)
    }
    Op::Shr => {
      if b.min() == 64 && b.max() == 64 {
        return (Range { hi: (0, 0), lo: a.hi }, false, false);
      }
      let (least, most) = (min128(b.min(), 127), min128(b.max(), 127));
      (Range::new(a.min() >> most, a.max() >> least), b.max() >= 128, false)
    }
    Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => (Range::new(0, 1), false, false),
    _ => (Range::ANY, true, false),
  }
}

/// The width in bits of a register of the target: 64 where `usize` has 64 bits or more, else 32.
///
/// A value narrower than a register is computed in a register all the same. On a 64-bit target,
/// a sum of 32 bits that is the size of the value (`40 + payload_length`) is not known to equal
/// the sum of 64 bits that a `size` rule compared with the size, since it may wrap. The bounds of
/// what it reads would then be tested again.
const REGISTER: u8 = if usize::BITS >= 64 { 64 } else { 32 };

/// Returns how a binary operation `op` of values of `a` and `b` is computed, and the range of its
/// result.
pub const fn binary_how(op: Op, a: Range, b: Range) -> (How, Range) {
  binary_in(op, a, b, REGISTER)
}

/// Returns how a binary operation `op` of values of `a` and `b` is computed where a register has
/// `register` bits, and the range of its result.
pub const fn binary_in(op: Op, a: Range, b: Range, register: u8) -> (How, Range) {
  let (range, fails, halves) = binary_range(op, a, b);
  let mut width = register;
  if a.bits() > width {
    width = a.bits();
  }
  if b.bits() > width {
    width = b.bits();
  }
  if range.bits() > width {
    width = range.bits();
  }
  // A shift by its width or more is not what it is on `u128`.
  if matches!(op, Op::Shl | Op::Shr) && b.max() >= width as u128 {
    width = 128;
  }
  let result = if range.bits() < register { register } else { range.bits() };
  (How { result, width, fails, halves: halves && width == 128 }, range)
}

/// The plan of an expression: how each operation is computed, and ranges of its values.
pub struct Plan {
  /// How each operation is computed, by index.
  pub how: [How; MAX_OPS],
  /// The range of the value of the expression.
  pub result: Range,
  /// The ranges of the variables at the walk where the analysis stopped, with the state of the
  /// walk in its variable.
  pub entry: Vars,
}

/// No variable: the tag of a value of the stack that is not a variable.
const NONE: u8 = u8::MAX;

/// A part of an expression that may not be computed, with the ranges of the variables before
/// it.
///
/// A part is the right side of a `&&` or a `||`, or a branch of an `if`. If it is not computed,
/// the variables after it are what they were before it.
#[derive(Copy, Clone)]
struct Part {
  /// The index of the operation that ends it.
  end: usize,
  /// The ranges of the variables before the part.
  vars: Vars,
}

/// Returns the ranges that hold the values of each variable in both `a` and `b`.
const fn join_vars(a: Vars, b: Vars) -> Vars {
  let (mut vars, mut i) = (a, 0);
  while i < VARIABLES {
    vars[i] = a[i].join(b[i]);
    i += 1;
  }
  vars
}

/// Returns the plan of an expression whose variables start as `entry` says.
pub const fn plan(ops: &[Op], entry: &Entry) -> Plan {
  analyze(ops, entry.vars, usize::MAX, entry.walks)
}

/// Returns where the expressions of the walk at the index `at` start, in an expression whose
/// variables start as `entry` says.
pub const fn walk_entry(ops: &[Op], entry: &Entry, at: usize) -> Entry {
  Entry { vars: analyze(ops, entry.vars, at, entry.walks).entry, walks: entry.walks + 1 }
}

/// The number of walks around a walk from which its state may be anything: two, for a walk three
/// deep.
///
/// A fixed point is found by analyzing a step a few times, and the steps of the walks within it
/// as many times each. This bound keeps the analysis a few times the length of the expression.
const FIXED_WALKS: usize = 2;

/// Returns the range of the state of a walk: what it starts as, and what any number of steps make
/// of it.
///
/// A step is the expression `next`, where the variables are `vars` and the state is the variable
/// `var`. The state starts in `start`. The step is applied twice as it is, then each bound that
/// still moves goes as far as it can. A walk in [`FIXED_WALKS`] walks or more has a state that may
/// be anything.
const fn fixed_point(next: &[Op], vars: Vars, var: u8, start: Range, walks: usize) -> Range {
  if walks >= FIXED_WALKS {
    return Range::ANY;
  }
  let (mut state, mut round) = (start, 0);
  loop {
    let mut inner = vars;
    inner[var as usize] = state;
    let after = state.join(analyze(next, inner, usize::MAX, walks + 1).result);
    if after.same(state) {
      return state;
    }
    // Twice as it is, then each bound that moves goes as far as it can: at most four more times.
    state = if round < 2 { after } else { state.widen(after) };
    round += 1;
  }
}

/// Returns the plan of the expression `ops`, whose variables start in `vars`.
///
/// The range of every value follows from the ranges of the variables and from what the
/// expression reads. With `stop`, the analysis stops at the walk at that index, and
/// [`Plan::entry`] says where the expressions of that walk start. `walks` is the number of walks
/// that the expression is in.
///
/// The operations are taken in order, as the evaluator applies them when it skips none. A value
/// that a part pushes is what the part makes. It may also be a placeholder that nothing reads
/// (the branch of an `if` that is not taken, which [`Op::Select`] leaves), or the value of the
/// left side of a `&&` or `||` that decided. A read that succeeds (`be(at, 2)`) shows that its
/// offset is in the value. A variable that was read at is then at most the size of a value.
const fn analyze(ops: &[Op], vars: Vars, stop: usize, walks: usize) -> Plan {
  let mut plan = Plan { how: [How::WIDE; MAX_OPS], result: Range::ANY, entry: vars };
  let mut vars = vars;
  // The values of the stack, and the variable that each is, if it was pushed by `Get`.
  let (mut stack, mut tags, mut depth) = ([Range::ANY; STACK], [NONE; STACK], 0usize);
  let (mut parts, mut open) = ([Part { end: usize::MAX, vars }; STACK], 0usize);
  let mut i = 0;
  while i < ops.len() && i < MAX_OPS {
    let op = ops[i];
    // The end of a part: what follows may come after it or after what was before it.
    let mut before = vars;
    if open > 0 && parts[open - 1].end == i {
      open -= 1;
      before = parts[open].vars;
      // The `else` branch is computed only when the first one is not: from what was before it.
      if !matches!(op, Op::Else(_)) {
        vars = join_vars(vars, before);
      }
    }
    let (pops, value) = match op {
      Op::Lit(value) => (0, Range::exact(value)),
      Op::Load { size, .. } => (0, Range::bytes(size)),
      Op::Size => (0, Range::new(0, SIZE_MAX)),
      Op::Get(index) => {
        stack[depth] = vars[index as usize];
        tags[depth] = index;
        plan.how[i].result = vars[index as usize].fits();
        depth += 1;
        i += 1;
        continue;
      }
      Op::Set(index) => {
        depth -= 1;
        vars[index as usize] = stack[depth];
        // What the stack holds of it is what it was.
        let mut k = 0;
        while k < depth {
          if tags[k] == index {
            tags[k] = NONE;
          }
          k += 1;
        }
        i += 1;
        continue;
      }
      Op::Not => (1, Range::new(0, 1)),
      Op::At { size, .. } => {
        refine(&mut vars, tags[depth - 1], SIZE_MAX - size as u128);
        (1, Range::bytes(size))
      }
      Op::Varint { form, length } => {
        refine(&mut vars, tags[depth - 1], SIZE_MAX);
        let value = match (form, length) {
          (Varint::Quic, false) => Range::new(0, (1 << 62) - 1),
          (Varint::Quic, true) => Range::new(1, 8),
          (_, false) => Range::ANY,
          (_, true) => Range::new(1, SIZE_MAX),
        };
        (1, value)
      }
      Op::Crc32 | Op::Sum16 | Op::Crc(_) | Op::CrcZeros(_) | Op::Checksum(_) | Op::Matches(_) => {
        let operands = if matches!(op, Op::CrcZeros(_)) { 4 } else { 2 };
        let mut k = depth - operands;
        while k < depth {
          refine(&mut vars, tags[k], SIZE_MAX);
          k += 1;
        }
        let value = match op {
          Op::Crc32 | Op::Checksum(layout::Checksum::Adler32) => Range::bytes(4),
          Op::Sum16 | Op::Checksum(layout::Checksum::Fletcher16) => Range::bytes(2),
          Op::Crc(_) | Op::CrcZeros(_) => Range::bytes(8),
          Op::Matches(_) => Range::new(0, 1),
          // A sum of bytes.
          _ => Range::new(0, 255 * SIZE_MAX),
        };
        (operands, value)
      }
      // What decides is the value of the `&&` or the `||`, which its end makes `0..=1` as well.
      Op::Shortcut { len, .. } => {
        parts[open] = Part { end: i + len as usize, vars };
        open += 1;
        (1, stack[depth - 1])
      }
      Op::Then(len) => {
        parts[open] = Part { end: i + len as usize, vars };
        open += 1;
        i += 1;
        continue;
      }
      // What the first branch left is joined at the `Select`, with what the second leaves.
      Op::Else(len) => {
        parts[open] = Part { end: i + len as usize + 1, vars };
        open += 1;
        vars = before;
        i += 1;
        continue;
      }
      Op::Both | Op::Either => (2, Range::new(0, 1)),
      Op::Select => (3, stack[depth - 2].join(stack[depth - 1])),
      Op::Walk { var, next, .. } => {
        let state = fixed_point(next, vars, var, stack[depth - 1], walks);
        if i == stop {
          plan.entry = vars;
          plan.entry[var as usize] = state;
          return plan;
        }
        (1, state)
      }
      op => {
        let (how, value) = binary_how(op, stack[depth - 2], stack[depth - 1]);
        plan.how[i] = how;
        (2, value)
      }
    };
    depth -= pops;
    stack[depth] = value;
    tags[depth] = NONE;
    plan.how[i].result = value.fits();
    depth += 1;
    i += 1;
  }
  plan.result = stack[0];
  plan
}

/// Records that the variable `index` is at most `max`, once it was read at in the value.
///
/// Does nothing if `index` is [`NONE`].
const fn refine(vars: &mut Vars, index: u8, max: u128) {
  if index != NONE {
    vars[index as usize] = vars[index as usize].at_most(max);
  }
}

/// The plan of the expression `X`, made once for it.
pub struct Planned<X>(core::marker::PhantomData<X>);

impl<X: crate::steps::Ops> Planned<X> {
  /// The plan of `X`, from where its variables start (`X::ENTRY`).
  pub const PLAN: Plan = plan(X::OPS, &X::ENTRY);
}
