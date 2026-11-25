//! Tests variable-length integers: LEB128, the variable-length integers of QUIC, and big-endian
//! base 128.

use puns::{Error, Pun, from_bytes, from_head};

/// A Protocol Buffers field of wire type 2. It holds a key, a length, then that many bytes.
#[derive(Pun)]
#[repr(C)]
#[pun(size = leb128_len(0) + leb128_len(leb128_len(0)) + leb128(leb128_len(0)))]
#[pun(assert(leb128(0) & 7 == 2))]
struct Delimited {
  bytes: [u8],
}

/// An MQTT packet. It holds a byte, then the remaining length in 1 to 4 bytes, then that many
/// bytes.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 1 + leb128_len(1) + leb128(1))]
#[pun(assert(leb128_len(1) <= 4))]
struct Mqtt {
  head: u8,
  rest: [u8],
}

#[test]
fn leb128() {
  let (field, rest) = from_head::<Delimited>(b"\x0a\x03abcdef").unwrap();
  assert_eq!((&field.bytes, rest), (&b"\x0a\x03abc"[..], &b"def"[..]));
  // A key of two bytes (field 18), a length of two bytes (130).
  let mut bytes = vec![0x92, 0x01, 0x82, 0x01];
  bytes.extend([7; 130]);
  bytes.push(9);
  let (field, rest) = from_head::<Delimited>(&bytes).unwrap();
  assert_eq!((field.bytes.len(), rest), (134, &[9][..]));
  // Not of wire type 2, or a length that is not over.
  assert!(matches!(from_head::<Delimited>(b"\x08\x03abc"), Err(Error::Validity { .. })));
  assert!(matches!(from_head::<Delimited>(b"\x0a\x83"), Err(Error::Validity { .. })));

  let (packet, rest) = from_head::<Mqtt>(&[0x30, 2, 1, 2, 3]).unwrap();
  assert_eq!((&packet.rest, rest), (&[2, 1, 2][..], &[3][..]));
  let mut bytes = vec![0x30, 0x80, 0x01];
  bytes.extend([0; 128]);
  assert_eq!(from_head::<Mqtt>(&bytes).map(|(packet, _)| packet.rest.len()), Ok(130));
  // Five bytes of length, which MQTT does not allow.
  assert!(from_head::<Mqtt>(&[0x30, 0x80, 0x80, 0x80, 0x80, 0x01]).is_err());
}

/// A struct of as many bytes as the LEB128 integer at its start takes.
#[derive(Pun)]
#[repr(C)]
#[pun(size = leb128_len(0))]
struct Leb128 {
  bytes: [u8],
}

/// Checks that an integer that a `u128` does not hold is not valid, however it is written.
/// Reading it neither panics nor returns another value.
#[test]
fn leb128_of_more_than_128_bits() {
  // 18 bytes of 7 bits and 2 bits: the most there is.
  let mut bytes = vec![0xff; 18];
  bytes.push(0x03);
  assert_eq!(from_head::<Leb128>(&bytes).map(|(value, _)| value.bytes.len()), Ok(19));
  // A third bit in the 19th byte.
  *bytes.last_mut().unwrap() = 0x04;
  assert!(matches!(from_head::<Leb128>(&bytes), Err(Error::Validity { .. })));
  // A 20th byte, whatever it holds, and even after bytes of no bits.
  for last in [0x00, 0x01] {
    let mut bytes = vec![0x80; 19];
    bytes.push(last);
    assert!(matches!(from_head::<Leb128>(&bytes), Err(Error::Validity { .. })));
  }
}

/// Returns whether `bytes` encode `expected` as a QUIC variable-length integer, as `QuicAt8`
/// checks it. The test gives it the examples of RFC 9000, appendix A.1.
fn quic(expected: u64, bytes: &[u8]) -> bool {
  let mut value = expected.to_be_bytes().to_vec();
  value.extend(bytes);
  from_bytes::<QuicAt8>(&value).is_ok()
}

#[derive(Pun)]
#[repr(C)]
#[pun(assert(8 + quic_len(8) == size_of_val(self) && quic(8) == expected))]
struct QuicAt8 {
  expected: puns::num::u64be,
  bytes: [u8],
}

#[test]
fn quic_varints() {
  assert!(quic(151_288_809_941_952_652, &[0xc2, 0x19, 0x7c, 0x5e, 0xff, 0x14, 0xe8, 0x8c]));
  assert!(quic(494_878_333, &[0x9d, 0x7f, 0x3e, 0x7d]));
  assert!(quic(15_293, &[0x7b, 0xbd]));
  assert!(quic(37, &[0x25]));
  assert!(quic(37, &[0x40, 0x25]));
  assert!(!quic(37, &[0x40]));
  assert!(!quic(38, &[0x25]));
}

/// A byte, then an object identifier arc of that value, encoded in big-endian base 128.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(vlq(1) == arc && 1 + vlq_len(1) == size_of_val(self)))]
struct Arc {
  arc: u8,
  bytes: [u8],
}

#[test]
fn vlq() {
  assert!(from_bytes::<Arc>(&[5, 5]).is_ok());
  assert!(from_bytes::<Arc>(&[200, 0x81, 0x48]).is_ok());
  assert!(from_bytes::<Arc>(&[200, 0x81, 0xc8]).is_err());
  assert!(from_bytes::<Arc>(&[200, 0x81]).is_err());
  // More than 128 bits.
  let mut long = vec![0];
  long.extend([0xff; 19]);
  long.push(0x7f);
  assert!(from_bytes::<Arc>(&long).is_err());
}
