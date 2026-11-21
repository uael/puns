//! Transport and session protocols, declared with `#[derive(Pun)]` and read in place from bytes.
//!
//! The protocols are TLS records and handshakes, QUIC long headers, HTTP/2 frames, WebSocket
//! frames, SSH binary packets, MQTT control packets, CoAP messages and SCTP packets.
//!
//! As in `ethernet.rs`, the first half of this file only declares types, with no `unsafe`, no
//! `impl` and no parsing code, and the second half reads messages in place from bytes, in safe
//! Rust.
//!
//! The following table lists what is specific to these protocols, and what in the declarations
//! expresses it:
//!
//! | Specific to these protocols                    | Declared with                              |
//! |------------------------------------------------|--------------------------------------------|
//! | a TLS record says what it carries              | `tag(ContentType = u8c<22>)`               |
//! | lengths of 24 bits (TLS handshakes, HTTP/2)    | `u24be`, `size = length + 4`               |
//! | vectors that follow each other                 | `regions(items = length, rest: R)`         |
//! | the chain of vectors fills the message         | `assert({ let .. = be(..); .. })`          |
//! | a server name is text                          | a `str` field                              |
//! | QUIC connection IDs of variable length         | `regions(dcid = dcid_length, ..)`          |
//! | QUIC integers of 1, 2, 4 or 8 bytes            | `1 << (be(at, 1) >> 6)`, `be(at, 8) >> ..` |
//! | HTTP/2 SETTINGS, an array of settings          | `[Setting]`, a multiple of 6 bytes         |
//! | HTTP/2 padding and priority, present by flags  | `if flags & 8 != 0 { .. }` in `regions`    |
//! | the HTTP/2 connection preface                  | `u128bec<..>`, `u64bec<..>`                |
//! | WebSocket lengths of 7, 16 or 64 bits          | `size = .. if .. { be(2, 2) } ..`          |
//! | WebSocket shortest forms, short control frames | `assert(.. be(2, 2) >= 126)`               |
//! | WebSocket frames masked from clients only      | `WebSocket<MASK>`, `assert(.. == MASK)`    |
//! | an SSH MAC whose size the bytes do not give    | a const parameter, `SshPacket<_, MAC>`     |
//! | SSH padding to the block size                  | `assert((packet_length + 4) % BLOCK ..)`   |
//! | SSH name-lists of US-ASCII                     | `regions(names: [u8in<0x21, 0x7e>] = ..)`  |
//! | an MQTT remaining length of 1 to 4 bytes       | `size = 1 + leb128_len(1) + leb128(1)`     |
//! | MQTT flags that depend on the packet type      | `assert({ let kind = ..; .. })`            |
//! | MQTT strings, a length then UTF-8 without NUL  | `matches(Dfa::UTF8_NO_NUL, ..)`, a `str`   |
//! | CoAP option nibbles that add 0, 1 or 2 bytes   | `size = { let delta = head >> 4; .. }`     |
//! | CoAP options up to a marker, then a payload    | `assert({ let end = walk(..); .. })`       |
//! | SCTP chunks, padded to 4 bytes                 | `size = (length + 3) / 4 * 4`              |
//! | an SCTP CRC-32C, with its own field as zeros   | `crc::<32, 0x1edc6f41, ..>(0, .., 8, 12)`  |
//!
//! The following are not covered:
//!
//! - The option numbers of CoAP are sums of the deltas before them. This is state carried along
//!   a sequence, which the code that reads the options carries.
//! - A TLS handshake message can span records, and records are encrypted after the handshake.
//!   Reassembly and decryption make copies, not views of the bytes. Typed regions are checked when
//!   they are read, so the cipher suites of a ClientHello are checked only when they are read.
//! - What follows a QUIC long header is protected (header protection, AEAD), so reading it makes
//!   a copy. HTTP/2 header blocks are HPACK, with Huffman codes and indexes into a table, so
//!   decoding them makes a copy too.
//! - WebSocket payloads from a client are masked. Unmasking is a transformation, which the code
//!   that reads the frames does.
//!
//! `main` reads a message of each protocol from bytes and asserts its fields, regions and getters.
//! It then changes bytes, asserts that the read returns an error, and prints `ok`.
#![forbid(unsafe_code)]

use core::marker::PhantomData;

use puns::{
  Error, Pun, Tag, from_bytes, from_head,
  layout::Dfa,
  num::{u8c, u8in, u16be, u16bec, u24be, u32be, u32bec, u32le, u64bec, u128bec},
  try_pun_ref,
};

// -------------------------------------------------------------------------------------------------
// TLS

/// The content type of a TLS record, when what the record carries is not known.
#[derive(Pun, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct ContentType(#[pun(valid(20..=24))] pub u8);

/// A TLS record.
///
/// Its length is at most 2^14 + 2048, the limit of `TLSCiphertext`.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 5)]
#[pun(assert(length <= 18432))]
pub struct Record<P: ?Sized + Tag<ContentType> = [u8]> {
  pub content_type: P::Tag,
  #[pun(valid(0x0300..=0x0304))]
  pub version: u16be,
  pub length: u16be,
  pub fragment: P,
}

/// The type of a TLS handshake message, when it is not known.
#[derive(Pun, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct HandshakeType(pub u8);

/// A TLS handshake message.
#[derive(Pun)]
#[repr(C)]
#[pun(tag(ContentType = u8c<22>))]
#[pun(size = length + 4)]
pub struct Handshake<P: ?Sized + Tag<HandshakeType> = [u8]> {
  pub msg_type: P::Tag,
  pub length: u24be,
  pub body: P,
}

/// A TLS vector of up to 255 bytes, followed by an `R`.
#[derive(Pun)]
#[repr(C)]
pub struct V8<R: ?Sized = [u8]> {
  pub length: u8,
  /// The type of the region `rest`.
  ///
  /// A type that only a region names must also be named by a field.
  pub rest_type: PhantomData<R>,
  #[pun(regions(items = length, rest: R))]
  pub data: [u8],
}

/// A TLS vector of up to 65535 bytes, followed by an `R`.
#[derive(Pun)]
#[repr(C)]
pub struct V16<R: ?Sized = [u8]> {
  pub length: u16be,
  pub rest_type: PhantomData<R>,
  #[pun(regions(items = length, rest: R))]
  pub data: [u8],
}

/// The cipher suites of a ClientHello, 2 to 65534 bytes of `u16`s, followed by an `R`.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(length >= 2 && length % 2 == 0))]
pub struct CipherSuites<R: ?Sized = [u8]> {
  pub length: u16be,
  pub rest_type: PhantomData<R>,
  #[pun(regions(suites: [u16be] = length, rest: R))]
  pub data: [u8],
}

/// A ClientHello, which is a version, a random value, then vectors that follow each other.
///
/// The vectors are the session ID, the cipher suites, the compression methods and the extensions.
/// Each vector is a type of its own, whose `rest` is the next vector. The extensions end the
/// message, so what remains after them is `[u8; 0]`.
#[derive(Pun)]
#[repr(C)]
#[pun(tag(HandshakeType = u8c<1>))]
// The chain fills the message (checked here, at once: typed regions are checked when read).
#[pun(assert({
  let suites = 35 + be(34, 1);
  let compression = suites + 2 + be(suites, 2);
  let extensions = compression + 1 + be(compression, 1);
  extensions + 2 + be(extensions, 2) == size_of_val(self)
}))]
pub struct ClientHello {
  #[pun(valid(0x0300..=0x0303))]
  pub legacy_version: u16be,
  pub random: [u8; 32],
  pub session: V8<CipherSuites<V8<V16<[u8; 0]>>>>,
}

/// A TLS extension.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 4)]
pub struct TlsExtension {
  pub kind: u16be,
  pub length: u16be,
  pub data: [u8],
}

/// The data of the server name extension, which holds a single host name.
///
/// A host name is ASCII. The field `name` is a `str`, which accepts any UTF-8, so it accepts more
/// than host names.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(list_length + 2 == size_of_val(self) && name_length + 5 == size_of_val(self)))]
pub struct ServerName {
  pub list_length: u16be,
  pub name_type: u8c<0>,
  pub name_length: u16be,
  pub name: str,
}

// -------------------------------------------------------------------------------------------------
// QUIC

/// A QUIC packet with a long header, of any type.
///
/// The form bit and the fixed bit are set, and each connection ID has up to 20 bytes.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(be(6 + dcid_length, 1) <= 20))]
#[pun(get(packet_type: u8 = (first >> 4) & 3))]
pub struct LongHeader {
  #[pun(valid(0xc0..=0xff))]
  pub first: u8,
  pub version: u32be,
  #[pun(valid(..=20))]
  pub dcid_length: u8,
  #[pun(regions(dcid = dcid_length, scid_length = 1, scid = be(6 + dcid_length, 1), rest))]
  pub data: [u8],
}

/// An Initial packet of QUIC version 1, followed by other packets coalesced with it.
///
/// After the connection IDs come a token, then the length of the rest of the packet. The rest of
/// the packet is the packet number and the protected payload.
///
/// The length of the token and the length of the packet are variable-length integers. Their first
/// two bits give their size, 1, 2, 4 or 8 bytes (`1 << (be(at, 1) >> 6)`). The other bits are the
/// value. Each integer is read as 8 bytes, shifted (`be(at, 8) >> (64 - 8 * n)`) and masked. At
/// least 20 bytes follow both integers in a packet that satisfies the rules. A region cannot name
/// a value that another region computed, so each region computes again the offsets it needs.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(be(6 + dcid_length, 1) <= 20))]
// The packet number (1 to 4 bytes) and the 16 bytes of the AEAD tag, at least.
#[pun(assert({
  let at = 7 + dcid_length + be(6 + dcid_length, 1);
  let n = 1 << (be(at, 1) >> 6);
  let at = at + n + ((be(at, 8) >> (64 - 8 * n)) & ((1 << (8 * n - 2)) - 1));
  let m = 1 << (be(at, 1) >> 6);
  ((be(at, 8) >> (64 - 8 * m)) & ((1 << (8 * m - 2)) - 1)) >= 20
}))]
pub struct Initial {
  /// The long header bit, the fixed bit and the type 0 (Initial).
  ///
  /// The 4 low bits are protected.
  #[pun(valid(0xc0..=0xcf))]
  pub first: u8,
  pub version: u32bec<1>,
  #[pun(valid(..=20))]
  pub dcid_length: u8,
  #[pun(regions(
    dcid = dcid_length,
    scid_length = 1,
    scid = be(6 + dcid_length, 1),
    token_length = 1 << (be(7 + dcid_length + be(6 + dcid_length, 1), 1) >> 6),
    token = {
      let at = 7 + dcid_length + be(6 + dcid_length, 1);
      let n = 1 << (be(at, 1) >> 6);
      (be(at, 8) >> (64 - 8 * n)) & ((1 << (8 * n - 2)) - 1)
    },
    length = {
      let at = 7 + dcid_length + be(6 + dcid_length, 1);
      let n = 1 << (be(at, 1) >> 6);
      let at = at + n + ((be(at, 8) >> (64 - 8 * n)) & ((1 << (8 * n - 2)) - 1));
      1 << (be(at, 1) >> 6)
    },
    protected = {
      let at = 7 + dcid_length + be(6 + dcid_length, 1);
      let n = 1 << (be(at, 1) >> 6);
      let at = at + n + ((be(at, 8) >> (64 - 8 * n)) & ((1 << (8 * n - 2)) - 1));
      let m = 1 << (be(at, 1) >> 6);
      (be(at, 8) >> (64 - 8 * m)) & ((1 << (8 * m - 2)) - 1)
    },
    coalesced,
  ))]
  pub data: [u8],
}

// -------------------------------------------------------------------------------------------------
// HTTP/2

/// The type of an HTTP/2 frame, when it is not known.
#[derive(Pun, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct FrameType(pub u8);

/// An HTTP/2 frame, of any type unless `P` gives one.
///
/// The maximum size of a frame is a setting of the connection, which the bytes of the frame do
/// not hold.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 9)]
#[pun(get(stream_id: u32 = stream & 0x7fff_ffff))]
pub struct Frame<P: ?Sized + Tag<FrameType> = [u8]> {
  pub length: u24be,
  pub kind: P::Tag,
  pub flags: u8,
  /// A reserved bit, then the stream identifier (31 bits).
  pub stream: u32be,
  pub payload: P,
}

/// An HTTP/2 setting.
#[derive(Pun, Debug, Copy, Clone)]
#[repr(C)]
pub struct Setting {
  pub id: u16be,
  pub value: u32be,
}

/// The payload of a SETTINGS frame, a slice of settings of 6 bytes each.
#[derive(Pun)]
#[repr(transparent)]
#[pun(tag(FrameType = u8c<4>))]
pub struct Settings(pub [Setting]);

/// A whole SETTINGS frame, with the rules that involve both its header and its payload.
///
/// The frame is on stream 0, and an ACK holds no settings.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 9)]
#[pun(assert(stream & 0x7fff_ffff == 0))]
#[pun(assert(flags & 1 == 0 || length == 0))]
pub struct SettingsFrame {
  pub length: u24be,
  pub kind: u8c<4>,
  pub flags: u8,
  pub stream: u32be,
  pub settings: [Setting],
}

/// An HTTP/2 DATA frame.
///
/// With the flag PADDED (`0x8`), the frame holds a pad length, the data, then that many pad bytes.
/// A field that only a flag makes present is a region, so the size of `data` reads the pad length
/// again with `be`.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 9)]
#[pun(assert(stream & 0x7fff_ffff != 0))]
#[pun(get(end_stream: bool = flags & 1 != 0, padded: bool = flags & 8 != 0))]
pub struct DataFrame {
  pub length: u24be,
  pub kind: u8c<0>,
  pub flags: u8,
  pub stream: u32be,
  #[pun(regions(
    pad_length = if flags & 8 != 0 { 1 } else { 0 },
    data = if flags & 8 != 0 { length - 1 - be(9, 1) } else { length },
    padding,
  ))]
  pub payload: [u8],
}

/// An HTTP/2 HEADERS frame.
///
/// It holds a pad length (with PADDED), a priority (with PRIORITY, `0x20`), a header block
/// fragment, then pad bytes.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 9)]
#[pun(assert(stream & 0x7fff_ffff != 0))]
pub struct HeadersFrame {
  pub length: u24be,
  pub kind: u8c<1>,
  pub flags: u8,
  pub stream: u32be,
  #[pun(regions(
    pad_length = if flags & 8 != 0 { 1 } else { 0 },
    priority = if flags & 0x20 != 0 { 5 } else { 0 },
    block = length
      - (if flags & 8 != 0 { 1 + be(9, 1) } else { 0 })
      - (if flags & 0x20 != 0 { 5 } else { 0 }),
    padding,
  ))]
  pub payload: [u8],
}

/// The connection preface of an HTTP/2 client, `PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n`.
#[derive(Pun)]
#[repr(C)]
pub struct Preface {
  pub magic: u128bec<0x5052_4920_2a20_4854_5450_2f32_2e30_0d0a>,
  pub magic_end: u64bec<0x0d0a_534d_0d0a_0d0a>,
}

// -------------------------------------------------------------------------------------------------
// WebSocket

/// The first byte of a WebSocket frame.
///
/// It holds FIN, RSV1 to RSV3 (0 without an extension) and an opcode (0 to 2 for data, 8 to 10 for
/// control frames).
#[derive(Pun, Debug, Copy, Clone)]
#[repr(transparent)]
#[pun(assert(self.0 & 0x70 == 0))]
#[pun(assert({ let opcode = self.0 & 0x0f; opcode <= 2 || (opcode >= 8 && opcode <= 10) }))]
// A control frame is not fragmented.
#[pun(assert(self.0 & 0x08 == 0 || self.0 & 0x80 != 0))]
#[pun(get(fin: bool = self.0 & 0x80 != 0, opcode: u8 = self.0 & 0x0f))]
pub struct Head(pub u8);

/// A WebSocket frame from a client (`MASK` is 1, masked) or from a server (`MASK` is 0).
///
/// The frame holds a head, then a byte of a mask bit and a length of 7 bits. A length of 126 or
/// 127 means that the length is in the 16 or 64 bits that follow. Then come the key of the mask,
/// if the frame is masked, and the payload.
#[derive(Pun)]
#[repr(C)]
#[pun(size = {
  let form = length & 0x7f;
  let extended = if form == 126 { 2 } else if form == 127 { 8 } else { 0 };
  let payload = if form == 126 { be(2, 2) } else if form == 127 { be(2, 8) } else { form };
  2 + extended + (length >> 7) * 4 + payload
})]
#[pun(assert(length >> 7 == MASK as u128))]
// The shortest form, a length whose first bit is 0, and control frames of the short form.
#[pun(assert(length & 0x7f != 126 || be(2, 2) >= 126))]
#[pun(assert(length & 0x7f != 127 || (be(2, 8) >= 0x1_0000 && be(2, 8) >> 63 == 0)))]
#[pun(assert(head & 0x08 == 0 || length & 0x7f <= 125))]
pub struct WebSocket<const MASK: u8> {
  pub head: Head,
  pub length: u8,
  #[pun(regions(
    extended = if length & 0x7f == 126 { 2 } else if length & 0x7f == 127 { 8 } else { 0 },
    key = (length >> 7) * 4,
    payload,
  ))]
  pub rest: [u8],
}

/// A WebSocket frame from a server, which is never masked.
pub type ServerFrame = WebSocket<0>;

/// A WebSocket frame from a client, which is always masked.
pub type ClientFrame = WebSocket<1>;

// -------------------------------------------------------------------------------------------------
// SSH

/// An SSH binary packet (RFC 4253), before encryption or with the "none" cipher.
///
/// It holds a packet length, a padding length, a payload, 4 to 255 bytes of padding and a MAC of
/// `MAC` bytes. The size of the MAC comes from the negotiation, not from the bytes. Without its
/// MAC, the packet is a multiple of the block size `BLOCK`, or of 8 if `BLOCK` is smaller.
#[derive(Pun)]
#[repr(C)]
#[pun(size = packet_length + 4 + MAC)]
#[pun(assert(packet_length >= padding_length + 1 && packet_length <= 35000))]
#[pun(assert((packet_length + 4) % (if BLOCK > 8 { BLOCK } else { 8 }) == 0))]
pub struct SshPacket<const BLOCK: usize, const MAC: usize> {
  pub packet_length: u32be,
  #[pun(valid(4..))]
  pub padding_length: u8,
  #[pun(regions(payload = packet_length - padding_length - 1, padding = padding_length, mac))]
  pub data: [u8],
}

/// An SSH name-list, followed by an `R`.
///
/// A name-list is a length, then US-ASCII names separated by commas.
#[derive(Pun)]
#[repr(C)]
pub struct NameList<R: ?Sized> {
  pub length: u32be,
  pub rest_type: PhantomData<R>,
  #[pun(regions(names: [u8in<0x21, 0x7e>] = length, rest: R))]
  pub data: [u8],
}

/// The end of a KEXINIT message.
#[derive(Pun)]
#[repr(C)]
pub struct KexEnd {
  pub first_kex_packet_follows: bool,
  pub reserved: u32bec<0>,
}

/// An SSH KEXINIT message.
///
/// It holds a cookie, ten name-lists and an end. The name-lists are the key exchange, the host
/// key, then the ciphers, the MACs, the compressions and the languages of both directions. Each
/// name-list is a type whose `R` is the next one, so they make a chain of ten types.
#[derive(Pun)]
#[repr(C)]
pub struct KexInit {
  pub message: u8c<20>,
  pub cookie: [u8; 16],
  pub lists: NameList<NameList<NameList<NameList<NameList<Lists5>>>>>,
}

/// The last five name-lists of KEXINIT, followed by its end.
pub type Lists5 = NameList<NameList<NameList<NameList<NameList<KexEnd>>>>>;

// -------------------------------------------------------------------------------------------------
// MQTT

/// The first byte of an MQTT 3.1.1 control packet, a packet type and flags.
///
/// The type is 4 bits, from 1 to 14. The flags are 4 bits, and their values depend on the type.
/// PUBLISH has the DUP flag, the `QoS` (3 is not one) and the retain flag. The flags are 2 for
/// PUBREL, SUBSCRIBE and UNSUBSCRIBE, and 0 for the other types.
#[derive(Pun, Debug, Copy, Clone)]
#[repr(transparent)]
#[pun(assert(self.0 >> 4 >= 1 && self.0 >> 4 <= 14))]
#[pun(assert({
  let kind = self.0 >> 4;
  let flags = self.0 & 15;
  (kind == 3 && flags & 6 != 6)
    || ((kind == 6 || kind == 8 || kind == 10) && flags == 2)
    || (kind != 3 && kind != 6 && kind != 8 && kind != 10 && flags == 0)
}))]
#[pun(get(kind: u8 = self.0 >> 4, flags: u8 = self.0 & 15))]
pub struct Fixed(pub u8);

/// An MQTT control packet.
///
/// It holds its first byte, the remaining length, then a body of that size. The remaining length
/// is a LEB128 of 1 to 4 bytes, in its shortest form. Its last byte is not 0, except for a length
/// of 0.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 1 + leb128_len(1) + leb128(1))]
#[pun(assert(leb128_len(1) <= 4 && (leb128_len(1) == 1 || be(leb128_len(1), 1) != 0)))]
pub struct Packet<P: ?Sized = [u8]> {
  pub header: Fixed,
  pub body_type: PhantomData<P>,
  #[pun(regions(length = leb128_len(1), body: P))]
  pub rest: [u8],
}

/// An MQTT UTF-8 string, followed by an `R`.
///
/// The string is a 16-bit length, then the text, which holds no `U+0000`.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(matches(Dfa::UTF8_NO_NUL, 2, 2 + length)))]
pub struct Utf8<R: ?Sized = [u8; 0]> {
  pub length: u16be,
  pub rest_type: PhantomData<R>,
  #[pun(regions(text: str = length, rest: R))]
  pub data: [u8],
}

/// The body of an MQTT CONNECT packet.
///
/// It holds the protocol name "MQTT", the level 4, flags and a keep alive. Then comes the payload,
/// a client identifier followed by what the flags announce (will, user name, password).
#[derive(Pun)]
#[repr(C)]
#[pun(assert(flags & 1 == 0))]
// Will QoS and will retain only with the will flag; QoS 3 is not one; a password needs a user.
#[pun(assert(flags & 4 != 0 || flags & 0x38 == 0))]
#[pun(assert(flags & 0x18 != 0x18))]
#[pun(assert(flags & 0x80 != 0 || flags & 0x40 == 0))]
#[pun(get(clean_session: bool = flags & 2 != 0, has_user: bool = flags & 0x80 != 0))]
pub struct Connect {
  pub name_length: u16bec<4>,
  pub name: u32bec<0x4d51_5454>,
  pub level: u8c<4>,
  pub flags: u8,
  pub keep_alive: u16be,
  pub client_id: Utf8<[u8]>,
}

// -------------------------------------------------------------------------------------------------
// CoAP

/// A CoAP message (RFC 7252).
///
/// It holds a header of 4 bytes, a token of 0 to 8 bytes, then options and a payload.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(version_type_tkl >> 6 == 1))]
#[pun(assert(version_type_tkl & 15 <= 8))]
// Code classes 0 (request), 2, 4, 5 (responses) and 7 (signaling); an empty message (code 0.00)
// is only its header.
#[pun(assert({
  let class = code >> 5;
  class == 0 || class == 2 || class == 4 || class == 5 || class == 7
}))]
#[pun(assert(code != 0 || size_of_val(self) == 4))]
// The options are well formed, up to the end or to a payload marker, which a payload follows.
// Each nibble of the head of an option is a value from 0 to 12, or 13 or 14 for 1 or 2 more
// bytes. A nibble of 15 is not allowed.
#[pun(assert({
  let end = walk(
    4 + (version_type_tkl & 15),
    |at| at < size_of_val(self) && be(at, 1) != 0xff,
    |at| {
      let head = be(at, 1);
      let delta = head >> 4;
      let length = head & 15;
      let after = at + 1 + if delta < 13 { 0 } else if delta < 15 { delta - 12 } else { 1 / 0 };
      let value = if length < 13 {
        length
      } else if length == 13 {
        13 + be(after, 1)
      } else if length == 14 {
        269 + be(after, 2)
      } else {
        1 / 0
      };
      after + (if length < 13 { 0 } else { length - 12 }) + value
    },
  );
  end == size_of_val(self) || end + 1 < size_of_val(self)
}))]
#[pun(get(kind: u8 = (version_type_tkl >> 4) & 3, class: u8 = code >> 5, detail: u8 = code & 31))]
pub struct Coap {
  /// The version (2 bits, always 1), the type (2 bits) and the token length (4 bits).
  pub version_type_tkl: u8,
  pub code: u8,
  pub message_id: u16be,
  #[pun(regions(token = version_type_tkl & 15, options_and_payload))]
  pub data: [u8],
}

/// A CoAP option.
///
/// It starts with a head of two nibbles, a delta and a length. Each nibble is a value from 0 to
/// 12, or 13 or 14 for 1 or 2 more bytes. A nibble of 15 is not allowed. The extra bytes follow
/// the head, then the value.
#[derive(Pun)]
#[repr(C)]
#[pun(size = {
  let delta = head >> 4;
  let length = head & 15;
  let after = 1 + if delta < 13 { 0 } else if delta < 15 { delta - 12 } else { 1 / 0 };
  let value = if length < 13 {
    length
  } else if length == 13 {
    13 + be(after, 1)
  } else if length == 14 {
    269 + be(after, 2)
  } else {
    1 / 0
  };
  after + (if length < 13 { 0 } else { length - 12 }) + value
})]
pub struct CoapOption {
  pub head: u8,
  #[pun(regions(
    delta = if head >> 4 < 13 { 0 } else { (head >> 4) - 12 },
    length = if head & 15 < 13 { 0 } else { (head & 15) - 12 },
    value,
  ))]
  pub rest: [u8],
}

// -------------------------------------------------------------------------------------------------
// SCTP

/// An SCTP packet, a common header followed by chunks.
///
/// Its checksum is the CRC-32C of the packet, computed with the checksum field as zeros.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(
  crc::<32, 0x1edc6f41, 0xffffffff, true, 0xffffffff>(0, size_of_val(self), 8, 12) == checksum
))]
pub struct Sctp {
  pub source_port: u16be,
  pub destination_port: u16be,
  pub verification_tag: u32be,
  /// The CRC-32C, least significant byte first.
  pub checksum: u32le,
  pub chunks: [u8],
}

/// An SCTP chunk.
///
/// Its length does not count the pad bytes that follow it up to a multiple of 4 bytes.
#[derive(Pun)]
#[repr(C)]
#[pun(size = (length + 3) / 4 * 4)]
#[pun(assert(length >= 4))]
pub struct Chunk {
  pub kind: u8,
  pub flags: u8,
  pub length: u16be,
  pub value: [u8],
}

// -------------------------------------------------------------------------------------------------
// Use

/// The bytes of a TLS record that holds a ClientHello with three cipher suites and a server name.
const CLIENT_HELLO: [u8; 108] = [
  0x16, 0x03, 0x01, 0x00, 0x67, 0x01, 0x00, 0x00, 0x63, 0x03, 0x03, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a,
  0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a,
  0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x20, 0xe0, 0xe0, 0xe0, 0xe0,
  0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0,
  0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0x00, 0x06, 0x13, 0x01,
  0x13, 0x02, 0xc0, 0x2f, 0x01, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x10, 0x00, 0x0e, 0x00, 0x00,
  0x0b, b'e', b'x', b'a', b'm', b'p', b'l', b'e', b'.', b'c', b'o', b'm',
];

/// The bytes of the same record with 3 bytes of cipher suites.
///
/// The chain of vectors still fills the message.
const ODD_HELLO: [u8; 105] = [
  0x16, 0x03, 0x01, 0x00, 0x64, 0x01, 0x00, 0x00, 0x60, 0x03, 0x03, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a,
  0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a,
  0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x20, 0xe0, 0xe0, 0xe0, 0xe0,
  0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0,
  0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0xe0, 0x00, 0x03, 0x13, 0x01,
  0x13, 0x01, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x10, 0x00, 0x0e, 0x00, 0x00, 0x0b, b'e', b'x',
  b'a', b'm', b'p', b'l', b'e', b'.', b'c', b'o', b'm',
];

/// The bytes of a QUIC Initial packet, followed by 3 bytes of another packet.
///
/// The packet has a destination connection ID of 8 bytes, no source connection ID and no token.
/// Its length takes 2 bytes (`0x4020`, which is 32), and 32 protected bytes follow.
const INITIAL: [u8; 53] = [
  0xc3, 0, 0, 0, 1, 8, 0x83, 0x94, 0xc8, 0xf0, 0x3e, 0x51, 0x57, 0x08, 0, 0, 0x40, 0x20, 0xaa,
  0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa,
  0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 1, 2,
  3,
];

/// The bytes of an HTTP/2 SETTINGS frame, then of a SETTINGS ACK.
///
/// The settings are at most 100 concurrent streams and an initial window of 65535.
const SETTINGS: [u8; 30] = [
  0, 0, 12, 4, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 100, 0, 4, 0, 0, 0xff, 0xff, 0, 0, 0, 4, 1, 0, 0, 0, 0,
];

/// The bytes of a padded HTTP/2 DATA frame on stream 1, with `END_STREAM`.
///
/// The frame holds a pad length of 3, "hello", then 3 pad bytes.
const DATA: [u8; 18] = [0, 0, 9, 0, 9, 0, 0, 0, 1, 3, b'h', b'e', b'l', b'l', b'o', 0, 0, 0];

/// The bytes of an HTTP/2 HEADERS frame on stream 3, with PRIORITY, PADDED and two bytes of
/// header block.
const HEADERS: [u8; 18] = [0, 0, 9, 1, 0x2c, 0, 0, 0, 3, 1, 0, 0, 0, 0, 15, 0x82, 0x86, 0];

/// The bytes of "Hello" from a server, an example of RFC 6455, section 5.7.
const HELLO: [u8; 7] = [0x81, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f];
/// The bytes of "Hello" from a client, masked, an example of RFC 6455, section 5.7.
const MASKED_HELLO: [u8; 11] = [0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58];
/// The bytes of a ping, an example of RFC 6455, section 5.7.
const PING: [u8; 7] = [0x89, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f];

/// The bytes of an SSH binary packet that holds a KEXINIT, with padding and a MAC of 32 bytes.
const KEXINIT: [u8; 192] = [
  0x00, 0x00, 0x00, 0x9c, 0x0b, 0x14, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x07,
  0x07, 0x07, 0x07, 0x07, 0x07, 0x07, 0x00, 0x00, 0x00, 0x11, b'c', b'u', b'r', b'v', b'e', b'2',
  b'5', b'5', b'1', b'9', b'-', b's', b'h', b'a', b'2', b'5', b'6', 0x00, 0x00, 0x00, 0x0b, b's',
  b's', b'h', b'-', b'e', b'd', b'2', b'5', b'5', b'1', b'9', 0x00, 0x00, 0x00, 0x0a, b'a', b'e',
  b's', b'1', b'2', b'8', b'-', b'c', b't', b'r', 0x00, 0x00, 0x00, 0x0a, b'a', b'e', b's', b'1',
  b'2', b'8', b'-', b'c', b't', b'r', 0x00, 0x00, 0x00, 0x0d, b'h', b'm', b'a', b'c', b'-', b's',
  b'h', b'a', b'2', b'-', b'2', b'5', b'6', 0x00, 0x00, 0x00, 0x0d, b'h', b'm', b'a', b'c', b'-',
  b's', b'h', b'a', b'2', b'-', b'2', b'5', b'6', 0x00, 0x00, 0x00, 0x04, b'n', b'o', b'n', b'e',
  0x00, 0x00, 0x00, 0x04, b'n', b'o', b'n', b'e', 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa,
  0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa, 0xaa,
];

/// The bytes of an MQTT CONNECT packet with the client identifier "abc", a clean session and a
/// keep alive of 60 seconds.
const CONNECT: [u8; 17] =
  [0x10, 15, 0, 4, b'M', b'Q', b'T', b'T', 4, 2, 0, 60, 0, 3, b'a', b'b', b'c'];

/// The bytes of a confirmable CoAP GET with the message ID `0x7d34`, the token `0x71` and the
/// Uri-Path "temperature" (option 11).
const GET: [u8; 17] = [
  0x41, 0x01, 0x7d, 0x34, 0x71, 0xbb, b't', b'e', b'm', b'p', b'e', b'r', b'a', b't', b'u', b'r',
  b'e',
];

/// The bytes of the response to [`GET`], 2.05 Content.
///
/// It holds Max-Age (option 14, a delta of 14 in an extended byte, "13 + 1"), the payload marker
/// and a payload.
const CONTENT: [u8; 16] =
  [0x61, 0x45, 0x7d, 0x34, 0x71, 0xd1, 0x01, 0x3c, 0xff, b'2', b'2', b'.', b'3', b' ', b'C', 0];

/// The bytes of an SCTP packet with an INIT chunk and the CRC-32C of the packet.
const SCTP: [u8; 32] = [
  0x13, 0x88, 0x13, 0x88, 0x00, 0x00, 0x00, 0x00, 0x94, 0x09, 0x49, 0x12, 0x01, 0x00, 0x00, 0x14,
  0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x0a, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x01,
];

/// Reads the samples of this file through its types, and asserts the results.
pub fn main() {
  // ---- TLS: a record, a handshake message, a ClientHello: a chain of vectors.
  let record: &Record<Handshake<ClientHello>> = try_pun_ref(&CLIENT_HELLO[..]).unwrap();
  assert_eq!(record.version.get(), 0x0301);
  let hello = &record.fragment.body;
  assert_eq!((hello.legacy_version.get(), hello.random), (0x0303, [0x5a; 32]));
  let session = &hello.session;
  assert_eq!(session.items(), [0xe0; 32]);
  let suites = session.rest().unwrap();
  let suites_list: Vec<u16> = suites.suites().unwrap().iter().map(|suite| suite.get()).collect();
  assert_eq!(suites_list, [0x1301, 0x1302, 0xc02f]);
  let compression = suites.rest().unwrap();
  assert_eq!(compression.items(), [0]);
  let extensions = compression.rest().unwrap();
  assert!(extensions.rest().is_ok());
  let (extension, rest) = from_head::<TlsExtension>(extensions.items()).unwrap();
  assert_eq!((extension.kind.get(), rest.len()), (0, 0));
  let server_name: &ServerName = try_pun_ref(&extension.data).unwrap();
  assert_eq!(&server_name.name, "example.com");
  // It is any record and any handshake message, too.
  let any: &Record = try_pun_ref(&CLIENT_HELLO[..]).unwrap();
  assert_eq!(any.content_type, ContentType(22));
  let (handshake, rest) = from_head::<Handshake>(&any.fragment).unwrap();
  assert_eq!((handshake.msg_type, rest.len()), (HandshakeType(1), 0));

  // ---- TLS: what is not a ClientHello.
  // Application data, not a content type, a version, a ServerHello, the lengths of the
  // handshake and of the record, a session ID longer than it is (the chain then does not fill
  // the message), the length of the cipher suites.
  for (at, value) in [(0, 23), (0, 19), (2, 5), (5, 2), (8, 0), (4, 0), (43, 33), (77, 8)] {
    let mut bytes = CLIENT_HELLO;
    bytes[at] = value;
    assert_eq!(
      try_pun_ref::<_, Record<Handshake<ClientHello>>>(&bytes[..]).err(),
      Some(Error::Validity)
    );
  }
  // An odd number of bytes of cipher suites, in a chain that still fills the message: the
  // message is valid, its typed region is not, when read.
  let odd: &Record<Handshake<ClientHello>> = try_pun_ref(&ODD_HELLO[..]).unwrap();
  assert_eq!(odd.fragment.body.session.rest().err(), Some(Error::Validity));
  // A server name that is not UTF-8.
  let mut bytes = CLIENT_HELLO;
  bytes[107] = 0xff;
  let record: &Record<Handshake<ClientHello>> = try_pun_ref(&bytes[..]).unwrap();
  let session = &record.fragment.body.session;
  let extensions = session.rest().unwrap().rest().unwrap().rest().unwrap();
  let (extension, _) = from_head::<TlsExtension>(extensions.items()).unwrap();
  assert_eq!(try_pun_ref::<_, ServerName>(&extension.data).err(), Some(Error::Validity));
  // A record too long for TLS.
  let mut long = vec![23u8, 3, 3, 0x48, 0x01];
  long.resize(5 + 0x4801, 0);
  assert_eq!(try_pun_ref::<_, Record>(&long[..]).err(), Some(Error::Validity));

  // ---- QUIC: connection IDs, then variable-length integers.
  let packet: &Initial = try_pun_ref(&INITIAL[..]).unwrap();
  assert_eq!(packet.dcid(), [0x83, 0x94, 0xc8, 0xf0, 0x3e, 0x51, 0x57, 0x08]);
  assert_eq!((packet.scid().len(), packet.token_length(), packet.token().len()), (0, &[0][..], 0));
  assert_eq!((packet.length(), packet.protected().len()), (&[0x40, 0x20][..], 32));
  assert_eq!(packet.coalesced(), [1, 2, 3]);
  let header: &LongHeader = try_pun_ref(&INITIAL[..]).unwrap();
  assert_eq!((header.packet_type(), header.version.get(), header.rest().len()), (0, 1, 38));
  // A token of 3 bytes; a length of 8 bytes (the shortest form is not required).
  let token = [&INITIAL[..15], &[3, 7, 7, 7], &INITIAL[16..]].concat();
  let packet: &Initial = try_pun_ref(&token[..]).unwrap();
  assert_eq!((packet.token(), packet.protected().len()), (&[7, 7, 7][..], 32));
  let wide = [&INITIAL[..16], &[0xc0, 0, 0, 0, 0, 0, 0, 0x20], &INITIAL[18..]].concat();
  let packet: &Initial = try_pun_ref(&wide[..]).unwrap();
  assert_eq!(
    (packet.length().len(), packet.protected().len(), packet.coalesced().len()),
    (8, 32, 3)
  );
  // A short header; a 0-RTT packet; version 2; connection IDs longer than 20 bytes; too short
  // for a packet number and a tag; a length beyond the packet.
  for (at, value) in [(0, 0x43), (0, 0xd3), (4, 2), (5, 21), (14, 21), (17, 0x10), (17, 0x30)] {
    let mut bytes = INITIAL;
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, Initial>(&bytes[..]).err(), Some(Error::Validity));
  }

  // ---- HTTP/2: the preface, then frames of typed payloads.
  let preface = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";
  assert!(try_pun_ref::<_, Preface>(&preface[..]).is_ok());
  let mut wrong = *preface;
  wrong[4] = b'X';
  assert_eq!(try_pun_ref::<_, Preface>(&wrong[..]).err(), Some(Error::Validity));
  let (frame, rest) = from_head::<Frame<Settings>>(&SETTINGS).unwrap();
  let settings: Vec<_> = frame.payload.0.iter().map(|s| (s.id.get(), s.value.get())).collect();
  assert_eq!((settings, frame.stream_id()), (vec![(3, 100), (4, 65535)], 0));
  let (ack, rest) = from_head::<SettingsFrame>(rest).unwrap();
  assert_eq!((ack.flags, ack.settings.len(), rest.len()), (1, 0, 0));
  assert!(from_head::<SettingsFrame>(&SETTINGS).is_ok());
  // A length that is not a multiple of 6; on a stream; an ACK that is not empty; not SETTINGS,
  // which is still a frame.
  let mut odd = SETTINGS;
  odd[2] = 11;
  assert!(from_head::<Frame<Settings>>(&odd).is_err());
  for at in [8, 4] {
    let mut bytes = SETTINGS;
    bytes[at] = 1;
    assert_eq!(from_head::<SettingsFrame>(&bytes).err(), Some(Error::Validity));
  }
  let mut other = SETTINGS;
  other[3] = 5;
  assert_eq!(from_head::<Frame<Settings>>(&other).err(), Some(Error::Validity));
  assert_eq!(from_head::<Frame>(&other).unwrap().0.kind, FrameType(5));
  // DATA, padded and not.
  let (data, rest) = from_head::<DataFrame>(&DATA).unwrap();
  assert_eq!(
    (data.data(), data.padding(), data.pad_length(), rest.len()),
    (&b"hello"[..], &[0; 3][..], &[3][..], 0)
  );
  assert!(data.end_stream() && data.padded());
  let plain = [0, 0, 5, 0, 0, 0, 0, 0, 1, b'h', b'e', b'l', b'l', b'o'];
  let (data, _) = from_head::<DataFrame>(&plain).unwrap();
  assert_eq!((data.data(), data.padding().len(), data.pad_length().len()), (&b"hello"[..], 0, 0));
  // Padding longer than the frame; DATA on stream 0.
  for (at, value) in [(9, 9), (8, 0)] {
    let mut bytes = DATA;
    bytes[at] = value;
    assert_eq!(from_head::<DataFrame>(&bytes).err(), Some(Error::Validity));
  }
  // HEADERS, with a priority and padding.
  let (headers, _) = from_head::<HeadersFrame>(&HEADERS).unwrap();
  assert_eq!(
    (headers.priority().len(), headers.block(), headers.padding()),
    (5, &[0x82, 0x86][..], &[0][..])
  );

  // ---- WebSocket: a length of 7, 16 or 64 bits, masked from a client.
  let (frame, rest) = from_head::<ServerFrame>(&HELLO).unwrap();
  assert_eq!(
    (frame.head.fin(), frame.head.opcode(), frame.payload(), rest.len()),
    (true, 1, &b"Hello"[..], 0)
  );
  assert!(matches!(from_bytes::<ServerFrame>(&PING), Ok(ping) if ping.head.opcode() == 9));
  // Unmasking makes new bytes, so the code that uses the types does it.
  let unmask = |frame: &ClientFrame| {
    let (key, payload) = (frame.key(), frame.payload());
    payload.iter().enumerate().map(|(i, byte)| byte ^ key[i % 4]).collect::<Vec<u8>>()
  };
  assert_eq!(unmask(from_bytes::<ClientFrame>(&MASKED_HELLO).unwrap()), b"Hello");
  // 256 bytes, then 65536 bytes, not masked.
  let mut medium = vec![0x82, 0x7e, 0x01, 0x00];
  medium.resize(4 + 256, 7);
  assert_eq!(from_bytes::<ServerFrame>(&medium).map(|frame| frame.payload().len()), Ok(256));
  let mut long = vec![0x82, 0x7f, 0, 0, 0, 0, 0, 1, 0, 0];
  long.resize(10 + 65536, 7);
  assert_eq!(from_bytes::<ServerFrame>(&long).map(|frame| frame.payload().len()), Ok(65536));
  // 256 bytes from a client, masked by a key of zeros.
  let mut masked = vec![0x82, 0xfe, 0x01, 0x00, 0, 0, 0, 0];
  masked.resize(8 + 256, 7);
  assert_eq!(unmask(from_bytes::<ClientFrame>(&masked).unwrap()), [7; 256]);
  // Masked from a server, not masked from a client.
  assert_eq!(from_bytes::<ServerFrame>(&MASKED_HELLO).err(), Some(Error::Validity));
  assert_eq!(from_bytes::<ClientFrame>(&HELLO).err(), Some(Error::Validity));
  // RSV bits, a reserved opcode, a fragmented control frame; a length that is not the one of
  // the bytes.
  for value in [0xc1, 0x83, 0x09] {
    let mut bytes = HELLO;
    bytes[0] = value;
    assert_eq!(from_bytes::<ServerFrame>(&bytes).err(), Some(Error::Validity));
  }
  assert!(from_bytes::<ServerFrame>(&HELLO[..6]).is_err());
  // Not the shortest form; a control frame of a long form; a length whose first bit is set.
  medium[2] = 0;
  medium[3] = 0x7d;
  medium.truncate(4 + 125);
  assert!(from_bytes::<ServerFrame>(&medium).is_err());
  let mut ping = vec![0x89, 0x7e, 0x01, 0x00];
  ping.resize(4 + 256, 7);
  assert!(from_bytes::<ServerFrame>(&ping).is_err());
  long[2] = 0x80;
  assert!(from_bytes::<ServerFrame>(&long).is_err());

  // ---- SSH: a packet of a block size and a MAC size that the bytes do not give.
  let (packet, rest) = from_head::<SshPacket<8, 32>>(&KEXINIT).unwrap();
  assert_eq!(
    (packet.payload().len(), packet.padding().len(), packet.mac(), rest.len()),
    (144, 11, &[0xaa; 32][..], 0)
  );
  let kex: &KexInit = try_pun_ref(packet.payload()).unwrap();
  assert_eq!(
    kex.lists.names().unwrap().iter().map(|c| c.get()).collect::<Vec<_>>(),
    b"curve25519-sha256"
  );
  // Ten levels down, each list checked when it is read.
  let lists = &kex.lists;
  let fifth = lists.rest().unwrap().rest().unwrap().rest().unwrap().rest().unwrap();
  assert_eq!(fifth.names().unwrap().len(), "hmac-sha2-256".len());
  let tenth = fifth.rest().unwrap().rest().unwrap().rest().unwrap().rest().unwrap().rest().unwrap();
  assert_eq!(tenth.names().unwrap().len(), 0);
  let end = tenth.rest().unwrap();
  assert!(!end.first_kex_packet_follows);
  // Not a multiple of the block size; padding under 4 bytes; another MAC size.
  let mut bytes = KEXINIT;
  bytes[3] += 1;
  assert!(from_head::<SshPacket<8, 32>>(&bytes).is_err());
  let mut bytes = KEXINIT;
  bytes[4] = 3;
  assert_eq!(from_head::<SshPacket<8, 32>>(&bytes).err(), Some(Error::Validity));
  assert_eq!(try_pun_ref::<_, SshPacket<8, 20>>(&KEXINIT[..]).err(), Some(Error::Validity));
  // A name with a space in it: the typed region is checked when it is read.
  let mut bytes = KEXINIT;
  bytes[5 + 21 + 3] = b' ';
  let (packet, _) = from_head::<SshPacket<8, 32>>(&bytes).unwrap();
  let kex: &KexInit = try_pun_ref(packet.payload()).unwrap();
  assert_eq!(kex.lists.names().err(), Some(Error::Validity));

  // ---- MQTT: a remaining length of 1 to 4 bytes, a typed body.
  let connect = from_bytes::<Packet<Connect>>(&CONNECT).unwrap();
  let body = connect.body().unwrap();
  assert_eq!((connect.header.kind(), body.keep_alive.get()), (1, 60));
  assert!(body.clean_session() && !body.has_user());
  assert_eq!(body.client_id.text(), Ok("abc"));
  let (any, rest) = from_head::<Packet>(&CONNECT).unwrap();
  assert_eq!((any.body().map(<[u8]>::len), rest.len()), (Ok(15), 0));
  // Flags of CONNECT, a reserved type, type 15; the length; the protocol name and level; a
  // reserved flag, a will QoS without a will, will QoS 3.
  for (at, value) in [
    (0, 0x12),
    (0, 0x00),
    (0, 0xf0),
    (1, 14),
    (4, b'm'),
    (8, 3),
    (9, 3),
    (9, 0x0a),
    (9, 0x1c),
    (9, 0x42),
  ] {
    let mut bytes = CONNECT;
    bytes[at] = value;
    assert!(from_bytes::<Packet<Connect>>(&bytes).and_then(|packet| packet.body()).is_err());
  }
  // A client identifier that is not UTF-8, or that holds a zero.
  for value in [0xff, 0] {
    let mut bytes = CONNECT;
    bytes[16] = value;
    let connect = from_bytes::<Packet<Connect>>(&bytes).unwrap();
    assert_eq!(connect.body().err(), Some(Error::Validity));
  }
  // PUBLISH of QoS 1 and 200 bytes (two bytes of length), then DISCONNECT.
  let mut publish = vec![0x32, 0xc8, 0x01];
  publish.resize(3 + 200, 0);
  publish.extend([0xe0, 0x00]);
  let (packet, rest) = from_head::<Packet>(&publish).unwrap();
  assert_eq!(
    (packet.header.flags(), packet.body().map(<[u8]>::len), rest),
    (2, Ok(200), &[0xe0, 0][..])
  );
  let (disconnect, _) = from_head::<Packet>(rest).unwrap();
  assert_eq!(disconnect.header.kind(), 14);
  // 16384 bytes: three bytes of length.
  let mut big = vec![0x30, 0x80, 0x80, 0x01];
  big.resize(4 + 16384, 0);
  assert_eq!(
    from_bytes::<Packet>(&big).map(|packet| packet.body().map(<[u8]>::len)),
    Ok(Ok(16384))
  );
  // Not the shortest form; PUBLISH of QoS 3; SUBSCRIBE without its flags; a fifth byte of length.
  for bytes in
    [&[0x30, 0x80, 0x00][..], &[0x36, 0x00], &[0x80, 0x00], &[0x30, 0xff, 0xff, 0xff, 0xff, 0x01]]
  {
    assert_eq!(from_bytes::<Packet>(bytes).err(), Some(Error::Validity));
  }
  assert!(from_bytes::<Packet>(&[0x82, 0x00]).is_ok());

  // ---- CoAP: a header, a token, then options of their own forms.
  let get: &Coap = try_pun_ref(&GET[..]).unwrap();
  assert_eq!((get.kind(), get.class(), get.detail(), get.token()), (0, 0, 1, &[0x71][..]));
  let (option, rest) = from_head::<CoapOption>(get.options_and_payload()).unwrap();
  assert_eq!((option.head >> 4, option.value(), rest.len()), (11, &b"temperature"[..], 0));
  let content: &Coap = try_pun_ref(&CONTENT[..]).unwrap();
  assert_eq!((content.class(), content.detail()), (2, 5));
  let (option, rest) = from_head::<CoapOption>(content.options_and_payload()).unwrap();
  assert_eq!(
    (option.delta()[0] + 13, option.value(), rest[0], &rest[1..]),
    (14, &[0x3c][..], 0xff, &b"22.3 C\0"[..])
  );
  // Options of the forms of 1 and 2 more bytes of length and delta.
  let mut long = vec![0x1d, 7];
  long.resize(2 + 20, b'x');
  assert_eq!(from_bytes::<CoapOption>(&long).map(|option| option.value().len()), Ok(20));
  let mut longer = vec![0xee, 0x01, 0x00, 0x00, 0x10];
  longer.resize(5 + 269 + 16, b'x');
  assert_eq!(from_bytes::<CoapOption>(&longer).map(|option| option.value().len()), Ok(285));
  // The payload marker, and a reserved nibble, are no option; a length past the end.
  for bytes in [&[0xff][..], &[0xf1, 0], &[0x1f], &[0x13, 1, 2]] {
    assert!(from_bytes::<CoapOption>(bytes).is_err());
  }
  // Version 2; a token of 9 bytes; class 1; an empty message with a token.
  for (at, value) in [(0, 0x81), (0, 0x49), (1, 0x21), (1, 0x00)] {
    let mut bytes = GET;
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, Coap>(&bytes[..]).err(), Some(Error::Validity));
  }
  // An option that extends beyond the message.
  let mut bytes = GET;
  bytes[5] = 0xbc;
  assert_eq!(try_pun_ref::<_, Coap>(&bytes[..]).err(), Some(Error::Validity));
  // A payload marker, and no payload.
  let mut bytes = GET.to_vec();
  bytes.push(0xff);
  assert_eq!(try_pun_ref::<_, Coap>(&bytes[..]).err(), Some(Error::Validity));
  bytes.push(b'!');
  assert!(try_pun_ref::<_, Coap>(&bytes[..]).is_ok());

  // ---- SCTP: chunks padded to 4 bytes, a CRC-32C over the packet.
  let sctp: &Sctp = try_pun_ref(&SCTP[..]).unwrap();
  assert_eq!((sctp.source_port.get(), sctp.checksum.get()), (5000, 0x1249_0994));
  let (chunk, rest) = from_head::<Chunk>(&sctp.chunks).unwrap();
  assert_eq!((chunk.kind, chunk.value.len(), rest.len()), (1, 16, 0));
  let (chunk, rest) = from_head::<Chunk>(&[0, 3, 0, 5, 9, 0, 0, 0, 0xee]).unwrap();
  assert_eq!((chunk.value.len(), rest), (4, &[0xee][..]));
  assert_eq!(from_head::<Chunk>(&[0, 3, 0, 3, 9, 0, 0, 0]).err(), Some(Error::Validity));
  let mut bytes = SCTP;
  bytes[20] ^= 1;
  assert_eq!(try_pun_ref::<_, Sctp>(&bytes[..]).err(), Some(Error::Validity));

  println!("ok");
}
