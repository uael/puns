//! Tests that an expression of `#[pun(..)]` means what the same Rust expression means when every
//! integer is a `u128`.
//!
//! Each test is a regression test for a case where the two meanings differed.

use puns::{Pun, num::u16be, try_pun_ref};

/// A struct whose rules use `as`, which truncates as it does in Rust.
#[derive(Pun)]
#[repr(C)]
#[pun(assert((a + b) as u8 == 0))]
#[pun(assert((a * 0x101) as u16 == a * 0x101 && (a << 60) as u64 == (a % 16) << 60))]
struct Cast {
  a: u8,
  b: u8,
}

#[test]
fn casts_truncate() {
  assert!(try_pun_ref::<_, Cast>(&[200u8, 56]).is_ok());
  assert!(try_pun_ref::<_, Cast>(&[0u8, 0]).is_ok());
  assert!(try_pun_ref::<_, Cast>(&[200u8, 55]).is_err());
  assert!(try_pun_ref::<_, Cast>(&[16u8, 240]).is_ok());
  assert!(try_pun_ref::<_, Cast>(&[1u8, 254]).is_err());
}

/// A struct whose data is as long as its `length` field, rounded up to a multiple of 4. The
/// rounding uses `!`, which is bitwise, as in Rust.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 4 + ((length + 3) & !3))]
struct Padded {
  kind: u16be,
  length: u16be,
  data: [u8],
}

#[test]
fn not_of_an_integer_is_bitwise() {
  let (padded, rest) = puns::from_head::<Padded>(&[0, 1, 0, 5, 1, 2, 3, 4, 5, 0, 0, 0, 9]).unwrap();
  assert_eq!((padded.kind.get(), padded.data.len(), rest), (1, 8, &[9][..]));
}

/// A constant that a rule names. In a rule, a constant of any unsigned integer type is read as a
/// `u128`, and so is a byte literal such as `b'a'`.
const SMALL: u8 = 0xff;
const WIDE: u64 = 0x1_0000_0000;

#[derive(Pun)]
#[repr(C)]
#[pun(assert(x + SMALL as u128 > 0xff && x + SMALL > 0xff || x == 0))]
#[pun(assert(x < u8::MAX && (x as u64 + WIDE) as u32 == x as u32 && x != b'!' as u128))]
#[pun(assert(1u8 + x > 0 && 0x1_0000 * x != 1))]
struct Constants {
  x: u8,
}

/// A `const fn` that a rule calls with constants, by its path. Rust computes the call.
const fn twice(n: u8) -> u8 {
  n * 2
}

#[derive(Pun)]
#[repr(C)]
#[pun(assert(x < self::twice(2) && x < crate::twice(3)))]
struct Called {
  x: u8,
}

#[test]
fn a_const_fn_is_called_by_its_path() {
  assert!(puns::from_bytes::<Called>(&[3]).is_ok());
  assert!(puns::from_bytes::<Called>(&[4]).is_err());
}

#[test]
fn constants_are_unsigned_integers() {
  assert!(puns::from_bytes::<Constants>(&[1]).is_ok());
  assert!(puns::from_bytes::<Constants>(&[0]).is_ok());
  assert!(puns::from_bytes::<Constants>(b"!").is_err());
  assert!(puns::from_bytes::<Constants>(&[0xff]).is_err());
}

/// A struct whose rule shifts by a field. `<<` drops the bits shifted out, as in Rust.
///
/// A shift by 128 or more fails the rule. Rust panics there with debug assertions, and shifts by
/// the amount modulo 128 without them.
#[derive(Pun)]
#[repr(C)]
#[pun(assert((x << 127) << 1 == 0 && x << y > 0 && x >> y < 0x100))]
struct Shifts {
  x: u8,
  y: u8,
}

#[test]
fn shifts_drop_bits_and_fail_beyond_the_width() {
  assert!(puns::from_bytes::<Shifts>(&[1, 0]).is_ok());
  assert!(puns::from_bytes::<Shifts>(&[1, 127]).is_ok());
  // `2 << 127` is `0`.
  assert!(puns::from_bytes::<Shifts>(&[2, 127]).is_err());
  assert!(puns::from_bytes::<Shifts>(&[1, 128]).is_err());
  assert!(puns::from_bytes::<Shifts>(&[0xff, 200]).is_err());
}

/// A struct whose rules depend on the order of evaluation of `&&` and `||`.
///
/// As in Rust, the left operand is evaluated first. The right operand is evaluated only if the
/// left one does not decide the result. A failure in the left operand fails the rule.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(be(1, 2) == 0xcafe || self.0 != 7))]
#[pun(assert(10 - self.0 > 0 && self.0 != 8 || true))]
struct LeftFirst(u8, [u8]);

#[test]
fn and_or_compute_the_left_side_first() {
  assert!(puns::from_bytes::<LeftFirst>(&[7, 0xca, 0xfe]).is_ok());
  assert!(puns::from_bytes::<LeftFirst>(&[1, 0xca, 0xfe]).is_ok());
  assert!(puns::from_bytes::<LeftFirst>(&[1, 0, 0]).is_ok());
  // Out of the value, on the left: as Rust would panic, the rule fails.
  assert!(puns::from_bytes::<LeftFirst>(&[1]).is_err());
  // `10 - 11` fails, whatever the right side.
  assert!(puns::from_bytes::<LeftFirst>(&[11, 0xca, 0xfe]).is_err());
  assert!(puns::from_bytes::<LeftFirst>(&[8, 0xca, 0xfe]).is_ok());
}

/// A name that ends with a zero byte, whose end the rule needs only for some flags. As in Rust,
/// the expression that a `let` binds is evaluated whether the binding is used or not.
#[derive(Pun)]
#[repr(C)]
#[pun(assert({
  let end = walk(1, |at| be(at, 1) != 0, |at| at + 1);
  flags & 2 == 0 || end + 1 == size_of_val(self)
}))]
struct Named {
  flags: u8,
  name: [u8],
}

/// A struct whose rule has a `let` in a branch of an `if`. The `let` is not evaluated when its
/// branch is not taken.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(if flags & 4 != 0 { let x = be(1, 1); x == 0 } else { true }))]
struct Branch {
  flags: u8,
  rest: [u8],
}

/// A struct whose rule binds values that it does not need. `x - 1` and `x / y` can fail, and
/// `x + 40` cannot, because `x` is a byte.
#[derive(Pun)]
#[repr(C)]
#[pun(assert({ let less = x - 1; let ratio = x / y; let more = x + 40; x == 0 || less + ratio + more > 0 }))]
struct Unused {
  x: u8,
  y: u8,
}

#[test]
fn what_a_let_binds_is_computed() {
  assert!(puns::from_bytes::<Unused>(&[1, 1]).is_ok());
  assert!(puns::from_bytes::<Unused>(&[0, 1]).is_err());
  assert!(puns::from_bytes::<Unused>(&[1, 0]).is_err());
  assert!(puns::from_bytes::<Named>(&[0, b'a', 0]).is_ok());
  assert!(puns::from_bytes::<Named>(&[2, b'a', 0]).is_ok());
  // No end: the walk fails, whatever the flags.
  assert!(puns::from_bytes::<Named>(&[2, b'a']).is_err());
  assert!(puns::from_bytes::<Named>(&[0, b'a']).is_err());
  assert!(puns::from_bytes::<Branch>(&[0]).is_ok());
  assert!(puns::from_bytes::<Branch>(&[4, 0]).is_ok());
  assert!(puns::from_bytes::<Branch>(&[4]).is_err());
}

/// A netmask, which is ones followed by zeros. Its walk takes a step per bit, more steps than the
/// value has bytes.
#[derive(Pun)]
#[repr(C)]
#[pun(assert({
  let ones = walk(0, |i| i < 32 && (mask >> (31 - i)) & 1 == 1, |i| i + 1);
  (mask << ones) as u32 == 0
}))]
struct Netmask {
  mask: puns::num::u32be,
}

#[test]
fn a_walk_takes_a_step_per_bit() {
  assert!(puns::from_bytes::<Netmask>(&[0xff, 0xff, 0xff, 0]).is_ok());
  assert!(puns::from_bytes::<Netmask>(&[0xff, 0xff, 0xff, 0xff]).is_ok());
  assert!(puns::from_bytes::<Netmask>(&[0, 0, 0, 0]).is_ok());
  assert!(puns::from_bytes::<Netmask>(&[0xff, 0xff, 0, 0xff]).is_err());
}

/// A struct whose walk fails in its step. A failure in any expression of a walk fails the walk.
/// Here the step adds a term that can fail to `at + 1`, which alone would end the walk at `4`.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(walk(0, |at| at < 4, |at| at + 1 + be(at + 1, 1) * 0) == 4))]
struct FailingStep {
  first: u8,
  rest: [u8],
}

/// A struct whose walk fails in its condition, as `FailingStep` fails in its step.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(walk(0, |at| at + be(at, 1) * 0 < 4, |at| at + 1) == 4))]
struct FailingCondition {
  first: u8,
  rest: [u8],
}

/// A struct whose rules hold walks that read past the value when they are evaluated.
///
/// A walk that is not evaluated does not fail. The walks are on the right of a `||`, as the first
/// walk of a rule and as a second one, and in a branch of an `if`.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(kind != 1 || walk(0, |at| be(at, 1) != 0, |at| at + 1) > 0))]
#[pun(assert(
  walk(0, |at| at < 1, |at| at + 1) == 1 && (kind != 2 || walk(0, |at| be(at, 1) != 0, |at| at + 1) > 0)
))]
#[pun(assert(if kind == 3 { walk(0, |at| at < 1000, |at| at + be(at, 1)) > 0 } else { true }))]
struct NotWalked {
  kind: u8,
  rest: [u8],
}

#[test]
fn a_walk_fails_where_its_expressions_do() {
  assert!(puns::from_bytes::<NotWalked>(&[0]).is_ok());
  assert!(puns::from_bytes::<NotWalked>(&[4, 4]).is_ok());
  assert!(puns::from_bytes::<NotWalked>(&[1, 0]).is_ok());
  assert!(puns::from_bytes::<NotWalked>(&[2, 0]).is_ok());
  // Computed, they read past the value.
  assert!(puns::from_bytes::<NotWalked>(&[1, 1]).is_err());
  assert!(puns::from_bytes::<NotWalked>(&[2, 1]).is_err());
  assert!(puns::from_bytes::<NotWalked>(&[3, 0]).is_err());
  assert!(puns::from_bytes::<FailingStep>(&[0; 5]).is_ok());
  assert!(puns::from_bytes::<FailingStep>(&[0; 4]).is_err());
  assert!(puns::from_bytes::<FailingCondition>(&[0; 5]).is_ok());
  assert!(puns::from_bytes::<FailingCondition>(&[0; 4]).is_err());
}

/// A struct whose getters are Rust expressions. In a getter, an integer field is its value as a
/// `u128`, wherever it is named.
#[derive(Pun)]
#[repr(C)]
#[pun(get(
  magnitude: u32 = if height >> 31 != 0 { 0x1_0000_0000 - height } else { height },
  value: f64 = f64::from_bits(bits as u64),
  low: u8 = { let bits = bits & 0xff; bits as u8 },
  wide: u32 = self.height.wrapping_add(0) as u32,
))]
struct Getters {
  height: puns::num::u32le,
  bits: puns::num::u64le,
}

/// A struct with a getter over a field of a generic type, which is not known to be `Copy`.
#[derive(Pun)]
#[repr(C)]
#[pun(get(is_two: bool = kind == 2))]
struct Generic<H> {
  kind: H,
}

#[test]
fn getters_are_rust() {
  let mut bytes = [0u8; 12];
  bytes[..4].copy_from_slice(&(-3i32).to_le_bytes());
  bytes[4..].copy_from_slice(&1.5f64.to_bits().to_le_bytes());
  let getters = puns::from_bytes::<Getters>(&bytes).unwrap();
  assert_eq!((getters.magnitude(), getters.value(), getters.low()), (3, 1.5, 0));
  assert_eq!(getters.wide(), 0xffff_fffd);
  assert!(puns::from_bytes::<Generic<u8>>(&[2]).unwrap().is_two());
  assert!(!puns::from_bytes::<Generic<u8>>(&[3]).unwrap().is_two());
}

/// A struct whose parameters have the bounds that the derive needs. The derive does not repeat
/// these bounds, or clippy would report `trait_duplication_in_bounds`.
#[derive(Pun)]
#[repr(C)]
struct Bounded<T: Pun, U>
where
  U: puns::Pun,
{
  value: T,
  other: U,
  again: T,
}

/// A struct whose `#[pun(bound(..))]` bounds a parameter that has bounds of its own. The derive
/// adds the bound to them, or clippy would report `type_repetition_in_bounds`.
#[derive(Pun)]
#[pun(bound(T: Pun))]
#[repr(C)]
struct Relaxed<T: ?Sized> {
  value: u8,
  kind: core::marker::PhantomData<T>,
}

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Choice<T: Pun + Copy> {
  A(T),
  B(T, T),
}

#[test]
fn bounds_are_not_repeated() {
  let bounded = puns::from_bytes::<Bounded<u8, bool>>(&[1, 1, 2]).unwrap();
  assert_eq!((bounded.value, bounded.other, bounded.again), (1, true, 2));
  assert_eq!(puns::from_bytes::<Relaxed<str>>(&[7]).map(|relaxed| relaxed.value), Ok(7));
  assert!(matches!(puns::try_pun::<_, Choice<bool>>([1u8, 1, 0]), Ok(Choice::B(true, false))));
  assert!(matches!(puns::try_pun::<_, Choice<bool>>([0u8, 1, 0]), Ok(Choice::A(true))));
  for choice in [Choice::A(true), Choice::B(false, true)] {
    let same: Choice<bool> = puns::pun(choice);
    assert!(puns::try_pun::<_, Choice<u8>>(same).is_ok());
  }
}

/// A struct whose parameter only a typed region names. A `PhantomData` field uses the parameter,
/// as Rust requires.
#[derive(Pun)]
#[repr(C)]
struct Packet<P: ?Sized + Pun> {
  kind: u8,
  payload: core::marker::PhantomData<P>,
  #[pun(regions(body: P))]
  rest: [u8],
}

#[test]
fn a_typed_region_of_a_parameter() {
  let packet = puns::from_bytes::<Packet<[bool]>>(&[7, 1, 0]).unwrap();
  assert_eq!((packet.kind, packet.body()), (7, Ok(&[true, false][..])));
  assert!(puns::from_bytes::<Packet<[bool]>>(&[7, 2]).unwrap().body().is_err());
}

/// Signed fields, which a rule reads under a cast, as Rust converts them: sign-extended to 128
/// bits, then truncated to the type of the cast.
#[derive(Pun)]
#[repr(C)]
#[pun(assert((small as u128) < 8 && small as u8 != 0x80))]
#[pun(assert(order as u16 == 0xfffe && order as u128 == u128::MAX - 1))]
#[pun(assert(never as u128 == u128::MAX && (self.wrapped as u128) < 0x8000))]
struct Signed {
  small: i8,
  order: puns::num::i16be,
  never: core::num::NonZero<i32>,
  wrapped: Delta,
}

/// A signed integer in a struct of its own, which a rule reads as the integer.
#[derive(Pun)]
#[repr(transparent)]
struct Delta(i16);

#[test]
fn a_signed_field_is_sign_extended() {
  // `small` at 0, `order` at 1 (a `num` type has an alignment of 1), `never` at 4, `wrapped` at 8.
  let bytes = |small: i8, wrapped: i16| {
    let mut bytes = [0u8; 12];
    bytes[0] = small.cast_unsigned();
    bytes[1..3].copy_from_slice(&(-2i16).to_be_bytes());
    bytes[4..8].copy_from_slice(&(-1i32).to_ne_bytes());
    bytes[8..10].copy_from_slice(&wrapped.to_ne_bytes());
    puns::pun::<[u8; 12], [u32; 3]>(bytes)
  };
  let check = |words: [u32; 3]| puns::from_bytes::<Signed>(puns::as_bytes(&words)).map(|_| ());
  assert!(check(bytes(7, 1)).is_ok());
  // -1 is `u128::MAX` under a cast, which is not less than 8, and -128 is `0x80` as a `u8`.
  assert!(check(bytes(-1, 1)).is_err());
  assert!(check(bytes(-128, 1)).is_err());
  assert!(check(bytes(7, -1)).is_err());
}

/// As in Rust, a sum that overflows fails, and a negative number is near `u128::MAX` under a
/// cast.
#[derive(Pun)]
#[repr(C)]
#[pun(assert((value as u128) + 2 > 0))]
struct Overflow {
  value: i8,
}

#[test]
fn a_negative_number_overflows_as_in_rust() {
  assert!(puns::from_bytes::<Overflow>(&[1]).is_ok());
  assert!(puns::from_bytes::<Overflow>(&[(-2i8).cast_unsigned()]).is_err());
}
