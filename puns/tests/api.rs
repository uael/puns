//! Tests the public functions of `puns` on types of every kind.
//!
//! The tests cover puns by value and by reference, values read from the head of bytes, rules,
//! regions, walks, view enums and calls in `const`. Some tests need the `unstable` feature,
//! because they use types whose layout Rust leaves to the compiler.

use core::num::NonZero;

use puns::{
  Error, Pun, as_bytes, from_head, head, head_ref,
  num::{u8c, u16be, u16le, u32bec},
  pun, pun_mut, pun_ref, try_pun, try_pun_mut, try_pun_ref,
};

/// An enum of the `Rust` representation. The compiler chooses its size and the values of its
/// discriminants.
#[cfg(feature = "unstable")]
#[derive(Pun, Debug, Copy, Clone, PartialEq)]
enum Level {
  Low,
  Mid,
  High,
}

#[derive(Pun, Debug, Copy, Clone, PartialEq)]
#[repr(i8)]
enum Sign {
  Minus = -1,
  Zero = 0,
  Plus = 1,
}

#[derive(Pun, Debug, Copy, Clone, PartialEq)]
#[repr(u16)]
enum Wide {
  A = 0x0102,
  B = 0xfffe,
}

/// A struct of the `Rust` representation. The compiler chooses the order of its fields.
#[cfg(feature = "unstable")]
#[derive(Pun, Debug, Copy, Clone, PartialEq)]
struct Rust {
  flag: bool,
  word: u32,
  level: Level,
  bytes: [u8; 2],
}

/// An unsized struct with trailing padding. A value whose `tail` has one byte is 8 bytes long.
#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
struct Padded {
  word: u32,
  tail: [u8],
}

/// A tuple struct whose first field gives the length of its slice (`len = self.0 / 2`).
#[derive(Pun)]
#[repr(C)]
struct List(u8, #[pun(len = self.0 / 2)] [u16le]);

#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
#[pun(assert(kind == 1 || kind == 2 && value >= 0x100))]
struct Checked {
  kind: u8,
  zero: u8c<0>,
  value: u16be,
}

#[test]
fn by_value() {
  assert_eq!(pun::<u32, [u8; 4]>(0x0102_0304), 0x0102_0304_u32.to_ne_bytes());
  assert_eq!(pun::<[bool; 2], [u8; 2]>([true, false]), [1, 0]);
  assert_eq!(pun::<Sign, i8>(Sign::Minus), -1);
  assert_eq!(pun::<u32bec<0xcafe>, [u8; 4]>(u32bec::default()), [0, 0, 0xca, 0xfe]);
  assert_eq!(pun::<NonZero<u8>, Option<NonZero<u8>>>(NonZero::new(3).unwrap()), NonZero::new(3));
  assert_eq!(head::<u32, [u8; 2]>(u32::from_ne_bytes([1, 2, 3, 4])), [1, 2]);

  assert_eq!(try_pun::<u8, Sign>(0xff), Ok(Sign::Minus));
  assert!(matches!(try_pun::<i8, Sign>(2), Err(Error::Validity { .. })));
  assert_eq!(try_pun::<u16, Wide>(0xfffe), Ok(Wide::B));
  assert!(matches!(try_pun::<[u8; 2], Wide>([0, 0]), Err(Error::Validity { .. })));
  assert_eq!(try_pun::<u32, char>(0x1f980), Ok('🦀'));
  assert!(matches!(try_pun::<u32, char>(0xd800), Err(Error::Validity { .. })));
  assert!(matches!(try_pun::<u16, NonZero<u16>>(0), Err(Error::Validity { .. })));
}

/// Checks the layouts that the compiler chose for `Level` and `Rust`.
///
/// Rust leaves the values of `Level` and the order of the fields of `Rust` to the compiler. This
/// test therefore needs the `unstable` feature.
#[cfg(feature = "unstable")]
#[test]
fn default_representation() {
  assert_eq!(try_pun::<u8, Level>(1), Ok(Level::Mid));
  assert!(matches!(try_pun::<u8, Level>(3), Err(Error::Validity { .. })));
  // Whatever the order the compiler chose for the fields.
  let rust = Rust { flag: true, word: 0xdead_beef, level: Level::High, bytes: [7, 8] };
  assert_eq!(try_pun::<[u8; 8], Rust>(pun(rust)), Ok(rust));
  let mut bytes: [u8; 8] = pun(rust);
  bytes.fill(0xff);
  assert!(matches!(try_pun::<[u8; 8], Rust>(bytes), Err(Error::Validity { .. })));
  // Enums, in slices.
  assert_eq!(
    try_pun_ref::<[u8], [Level]>(&[0, 1, 2]),
    Ok(&[Level::Low, Level::Mid, Level::High][..])
  );
  assert!(matches!(try_pun_ref::<[u8], [Level]>(&[0, 1, 3]), Err(Error::Validity { .. })));
  assert_eq!(pun_ref::<[Level], [u8]>(&[Level::High]), &[2]);
}

#[test]
fn by_reference() {
  // (Aligned for the `u32` it is seen as below: nothing tells where a `[u16; 2]` is otherwise,
  // and `try_pun_mut` would rightly answer `Error::Alignment` half of the time.)
  #[repr(align(4))]
  struct Aligned([u16; 2]);
  let mut words = Aligned([0x0102u16, 0x0304]).0;
  let mut aligned = Aligned(words);
  assert_eq!(pun_ref::<_, [u8; 4]>(&words).len(), 4);
  assert_eq!(pun_ref::<[u16], [u8]>(&words[..]).len(), 4);
  assert_eq!(head_ref::<[u16; 2], [u8; 3]>(&words).len(), 3);
  assert_eq!(head_ref::<[u16; 2], [[u8; 3]]>(&words).len(), 1);
  pun_mut::<[u16], [[u8; 2]]>(&mut words[..])[1] = [0, 0];
  assert_eq!(words, [0x0102, 0]);
  *try_pun_mut::<[u16; 2], u32>(&mut aligned.0).unwrap() = u32::MAX;
  assert_eq!(aligned.0, [0xffff; 2]);
  // The second of two `u16`s that are where a `u32` can be is not where a `u32` can be.
  let mut pairs = Aligned([0; 2]);
  let second = try_pun_mut::<[u16], [u8; 2]>(&mut pairs.0[1..]).unwrap();
  assert!(try_pun_ref::<[u8; 2], u16>(second).is_ok());
  assert_eq!(try_pun_ref::<[u8], [u32]>(&second[..]).err(), Some(Error::Alignment));

  // A greater alignment: the address is checked.
  let bytes = pun_ref::<_, [u8; 8]>(&[1u32, 2]);
  assert_eq!(try_pun_ref::<[u8], [u32]>(&bytes[..]), Ok(&[1u32, 2][..]));
  assert_eq!(try_pun_ref::<[u8], [u32]>(&bytes[1..5]), Err(Error::Alignment));
  assert_eq!(try_pun_ref::<[u8], [u32]>(&bytes[..6]), Err(Error::Size));
  assert_eq!(try_pun_ref::<[u8], u32>(&bytes[4..]), Ok(&2));

  // Trailing padding: 5 bytes of data, 8 bytes of value.
  let padded: &Padded = try_pun_ref(&bytes[..]).unwrap();
  assert_eq!((padded.word, &padded.tail, size_of_val(padded)), (1, &2u32.to_ne_bytes()[..], 8));
  assert_eq!(try_pun_ref::<[u8], Padded>(&bytes[..6]), Err(Error::Size));
  assert_eq!(try_pun_ref::<[u8], Padded>(&bytes[..4]).map(|p| p.tail.len()), Ok(0));
  // Nothing of it is read from padding.
  assert_eq!(as_bytes(padded), bytes);
  let (padded, rest) = from_head::<Padded>(&bytes[..7]).unwrap();
  assert_eq!((padded.tail.len(), rest.len()), (0, 3));
}

#[test]
fn rules() {
  // A number of elements.
  let (list, rest) = from_head::<List>(&[4, 1, 0, 2, 0, 9]).unwrap();
  assert_eq!((list.1.len(), list.1[1].get(), rest), (2, 2, &[9][..]));
  assert_eq!(from_head::<List>(&[6, 1, 0, 2, 0, 9]).err(), Some(Error::Size));
  assert!(try_pun_ref::<[u8], List>(&[4, 1, 0, 2, 0]).is_ok());
  assert!(matches!(try_pun_ref::<[u8], List>(&[2, 1, 0, 2, 0]), Err(Error::Validity { .. })));
  assert_eq!(try_pun_ref::<[u8], List>(&[4, 1, 0, 2]).err(), Some(Error::Size));

  // An assertion over two fields.
  let checked = |bytes: [u8; 4]| {
    try_pun::<_, [u8; 4]>(bytes)
      .and_then(|bytes| try_pun_ref::<[u8; 4], Checked>(&bytes).map(|checked| checked.value.get()))
  };
  assert_eq!(checked([1, 0, 0, 5]), Ok(5));
  assert_eq!(checked([2, 0, 1, 0]), Ok(0x100));
  assert!(matches!(checked([2, 0, 0, 5]), Err(Error::Validity { .. })));
  assert!(matches!(checked([3, 0, 1, 0]), Err(Error::Validity { .. })));
  assert!(matches!(checked([1, 1, 0, 5]), Err(Error::Validity { .. })));
  // The same type proves its own rules: no check, and this would not compile otherwise.
  let checked: &Checked = try_pun_ref(&[1u8, 0, 0, 5]).unwrap();
  assert_eq!(pun_ref::<Checked, Checked>(checked), checked);
}

// `Level` is of the `Rust` representation: what its values are is the `unstable` feature.
#[cfg(feature = "unstable")]
#[test]
fn in_const() {
  const BYTES: [u8; 4] = pun(0x0102_0304_u32.to_be());
  const WORD: &u16be = head_ref(&BYTES);
  const LEVEL: Result<Level, Error> = try_pun(2u8);
  const CHECKED: Result<&Checked, Error> = try_pun_ref(&[2u8, 0, 1, 0]);
  const LIST: Result<(&List, &[u8]), Error> = from_head(&[2, 7, 0, 1]);
  assert_eq!((BYTES, WORD.get(), LEVEL), ([1, 2, 3, 4], 0x0102, Ok(Level::High)));
  assert_eq!(CHECKED.map(|checked| checked.kind), Ok(2));
  assert_eq!(LIST.map(|(list, rest)| (list.1[0].get(), rest.len())), Ok((7, 1)));
}

/// A header byte, a body, and the 16-bit sum of the header and the body. The tail `T` is a slice
/// or an array.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(sum16(0, size_of_val(self) - 2) == be(size_of_val(self) - 2, 2)))]
#[pun(get(kind: u8 = self.0 >> 4, flag: bool = self.0 & 1 == 1, twice: u32 = self.0 * 2))]
struct Summed<T: ?Sized>(u8, #[pun(regions(body = size_of_val(self) - 3, sum))] T);

#[derive(Pun)]
#[repr(C)]
struct Counted<T: ?Sized>(u8, #[pun(len = self.0)] T);

#[test]
fn rules_on_sized_and_unsized() {
  // `0x51`, `0x02 0x03`: the words `0x5102` and `0x0300` sum to `0x5402`.
  let bytes: [u8; 5] = [0x51, 0x02, 0x03, 0x54, 0x02];
  let summed: &Summed<[u8]> = try_pun_ref(&bytes[..]).unwrap();
  assert_eq!((summed.kind(), summed.flag(), summed.twice()), (5, true, 0xa2));
  assert_eq!((summed.body(), summed.sum()), (&[2, 3][..], &[0x54, 2][..]));
  assert!(try_pun_ref::<_, Summed<[u8]>>(&[0x51u8, 0x02, 0x03, 0x54, 0x03][..]).is_err());
  assert!(try_pun_ref::<_, Summed<[u8]>>(&[0x51u8][..]).is_err());
  // The same, of a known size.
  let summed: &Summed<[u8; 4]> = try_pun_ref(&bytes).unwrap();
  assert_eq!((summed.body(), summed.sum(), summed.kind()), (&[2, 3][..], &[0x54, 2][..], 5));
  assert!(try_pun_ref::<_, Summed<[u8; 4]>>(&[0x51u8, 0x02, 0x03, 0x54, 0x03]).is_err());

  // A count of elements, of a slice or of an array.
  assert!(try_pun_ref::<_, Counted<[u8]>>(&[2u8, 7, 8][..]).is_ok());
  assert!(try_pun_ref::<_, Counted<[u8]>>(&[3u8, 7, 8][..]).is_err());
  assert!(try_pun_ref::<_, Counted<[u8; 2]>>(&[2u8, 7, 8]).is_ok());
  assert!(try_pun_ref::<_, Counted<[u8; 2]>>(&[1u8, 7, 8]).is_err());
  let (counted, rest) = from_head::<Counted<[u8]>>(&[1, 7, 8]).unwrap();
  assert_eq!((&counted.1, rest), (&[7][..], &[8][..]));

  assert_eq!(puns::crc32(b"123456789"), 0xcbf4_3926);
  assert_eq!(puns::sum16(&[0xff, 0xff, 0x00, 0x01, 0x80]), 0x8001);
}

/// A struct whose rules name bytes past the end of some values. A byte that `&&` or `||` does not
/// need to evaluate may be outside the value.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(self.0 != 7 || be(3, 2) == 0xcafe))]
#[pun(assert(self.0 == 7 && be(3, 2) == 0xcafe || 10 - self.0 >= 3))]
struct Lazy(u8, [u8]);

#[test]
fn what_is_not_needed_is_not_read() {
  let lazy = |bytes: &'static [u8]| try_pun_ref::<[u8], Lazy>(bytes).is_ok();
  // Not of kind 7: nothing is read at 3, where there is nothing.
  assert!(lazy(&[1]));
  assert!(lazy(&[1, 0, 0, 0xff, 0xff]));
  // Of kind 7: it is.
  assert!(lazy(&[7, 0, 0, 0xca, 0xfe]));
  assert!(!lazy(&[7, 0, 0, 0xca, 0xff]));
  assert!(!lazy(&[7, 0, 0, 0xca]));
  assert!(!lazy(&[7]));
  // `10 - 11` cannot be computed, and nothing else decides.
  assert!(!lazy(&[11]));
  assert!(!lazy(&[8]));
}

/// Messages that tell the type of what they carry, and a view enum that reads one of them.
mod views {
  use puns::{
    Error, Pun, Tag,
    num::{u8c, u16be},
  };

  /// The kind of a message, when the type of what it carries is not known.
  #[derive(Pun, Debug, Copy, Clone, PartialEq, Eq)]
  #[repr(transparent)]
  pub struct Kind(pub u8);

  #[derive(Pun)]
  #[repr(C)]
  #[pun(size = length)]
  pub struct Message<P: ?Sized + Tag<Kind> = [u8]> {
    pub kind: P::Tag,
    pub length: u8,
    #[pun(regions(body: P))]
    pub data: [u8],
  }

  #[derive(Pun, Debug, PartialEq, Eq)]
  #[repr(C)]
  #[pun(tag(Kind = u8c<1>))]
  pub struct Ping(pub u16be);

  #[derive(Pun, Debug, PartialEq, Eq)]
  #[repr(transparent)]
  #[pun(tag(Kind = u8c<2>))]
  pub struct Text(pub str);

  #[derive(Pun)]
  pub enum Any<'a> {
    Ping(&'a Message<Ping>),
    Text(&'a Message<Text>),
    Other(&'a Message),
  }

  #[test]
  fn one_or_the_other() {
    let bytes = [1, 4, 0, 7, 2, 4, b'h', b'i', 9, 2, 3, 3];
    let (first, rest) = Any::from_head(&bytes).unwrap();
    let Any::Ping(ping) = first else { panic!() };
    assert_eq!(ping.body(), Ok(&Ping(u16be::new(7))));
    let (second, rest) = Any::from_head(rest).unwrap();
    let Any::Text(text) = second else { panic!() };
    assert_eq!(text.body().map(|text| &text.0), Ok("hi"));
    let (third, rest) = Any::from_head(rest).unwrap();
    let Any::Other(other) = third else { panic!() };
    assert_eq!((other.kind, other.body(), rest), (Kind(9), Ok(&[][..]), &[3, 3][..]));
    // A size that is not the one of the message.
    assert!(matches!(Any::from_head(rest), Err(Error::Validity { .. })));
    // The whole of the bytes, or nothing.
    assert!(matches!(Any::from_bytes(&bytes[..4]), Ok(Any::Ping(_))));
    assert!(Any::from_bytes(&bytes[..5]).is_err());
    // What a message says it carries is checked when it is looked at.
    let Ok(Any::Text(text)) = Any::from_bytes(&[2, 3, 0xff]) else { panic!() };
    assert!(matches!(text.body(), Err(Error::Validity { .. })));
    // In `const`.
    let first: Result<(Any<'static>, &[u8]), Error> = const { Any::from_head(&[1, 4, 0, 7, 0]) };
    assert!(matches!(first, Ok((Any::Ping(_), [0]))));
  }
}

mod walks {
  use puns::{Error, Pun, try_pun_ref};

  /// A version, then type, length and value entries up to a type of 0, then the 16-bit sum of the
  /// bytes before that type. The position of the sum depends on every length before it.
  #[derive(Pun)]
  #[repr(C)]
  #[pun(assert({
    let end = walk(1, |at| be(at, 1) != 0, |at| at + 2 + be(at + 1, 1));
    let sum = sum16(0, end);
    end + 3 == size_of_val(self) && be(end + 1, 2) == sum
  }))]
  struct Chain {
    version: u8,
    values: [u8],
  }

  /// A kind, then a value of 4 bytes if the kind is 1 and of 2 bytes otherwise, that is not 0.
  /// Its rule uses `if` expressions and `let` bindings.
  #[derive(Pun)]
  #[repr(C)]
  #[pun(assert({
    let wide = if kind == 1 { 4 } else { 2 };
    let value = if kind == 1 { be(1, 4) } else { be(1, 2) };
    size_of_val(self) == 1 + wide && value != 0
  }))]
  struct Either {
    kind: u8,
    value: [u8],
  }

  #[test]
  fn a_chain() {
    // Two values, the end, the sum.
    let mut bytes = vec![1, 7, 2, 0xaa, 0xbb, 9, 0, 0, 0, 0];
    let sum = puns::sum16(&bytes[..7]);
    bytes[8..].copy_from_slice(&sum.to_be_bytes());
    assert!(try_pun_ref::<_, Chain>(&bytes[..]).is_ok());
    // No value at all.
    assert!(try_pun_ref::<_, Chain>(&[1u8, 0, 0x01, 0x00][..]).is_ok());
    // A sum that is not the one of what is before.
    bytes[3] ^= 1;
    assert!(matches!(try_pun_ref::<_, Chain>(&bytes[..]), Err(Error::Validity { .. })));
    bytes[3] ^= 1;
    // A length that leads out of the value, or to something that is not its end.
    for length in [3, 200, 1] {
      let mut bytes = bytes.clone();
      bytes[6] = length;
      assert!(matches!(try_pun_ref::<_, Chain>(&bytes[..]), Err(Error::Validity { .. })));
    }
    // No end.
    assert!(matches!(try_pun_ref::<_, Chain>(&[1u8, 7, 0, 7, 0][..]), Err(Error::Validity { .. })));
    assert!(matches!(try_pun_ref::<_, Chain>(&[1u8][..]), Err(Error::Validity { .. })));
    // In `const`.
    let (valid, invalid) = const {
      (
        try_pun_ref::<[u8], Chain>(&[1, 0, 0x01, 0x00]).is_ok(),
        try_pun_ref::<[u8], Chain>(&[1, 0, 0x01, 0x01]).is_ok(),
      )
    };
    assert!(valid && !invalid);
    // The same rule proves itself: nothing is walked.
    let chain: &Chain = try_pun_ref(&bytes[..]).unwrap();
    let same: &Chain = puns::pun_ref(chain);
    assert_eq!(same.version, 1);
  }

  #[test]
  fn a_choice() {
    assert!(try_pun_ref::<_, Either>(&[1u8, 0, 0, 0, 1][..]).is_ok());
    assert!(try_pun_ref::<_, Either>(&[2u8, 0, 1][..]).is_ok());
    // The size of the other kind: what is not chosen may not be there to read.
    assert!(try_pun_ref::<_, Either>(&[1u8, 0, 1][..]).is_err());
    assert!(try_pun_ref::<_, Either>(&[2u8, 0, 0, 0, 1][..]).is_err());
    assert!(try_pun_ref::<_, Either>(&[2u8, 0, 0][..]).is_err());
  }
}

/// A tuple of 12 elements, the most that `Pun` is implemented for.
type Twelve<T> = (T, T, T, T, T, T, T, T, T, T, T, T);

/// A tuple of 12 elements of two types of 1 byte, which Rust may put in any order.
type Mixed = (bool, u8, bool, u8, bool, u8, bool, u8, bool, u8, bool, u8);

/// Checks that a tuple of 12 elements is proven and checked as a smaller one is. Rust leaves the
/// order of its elements to the compiler, so each pun but the last is the same in every order.
/// The cases `tuple_of_twelve` and `tuple_of_twelve_unspecified` of the compile-fail tests are
/// refused, and `tuple_of_thirteen` is not a `Pun` type.
#[test]
fn a_tuple_of_twelve() {
  let bytes: [u8; 12] = core::array::from_fn(|i| u8::try_from(i).unwrap());
  // Proven: every byte is a `u8`, and every `bool` a `u8`, wherever it is.
  assert_eq!(pun::<Twelve<u8>, [u8; 12]>(pun::<[u8; 12], Twelve<u8>>(bytes)), bytes);
  let mixed: Mixed = pun([true; 12]);
  assert_eq!(pun::<Mixed, [u8; 12]>(mixed), [1; 12]);
  // The same type proves itself, whichever element each byte is.
  assert_eq!(pun::<Mixed, Mixed>(mixed), mixed);
  // Every two bytes at an even offset are a `u16`.
  let words: Twelve<u16> = pun([0xab_u8; 24]);
  assert_eq!(pun::<Twelve<u16>, [u16; 12]>(words), [0xabab; 12]);

  // Checked: each byte is 0 or 1.
  let bools = try_pun::<[u8; 12], Twelve<bool>>([1; 12]).map(pun::<Twelve<bool>, [bool; 12]>);
  assert_eq!(bools, Ok([true; 12]));
  let mut two = [1; 12];
  two[11] = 2;
  assert!(matches!(try_pun::<[u8; 12], Twelve<bool>>(two), Err(Error::Validity { .. })));
  assert!(puns::from_bytes::<Twelve<NonZero<u8>>>(&[1; 12]).is_ok());
  assert!(matches!(
    puns::from_bytes::<Twelve<NonZero<u8>>>(&two.map(|b| b - 1)),
    Err(Error::Validity { .. })
  ));
  // The layout that `rustc` chose tells which byte is the first `bool`.
  #[cfg(feature = "unstable")]
  {
    type First = (bool, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8, u8);
    let first = core::mem::offset_of!(First, 0);
    let mut flags = [2; 12];
    flags[first] = 1;
    assert_eq!(try_pun::<[u8; 12], First>(flags).map(|t| (t.0, t.1)), Ok((true, 2)));
    flags[first] = 2;
    assert!(matches!(try_pun::<[u8; 12], First>(flags), Err(Error::Validity { .. })));
  }
}

/// Checks that a signed `NonZero` accepts every bit pattern of its integer but 0, negative ones
/// included, and that an `Option` of it has the same values as the integer.
#[test]
fn signed_non_zero() {
  let minus = NonZero::new(-1i32).unwrap();
  assert_eq!(pun::<NonZero<i32>, [u8; 4]>(minus), [0xff; 4]);
  assert_eq!(try_pun::<i32, NonZero<i32>>(-5), Ok(NonZero::new(-5).unwrap()));
  assert!(matches!(try_pun::<i32, NonZero<i32>>(0), Err(Error::Validity { .. })));
  assert_eq!(try_pun::<[u8; 1], NonZero<i8>>([0x80]), Ok(NonZero::new(i8::MIN).unwrap()));
  assert_eq!(try_pun::<i128, NonZero<i128>>(i128::MIN).map(NonZero::get), Ok(i128::MIN));
  assert!(matches!(try_pun::<isize, NonZero<isize>>(0), Err(Error::Validity { .. })));
  assert_eq!(pun::<i16, Option<NonZero<i16>>>(-2), NonZero::new(-2));
  assert_eq!(pun::<Option<NonZero<i64>>, i64>(None), 0);
  // Every `NonZero<i32>` is a `NonZero<u32>`, and back: the same bit patterns.
  assert_eq!(pun::<NonZero<i32>, NonZero<u32>>(minus).get(), u32::MAX);
}

/// Checks, for each integer of a fixed byte order of `puns::num` that has a `NonZero`, that its
/// `NonZero` has its bytes and accepts every bit pattern but zeros, that an `Option` of it has its
/// size, `None` being zeros, without the `unstable` feature, and that the integer proves it only
/// where it is not zero.
#[test]
fn non_zero_of_a_fixed_byte_order() {
  macro_rules! each {
    ($($name:ident: $raw:ident;)+) => {$({
      use puns::num::{NonZero, $name};
      const N: usize = size_of::<$raw>();
      let value = $name::new($raw::MAX);
      let non_zero = NonZero::new(value).unwrap();
      assert_eq!((size_of::<NonZero<$name>>(), align_of::<NonZero<$name>>()), (N, 1));
      assert_eq!(size_of::<Option<NonZero<$name>>>(), N);
      assert_eq!((non_zero.get(), $name::from(non_zero)), (value, value));
      assert_eq!(NonZero::new($name::new(0)), None);
      // Its bytes are those of the integer, in its byte order.
      assert_eq!(pun::<NonZero<$name>, [u8; N]>(non_zero), value.to_bytes());
      assert_eq!(pun::<Option<NonZero<$name>>, [u8; N]>(None), [0; N]);
      assert_eq!(try_pun::<[u8; N], Option<NonZero<$name>>>([0; N]), Ok(None));
      let one = $name::new(1);
      assert_eq!(try_pun::<[u8; N], NonZero<$name>>(one.to_bytes()).map(NonZero::get), Ok(one));
      assert_eq!(try_pun::<[u8; N], Option<NonZero<$name>>>(one.to_bytes()), Ok(NonZero::new(one)));
      assert!(matches!(try_pun::<[u8; N], NonZero<$name>>([0; N]), Err(Error::Validity { .. })));
      assert_eq!(try_pun::<$name, NonZero<$name>>($name::new(0)), Err(Error::Validity { at: 0 }));
      // Every `NonZero` of it is the integer: proven.
      assert_eq!(pun::<NonZero<$name>, $name>(non_zero), value);
      assert_eq!(pun::<Option<NonZero<$name>>, $name>(Some(non_zero)), value);
      assert!(NonZero::new($name::new(1)).unwrap() < non_zero);
    })+};
  }
  each! {
    u16le: u16;
    u16be: u16;
    u32le: u32;
    u32be: u32;
    u64le: u64;
    u64be: u64;
    u128le: u128;
    u128be: u128;
    i16le: i16;
    i16be: i16;
    i32le: i32;
    i32be: i32;
    i64le: i64;
    i64be: i64;
    i128le: i128;
    i128be: i128;
  }
}

/// Checks that `ManuallyDrop` of an unsized type is reinterpreted as that type: the layout of
/// `ManuallyDrop<[u8]>` checks that a pointer to it is represented as a pointer to a slice.
#[test]
fn manually_drop_of_a_slice() {
  use core::mem::ManuallyDrop;
  let bytes = [1u8, 2, 3];
  let wrapped: &ManuallyDrop<[u8]> = pun_ref::<[u8], _>(&bytes[..]);
  assert_eq!(&wrapped[..], &bytes[..]);
  let text: &ManuallyDrop<str> = puns::from_bytes(b"pun").expect("UTF-8");
  assert_eq!(&**text, "pun");
}
