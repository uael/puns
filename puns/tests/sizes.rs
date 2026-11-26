//! Tests sizes, lengths and regions read from the value, with `be`, `le` and `walk` anywhere in
//! an expression, several walks, and walks in walks.

use puns::{Error, Pun, from_bytes, from_head};

/// A WebSocket frame. Its length is 7 bits, or 16 or 64 more bits if those 7 bits are 126 or 127.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 2 + if length & 0x7f == 126 {
  2 + be(2, 2)
} else if length & 0x7f == 127 {
  8 + be(2, 8)
} else {
  length & 0x7f
})]
struct Frame {
  head: u8,
  length: u8,
  rest: [u8],
}

#[test]
fn a_size_read_from_the_value() {
  let (frame, rest) = from_head::<Frame>(&[0x81, 3, 1, 2, 3, 9]).unwrap();
  assert_eq!((&frame.rest, rest), (&[1, 2, 3][..], &[9][..]));
  let (frame, rest) = from_head::<Frame>(&[0x81, 126, 0, 2, 1, 2, 9]).unwrap();
  assert_eq!((&frame.rest, rest), (&[0, 2, 1, 2][..], &[9][..]));
  let mut long = vec![0x82, 127, 0, 0, 0, 0, 0, 0, 1, 0];
  long.extend(core::iter::repeat_n(7, 256));
  let (frame, rest) = from_head::<Frame>(&long).unwrap();
  assert_eq!((frame.rest.len(), rest.len()), (264, 0));
  // What the length says is not there.
  assert_eq!(from_head::<Frame>(&[0x81, 126, 0, 9, 1]).err(), Some(Error::Size));
  // The length itself is not there: it cannot be read.
  assert!(matches!(from_head::<Frame>(&[0x81, 126, 0]), Err(Error::Validity { .. })));
  // The same rule, checked against the bytes that are the value.
  assert!(from_bytes::<Frame>(&[0x81, 126, 0, 2, 1, 2]).is_ok());
  assert!(from_bytes::<Frame>(&[0x81, 126, 0, 2, 1, 2, 3]).is_err());
}

/// A string that ends with a zero byte, whose size is a walk. `Named` uses the same walk in a
/// region, and has what follows the string.
#[derive(Pun)]
#[repr(C)]
#[pun(size = walk(0, |at| be(at, 1) != 0, |at| at + 1) + 1)]
struct CString {
  bytes: [u8],
}

#[derive(Pun)]
#[repr(C)]
struct Named {
  kind: u8,
  // The walk gives an offset in the value, the region a number of bytes after `kind`.
  #[pun(regions(name = walk(1, |at| be(at, 1) != 0, |at| at + 1) - 1, terminator = 1, value))]
  rest: [u8],
}

#[test]
fn a_walk_in_a_size_and_in_a_region() {
  let (string, rest) = from_head::<CString>(b"abc\0def").unwrap();
  assert_eq!((&string.bytes, rest), (&b"abc\0"[..], &b"def"[..]));
  assert!(matches!(from_head::<CString>(b"abc"), Err(Error::Validity { .. })));
  let named = from_bytes::<Named>(b"\x07key\0value").unwrap();
  assert_eq!((named.kind, named.name(), named.value()), (7, &b"key"[..], &b"value"[..]));
}

/// A struct whose length is the sum of two walks.
///
/// `Nested` has a walk in a walk, over type, length and value entries whose values are themselves
/// length and value entries, up to a type of 0. Each walk adds the lengths that the other one
/// does not.
#[derive(Pun)]
#[repr(C)]
struct Twice {
  count: u8,
  #[pun(len = walk(0, |at| at < count, |at| at + 1) + walk(0, |at| at < count, |at| at + 1))]
  items: [u8],
}

#[derive(Pun)]
#[repr(C)]
#[pun(assert(
  walk(0, |at| be(at, 1) != 0, |at| {
    let end = at + 2 + be(at + 1, 1);
    let inner = walk(at + 2, |i| i < end, |i| i + 1 + be(i, 1));
    if inner == end { end } else { 1 / 0 }
  }) + 1 == size_of_val(self)
))]
struct Nested {
  values: [u8],
}

#[test]
fn walks_and_walks_in_walks() {
  let (twice, rest) = from_head::<Twice>(&[2, 1, 2, 3, 4, 5]).unwrap();
  assert_eq!((&twice.items, rest), (&[1, 2, 3, 4][..], &[5][..]));
  // Type 1, 4 bytes: "1 byte: 7", "1 byte: 8"; type 2, 0 bytes; the end.
  assert!(from_bytes::<Nested>(&[1, 4, 1, 7, 1, 8, 2, 0, 0]).is_ok());
  // A value whose inner lengths do not fill it.
  assert!(from_bytes::<Nested>(&[1, 4, 1, 7, 2, 8, 2, 0, 0]).is_err());
  assert!(from_bytes::<Nested>(&[1, 4, 1, 7, 1, 8, 2, 0]).is_err());
}

/// A struct whose walk has a step of 70 operations. `LongExpression` has an expression of more
/// than 80. Each is applied in an instance of a function of its own, as any other expression is.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(walk(0, |at| at < 4, |at| at + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1
  + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 - 33)
  == a + a + a + a))]
struct LongStep {
  a: u8,
}

#[derive(Pun)]
#[repr(C)]
#[pun(assert(walk(0, |at| at < a, |at| at + 1) + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0
  + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0
  + 0 + 0 + 0 + 0 + 0 == a))]
struct LongExpression {
  a: u8,
}

#[test]
fn long_walks() {
  assert!(from_bytes::<LongStep>(&[1]).is_ok());
  assert!(from_bytes::<LongStep>(&[2]).is_err());
  assert!(from_bytes::<LongExpression>(&[3]).is_ok());
  // More steps than 8 per bit of the value, and 128.
  assert!(from_bytes::<LongExpression>(&[200]).is_err());
}

/// A struct with the longest expression that a rule can have, and walks nested as deep as they
/// can be.
///
/// The expression has 256 operations: `a` and 125 times `+ 1` are 251, `a + 125` is 3, then `!=`
/// and `!` are 2. The walks are nested 4 deep.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(!(a + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 != a + 125)))]
#[pun(assert(
  walk(0, |i| i < 1 && walk(0, |j| j < 1 && walk(0, |k| k < 1 && walk(0, |l| l < a, |l| l + 1) == a,
    |k| k + 1) == 1, |j| j + 1) == 1, |i| i + 1) == 1
))]
struct Longest {
  a: u8,
}

#[test]
fn the_limits_of_an_expression() {
  assert!(from_bytes::<Longest>(&[3]).is_ok());
  // The innermost walk takes more steps than 8 per bit of the value, and 128.
  assert!(from_bytes::<Longest>(&[200]).is_err());
}

/// A DER type, length and value. The length is short, below 0x80, or long, 0x81 to 0x84 followed
/// by that many bytes.
///
/// Its size is a sum of five forms, which nests deeper than 8 values. Its rule binds six names at
/// once.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 2 + (if length < 0x80 {
  length
} else if length == 0x81 {
  1 + be(2, 1)
} else if length == 0x82 {
  2 + be(2, 2)
} else if length == 0x83 {
  3 + be(2, 3)
} else if length == 0x84 {
  4 + be(2, 4)
} else {
  1 / 0
}))]
#[pun(assert({
  let a = kind; let b = a; let c = b; let d = c; let e = d; let f = e;
  a + b + c + d + e + f == kind * 6
}))]
struct Der {
  kind: u8,
  length: u8,
  rest: [u8],
}

#[test]
fn deeper_expressions() {
  let (der, rest) = from_head::<Der>(&[0x30, 2, 1, 2, 9]).unwrap();
  assert_eq!((der.rest.len(), rest), (2, &[9][..]));
  let (der, rest) = from_head::<Der>(&[0x30, 0x82, 0, 3, 1, 2, 3]).unwrap();
  assert_eq!((der.rest.len(), rest.len()), (5, 0));
  assert!(matches!(from_head::<Der>(&[0x30, 0x85, 0, 3, 1, 2, 3]), Err(Error::Validity { .. })));
}
