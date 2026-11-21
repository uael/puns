//! Data encodings, declared with `#[derive(Pun)]` and read in place from bytes: ASN.1 DER (SNMP,
//! Kerberos), the wire format of Protocol Buffers, MessagePack, CBOR, and text.
//!
//! The text covers UTF-8 fields, C strings, padded names, an HTTP/1.1 request line and a
//! `Content-Length` header.
//!
//! As in `ethernet.rs`, the first half of this file holds only type declarations, with no `unsafe`,
//! no `impl` and no parsing code, and the second half reads values in place, from bytes.
//!
//! The following table lists what is specific to these encodings, and what in the declarations
//! expresses it:
//!
//! | Specific to these encodings               | Declared with                                  |
//! |-------------------------------------------|------------------------------------------------|
//! | a DER length of 1 byte, or of 1 to 4 more | `regions(long_length = .., contents: C)`       |
//! | the shortest DER long form                | `assert(length != 0x81 \|\| be(2, 1) >= 0x80)` |
//! | DER contents fill the value               | `assert({ let n = ..; .. == .. })`             |
//! | typed DER values that follow each other   | `regions(first: A = .., rest: B)`              |
//! | INTEGERs and OIDs of the shortest forms   | `assert(..)`, `assert(walk(..) == ..)`         |
//! | digits                                    | `[u8in<b'0', b'9'>; 14]`                       |
//! | a Protocol Buffers key and wire type      | `assert(key & 7 == 2 && key >= 8)`             |
//! | varints and lengths of 1 or 2 bytes       | a struct per form, an enum of references       |
//! | ZigZag                                    | `get(zigzag: i64 = ((v0 >> 1) as i64) ^ ..)`   |
//! | MessagePack and CBOR heads of a few forms | `u8in<0xa0, 0xbf>`, an enum of references      |
//! | a string whose length is in its head      | `size = (head & 0x1f) + 1` on a `str`          |
//! | big-endian floats                         | `get(value: f64 = f64::from_bits(..))`         |
//! | UTF-8 of a counted length                 | `len = length` on a `str`                      |
//! | a C string, with a NUL at its end only    | `size = walk(0, ..) + 1`                       |
//! | a name padded with NULs                   | a `walk` of an offset and a flag               |
//! | an HTTP request line, a decimal number    | `assert(walk(..))`, `u128bec<..>` for a name   |
//!
//! The following are not covered:
//!
//! - DER tags of more than one byte (numbers above 30) are left out. They are base-128 integers
//!   (`vlq`), which would move every offset after them. Each `OPTIONAL` element and each
//!   `CHOICE` needs a type of its own, since a chain of [`Then`] is one sequence of types.
//! - The values of a Protocol Buffers message are read only where a varint or a length takes 1 or 2
//!   bytes, and nested messages are left as bytes. A [`Message`] is checked whole, by a walk over
//!   fields of any form. But a getter reads fields, not bytes, so each form of a field is a struct
//!   whose getters decode it, and an enum of references ([`Field`]) tells the forms apart.
//! - The items of the arrays and maps of MessagePack and CBOR are not declared, only the head of
//!   each item. A head has one of a few forms. Each form is a struct, and an enum of references
//!   tells them apart. Arrays and maps are counts of nested items of their own sizes, and declaring
//!   them would need recursion and a count. An enum of references does not implement `Pun`, so it
//!   cannot be the type of a field.
//! - This file declares text only as `str`. A struct can require a pattern of other text with a
//!   rule: `matches(Dfa::ASCII, ..)` for ASCII, or `matches(Dfa::UTF8_NO_NUL, ..)` for UTF-8
//!   without U+0000 (MQTT). The crate has no automaton for printable ASCII (`0x20` to `0x7e`),
//!   "modified UTF-8" (Java) or UTF-16, but one can be written for `matches`.
//!
//! `main` reads values of each encoding from bytes and asserts their fields, regions and getters.
//! It reinterprets Protocol Buffers fields, MessagePack items and CBOR items as enums of
//! references. It changes bytes so that a rule fails, asserts that each read then returns an error,
//! and prints `ok`.
#![forbid(unsafe_code)]

use core::marker::PhantomData;

use puns::{
  Error, Pun, from_head,
  num::{u8c, u8in, u16be, u32be, u32le, u64be, u64le, u128bec},
  try_pun_ref,
};

// -------------------------------------------------------------------------------------------------
// ASN.1 DER

/// A DER value of tag `T` whose contents are a `C`: a tag, a length, then the contents.
///
/// The length has the short form, or the shortest long form of 1 to 4 bytes.
#[derive(Pun)]
#[repr(C)]
// The shortest long forms.
#[pun(assert(length != 0x81 || be(2, 1) >= 0x80))]
#[pun(assert(length != 0x82 || be(2, 2) >= 0x100))]
#[pun(assert(length != 0x83 || be(2, 3) >= 0x1_0000))]
#[pun(assert(length != 0x84 || be(2, 4) >= 0x100_0000))]
// The tag, the length, its long form, the contents.
#[pun(size = 2 + (if length < 0x80 {
  length
} else if length == 0x81 {
  1 + be(2, 1)
} else if length == 0x82 {
  2 + be(2, 2)
} else if length == 0x83 {
  3 + be(2, 3)
} else {
  4 + be(2, 4)
}))]
pub struct Der<T, C: ?Sized = [u8]> {
  pub tag: T,
  #[pun(valid(..=0x7f, 0x81..=0x84))]
  pub length: u8,
  pub contents_type: PhantomData<C>,
  #[pun(regions(long_length = if length < 0x80 { 0 } else { length & 0x7f }, contents: C))]
  pub data: [u8],
}

/// Two DER values that follow each other.
///
/// The size of the first value is read from its DER length.
#[derive(Pun)]
#[repr(C)]
pub struct Then<A: ?Sized, B: ?Sized> {
  /// A marker for `A` and `B`. It is not `PhantomData<(A, B)>`, because `(A, B)` is not a type
  /// when `A` is unsized.
  pub types: PhantomData<(PhantomData<A>, PhantomData<B>)>,
  #[pun(regions(
    first: A = {
      let n = if be(1, 1) < 0x80 { 0 } else { be(1, 1) & 0x7f };
      2 + n + (if n == 0 { be(1, 1) } else { be(2, 4) >> (32 - 8 * n) })
    },
    rest: B,
  ))]
  pub data: [u8],
}

/// The contents of a DER INTEGER, of at least 1 byte and of the shortest form.
///
/// In the shortest form, the first 9 bits of contents of 2 bytes or more are not all equal.
#[derive(Pun)]
#[repr(transparent)]
#[pun(assert(size_of_val(self) >= 1))]
#[pun(assert(size_of_val(self) == 1 || (be(0, 2) >> 7 != 0 && be(0, 2) >> 7 != 0x1ff)))]
pub struct Integer(pub [u8]);

/// The contents of a DER OBJECT IDENTIFIER: one or more base-128 numbers.
///
/// Each number has the shortest form (no leading `0x80`), and ends with a byte below `0x80`.
#[derive(Pun)]
#[repr(transparent)]
#[pun(assert(size_of_val(self) >= 1 && be(size_of_val(self) - 1, 1) < 0x80))]
#[pun(assert(walk(
  0,
  |at| at < size_of_val(self),
  |at| if be(at, 1) == 0x80 && (at == 0 || be(at - 1, 1) < 0x80) { 1 / 0 } else { at + 1 },
) == size_of_val(self)))]
pub struct ObjectIdentifier(pub [u8]);

/// A GeneralizedTime in the form that Kerberos uses, `YYYYMMDDHHMMSSZ`.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
pub struct KerberosTime {
  pub digits: [u8in<b'0', b'9'>; 14],
  pub zulu: u8c<b'Z'>,
}

/// The version of an SNMP message, 0 (v1) or 1 (v2c).
#[derive(Pun)]
#[repr(C)]
pub struct SnmpVersion(#[pun(valid(0..=1))] pub u8);

/// The error status of an SNMP PDU, from 0 to 18.
#[derive(Pun)]
#[repr(C)]
pub struct ErrorStatus(#[pun(valid(0..=18))] pub u8);

/// An SNMP PDU: a request ID, an error status, an error index and the variable bindings.
pub type Pdu = Then<
  Der<u8c<0x02>, Integer>,
  Then<Der<u8c<0x02>, ErrorStatus>, Then<Der<u8c<0x02>, Integer>, Der<u8c<0x30>>>>,
>;

/// An SNMPv1 or v2c message: SEQUENCE { version, community, PDU }.
///
/// The PDU has a context tag from 0 to 8.
pub type SnmpMessage = Der<
  u8c<0x30>,
  Then<Der<u8c<0x02>, SnmpVersion>, Then<Der<u8c<0x04>>, Der<u8in<0xa0, 0xa8>, Pdu>>>,
>;

/// An SNMP variable binding: SEQUENCE { OID, value }.
pub type VarBind = Der<u8c<0x30>, Then<Der<u8c<0x06>, ObjectIdentifier>, Der<u8>>>;

/// The head of a Kerberos AS-REQ.
///
/// Its form is `[APPLICATION 10] SEQUENCE { [1] pvno 5, [2] msg-type 10, rest }`. The rest is bytes
/// that hold the padata, which is OPTIONAL, and the request body.
pub type AsReq = Der<u8c<0x6a>, Der<u8c<0x30>, Then<Pvno, Then<MessageType, [u8]>>>>;

/// The Kerberos field `[1] pvno`, of value 5.
pub type Pvno = Der<u8c<0xa1>, Der<u8c<0x02>, u8c<5>>>;

/// The Kerberos field `[2] msg-type`, of value 10 (AS-REQ).
pub type MessageType = Der<u8c<0xa2>, Der<u8c<0x02>, u8c<10>>>;

// -------------------------------------------------------------------------------------------------
// Protocol Buffers

/// A Protocol Buffers varint field (wire type 0) of a field number from 1 to 15, whose value
/// takes 1 byte.
///
/// A varint is 1 to 10 bytes of 7 bits each. Each length is a struct, and an enum of references
/// ([`Field`]) tells the forms apart. This file declares two lengths, and the others would be
/// declared the same way.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
#[pun(assert(key & 7 == 0 && key >= 8))]
#[pun(get(
  field: u8 = key >> 3,
  value: u64 = v0,
  zigzag: i64 = ((v0 >> 1) as i64) ^ -((v0 & 1) as i64),
))]
pub struct Varint1 {
  #[pun(valid(..0x80))]
  pub key: u8,
  #[pun(valid(..0x80))]
  pub v0: u8,
}

/// A Protocol Buffers varint field like [`Varint1`], whose value takes 2 bytes.
///
/// The last byte is not 0, so the varint has the shortest form.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
#[pun(assert(key & 7 == 0 && key >= 8))]
#[pun(get(field: u8 = key >> 3, value: u64 = (v0 & 0x7f) | (v1 << 7)))]
pub struct Varint2 {
  #[pun(valid(..0x80))]
  pub key: u8,
  #[pun(valid(0x80..))]
  pub v0: u8,
  #[pun(valid(1..0x80))]
  pub v1: u8,
}

/// A Protocol Buffers length-delimited field (wire type 2) whose length takes 1 byte.
///
/// Its data is a string, bytes or a nested message.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 2)]
#[pun(assert(key & 7 == 2 && key >= 8))]
#[pun(get(field: u8 = key >> 3))]
pub struct Bytes1 {
  #[pun(valid(..0x80))]
  pub key: u8,
  #[pun(valid(..0x80))]
  pub length: u8,
  pub data: [u8],
}

/// A Protocol Buffers length-delimited field like [`Bytes1`], whose length takes 2 bytes.
///
/// The last byte of the length is not 0, so the length has the shortest form.
#[derive(Pun)]
#[repr(C)]
#[pun(size = (l0 & 0x7f) + (l1 << 7) + 3)]
#[pun(assert(key & 7 == 2 && key >= 8))]
#[pun(get(field: u8 = key >> 3))]
pub struct Bytes2 {
  #[pun(valid(..0x80))]
  pub key: u8,
  #[pun(valid(0x80..))]
  pub l0: u8,
  #[pun(valid(1..0x80))]
  pub l1: u8,
  pub data: [u8],
}

/// A Protocol Buffers fixed field of 32 bits (wire type 5), little-endian.
#[derive(Pun)]
#[repr(C, packed)]
#[pun(assert(key & 7 == 5 && key >= 8))]
pub struct Fixed32 {
  #[pun(valid(..0x80))]
  pub key: u8,
  pub value: u32le,
}

/// A Protocol Buffers fixed field of 64 bits (wire type 1), little-endian.
#[derive(Pun)]
#[repr(C, packed)]
#[pun(assert(key & 7 == 1 && key >= 8))]
pub struct Fixed64 {
  #[pun(valid(..0x80))]
  pub key: u8,
  pub value: u64le,
}

/// A whole Protocol Buffers message, which fields of any form fill.
///
/// Each field has wire type 0, 1, 2 or 5, a field number of at least 1 and a key of at most 5
/// bytes. A varint takes at most 10 bytes. The rule is a walk with one field per step.
#[derive(Pun)]
#[repr(transparent)]
#[pun(assert(walk(0, |at| at < size_of_val(self), |at| {
  let kind = leb128(at) & 7;
  let value = at + leb128_len(at);
  if leb128(at) >> 3 == 0 || leb128_len(at) > 5 {
    1 / 0
  } else if kind == 0 && leb128_len(value) <= 10 {
    value + leb128_len(value)
  } else if kind == 1 {
    value + 8
  } else if kind == 2 {
    value + leb128_len(value) + leb128(value)
  } else if kind == 5 {
    value + 4
  } else {
    1 / 0
  }
}) == size_of_val(self)))]
pub struct Message(pub [u8]);

/// A Protocol Buffers field, of any of the forms above.
#[derive(Pun)]
pub enum Field<'a> {
  Varint1(&'a Varint1),
  Varint2(&'a Varint2),
  Bytes1(&'a Bytes1),
  Bytes2(&'a Bytes2),
  Fixed32(&'a Fixed32),
  Fixed64(&'a Fixed64),
}

// -------------------------------------------------------------------------------------------------
// MessagePack

/// A MessagePack string of up to 31 bytes.
#[derive(Pun)]
#[repr(C)]
#[pun(size = (head & 0x1f) + 1)]
pub struct FixStr {
  pub head: u8in<0xa0, 0xbf>,
  pub text: str,
}

/// A MessagePack string of up to 255 bytes.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 2)]
pub struct Str8 {
  pub head: u8c<0xd9>,
  pub length: u8,
  pub text: str,
}

/// A MessagePack string of up to 65535 bytes.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 3)]
pub struct Str16 {
  pub head: u8c<0xda>,
  pub length: u16be,
  pub text: str,
}

/// A MessagePack byte array of up to 255 bytes.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 2)]
pub struct Bin8 {
  pub head: u8c<0xc4>,
  pub length: u8,
  pub data: [u8],
}

/// A MessagePack unsigned integer of 16 bits.
#[derive(Pun)]
#[repr(C, packed)]
pub struct Uint16 {
  pub head: u8c<0xcd>,
  pub value: u16be,
}

/// A MessagePack unsigned integer of 32 bits.
#[derive(Pun)]
#[repr(C, packed)]
pub struct Uint32 {
  pub head: u8c<0xce>,
  pub value: u32be,
}

/// A MessagePack float of 64 bits, IEEE 754, big-endian.
///
/// The getter `value` decodes the float from its bits.
#[derive(Pun)]
#[repr(C, packed)]
#[pun(get(value: f64 = f64::from_bits(bits as u64)))]
pub struct Float64 {
  pub head: u8c<0xcb>,
  pub bits: u64be,
}

/// The head of a MessagePack array of up to 15 items.
///
/// The items, of their own sizes, follow it.
#[derive(Pun)]
#[repr(C)]
#[pun(get(count: u8 = self.0 & 15))]
pub struct FixArray(pub u8in<0x90, 0x9f>);

/// The head of a MessagePack map of up to 15 pairs.
#[derive(Pun)]
#[repr(C)]
#[pun(get(count: u8 = self.0 & 15))]
pub struct FixMap(pub u8in<0x80, 0x8f>);

/// A MessagePack item, of the forms above.
#[derive(Pun)]
pub enum MsgPack<'a> {
  PositiveFixint(&'a u8in<0x00, 0x7f>),
  NegativeFixint(&'a u8in<0xe0, 0xff>),
  Nil(&'a u8c<0xc0>),
  False(&'a u8c<0xc2>),
  True(&'a u8c<0xc3>),
  FixStr(&'a FixStr),
  Str8(&'a Str8),
  Str16(&'a Str16),
  Bin8(&'a Bin8),
  Uint16(&'a Uint16),
  Uint32(&'a Uint32),
  Float64(&'a Float64),
  /// The head of an array. The items are not declared (see the documentation of this module).
  FixArray(&'a FixArray),
  FixMap(&'a FixMap),
}

// -------------------------------------------------------------------------------------------------
// CBOR

/// A CBOR unsigned integer (major type 0) from 0 to 23, held in the initial byte.
#[derive(Pun)]
#[repr(C)]
pub struct CborSmall(pub u8in<0x00, 0x17>);

/// A CBOR unsigned integer in 1 byte after the initial byte.
///
/// It is at least 24, so it has the shortest form.
#[derive(Pun)]
#[repr(C, packed)]
pub struct CborU8(pub u8c<0x18>, #[pun(valid(24..))] pub u8);

/// A CBOR unsigned integer in 2 bytes after the initial byte.
///
/// It is at least `0x100`, so it has the shortest form.
#[derive(Pun)]
#[repr(C, packed)]
pub struct CborU16(pub u8c<0x19>, #[pun(valid(0x100..))] pub u16be);

/// A CBOR unsigned integer in 4 bytes after the initial byte.
///
/// It is at least `0x1_0000`, so it has the shortest form.
#[derive(Pun)]
#[repr(C, packed)]
pub struct CborU32(pub u8c<0x1a>, #[pun(valid(0x1_0000..))] pub u32be);

/// A CBOR text string (major type 3) of 0 to 23 bytes.
#[derive(Pun)]
#[repr(C)]
#[pun(size = (head & 0x1f) + 1)]
pub struct CborText {
  pub head: u8in<0x60, 0x77>,
  pub text: str,
}

/// A CBOR text string of 24 to 255 bytes, whose length takes 1 byte.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 2)]
pub struct CborText8 {
  pub head: u8c<0x78>,
  #[pun(valid(24..))]
  pub length: u8,
  pub text: str,
}

/// A CBOR item, of the forms above.
#[derive(Pun)]
pub enum Cbor<'a> {
  Small(&'a CborSmall),
  U8(&'a CborU8),
  U16(&'a CborU16),
  U32(&'a CborU32),
  Text(&'a CborText),
  Text8(&'a CborText8),
  False(&'a u8c<0xf4>),
  True(&'a u8c<0xf5>),
  Null(&'a u8c<0xf6>),
}

// -------------------------------------------------------------------------------------------------
// Text

/// UTF-8 text after its length in bytes, a big-endian integer of 16 bits.
#[derive(Pun)]
#[repr(C)]
pub struct Utf8Field {
  pub length: u16be,
  #[pun(len = length)]
  pub text: str,
}

/// A C string: bytes up to the first NUL, which is its last byte.
#[derive(Pun)]
#[repr(transparent)]
#[pun(size = walk(0, |at| be(at, 1) != 0, |at| at + 1) + 1)]
pub struct CStr(pub [u8]);

/// A C string whose text is read as UTF-8.
///
/// Its text is a region of type `str`, checked when it is read: bytes that are not UTF-8 are a
/// valid `CText`, whose `text` returns an error.
#[derive(Pun)]
#[repr(transparent)]
#[pun(size = walk(0, |at| be(at, 1) != 0, |at| at + 1) + 1)]
pub struct CText {
  #[pun(regions(text: str = size_of_val(self) - 1, nul))]
  pub bytes: [u8],
}

/// A name in a field of 32 bytes: a string, then at least one NUL, and only NULs to the end.
#[derive(Pun)]
#[repr(transparent)]
// One walk: bit 16 of its state is "a NUL was seen", after which only NULs may follow.
#[pun(assert(walk(
  0,
  |s| s & 0xffff < 32,
  |s| if be(s & 0xffff, 1) == 0 { (s | 0x1_0000) + 1 } else if s >> 16 == 0 { s + 1 } else { 1 / 0 },
) == 0x1_0020))]
pub struct Padded32(pub [u8; 32]);

/// An HTTP/1.1 request line: a method (capital letters), a space, a target (visible ASCII), a
/// space, `HTTP/1.1` and CRLF.
#[derive(Pun)]
#[repr(transparent)]
#[pun(assert(size_of_val(self) >= 14 && be(size_of_val(self) - 11, 1) == 0x20))]
#[pun(assert(be(size_of_val(self) - 10, 8) == 0x4854_5450_2f31_2e31
  && be(size_of_val(self) - 2, 2) == 0x0d0a))]
// The method: letters, then a space.
#[pun(assert({
  let end = walk(0, |at| be(at, 1) >= 0x41 && be(at, 1) <= 0x5a, |at| at + 1);
  end > 0 && be(end, 1) == 0x20
}))]
// Up to the version: visible ASCII and one space (counted from bit 16).
#[pun(assert({
  let s = walk(
    0,
    |s| s & 0xffff < size_of_val(self) - 11,
    |s| {
      let byte = be(s & 0xffff, 1);
      if byte == 0x20 { s + 0x1_0001 } else if byte > 0x20 && byte < 0x7f { s + 1 } else { 1 / 0 }
    },
  );
  s >> 16 == 1 && be(size_of_val(self) - 12, 1) != 0x20
}))]
pub struct RequestLine(pub [u8]);

/// An HTTP header: `Content-Length: `, then decimal digits and CRLF.
///
/// The digits have no sign, and no leading zero unless the value is `0`. A walk decodes the
/// value, which is at most 2^40.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(size_of_val(self) >= 19 && be(size_of_val(self) - 2, 2) == 0x0d0a))]
#[pun(assert(be(16, 1) != 0x30 || size_of_val(self) == 19))]
#[pun(assert({
  let s = walk(
    16,
    |s| s & 0xffff < size_of_val(self) - 2,
    |s| {
      let byte = be(s & 0xffff, 1);
      if byte >= 0x30 && byte <= 0x39 {
        ((((s >> 16) * 10 + byte - 0x30) << 16) + (s & 0xffff)) + 1
      } else {
        1 / 0
      }
    },
  );
  s >> 16 <= 1 << 40
}))]
pub struct ContentLength {
  /// The name of the header, its colon and a space, `Content-Length: `, in this exact case.
  pub name: u128bec<0x436f_6e74_656e_742d_4c65_6e67_7468_3a20>,
  pub value: [u8],
}

// -------------------------------------------------------------------------------------------------
// Use

/// The bytes of an SNMP `GetRequest` (v2c) of community "public" for `sysDescr.0`.
const GET_REQUEST: [u8; 41] = [
  0x30, 0x27, 0x02, 0x01, 0x01, 0x04, 0x06, b'p', b'u', b'b', b'l', b'i', b'c', 0xa0, 0x1a, 0x02,
  0x02, 0x12, 0x34, 0x02, 0x01, 0x00, 0x02, 0x01, 0x00, 0x30, 0x0e, 0x30, 0x0c, 0x06, 0x08, 0x2b,
  0x06, 0x01, 0x02, 0x01, 0x01, 0x01, 0x00, 0x05, 0x00,
];

/// The bytes of the head of a Kerberos AS-REQ: pvno 5, msg-type 10 and an empty request body.
const AS_REQ: [u8; 18] = [
  0x6a, 0x10, 0x30, 0x0e, 0xa1, 0x03, 0x02, 0x01, 0x05, 0xa2, 0x03, 0x02, 0x01, 0x0a, 0xa4, 0x02,
  0x30, 0x00,
];

/// The bytes of a Protocol Buffers message of four fields: field 1 = 150 (a varint of 2 bytes),
/// field 2 = "testing", field 3 = -2 as `sint32`, and field 4 = 1 as `fixed32`.
const PROTOBUF: [u8; 19] = [
  0x08, 0x96, 0x01, 0x12, 0x07, b't', b'e', b's', b't', b'i', b'n', b'g', 0x18, 0x03, 0x25, 1, 0,
  0, 0,
];

/// The bytes of `{"compact": true, "schema": 0}` in MessagePack.
const MSGPACK: [u8; 18] = [
  0x82, 0xa7, b'c', b'o', b'm', b'p', b'a', b'c', b't', 0xc3, 0xa6, b's', b'c', b'h', b'e', b'm',
  b'a', 0x00,
];

/// The bytes of `"IETF"`, 100, 1000000 and `true` in CBOR (RFC 8949, appendix A).
const CBOR: [u8; 13] =
  [0x64, 0x49, 0x45, 0x54, 0x46, 0x18, 0x64, 0x1a, 0x00, 0x0f, 0x42, 0x40, 0xf5];

/// Reads the samples of this file through its types, and asserts the results.
pub fn main() {
  // ---- DER: an SNMP GetRequest, a chain of values of their own sizes.
  // A value from a stream: its length, of the short or the long form, says where it ends.
  let mut stream = vec![0x04, 0x81, 0x80];
  stream.resize(3 + 128 + 2, 7);
  let (value, rest) = from_head::<Der<u8c<0x04>>>(&stream).unwrap();
  assert_eq!((value.contents().map(<[u8]>::len), rest.len()), (Ok(128), 2));
  assert!(from_head::<Der<u8c<0x04>>>(&stream[..100]).is_err());
  assert_eq!(from_head::<Der<u8>>(&[0x02, 0x01, 5, 9]).map(|(_, rest)| rest), Ok(&[9][..]));
  let message: &SnmpMessage = try_pun_ref(&GET_REQUEST[..]).unwrap();
  let body = message.contents().unwrap();
  assert_eq!(body.first().unwrap().contents().unwrap().0, 1);
  let rest = body.rest().unwrap();
  assert_eq!(rest.first().unwrap().contents().unwrap(), b"public");
  let pdu = rest.rest().unwrap();
  assert_eq!(pdu.tag.get(), 0xa0);
  let fields = pdu.contents().unwrap();
  assert_eq!(&fields.first().unwrap().contents().unwrap().0, &[0x12, 0x34]);
  let bindings = fields.rest().unwrap().rest().unwrap().rest().unwrap();
  let binding: &VarBind = try_pun_ref(bindings.contents().unwrap()).unwrap();
  let pair = binding.contents().unwrap();
  let oid = pair.first().unwrap().contents().unwrap();
  assert_eq!((&oid.0, pair.rest().unwrap().tag), (&[0x2b, 6, 1, 2, 1, 1, 1, 0][..], 0x05));
  // A community of 200 bytes: lengths of the long form, at two levels.
  let long =
    [&[0x30, 0x81, 0xea, 0x02, 0x01, 0x01, 0x04, 0x81, 0xc8][..], &[b'p'; 200], &GET_REQUEST[13..]]
      .concat();
  let message: &SnmpMessage = try_pun_ref(&long[..]).unwrap();
  let community = message.contents().unwrap().rest().unwrap().first().unwrap();
  assert_eq!((community.long_length(), community.contents().unwrap().len()), (&[0xc8][..], 200));
  // Not a SEQUENCE; a length that is not the one of the bytes; a long form that is not the
  // shortest.
  for (at, value) in [(0, 0x31), (1, 0x28)] {
    let mut bytes = GET_REQUEST;
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, SnmpMessage>(&bytes[..]).err(), Some(Error::Validity));
  }
  let longer = [&[0x30, 0x81][..], &GET_REQUEST[1..]].concat();
  assert_eq!(try_pun_ref::<_, SnmpMessage>(&longer[..]).err(), Some(Error::Validity));
  // What is inside is a typed region, checked when read: version 3, a PDU tag of `0xa9`.
  let mut bytes = GET_REQUEST;
  bytes[4] = 3;
  let message: &SnmpMessage = try_pun_ref(&bytes[..]).unwrap();
  assert_eq!(message.contents().unwrap().first().unwrap().contents().err(), Some(Error::Validity));
  let mut bytes = GET_REQUEST;
  bytes[13] = 0xa9;
  let message: &SnmpMessage = try_pun_ref(&bytes[..]).unwrap();
  assert_eq!(message.contents().unwrap().rest().unwrap().rest().err(), Some(Error::Validity));
  // INTEGERs and OIDs of the shortest forms.
  assert_eq!(try_pun_ref::<_, Integer>(&[0x00u8, 0x7f][..]).err(), Some(Error::Validity));
  assert_eq!(try_pun_ref::<_, Integer>(&[0xffu8, 0x80][..]).err(), Some(Error::Validity));
  assert!(try_pun_ref::<_, Integer>(&[0x00u8, 0x80][..]).is_ok());
  assert_eq!(
    try_pun_ref::<_, ObjectIdentifier>(&[0x2bu8, 0x80, 0x01][..]).err(),
    Some(Error::Validity)
  );
  assert!(try_pun_ref::<_, ObjectIdentifier>(&[0x2bu8, 0x81, 0x00][..]).is_ok());
  assert_eq!(try_pun_ref::<_, ObjectIdentifier>(&[0x2bu8, 0x81][..]).err(), Some(Error::Validity));
  // Kerberos: fixed values in a chain, then the rest as bytes.
  let request: &AsReq = try_pun_ref(&AS_REQ[..]).unwrap();
  let sequence = request.contents().unwrap().contents().unwrap();
  assert!(sequence.first().unwrap().contents().unwrap().contents().is_ok());
  assert_eq!(sequence.rest().unwrap().rest().unwrap(), &AS_REQ[14..]);
  let mut bytes = AS_REQ;
  bytes[8] = 4;
  let request: &AsReq = try_pun_ref(&bytes[..]).unwrap();
  let pvno = request.contents().unwrap().contents().unwrap().first().unwrap();
  assert_eq!(pvno.contents().unwrap().contents().err(), Some(Error::Validity));
  let time: &KerberosTime = try_pun_ref(b"20261117120000Z").unwrap();
  assert_eq!(time.digits[3].get(), b'6');
  assert_eq!(try_pun_ref::<_, KerberosTime>(b"2026111712000aZ").err(), Some(Error::Validity));

  // ---- Protocol Buffers: a message, then its fields, of their own forms.
  assert!(try_pun_ref::<_, Message>(&PROTOBUF[..]).is_ok());
  // A varint that extends into the next field; wire type 3; a length past the end.
  for (at, value) in [(13, 0x83), (12, 0x1b), (4, 0x30)] {
    let mut bytes = PROTOBUF;
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, Message>(&bytes[..]).err(), Some(Error::Validity));
  }
  let mut bytes = PROTOBUF.to_vec();
  bytes.extend([0x18, 0x80, 0x80, 0x01]);
  assert!(try_pun_ref::<_, Message>(&bytes[..]).is_ok());
  let (Field::Varint2(first), rest) = Field::from_head(&PROTOBUF).unwrap() else { panic!() };
  assert_eq!((first.field(), first.value()), (1, 150));
  let (Field::Bytes1(second), rest) = Field::from_head(rest).unwrap() else { panic!() };
  assert_eq!((second.field(), &second.data), (2, &b"testing"[..]));
  let (Field::Varint1(third), rest) = Field::from_head(rest).unwrap() else { panic!() };
  assert_eq!((third.field(), third.value(), third.zigzag()), (3, 3, -2));
  let (Field::Fixed32(fourth), rest) = Field::from_head(rest).unwrap() else { panic!() };
  assert_eq!(({ fourth.value }.get(), rest.len()), (1, 0));
  // A string of 200 bytes: a length of 2 bytes; a fixed field of 64 bits.
  let mut string = vec![0x2a, 0xc8, 0x01];
  string.resize(3 + 200, b's');
  let Ok(Field::Bytes2(long)) = Field::from_bytes(&string) else { panic!() };
  assert_eq!((long.field(), long.data.len()), (5, 200));
  let Ok(Field::Fixed64(fixed)) = Field::from_bytes(&[0x31, 1, 0, 0, 0, 0, 0, 0, 0x80]) else {
    panic!()
  };
  assert_eq!({ fixed.value }.get(), 0x8000_0000_0000_0001);
  // Wire types 3, 6; field number 0; not the shortest varint; a varint of 3 bytes, which is not
  // one of the forms declared here.
  for bytes in
    [&[0x0b, 0][..], &[0x0e, 0], &[0x00, 1], &[0x08, 0x80, 0x00], &[0x08, 0x80, 0x80, 0x01]]
  {
    assert_eq!(Field::from_bytes(bytes).err(), Some(Error::Validity));
  }

  // ---- MessagePack: a map, item by item.
  let Ok((MsgPack::FixMap(map), rest)) = MsgPack::from_head(&MSGPACK) else { panic!() };
  assert_eq!(map.count(), 2);
  let mut items = vec![];
  let mut rest = rest;
  while let Ok((item, next)) = MsgPack::from_head(rest) {
    items.push(match item {
      MsgPack::PositiveFixint(n) => format!("{}", n.get()),
      MsgPack::NegativeFixint(n) => format!("-{}", 256 - u16::from(n.get())),
      // Values that are only their head.
      MsgPack::Nil(head) => format!("nil ({:#x})", head.get()),
      MsgPack::False(head) => format!("false ({:#x})", head.get()),
      MsgPack::True(head) => format!("true ({:#x})", head.get()),
      MsgPack::FixStr(s) => format!("{:?}", &s.text),
      MsgPack::Str8(s) => format!("{:?}", &s.text),
      MsgPack::Str16(s) => format!("{:?}", &s.text),
      MsgPack::Bin8(b) => format!("{:?}", &b.data),
      MsgPack::Uint16(n) => format!("{}", { n.value }),
      MsgPack::Uint32(n) => format!("{}", { n.value }),
      MsgPack::Float64(f) => format!("{}", f.value()),
      MsgPack::FixArray(a) => format!("[{}]", a.count()),
      MsgPack::FixMap(m) => format!("{{{}}}", m.count()),
    });
    rest = next;
  }
  assert_eq!(items, ["\"compact\"", "true (0xc3)", "\"schema\"", "0"]);
  let Ok(MsgPack::Float64(pi)) =
    MsgPack::from_bytes(&[0xcb, 0x40, 0x09, 0x21, 0xfb, 0x54, 0x44, 0x2d, 0x18])
  else {
    panic!()
  };
  assert_eq!(pi.value(), core::f64::consts::PI);
  // Not UTF-8; `0xc1`, never used; too short.
  for bytes in [&[0xa2, 0xff, 0xfe][..], &[0xc1], &[0xa3, b'a', b'b']] {
    assert_eq!(MsgPack::from_bytes(bytes).err(), Some(Error::Validity));
  }

  // ---- CBOR: items of the shortest forms.
  let (Cbor::Text(text), rest) = Cbor::from_head(&CBOR).unwrap() else { panic!() };
  assert_eq!(&text.text, "IETF");
  let (Cbor::U8(hundred), rest) = Cbor::from_head(rest).unwrap() else { panic!() };
  assert_eq!(hundred.1, 100);
  let (Cbor::U32(million), rest) = Cbor::from_head(rest).unwrap() else { panic!() };
  assert_eq!({ million.1 }.get(), 1_000_000);
  assert!(matches!(Cbor::from_head(rest), Ok((Cbor::True(_), rest)) if rest.is_empty()));
  // Any item, in diagnostic notation.
  let describe = |bytes: &[u8]| match Cbor::from_bytes(bytes) {
    Ok(Cbor::Small(n)) => format!("{}", n.0.get()),
    Ok(Cbor::U8(n)) => format!("{}", n.1),
    Ok(Cbor::U16(n)) => format!("{}", { n.1 }),
    Ok(Cbor::U32(n)) => format!("{}", { n.1 }),
    Ok(Cbor::Text(text)) => format!("{:?}", &text.text),
    Ok(Cbor::Text8(text)) => format!("{:?}", &text.text),
    Ok(Cbor::False(head)) => format!("false ({:#x})", head.get()),
    Ok(Cbor::True(head)) => format!("true ({:#x})", head.get()),
    Ok(Cbor::Null(head)) => format!("null ({:#x})", head.get()),
    Err(error) => format!("not an item: {error}"),
  };
  assert_eq!(describe(&[0x17]), "23");
  assert_eq!(describe(&[0x19, 0x01, 0x00]), "256");
  assert_eq!(describe(&CBOR[..5]), "\"IETF\"");
  assert_eq!(describe(&[&[0x78, 24][..], &[b'x'; 24]].concat()), format!("{:?}", "x".repeat(24)));
  assert_eq!(describe(&[0xf4]), "false (0xf4)");
  assert_eq!(describe(&[0xf6]), "null (0xf6)");
  assert_eq!(describe(&[0xf7]), "not an item: value is not valid for the target type");
  // Not the shortest forms; not UTF-8.
  for bytes in [&[0x18, 0x17][..], &[0x19, 0x00, 0xff], &[0x61, 0xff]] {
    assert_eq!(Cbor::from_bytes(bytes).err(), Some(Error::Validity));
  }

  // ---- Text: lengths, NULs, padding.
  let (field, rest) = from_head::<Utf8Field>(b"\x00\x06h\xc3\xa9llo!").unwrap();
  assert_eq!((&field.text, rest), ("héllo", &b"!"[..]));
  for bytes in [&b"\x00\x02\xc3\x28"[..], b"\x00\x01\xc3\xa9"] {
    assert_eq!(from_head::<Utf8Field>(bytes).err(), Some(Error::Validity));
  }
  assert!(try_pun_ref::<_, CStr>(&b"hello\0"[..]).is_ok());
  // From a stream: up to the first NUL.
  let (string, rest) = from_head::<CStr>(b"hello\0world\0").unwrap();
  assert_eq!((&string.0, rest), (&b"hello\0"[..], &b"world\0"[..]));
  assert_eq!(from_head::<CStr>(b"hello").err(), Some(Error::Validity));
  for bytes in [&b"hel\0lo\0"[..], b"hello"] {
    assert_eq!(try_pun_ref::<_, CStr>(bytes).err(), Some(Error::Validity));
  }
  let text: &CText = try_pun_ref("café\0".as_bytes()).unwrap();
  assert_eq!(text.text(), Ok("café"));
  let text: &CText = try_pun_ref(&b"caf\xe9\0"[..]).unwrap();
  assert_eq!(text.text(), Err(Error::Validity));
  let mut name = [0u8; 32];
  name[..5].copy_from_slice(b"alice");
  assert!(try_pun_ref::<_, Padded32>(&name).is_ok());
  name[20] = b'x';
  assert_eq!(try_pun_ref::<_, Padded32>(&name).err(), Some(Error::Validity));
  assert_eq!(try_pun_ref::<_, Padded32>(&[b'a'; 32]).err(), Some(Error::Validity));
  // HTTP: a request line, a header of a decimal number.
  assert!(try_pun_ref::<_, RequestLine>(&b"GET /index.html?q=1 HTTP/1.1\r\n"[..]).is_ok());
  // A method in small letters; two spaces; another version; a space in the target; a line feed
  // alone; no method; a control character.
  for bytes in [
    &b"get / HTTP/1.1\r\n"[..],
    b"GET  / HTTP/1.1\r\n",
    b"GET / HTTP/1.0\r\n",
    b"GET /a b HTTP/1.1\r\n",
    b"GET / HTTP/1.1\n",
    b" / HTTP/1.1\r\n",
    b"GET /\x7f HTTP/1.1\r\n",
  ] {
    assert_eq!(try_pun_ref::<_, RequestLine>(bytes).err(), Some(Error::Validity));
  }
  let header: &ContentLength = try_pun_ref(&b"Content-Length: 1234\r\n"[..]).unwrap();
  assert_eq!(&header.value, b"1234\r\n");
  assert!(try_pun_ref::<_, ContentLength>(&b"Content-Length: 0\r\n"[..]).is_ok());
  // A letter; a leading zero; no digits; a sign; more than 2^40; another case.
  for bytes in [
    &b"Content-Length: 12a4\r\n"[..],
    b"Content-Length: 0123\r\n",
    b"Content-Length: \r\n",
    b"Content-Length: -1\r\n",
    b"Content-Length: 99999999999999999999\r\n",
    b"Content-length: 1\r\n",
  ] {
    assert_eq!(try_pun_ref::<_, ContentLength>(bytes).err(), Some(Error::Validity));
  }

  println!("ok");
}
