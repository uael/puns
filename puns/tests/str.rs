//! Tests `str` as a `Pun` type, whose values are the bytes that the UTF-8 automaton accepts.

use core::num::NonZero;

use puns::{
  Error, Pun, from_head,
  num::{u8c, u32be},
  pun_mut, pun_ref, try_pun_mut, try_pun_ref,
};

/// An ASCII character, as a type whose every value is ASCII.
type Ascii = puns::num::u8in<0, 0x7f>;

const fn ascii(byte: u8) -> Ascii {
  Ascii::new(byte).unwrap()
}

/// A byte that the struct requires to be ASCII. Safe code can still write any byte to the field.
#[derive(Pun, Debug, Copy, Clone, PartialEq)]
#[repr(transparent)]
struct Unchecked(#[pun(valid(..=0x7f))] u8);

#[derive(Pun, Debug, Copy, Clone, PartialEq)]
#[repr(u8)]
enum Digit {
  Zero = b'0',
  One = b'1',
  Two = b'2',
  Three = b'3',
}

/// The UTF-8 encoding of "é", one byte per field.
#[derive(Pun, Default)]
#[repr(C)]
struct EAcute(u8c<0xc3>, u8c<0xa9>);

/// The bytes `GET ` followed by a path.
#[derive(Pun)]
#[repr(C)]
struct Get<T: ?Sized> {
  method: [u8c<b'G'>; 1],
  e: u8c<b'E'>,
  t: u8c<b'T'>,
  space: u8c<b' '>,
  path: T,
}

#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
struct Named {
  id: u32be,
  name: str,
}

#[test]
fn every_str_is_bytes() {
  let bytes: &[u8] = pun_ref("héllo");
  assert_eq!(bytes, "héllo".as_bytes());
  // A `str` in a struct.
  let named: &Named = try_pun_ref(&b"\0\0\0\x07caf\xc3\xa9"[..]).unwrap();
  assert_eq!((named.id.get(), &named.name), (7, "café"));
  assert_eq!(pun_ref::<Named, [u8]>(named), b"\0\0\0\x07caf\xc3\xa9");
}

#[test]
fn proven_at_compile_time() {
  // ASCII is UTF-8: no check.
  let hello: &[Ascii] = try_pun_ref(&b"hello"[..]).unwrap();
  assert_eq!(pun_ref::<[Ascii], str>(hello), "hello");
  assert_eq!(pun_ref::<[Ascii; 2], str>(&[ascii(b'h'), ascii(b'i')]), "hi");
  // Whatever the values of an enum are, as long as they are ASCII.
  assert_eq!(pun_ref::<[Digit; 3], str>(&[Digit::Three, Digit::One, Digit::Two]), "312");
  assert_eq!(pun_ref::<[Digit], str>(&[Digit::Zero][..]), "0");
  // A two bytes character, known byte by byte.
  assert_eq!(pun_ref::<EAcute, str>(&EAcute::default()), "é");
  assert_eq!(pun_ref::<[EAcute; 2], str>(&Default::default()), "éé");
  // Literals, then ASCII.
  let get: &Get<[Ascii]> = &Get {
    method: Default::default(),
    e: u8c::default(),
    t: u8c::default(),
    space: u8c::default(),
    path: [ascii(b'/'), ascii(b'a')],
  };
  assert_eq!(pun_ref::<Get<[Ascii]>, str>(get), "GET /a");
  // A restriction that the type system does not enforce is checked when the value comes in...
  assert!(matches!(try_pun_ref::<u8, Unchecked>(&0x80), Err(Error::Validity { .. })));
  // ... but proves nothing of a value that safe code made: this is checked.
  let unchecked = [Unchecked(0xff)];
  assert!(matches!(try_pun_ref::<[Unchecked], str>(&unchecked[..]), Err(Error::Validity { .. })));
  assert!(matches!(
    try_pun_ref::<[Unchecked], [Ascii]>(&unchecked[..]),
    Err(Error::Validity { .. })
  ));
  // A `str` is a `str`, even behind a `&mut`.
  let mut text = String::from("abc");
  pun_mut::<str, str>(&mut text).make_ascii_uppercase();
  assert_eq!(text, "ABC");
}

#[test]
fn checked_at_run_time() {
  let utf8 = |bytes: &'static [u8]| try_pun_ref::<[u8], str>(bytes);
  assert_eq!(utf8(b""), Ok(""));
  assert_eq!(utf8(b"caf\xc3\xa9 \xe2\x82\xac \xf0\x9f\xa6\x80"), Ok("café € 🦀"));
  // Truncated, overlong, surrogate, too large, stray continuation byte.
  for bytes in [&b"caf\xc3"[..], b"\xc0\x80", b"\xed\xa0\x80", b"\xf4\x90\x80\x80", b"\x80"] {
    assert!(matches!(utf8(bytes), Err(Error::Validity { .. })));
    assert!(core::str::from_utf8(bytes).is_err());
  }
  // From an array, from a struct.
  assert_eq!(try_pun_ref::<[u8; 2], str>(&[0xc3, 0xa9]), Ok("é"));
  assert!(matches!(try_pun_ref::<[u8; 2], str>(&[0xc3, 0x29]), Err(Error::Validity { .. })));
  assert_eq!(
    try_pun_ref::<Get<[u8]>, str>(&Get::<[u8; 2]> {
      method: Default::default(),
      e: u8c::default(),
      t: u8c::default(),
      space: u8c::default(),
      path: [b'/', 0xff],
    } as &Get<[u8]>),
    Err(Error::Validity { at: 0 })
  );
  // In a struct.
  assert!(try_pun_ref::<[u8], Named>(b"\0\0\0\x07\xff").is_err());
  assert!(try_pun_ref::<[u8], Named>(b"\0\0").is_err());
  let (named, rest) = from_head::<Named>(b"\0\0\0\x01ok").unwrap();
  assert_eq!((&named.name, rest), ("ok", &[][..]));
  // Bytes can be seen as a `&mut str`: whatever is written through it is bytes.
  let mut bytes = *b"abc";
  try_pun_mut::<[u8], str>(&mut bytes[..]).unwrap().make_ascii_uppercase();
  assert_eq!(&bytes, b"ABC");
}

/// Two bytes of any value.
#[derive(Pun, Debug, Copy, Clone, PartialEq)]
#[repr(C)]
struct TwoBytes {
  a: u8,
  b: u8,
}

/// A wide integer, then a byte.
#[derive(Pun, Debug, Copy, Clone, PartialEq)]
#[repr(C)]
struct WideThenByte {
  wide: u16,
  narrow: u8,
  last: u8,
}

/// A scalar that is not all UTF-8, then a byte.
#[derive(Pun, Debug, Copy, Clone, PartialEq)]
#[repr(C)]
struct ScalarThenByte {
  first: NonZero<u8>,
  second: u8,
}

#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
struct Text {
  kind: u8,
  text: str,
}

#[derive(Pun, Debug, Copy, Clone, PartialEq)]
#[repr(C)]
struct KindThenText {
  kind: u8,
  text: TwoBytes,
}

/// Checks that a string that the source does not prove is checked, every byte of it, where every
/// byte of the source is initialized. The automaton stops at the first byte that it cannot prove.
#[test]
fn checked_after_what_is_not_proven() {
  let two = |a, b| try_pun_ref::<TwoBytes, str>(&TwoBytes { a, b }).map(str::to_owned);
  assert_eq!(two(b'o', b'k'), Ok("ok".to_owned()));
  assert_eq!(two(0xc3, 0xa9), Ok("é".to_owned()));
  assert!(matches!(two(b'o', 0xff), Err(Error::Validity { .. })));
  assert!(matches!(two(0xc3, 0x29), Err(Error::Validity { .. })));
  let wide = |wide: u16, narrow, last| {
    let source = WideThenByte { wide, narrow, last };
    try_pun_ref::<WideThenByte, str>(&source).map(str::to_owned)
  };
  let ab = u16::from_ne_bytes(*b"ab");
  assert_eq!(wide(ab, b'c', b'd'), Ok("abcd".to_owned()));
  assert!(matches!(wide(ab, b'c', 0x80), Err(Error::Validity { .. })));
  // A scalar that is not all UTF-8, then a byte.
  let scalar = |first, second| {
    let source = ScalarThenByte { first: NonZero::new(first).unwrap(), second };
    try_pun_ref::<ScalarThenByte, str>(&source).map(str::to_owned)
  };
  assert_eq!(scalar(b'o', b'k'), Ok("ok".to_owned()));
  assert!(matches!(scalar(0xff, b'k'), Err(Error::Validity { .. })));
  // Behind a field, in a struct that ends with a `str`.
  let text = |kind, a, b| {
    let source = KindThenText { kind, text: TwoBytes { a, b } };
    try_pun_ref::<KindThenText, Text>(&source).map(|text| (text.kind, text.text.to_owned()))
  };
  assert_eq!(text(1, b'o', b'k'), Ok((1, "ok".to_owned())));
  assert!(matches!(text(1, b'o', 0xff), Err(Error::Validity { .. })));
  // A slice of elements whose bytes are all initialized.
  let slice = |pairs: &[TwoBytes]| try_pun_ref::<[TwoBytes], str>(pairs).map(str::to_owned);
  assert_eq!(slice(&[]), Ok(String::new()));
  let pairs = [TwoBytes { a: b'h', b: b'i' }, TwoBytes { a: 0xc3, b: 0xa9 }];
  assert_eq!(slice(&pairs), Ok("hié".to_owned()));
  assert!(matches!(slice(&[TwoBytes { a: b'h', b: 0xc3 }]), Err(Error::Validity { .. })));
}

#[test]
fn in_const() {
  const TEXT: &str = match try_pun_ref::<[u8], str>(b"const") {
    Ok(text) => text,
    Err(_) => panic!(),
  };
  const BYTES: &[u8] = pun_ref(TEXT);
  assert_eq!((TEXT, BYTES), ("const", &b"const"[..]));
}
