//! Tests that a rule means what the same Rust expression means when every integer is a `u128`.
//!
//! Each expression here is both the rule of a struct and the body of a function. The two must
//! agree on every value, and the rule fails where Rust would panic.
//!
//! The evaluator computes in 64 or 32 bits what it knows to fit there (`puns/src/narrow.rs`). The
//! expressions after `e37` are at the limits of those widths: a sum, a product or a shift just
//! past 64 bits, two numbers in the halves of a `u128`, and a carry from one half into the other.
//! The expressions of `walks!` walk over the bytes of the value, as `be` reads them.
//!
//! This file is not built under Miri. It tests what the derive makes of an expression, and the
//! unsafe reads of the evaluator are the ones that `expressions.rs` runs under Miri.

#![cfg(not(miri))]
#![expect(
  unused_variables,
  clippy::bad_bit_mask,
  clippy::eq_op,
  clippy::erasing_op,
  clippy::let_and_return,
  clippy::manual_is_multiple_of,
  clippy::nonminimal_bool,
  clippy::overly_complex_bool_expr,
  reason = "the expressions are there for what they compute, and how: as rules, they are not Rust"
)]

/// Declares, for each `name: |a, b, c, d| expression;`, a struct of four fields with the rule
/// `assert(expression)` and a function of four `u128`s whose body is the expression.
///
/// It also declares `compare`, which compares each struct with its function.
macro_rules! rules {
  ($($name:ident: |$a:ident, $b:ident, $c:ident, $d:ident| $e:expr;)+) => {
    $(mod $name {
      #[derive(puns::Pun)]
      #[repr(C)]
      #[pun(assert($e))]
      pub struct Rule {
        pub $a: u8,
        pub $b: puns::num::u16le,
        pub $c: puns::num::u32be,
        pub $d: puns::num::u64le,
      }

      pub const fn rust($a: u128, $b: u128, $c: u128, $d: u128) -> bool {
        $e
      }
    })+

    /// Evaluates each expression as a rule and as Rust over the same values. Returns, for each
    /// expression, its name and how many values it holds and fails on.
    ///
    /// # Panics
    ///
    /// Panics if the rule and the function disagree on a value.
    fn compare(values: &[(u128, u128, u128, u128)]) -> Vec<(&'static str, usize, usize)> {
      let mut counts = vec![];
      $(
        let (mut holds, mut fails) = (0, 0);
        for &(a, b, c, d) in values {
          let mut bytes = vec![a as u8];
          bytes.extend((b as u16).to_le_bytes());
          bytes.extend((c as u32).to_be_bytes());
          bytes.extend((d as u64).to_le_bytes());
          let rule = puns::from_bytes::<$name::Rule>(&bytes).is_ok();
          // A panic of Rust (an overflow, a division by zero, a shift too far) is a rule that
          // fails.
          let rust = std::panic::catch_unwind(|| $name::rust(a, b, c, d)).unwrap_or(false);
          assert_eq!(rule, rust, "{}: a = {a}, b = {b}, c = {c}, d = {d}", stringify!($name));
          if rust { holds += 1 } else { fails += 1 }
        }
        counts.push((stringify!($name), holds, fails));
      )+
      counts
    }
  };
}

/// Returns `count` values of the four fields, the same on every run.
///
/// Each value is a bound of its field, a small number, a number around the sign bit of a byte, or
/// any number.
fn values(count: usize) -> Vec<(u128, u128, u128, u128)> {
  let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
  let mut next = move || {
    seed ^= seed << 13;
    seed ^= seed >> 7;
    seed ^= seed << 17;
    seed
  };
  let mut pick = |bits: u32| -> u128 {
    let max = u64::MAX >> (64 - bits);
    let value = match next() % 8 {
      0 => 0,
      1 => 1,
      2 => max,
      3 => max - 1,
      4 => next() % 8,
      5 => 127 + next() % 3,
      _ => next(),
    };
    u128::from(value & max)
  };
  (0..count).map(|_| (pick(8), pick(16), pick(32), pick(64))).collect()
}

#[test]
fn rules_are_rust() {
  let values = values(4000);
  let hook = std::panic::take_hook();
  std::panic::set_hook(Box::new(|_| {}));
  let counts = std::panic::catch_unwind(|| (compare(&values), compare_walks(&values)));
  std::panic::set_hook(hook);
  let (counts, walks) = counts.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
  // The expressions are worth something if most of them both hold and fail.
  let both = counts.iter().filter(|(_, holds, fails)| *holds > 0 && *fails > 0).count();
  assert!(both >= 30, "{counts:?}");
  let both = walks.iter().filter(|(_, holds, fails)| *holds > 0 && *fails > 0).count();
  assert!(both >= 4, "{walks:?}");
}

/// Declares, as `rules!` does, a struct and a function for each `name: |a, b, c, d| expression;`
/// whose expression reads the bytes of the value (`be`) and walks.
///
/// In the functions, `be` and `walk` are the functions below, which read the bytes of the value
/// that is compared. It also declares `compare_walks`, which compares each struct with its
/// function.
macro_rules! walks {
  ($($name:ident: |$a:ident, $b:ident, $c:ident, $d:ident| $e:expr;)+) => {
    $(mod $name {
      use super::{be, walk};

      #[derive(puns::Pun)]
      #[repr(C)]
      #[pun(assert($e))]
      pub struct Rule {
        pub $a: u8,
        pub $b: puns::num::u16le,
        pub $c: puns::num::u32be,
        pub $d: puns::num::u64le,
      }

      pub fn rust($a: u128, $b: u128, $c: u128, $d: u128) -> bool {
        $e
      }
    })+

    fn compare_walks(values: &[(u128, u128, u128, u128)]) -> Vec<(&'static str, usize, usize)> {
      let mut counts = vec![];
      $(
        let (mut holds, mut fails) = (0, 0);
        for &(a, b, c, d) in values {
          let mut bytes = vec![a as u8];
          bytes.extend((b as u16).to_le_bytes());
          bytes.extend((c as u32).to_be_bytes());
          bytes.extend((d as u64).to_le_bytes());
          let rule = puns::from_bytes::<$name::Rule>(&bytes).is_ok();
          BYTES.with_borrow_mut(|value| *value = bytes);
          let rust = std::panic::catch_unwind(|| $name::rust(a, b, c, d)).unwrap_or(false);
          assert_eq!(rule, rust, "{}: a = {a}, b = {b}, c = {c}, d = {d}", stringify!($name));
          if rust { holds += 1 } else { fails += 1 }
        }
        counts.push((stringify!($name), holds, fails));
      )+
      counts
    }
  };
}

std::thread_local! {
  /// The bytes of the value whose rule is compared.
  static BYTES: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Returns the `n` bytes at `offset` in the value, big-endian, as `be(offset, n)` does in a rule.
///
/// # Panics
///
/// Panics if the bytes are not all in the value, where the rule fails.
fn be(offset: u128, n: u128) -> u128 {
  BYTES.with_borrow(|bytes| {
    let (offset, n) = (usize::try_from(offset).unwrap(), usize::try_from(n).unwrap());
    bytes[offset..offset.checked_add(n).unwrap()].iter().fold(0, |v, &b| v << 8 | u128::from(b))
  })
}

/// Returns `start` replaced by `next` of it for as long as `cond` holds of it, as
/// `walk(start, cond, next)` does in a rule.
///
/// # Panics
///
/// Panics if the walk takes more steps than the value and a `u128` have bits, where the rule
/// fails.
fn walk(start: u128, cond: impl Fn(u128) -> bool, next: impl Fn(u128) -> u128) -> u128 {
  let limit = (BYTES.with_borrow(Vec::len) + 16) * 8;
  let (mut state, mut steps) = (start, 0);
  while cond(state) {
    assert!(steps < limit, "a walk takes too many steps");
    steps += 1;
    state = next(state);
  }
  state
}

rules! {
  e01: |a, b, c, d| a + b * 2 == c - d;
  e02: |a, b, c, d| ((a as u8 as u128) << 4) >> 4 == a & 0x0f;
  e03: |a, b, c, d| (!a & 0xff) == 255 - a;
  e04: |a, b, c, d| a < 100 && b / a > 3;
  e05: |a, b, c, d| a == 0 || b % a == 1;
  e06: |a, b, c, d| (if a > 100 { c / (a - a) } else { 5 }) == 5;
  e07: |a, b, c, d| { let x = a + b; let y = x * x; y % 7 } < 4;
  e08: |a, b, c, d| { let unused = c / (a & 0); 1 } == 1;
  e09: |a, b, c, d| (b << (a & 127)) >> (a & 127) == b;
  e10: |a, b, c, d| (d << a) != 0 || a >= 128;
  e11: |a, b, c, d| (a as u16 as u128) + (b as u8 as u128) == (c as u32 as u128) % 300;
  e12: |a, b, c, d| d - c > b + a;
  e13: |a, b, c, d| (a ^ b | c & d) == ((a ^ b) | (c & d));
  e14: |a, b, c, d| a + b + c - d * 0 >= a;
  e15: |a, b, c, d| !(a < b) == (a >= b);
  e16: |a, b, c, d| ((if a < 128 { a } else { 255 - a }) < 64) != (b > 1000);
  e17: |a, b, c, d| d * d * d < 1000 || d * d * d >= 1000;
  e18: |a, b, c, d| 1u128 << a < 5;
  e19: |a, b, c, d| (a as usize) + 1 > 0;
  e20: |a, b, c, d| { let a = b; ({ let b = a + 1; b }) + a } == 2 * b + 1;
  e21: |a, b, c, d| a - b - c == 0 || a > b;
  e22: |a, b, c, d| a / (b + 1) / (c + 1) == a / ((b + 1) * (c + 1));
  e23: |a, b, c, d| (a < b) == (b > a) && (a <= b) != (a > b);
  e24: |a, b, c, d| b >> a == 0 || a < 16;
  e25: |a, b, c, d| (c as u8 as u16 as u8 as u128) == (c & 0xff) && (d as u32 as u128) == d % 0x1_0000_0000;
  e26: |a, b, c, d| a % 2 == 0 && b % 2 == 0 || c % 2 == 0 && d % 2 == 0;
  e27: |a, b, c, d| a > 0 && (b / a > 0 || c / (a - 1) > 0);
  e28: |a, b, c, d| (if a == 0 { 0 } else if a == 1 { b } else if a < 128 { c } else { d }) & 1 == 0;
  e29: |a, b, c, d| !!a == a && !(!(a == 1)) == (a == 1);
  e30: |a, b, c, d| { let s = a + b + c; let t = s - a; t - b } == c;
  e31: |a, b, c, d| a * b * c * d / (a + 1) < d || a == 255;
  e32: |a, b, c, d| (a | 1) != 0 && (d >> 63 == 1 || d < 0x8000_0000_0000_0000);
  e33: |a, b, c, d| a > 200 || { let q = b / (a - a); q == 0 };
  e34: |a, b, c, d| (a < 5 && (if b > 7 { c / (b - b) } else { d }) > 0) || a == 7;
  e35: |a, b, c, d| a == 1 || (b == 2 && (c == 3 || d / (a - 1) > 0));
  e36: |a, b, c, d| (if if a > 3 { b > 2 } else { c / (a - a) > 0 } { 1 } else { d / (a & 0) }) == 1;
  e37: |a, b, c, d| !(a < 9 || { let x = b - c; x > d }) && (if a & 1 == 0 { b } else { c - b }) < d;
  // At the limits of 32 and 64 bits.
  e38: |a, b, c, d| d + d > d || d == 0;
  e39: |a, b, c, d| d * d / (d + 1) < d || d == 0;
  e40: |a, b, c, d| d * d * d / (c + 1) > a;
  e41: |a, b, c, d| c * c + c * c > c && (c * c) >> 32 < c;
  e42: |a, b, c, d| ((d << 64) + d) >> 64 == d && ((d << 64) + d) & 0xffff_ffff_ffff_ffff == d;
  e43: |a, b, c, d| (((d << 64) + d + d) >> 64) - d == (d + d) >> 64;
  e44: |a, b, c, d| (d << 63) >> 63 == d && (d << 65) >> 65 == d;
  e45: |a, b, c, d| d - c - b - a < d || d < c;
  e46: |a, b, c, d| (d + 1) % (c + 1) < c && (d + 1) / (b + 1) > a;
  e47: |a, b, c, d| (d >> (a & 127)) < d && (d >> (a % 200)) <= d;
  e48: |a, b, c, d| (1u128 << (a & 127)) > d && (c << (b % 140)) > 0;
  e49: |a, b, c, d| (d | (d << 64)) + 1 > d;
  e50: |a, b, c, d| c * c / c == c && (c * c) % (d + 1) <= d;
  e51: |a, b, c, d| (d + 1 > 0xffff_ffff_ffff_ffff) == (d == 0xffff_ffff_ffff_ffff) && d + 0xffff_ffff_ffff_ffff > c;
  e52: |a, b, c, d| ((a << 64) | d) - (d + 1) < (a << 64);
  e53: |a, b, c, d| (d << 64) - d > c || d == 0;
  e54: |a, b, c, d| b * c * a < (1 << 55) && a * b * c * d > c;
  e55: |a, b, c, d| (c << 32) + c == c * 0x1_0000_0001 && (c << 32) - c == c * 0xffff_ffff;
  e56: |a, b, c, d| { let x = (b << 64) + d; let y = x + (c << 63); (y >> 64) - b } < 2;
  e57: |a, b, c, d| ((if a < 128 { d << 64 } else { d }) + (d + 1)) >> 64 > a;
  e58: |a, b, c, d| (d * 0xffff_ffff_ffff_ffff) >> 64 == d - 1 || d == 0;
}

walks! {
  // Two numbers in the halves of the state, as the walk of IPv6: a count in the high one, an
  // offset in the low one.
  w1: |a, b, c, d| walk(
    ((a & 3) << 64) + (b & 7),
    |s| (s >> 64) % 4 != 3 && (s & 0xffff_ffff_ffff_ffff) < 14,
    |s| {
      let at = s & 0xffff_ffff_ffff_ffff;
      (((s >> 64) + be(at, 1) % 2) << 64) + at + 1 + be(at + 1, 1) % 3
    },
  ) >> 64 < 3;
  // Until a byte is zero, or past the steps a walk may take.
  w2: |a, b, c, d| walk(b % 15, |at| be(at % 15, 1) != 0, |at| at + 1) < 100;
  // A state that grows past 64 bits.
  w3: |a, b, c, d| walk(d, |s| s < (1 << 64) + 5 && be(0, 1) < 200, |s| s + (1 << 63) + be(1, 1)) > c;
  // A state that may overflow.
  w4: |a, b, c, d| walk(c, |s| s != 0 && be(3, 1) % 8 != 7, |s| s * 3 + be(4, 1) + (s >> 120)) % 5 != 0;
  // A walk in a walk.
  w5: |a, b, c, d| walk(
    0,
    |i| i < (a & 3) + be(14, 1) % 2,
    |i| i + walk(0, |j| j < (b & 3), |j| j + 1 + be(j, 1) % 2) + 1,
  ) < 6;
  // An offset read at, then added to: it is in the value, and the sum fits 64 bits.
  w6: |a, b, c, d| walk(
    (d & 0xffff_ffff_ffff_ffff) % 20,
    |at| at < 14,
    |at| at + 1 + be(at, 1) * (d >> 56),
  ) < 1 << 70;
}
