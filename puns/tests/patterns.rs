//! Tests the patterns that a struct requires of its bytes (`matches`), and numbers of a given
//! byte order.

use puns::{
  Error, Pun, from_bytes, from_head,
  layout::{Dfa, DfaState, Layout},
  num::{f64be, i16be, i32le},
  pun_ref, try_pun_ref,
};

#[path = "support/non_empty.rs"]
mod non_empty;

use non_empty::{Msg, NonEmpty};

/// A reduced tar header. It holds an ASCII name, then a size in octal digits ended by a space or
/// a zero byte.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(matches(Dfa::ASCII, 0, 8)))]
#[pun(assert(matches(Dfa::OCTAL, 8, 19) && (be(19, 1) == 0 || be(19, 1) == 32)))]
struct Tar {
  name: [u8; 8],
  size: [u8; 12],
}

/// An MQTT string, which is a length followed by UTF-8 without `U+0000`.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 2 + length)]
#[pun(assert(matches(Dfa::UTF8_NO_NUL, 2, size_of_val(self))))]
struct MqttString {
  length: puns::num::u16be,
  text: [u8],
}

/// A pattern that the user defines, for identifiers. An identifier is a letter or `_`, then
/// letters, digits or `_`.
const IDENTIFIER: &Dfa = &Dfa {
  id: 1000,
  states: &[
    DfaState { accept: false, edges: &[(b'A', b'Z', 1), (b'_', b'_', 1), (b'a', b'z', 1)] },
    DfaState {
      accept: true,
      edges: &[(b'0', b'9', 1), (b'A', b'Z', 1), (b'_', b'_', 1), (b'a', b'z', 1)],
    },
  ],
};

#[derive(Pun)]
#[repr(C)]
#[pun(assert(matches(IDENTIFIER, 0, size_of_val(self))))]
struct Identifier {
  bytes: [u8],
}

#[test]
fn patterns() {
  assert!(from_bytes::<Tar>(b"file.txt00000000017\0").is_ok());
  assert!(from_bytes::<Tar>(b"file.txt00000000017 ").is_ok());
  assert!(from_bytes::<Tar>(b"file.txt00000000018\0").is_err());
  assert!(from_bytes::<Tar>(b"fil\xe9.txt00000000017\0").is_err());

  let (string, rest) = from_head::<MqttString>(b"\0\x04ab\xc3\xa9!").unwrap();
  assert_eq!((&string.text, rest), (&b"ab\xc3\xa9"[..], &b"!"[..]));
  // Half of `é`, or a zero.
  assert!(from_head::<MqttString>(b"\0\x03ab\xc3\xa9").is_err());
  assert!(from_head::<MqttString>(b"\0\x02a\0").is_err());

  assert!(from_bytes::<Identifier>(b"_puns2").is_ok());
  assert!(from_bytes::<Identifier>(b"2puns").is_err());
  assert!(from_bytes::<Identifier>(b"").is_err());
}

/// Numbers of a given byte order. The height of a bitmap is a signed little-endian integer, which
/// is negative when the rows go from top to bottom. A GPS latitude is a big-endian `f64`.
#[derive(Pun)]
#[repr(C)]
struct Numbers {
  height: i32le,
  latitude: f64be,
  offset: i16be,
}

#[test]
fn numbers_of_a_byte_order() {
  let mut bytes = (-3i32).to_le_bytes().to_vec();
  bytes.extend(48.85f64.to_be_bytes());
  bytes.extend((-2i16).to_be_bytes());
  let numbers = from_bytes::<Numbers>(&bytes).unwrap();
  assert_eq!((numbers.height.get(), numbers.latitude.get(), numbers.offset.get()), (-3, 48.85, -2));
  assert_eq!((i32le::new(-3).to_bytes(), i16be::from(-2).get()), ([0xfd, 0xff, 0xff, 0xff], -2));
}

/// A pattern that the user defines, which accepts only the bytes 0 and 1. It has the `id` of
/// UTF-8, but accepts fewer byte strings than UTF-8.
const BITS: &Dfa =
  &Dfa { id: Dfa::UTF8.id, states: &[DfaState { accept: true, edges: &[(0, 1, 0)] }] };

/// A type that the user defines, whose values are the bytes that `BITS` accepts.
#[repr(transparent)]
struct Bits([u8]);

// SAFETY: `Pun` is not to be implemented by hand, and this test does so to check a pattern that
// a user defines. `Bits` is `repr(transparent)` over `[u8]`, and this test makes a `Bits` only of
// bytes that `BITS` accepts, through `puns`. So the layout of `Layout::pattern` with `BITS`, a tail
// of bytes at offset 0 that the automaton accepts, is its layout. The lists are `()`, and it holds
// no `UnsafeCell`.
unsafe impl Pun for Bits {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Rules = ();
  const PROBE: *const Self = <[u8] as Pun>::PROBE as *const Self;
  // SAFETY: `Bits` is a struct whose last field is a slice. `Self::PROBE` is `<[u8] as Pun>::PROBE`
  // cast with an `as` cast, and its 3 bytes fit in an `isize`. These are the requirements of
  // `Layout::pattern`.
  const LAYOUT: Layout = unsafe { Layout::pattern::<Self>(Self::PROBE, BITS) };
}

/// Checks that a pattern is checked by its states and edges, whatever its `id`. `BITS` has the
/// `id` of UTF-8, and a check by `id` alone, with `str::from_utf8`, would accept `"a"`.
#[test]
fn a_pattern_is_checked_by_its_states() {
  assert_eq!(from_bytes::<Bits>(&[0, 1, 1]).map(|bits| &bits.0), Ok(&[0, 1, 1][..]));
  assert_eq!(from_bytes::<Bits>(&[]).map(|bits| &bits.0), Ok(&[][..]));
  // UTF-8, which `BITS` does not accept.
  assert!(from_bytes::<Bits>(b"a").is_err());
  assert!(from_bytes::<Bits>(&[0, 2]).is_err());
  // A `str` is still UTF-8.
  assert_eq!(from_bytes::<str>("h\u{e9}".as_bytes()), Ok("h\u{e9}"));
  assert!(from_bytes::<str>(b"h\xc3").is_err());
}

/// Any bytes, the empty string included.
const ANY: &Dfa = &Dfa {
  id: u32::from_be_bytes(*b"anyB"),
  states: &[DfaState { accept: true, edges: &[(0, 255, 0)] }],
};

/// A type that the user defines, whose values are any bytes.
#[repr(transparent)]
struct AnyBytes([u8]);

// SAFETY: `Pun` is not to be implemented by hand, and this test does so to check a pattern that
// a user defines. `AnyBytes` is `repr(transparent)` over `[u8]`, and any bytes are a valid
// `AnyBytes`. So the layout of `Layout::pattern` with `ANY`, a tail of bytes at offset 0 that the
// automaton accepts, is its layout. The lists are `()`, and it holds no `UnsafeCell`.
unsafe impl Pun for AnyBytes {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Rules = ();
  const PROBE: *const Self = <[u8] as Pun>::PROBE as *const Self;
  // SAFETY: `AnyBytes` is a struct whose last field is a slice. `Self::PROBE` is
  // `<[u8] as Pun>::PROBE` cast with an `as` cast, and its 3 bytes fit in an `isize`. These are the
  // requirements of `Layout::pattern`.
  const LAYOUT: Layout = unsafe { Layout::pattern::<Self>(Self::PROBE, ANY) };
}

/// Checks that the empty string of a string type is checked where the source may leave no byte to
/// it, if its automaton does not accept the empty string. `NonEmpty` takes at least one byte.
#[test]
fn the_empty_string_is_checked() {
  assert_eq!(from_bytes::<NonEmpty>(&[]).map(|bytes| &bytes.0), Err(Error::Validity));
  assert_eq!(from_bytes::<NonEmpty>(&[7]).map(|bytes| &bytes.0), Ok(&[7][..]));
  // The kind, and no byte of the body.
  assert!(from_bytes::<Msg>(&[7]).is_err());
  let msg = from_bytes::<Msg>(&[7, 1]).unwrap();
  assert_eq!((msg.kind, &msg.body.0), (7, &[1][..]));
  assert!(from_head::<NonEmpty>(&[]).is_err());
  assert!(from_head::<Msg>(&[7]).is_err());
  let (msg, rest) = from_head::<Msg>(&[7, 1]).unwrap();
  assert_eq!((msg.kind, &msg.body.0, rest), (7, &[1][..], &[][..]));
  // Proven: the source has a byte for the body, or the automaton accepts every string.
  assert_eq!(&pun_ref::<[u8; 1], NonEmpty>(&[7]).0, &[7]);
  let msg = pun_ref::<[u8; 2], Msg>(&[7, 1]);
  assert_eq!((msg.kind, &msg.body.0), (7, &[1][..]));
  assert_eq!(&pun_ref::<[u8], AnyBytes>(&[]).0, &[]);
  assert_eq!(&pun_ref::<[u8], AnyBytes>(&[7, 1]).0, &[7, 1]);
  // A slice of elements whose bytes are all initialized, which may have no element: checked, every
  // byte of it.
  let pairs = |pairs: &[Pair]| try_pun_ref::<[Pair], NonEmpty>(pairs).map(|bytes| bytes.0.to_vec());
  assert_eq!(pairs(&[]), Err(Error::Validity));
  assert_eq!(pairs(&[Pair { a: 7, b: true }]), Ok(vec![7, 1]));
}

/// A byte of any value, then a `bool`.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Pair {
  a: u8,
  b: bool,
}
