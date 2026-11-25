//! Tests that the narrowed evaluation of expressions (`narrow`) is the evaluation on `u128`,
//! which is what they mean.
//!
//! The tests run each operation over values at the bounds of the ranges that the analysis finds
//! of its operands. They also run random expressions, with variables, parts and walks, over
//! random and boundary bytes.

use std::{cell::RefCell, collections::HashMap};

use crate::{
  layout::{self, Machine, Op, Varint, check_ops, walk_limit},
  narrow::{self, How, Range, fit, walk_entry},
  steps::Entry,
};

/// A generator of random numbers (xorshift).
struct Random(u64);

impl Random {
  fn next(&mut self) -> u64 {
    self.0 ^= self.0 << 13;
    self.0 ^= self.0 >> 7;
    self.0 ^= self.0 << 17;
    self.0
  }

  fn below(&mut self, n: u64) -> u64 {
    self.next() % n
  }

  fn wide(&mut self) -> u128 {
    u128::from(self.next()) << 64 | u128::from(self.next())
  }

  /// Returns a value of `lo..=hi`: one of its bounds, one next to a bound, or any.
  fn within(&mut self, lo: u64, hi: u64) -> u64 {
    match self.below(6) {
      0 => lo,
      1 => hi,
      2 => lo.saturating_add(1).min(hi),
      3 => hi.saturating_sub(1).max(lo),
      _ => lo + self.next() % (hi - lo).saturating_add(1).max(1),
    }
  }
}

const P32: u128 = 1 << 32;
const P64: u128 = 1 << 64;
const SIZE: u128 = isize::MAX as u128;

/// Returns ranges of operands: exact values, ranges at the bounds of the widths, and halves, with
/// a number in each, as in the state of a walk.
fn ranges() -> Vec<Range> {
  let mut ranges = vec![];
  for value in [0, 1, 2, 7, 63, 64, 65, 127, 128, 255, P32 - 1, P32, P64 - 1, P64, 1 << 127] {
    ranges.push(Range::exact(value));
  }
  ranges.push(Range::exact(u128::MAX));
  for (min, max) in [
    (0, 1),
    (0, 255),
    (0, 0xffff),
    (0, P32 - 1),
    (0, P32),
    (P32 - 2, P32 + 1),
    (1, 1 << 63),
    (0, SIZE),
    (0, P64 - 1),
    (1 << 63, P64 - 1),
    (P64 - 3, P64 + 3),
    (0, P64),
    (0, 1 << 65),
    (1 << 100, 1 << 101),
    (1, u128::MAX),
    (u128::MAX - 5, u128::MAX),
    (0, u128::MAX),
  ] {
    ranges.push(Range::new(min, max));
  }
  let max = u64::MAX;
  for (hi, lo) in [
    ((0, 255), (0, max)),
    ((0, 255), (8, 1 << 63)),
    ((3, 3), (0, 40)),
    ((1, 2), (max - 1, max)),
    ((max, max), (0, 10)),
    ((0, 1), (0, max >> 1)),
  ] {
    ranges.push(Range::halves(hi, lo));
  }
  ranges
}

/// Returns `count` values of `range`, at its bounds and anywhere in it.
fn values(range: Range, random: &mut Random, count: usize) -> Vec<u128> {
  let (hi, lo) = ((range.min() >> 64) as u64, (range.max() >> 64) as u64);
  let (low, high) = (range.min() as u64, range.max() as u64);
  let mut values = vec![range.min(), range.max()];
  while values.len() < count {
    let h = random.within(hi, lo);
    let l = random.within(low, high);
    let value = u128::from(h) << 64 | u128::from(l);
    assert!(range.holds(value));
    values.push(value);
  }
  values
}

const BINARY: [Op; 16] = [
  Op::Add,
  Op::Sub,
  Op::Mul,
  Op::Div,
  Op::Rem,
  Op::And,
  Op::Or,
  Op::Xor,
  Op::Shl,
  Op::Shr,
  Op::Eq,
  Op::Ne,
  Op::Lt,
  Op::Le,
  Op::Gt,
  Op::Ge,
];

/// Every binary operation of every pair of ranges, over values at their bounds and others: the
/// narrowed computation is the one on `u128`, the result is in the range that the analysis says,
/// and an operation that it says cannot fail does not.
#[test]
#[cfg_attr(
  miri,
  ignore = "arithmetic, without unsafe code: `expressions_are_the_ones_on_u128` reads"
)]
fn each_operation_is_the_one_on_u128() {
  let mut random = Random(0x2545_f491_4f6c_dd1d);
  let ranges = ranges();
  let count = 12;
  let (mut narrowed, mut unchecked, mut halves) = (0, 0, 0);
  for &a in &ranges {
    for &b in &ranges {
      // As on a target of 64 bits, and of 32 (where each width is computed in).
      for (op, register) in BINARY.into_iter().flat_map(|op| [(op, 64), (op, 32)]) {
        let (how, range) = narrow::binary_in(op, a, b, register);
        narrowed += usize::from(how.width < 128);
        unchecked += usize::from(!how.fails);
        halves += usize::from(how.halves);
        for x in values(a, &mut random, count) {
          for y in values(b, &mut random, count) {
            let wide = layout::binary(op, x, y);
            let value = narrow::binary(op, how, x, y).map(|value| fit(value, how.result));
            assert_eq!(value, wide, "{op:?} of {x} and {y}, {how:?} ({a:?}, {b:?})");
            match wide {
              Some(value) => {
                assert!(range.holds(value), "{op:?} of {x} and {y} is {value}, not in {range:?}");
              }
              None => assert!(how.fails, "{op:?} of {x} and {y} fails, {how:?}"),
            }
          }
        }
      }
    }
  }
  // The operations are not all on `u128` with every test: what is checked is narrowed.
  assert!(narrowed > 5000 && unchecked > 5000 && halves > 100, "{narrowed} {unchecked} {halves}");
}

// -------------------------------------------------------------------------------------------------
// Random expressions

/// The bytes of a struct read by the expressions: its fields are its first `HEAD` bytes, and it
/// may be longer (an unsized struct, which `be(..)` reads).
const HEAD: usize = 40;
const BYTES: usize = 64;

/// What the generator of an expression has defined, and how deep it is.
#[derive(Copy, Clone)]
struct Scope {
  /// The variables defined, a bit for each.
  defined: u8,
  /// How many more levels the expression may have.
  depth: u32,
  /// How many walks the expression is in.
  walks: usize,
}

impl Scope {
  fn deeper(self) -> Self {
    Scope { depth: self.depth.saturating_sub(1), ..self }
  }

  fn free(self) -> Option<u8> {
    (0..8).find(|&i| self.defined & (1 << i) == 0)
  }
}

/// The steps of a walk under Miri, where a step of a walk that runs to the limit of the library
/// (`walk_limit`, 640 steps for 64 bytes) takes milliseconds, and the evaluations that are compared
/// take the same steps: a walk that does not end within them fails in both.
const MIRI_WALK_STEPS: usize = 16;

/// Returns a constant at the bounds of the widths, or a small one.
fn constant(random: &mut Random) -> u128 {
  const BOUNDS: [u128; 20] = [
    0,
    1,
    2,
    3,
    8,
    40,
    63,
    64,
    65,
    127,
    128,
    255,
    0xffff,
    P32 - 1,
    P32,
    P64 - 1,
    P64,
    SIZE,
    1 << 127,
    u128::MAX,
  ];
  match random.below(4) {
    0 => random.wide() >> random.below(128),
    1 => u128::from(random.below(16)),
    _ => BOUNDS[random.below(BOUNDS.len() as u64) as usize],
  }
}

/// Appends to `ops` a random expression, compiled to operations as `#[derive(Pun)]` compiles one.
fn expression(random: &mut Random, scope: Scope, ops: &mut Vec<Op>) {
  let leaf = scope.depth == 0 || random.below(5) == 0;
  if leaf {
    let variables = (0..8).filter(|&i| scope.defined & (1 << i) != 0).collect::<Vec<u8>>();
    match random.below(5) {
      0 | 1 if !variables.is_empty() => {
        ops.push(Op::Get(variables[random.below(variables.len() as u64) as usize]));
      }
      0..=2 => ops.push(Op::Lit(constant(random))),
      3 => {
        let size = [1u8, 2, 4, 8, 16][random.below(5) as usize];
        let offset = random.below((HEAD - usize::from(size) + 1) as u64) as usize;
        ops.push(Op::Load {
          offset,
          size,
          big: random.below(2) == 0,
          signed: random.below(4) == 0,
        });
      }
      _ => ops.push(Op::Size),
    }
    return;
  }
  let inner = scope.deeper();
  match random.below(16) {
    // A read at an offset: small ones, mostly, which are in the value.
    0 | 1 => {
      if random.below(2) == 0 {
        expression(random, inner, ops);
      } else {
        ops.push(Op::Lit(u128::from(random.below(BYTES as u64 + 4))));
      }
      let size = [1u8, 2, 4, 8, 16][random.below(5) as usize];
      ops.push(Op::At { size, big: random.below(2) == 0 });
    }
    2 => {
      expression(random, inner, ops);
      let form = [Varint::Leb128, Varint::Quic, Varint::Vlq][random.below(3) as usize];
      ops.push(Op::Varint { form, length: random.below(2) == 0 });
    }
    3 => {
      expression(random, inner, ops);
      expression(random, inner, ops);
      ops.push(Op::Sum16);
    }
    4 => {
      expression(random, inner, ops);
      ops.push(Op::Not);
    }
    // `&&` and `||`.
    5 => {
      let either = random.below(2) == 0;
      expression(random, inner, ops);
      let mut right = vec![];
      expression(random, inner, &mut right);
      ops.push(Op::Shortcut { either, len: right.len() as u16 + 1 });
      ops.extend(right);
      ops.push(if either { Op::Either } else { Op::Both });
    }
    // `if`.
    6 => {
      expression(random, inner, ops);
      let (mut then, mut other) = (vec![], vec![]);
      expression(random, inner, &mut then);
      expression(random, inner, &mut other);
      ops.push(Op::Then(then.len() as u16 + 1));
      ops.extend(then);
      ops.push(Op::Else(other.len() as u16));
      ops.extend(other);
      ops.push(Op::Select);
    }
    // `let`.
    7 if scope.free().is_some() => {
      let index = scope.free().unwrap_or(0);
      expression(random, inner, ops);
      ops.push(Op::Set(index));
      expression(random, Scope { defined: scope.defined | 1 << index, ..inner }, ops);
    }
    8 if scope.walks < 2 && scope.free().is_some() => walk(random, scope, ops),
    _ => {
      expression(random, inner, ops);
      expression(random, inner, ops);
      ops.push(BINARY[random.below(BINARY.len() as u64) as usize]);
    }
  }
}

thread_local! {
  /// The expressions of the walks of an expression, which are `&'static`, until it is tested.
  static KEPT: RefCell<Vec<*mut [Op]>> = const { RefCell::new(Vec::new()) };
}

/// Returns `ops` as a `&'static` slice, valid for as long as the expression it is a part of is
/// tested, until [`release`].
fn keep(ops: Vec<Op>) -> &'static [Op] {
  let ops = Box::leak(ops.into_boxed_slice());
  KEPT.with_borrow_mut(|kept| kept.push(core::ptr::from_mut(ops)));
  ops
}

/// Ends the test of an expression: frees what it kept, and forgets its plans.
fn release() {
  PLANS.with_borrow_mut(HashMap::clear);
  for ops in KEPT.take() {
    // SAFETY: `ops` is a pointer that `Box::leak` returned in `keep`, of a `Box<[Op]>` of the
    // global allocator, as `Box::from_raw` requires. `KEPT.take()` empties the list, so it is
    // freed once. Each test calls `release` only after its last use of the expressions that
    // `keep` made, so no reference to it is used after this.
    drop(unsafe { Box::from_raw(ops) });
  }
}

/// Appends to `ops` a walk: a random one, or one of the shapes that walks have.
///
/// Such a shape is an offset that moves by what it reads, or two numbers packed in the halves of
/// the state, as in the walk of IPv6.
fn walk(random: &mut Random, scope: Scope, ops: &mut Vec<Op>) {
  const MASK: u128 = u64::MAX as u128;
  let var = scope.free().unwrap_or(0);
  let inner =
    Scope { defined: scope.defined | 1 << var, depth: scope.depth / 2, walks: scope.walks + 1 };
  let (mut cond, mut next) = (vec![], vec![]);
  match random.below(3) {
    // `walk(START, |at| be(at, 1) != 0, |at| at + 1 + be(at, 1) % 4)`, or another step.
    0 => {
      ops.push(Op::Lit(u128::from(random.below(HEAD as u64))));
      cond.extend([Op::Get(var), Op::At { size: 1, big: true }, Op::Lit(0), Op::Ne]);
      next.extend([Op::Get(var), Op::Lit(1), Op::Add, Op::Get(var), Op::At { size: 1, big: true }]);
      next.extend([Op::Lit(u128::from(random.below(5) + 1)), Op::Rem, Op::Add]);
    }
    // The walk of IPv6: the kind in the high half, the offset in the low one.
    1 => {
      ops.extend([
        Op::Load { offset: 6, size: 1, big: false, signed: false },
        Op::Lit(64),
        Op::Shl,
      ]);
      ops.extend([Op::Lit(u128::from(random.below(HEAD as u64))), Op::Add]);
      let kind = u128::from(random.below(256));
      cond.extend([Op::Get(var), Op::Lit(64), Op::Shr, Op::Lit(kind), Op::Lt]);
      let start = [Op::Get(var), Op::Lit(MASK), Op::And];
      next.extend(start);
      next.extend([Op::At { size: 1, big: true }, Op::Lit(64), Op::Shl]);
      next.extend(start);
      next.extend([Op::Add, Op::Lit(u128::from(random.below(9))), Op::Add]);
      next.extend(start);
      next.extend([Op::Lit(1), Op::Add, Op::At { size: 1, big: true }, Op::Lit(8), Op::Mul]);
      next.push(Op::Add);
    }
    _ => {
      expression(random, scope.deeper(), ops);
      expression(random, inner, &mut cond);
      expression(random, inner, &mut next);
    }
  }
  ops.push(Op::Walk { var, cond: keep(cond), next: keep(next) });
}

/// How each operation of an expression is computed, and where each of its walks starts.
type Planned = (Vec<How>, Vec<Option<Entry>>);

thread_local! {
  /// The plans of an expression and of the expressions of its walks, which are the same at each
  /// step of a walk, made once.
  static PLANS: RefCell<HashMap<(*const Op, usize), Planned>> = RefCell::new(HashMap::new());
}

/// Applies the operations `ops` of an expression whose variables start as `entry` says.
///
/// With `narrowed`, each operation is computed as the analysis says. Otherwise every operation is
/// computed on `u128` with every test ([`How::WIDE`]). This is what `validate::steps` and
/// `validate::walk` do, with the operations known at run time. Returns `false` if the expression
/// fails.
fn run(ops: &[Op], entry: &Entry, narrowed: bool, machine: &mut Machine, bytes: &[u8]) -> bool {
  let (hows, entries) = PLANS.with_borrow_mut(|plans| {
    plans
      .entry((ops.as_ptr(), ops.len()))
      .or_insert_with(|| {
        let hows = narrow::plan(ops, entry).how[..ops.len()].to_vec();
        let entries = (0..ops.len())
          .map(|at| matches!(ops[at], Op::Walk { .. }).then(|| walk_entry(ops, entry, at)))
          .collect();
        (hows, entries)
      })
      .clone()
  });
  let (base, size) = (bytes.as_ptr(), bytes.len());
  for (at, &op) in ops.iter().enumerate() {
    let how = if narrowed { hows[at] } else { How::WIDE };
    if let Op::Walk { var, cond, next } = op {
      if !machine.computes(at) {
        continue;
      }
      let inner = entries[at].unwrap_or(*entry);
      let mut state = fit(machine.top(), how.result);
      let (mut steps, limit) = (0, if cfg!(miri) { MIRI_WALK_STEPS } else { walk_limit(size) });
      let result = loop {
        let (mut c, mut n) = (machine.enter(var, state), machine.enter(var, state));
        if !run(cond, &inner, narrowed, &mut c, bytes) {
          break None;
        }
        if c.result() == 0 {
          break Some(state);
        }
        if steps == limit {
          break None;
        }
        steps += 1;
        if !run(next, &inner, narrowed, &mut n, bytes) {
          break None;
        }
        state = fit(n.result(), how.result);
      };
      if !machine.replace(result) {
        return false;
      }
    // SAFETY: Every expression that is run passed `check_ops` with `HEAD`, and so did the
    // expressions of its walks, so a load reads 1 to 16 bytes within the first `HEAD` bytes.
    // `bytes` has `HEAD` or `BYTES` bytes, at least `HEAD`, and `size` is their number. They are
    // valid for reads and initialized, since they are the `u8`s of a slice. These are the
    // requirements of `Machine::apply`.
    } else if !unsafe { machine.apply(op, at, how, base, size) } {
      return false;
    }
  }
  true
}

fn evaluate(ops: &[Op], narrowed: bool, bytes: &[u8]) -> Option<u128> {
  let mut machine = Machine::NEW;
  run(ops, &Entry::RULE, narrowed, &mut machine, bytes).then(|| machine.result())
}

/// Returns the bytes of a struct: random, small (offsets that are in the value, walks that end),
/// zeros, ones, and as long as its fields or longer.
fn bytes(random: &mut Random) -> Vec<u8> {
  let len = if random.below(2) == 0 { HEAD } else { BYTES };
  let kind = random.below(5);
  (0..len)
    .map(|_| match kind {
      0 => 0,
      1 => 0xff,
      2 => random.below(8) as u8,
      3 => [0, 1, 0x7f, 0x80, 0xff][random.below(5) as usize],
      _ => random.next() as u8,
    })
    .collect()
}

/// Random expressions, narrowed as the analysis says and on `u128` with every test, over random
/// and boundary bytes: the same value, or the same failure.
#[test]
fn expressions_are_the_ones_on_u128() {
  let mut random = Random(0x9e37_79b9_7f4a_7c15);
  let (programs, inputs) = if cfg!(miri) { (6, 2) } else { (2000, 16) };
  let (mut narrowed, mut ops_count, mut values, mut failures) = (0, 0, 0, 0);
  for _ in 0..programs {
    let mut ops = vec![];
    expression(&mut random, Scope { defined: 0, depth: 5, walks: 0 }, &mut ops);
    if ops.len() > layout::MAX_OPS {
      release();
      continue;
    }
    check_ops(&ops, HEAD, false, 0, 0);
    let plan = narrow::plan(&ops, &Entry::RULE);
    ops_count += ops.len();
    narrowed += plan.how[..ops.len()].iter().filter(|how| **how != How::WIDE).count();
    for _ in 0..inputs {
      let bytes = bytes(&mut random);
      let wide = evaluate(&ops, false, &bytes);
      assert_eq!(evaluate(&ops, true, &bytes), wide, "{ops:?} over {bytes:?}");
      if let Some(value) = wide {
        assert!(
          plan.result.holds(value),
          "{ops:?} over {bytes:?}: {value} not in {:?}",
          plan.result
        );
        values += 1;
      } else {
        failures += 1;
      }
    }
    release();
  }
  // Most operations are narrowed, and expressions both have values and fail.
  if !cfg!(miri) {
    assert!(
      narrowed * 2 > ops_count && values > 10_000 && failures > 10_000,
      "{narrowed} of {ops_count}, {values}, {failures}"
    );
  }
}

/// The walk of IPv6 (`examples/codegen.rs`): its state is two numbers, one in each half, and the
/// step adds to the low one what does not carry: each half is computed alone.
#[test]
fn the_walk_of_ipv6_is_two_halves() {
  const MASK: u128 = u64::MAX as u128;
  let mut next = vec![Op::Get(0), Op::Lit(MASK), Op::And, Op::Set(1)];
  next.extend([Op::Get(1), Op::At { size: 1, big: true }, Op::Lit(64), Op::Shl]);
  next.extend([Op::Get(1), Op::Add, Op::Lit(8), Op::Add, Op::Get(1), Op::Lit(1), Op::Add]);
  next.extend([Op::At { size: 1, big: true }, Op::Lit(8), Op::Mul, Op::Add]);
  let next = keep(next);
  let cond = keep(vec![Op::Get(0), Op::Lit(64), Op::Shr, Op::Lit(60), Op::Lt]);
  let ops = [
    Op::Load { offset: 6, size: 1, big: false, signed: false },
    Op::Lit(64),
    Op::Shl,
    Op::Lit(40),
    Op::Add,
    Op::Walk { var: 0, cond, next },
  ];
  check_ops(&ops, HEAD, false, 0, 0);
  let entry = walk_entry(&ops, &Entry::RULE, 5);
  let step = narrow::plan(next, &entry);
  // `start + 8` and the length: the low half alone.
  for at in [9, 11, 14, 18] {
    assert!(step.how[at].halves || step.how[at].width < 128, "{at}: {:?}", step.how[at]);
    assert!(!step.how[at].fails);
  }
  // The state: a kind of a byte in the high half, an offset in the low one.
  let plan = narrow::plan(&ops, &Entry::RULE);
  assert_eq!(plan.how[5].result, 128);
  assert!(plan.result.holds(255 << 64 | u128::from(u64::MAX)) && !plan.result.holds(256 << 64));
  release();
}

/// A read at a variable tells that it is in the value only where the read is computed: not after
/// a part that may be skipped (the right side of a `&&`, a branch of an `if`), and not in the
/// `else` branch after the first one read it.
#[test]
fn parts_tell_what_they_compute() {
  let at = Op::At { size: 1, big: true };
  // `v` is the 16 bytes at 8: mostly more than any offset.
  let v = [Op::Load { offset: 8, size: 16, big: false, signed: false }, Op::Set(0)];
  let programs: [&[Op]; 3] = [
    // `{ let v = ..; (v < 10 && be(v, 1) == 0) as u128 + v }`
    &[
      v[0],
      v[1],
      Op::Get(0),
      Op::Lit(10),
      Op::Lt,
      Op::Shortcut { either: false, len: 5 },
      Op::Get(0),
      at,
      Op::Lit(0),
      Op::Eq,
      Op::Both,
      Op::Get(0),
      Op::Add,
    ],
    // `{ let v = ..; (if v < 10 { be(v, 1) } else { 0 }) + v }`
    &[
      v[0],
      v[1],
      Op::Get(0),
      Op::Lit(10),
      Op::Lt,
      Op::Then(3),
      Op::Get(0),
      at,
      Op::Else(1),
      Op::Lit(0),
      Op::Select,
      Op::Get(0),
      Op::Add,
    ],
    // `{ let v = ..; if v < 10 { be(v, 1) } else { v } }`
    &[
      v[0],
      v[1],
      Op::Get(0),
      Op::Lit(10),
      Op::Lt,
      Op::Then(3),
      Op::Get(0),
      at,
      Op::Else(1),
      Op::Get(0),
      Op::Select,
    ],
  ];
  let mut random = Random(0x1234_5678_9abc_def1);
  for ops in programs {
    check_ops(ops, HEAD, false, 0, 0);
    release();
    for _ in 0..if cfg!(miri) { 4 } else { 64 } {
      let mut bytes = bytes(&mut random);
      // Small values of `v` too, which are read at.
      if random.below(2) == 0 {
        bytes[8..24].copy_from_slice(&u128::from(random.below(12)).to_le_bytes());
      }
      assert_eq!(
        evaluate(ops, true, &bytes),
        evaluate(ops, false, &bytes),
        "{ops:?} over {bytes:?}"
      );
    }
  }
}
