//! Ethernet frames and the packets they carry, declared with `#[derive(Pun)]` and read in place
//! from bytes.
//!
//! The first half of this file declares types, and nothing else in it describes Ethernet. The file
//! has no `unsafe`, no `impl`, no parsing code, no code that chooses a type from an EtherType, and
//! no size computation. The second half reads frames in place from bytes, in safe Rust.
//!
//! Another EtherType, protocol or header is one more declaration of the same kind. It says what
//! the header is made of, and which `tag` comes with it.
//!
//! The following table lists what is specific to Ethernet, and what in the declarations
//! expresses it:
//!
//! | Specific to Ethernet                                | Declared with                         |
//! |-----------------------------------------------------|---------------------------------------|
//! | big-endian fields                                   | `u16be`                               |
//! | the EtherType says what the payload is              | `tag(EtherType = u16bec<0x0800>)`     |
//! | an EtherType is at least `0x0600`                   | `valid(0x0600..)`                     |
//! | up to `1500`, the length of the payload             | `valid(..=1500)`, `regions(..)`       |
//! | a frame has 60 to 1514 bytes, padded if shorter     | `assert(size_of_val(self) >= 60 ..)`  |
//! | the frame check sequence, a CRC-32 at the end       | `regions(..)`, `assert(crc32(..) ..)` |
//! | the preamble and the start frame delimiter          | `[u8c<0x55>; 7]`, `u8c<0xd5>`         |
//! | LLC and SNAP headers                                | `u8c<0xaa>`                           |
//! | 802.1Q tags, nested or not                          | a payload that has a payload          |
//! | bit fields (VLAN identifier, IPv4 header length)    | `get(..)`, `assert(..)`, `valid(..)`  |
//! | the size of an IPv4 packet, in its header           | `size = total_length`                 |
//! | IPv4 options, whose size is in the header too       | `regions(options = .., payload)`      |
//! | the checksum of the IPv4 header                     | `assert(sum16(..) == 0xffff)`         |
//! | the checksums of UDP and TCP, over the IP addresses | `assert(.. sum16(12, 20) + ..)`       |
//! | the bits of a MAC address (group, local)            | `get(multicast: bool = ..)`           |
//! | a frame longer than its packet                      | `from_head` returns the rest          |
//! | the protocol of a packet says what it carries       | `tag(Protocol = u8c<17>)`             |
//! | IPv6 extension headers, chained by "next header"    | a payload that has a payload          |
//! | the checksums of UDP and TCP behind such a chain    | `assert({ let .. = walk(..); .. })`   |
//! | a checksum over a whole message (ICMP)              | `assert(sum16(0, ..) == 0xffff)`      |
//! | label stacks, sessions, lengths in other units      | `get(..)`, `size = length * 8 + ..`   |
//! | a frame that is one of several kinds                | an enum of references ([`AnyFrame`])  |
//!
//! The following are not covered:
//!
//! - The options of TCP and of the IPv6 hop-by-hop header are a region of bytes. No rule reads
//!   them, so a wrong option is not an error.
//! - A fragment is not the whole of what is carried, so it has no checksum to check. Putting the
//!   fragments back together makes a copy, not a view of the bytes.
//! - What is encrypted cannot be read, so the checksums of IPv6 are not checked behind it.
//!
//! `main` reads frames of each kind from bytes and asserts their fields, regions and getters. Some
//! conversions are proven, and use [`pun_ref`], such as an ARP frame reinterpreted as bytes. `main`
//! also changes bytes, such as an EtherType, a size or a checksum, and asserts the [`Error`] that
//! the read then returns. Last, it tells frames apart with [`AnyFrame`], and prints `ok`.
#![forbid(unsafe_code)]

use puns::{
  Error, Pun, Tag, from_head,
  num::{u8c, u16be, u16bec, u32be, u48be},
  pun_ref, try_pun_ref,
};

// -------------------------------------------------------------------------------------------------
// Ethernet

/// A MAC address.
#[derive(Pun, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
#[pun(get(
  broadcast: bool = self.0 == 0xffff_ffff_ffff,
  multicast: bool = self.0 & 0x0100_0000_0000 != 0,
  local: bool = self.0 & 0x0200_0000_0000 != 0,
))]
pub struct Mac(pub u48be);

/// An EtherType whose meaning is not known.
///
/// An EtherType is at least `0x0600`. In an IEEE 802.3 frame, a value up to 1500 in its place is
/// the length of the payload (see [`Frame8023`]).
///
/// A payload declares its EtherType with `#[pun(tag(EtherType = ..))]`.
#[derive(Pun, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct EtherType(#[pun(valid(0x0600..))] pub u16be);

/// An Ethernet II frame, without its preamble or its frame check sequence.
///
/// By default, the payload is bytes, of any EtherType. A `Frame<Ipv4>` is a frame whose EtherType
/// is the one of IPv4, and whose payload is an [`Ipv4`] packet.
#[derive(Pun)]
#[repr(C)]
pub struct Frame<P: ?Sized + Tag<EtherType> = [u8]> {
  pub destination: Mac,
  pub source: Mac,
  pub ether_type: P::Tag,
  pub payload: P,
}

/// A frame as it is on the wire, of 60 to 1514 bytes, padded if it would be shorter.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(size_of_val(self) >= 60 && size_of_val(self) <= 1514))]
pub struct WireFrame {
  pub destination: Mac,
  pub source: Mac,
  pub ether_type: EtherType,
  pub payload: [u8],
}

/// A frame with its frame check sequence.
///
/// The frame check sequence is the last 4 bytes. It is the CRC-32 of the bytes before it, least
/// significant byte first.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(size_of_val(self) >= 64 && size_of_val(self) <= 1518))]
#[pun(assert(crc32(0, size_of_val(self) - 4) == le(size_of_val(self) - 4, 4)))]
pub struct CheckedFrame {
  pub destination: Mac,
  pub source: Mac,
  pub ether_type: EtherType,
  #[pun(regions(payload = size_of_val(self) - 18, fcs))]
  pub data: [u8],
}

/// A frame as it is on the medium, after a preamble and a start frame delimiter, with its frame
/// check sequence.
#[derive(Pun)]
#[repr(C)]
pub struct Packet {
  pub preamble: [u8c<0x55>; 7],
  pub start: u8c<0xd5>,
  pub frame: CheckedFrame,
}

/// An IEEE 802.3 frame, which has the length of its payload in place of an EtherType.
///
/// The bytes after the payload, up to the end of the frame, are the region `padding`.
#[derive(Pun)]
#[repr(C)]
pub struct Frame8023 {
  pub destination: Mac,
  pub source: Mac,
  #[pun(valid(..=1500))]
  pub length: u16be,
  #[pun(regions(payload = length, padding))]
  pub data: [u8],
}

/// LLC and SNAP headers, which carry an EtherType in an 802.3 frame.
#[derive(Pun)]
#[repr(C)]
pub struct Snap<P: ?Sized + Tag<EtherType> = [u8]> {
  pub dsap: u8c<0xaa>,
  pub ssap: u8c<0xaa>,
  pub control: u8c<0x03>,
  pub organization: [u8; 3],
  pub ether_type: P::Tag,
  pub payload: P,
}

/// An IEEE 802.1Q tag, and the payload that follows it.
///
/// A `Vlan` is a payload that has a payload of its own, such as an [`Ipv4`] packet or another
/// `Vlan`.
#[derive(Pun)]
#[repr(C)]
#[pun(tag(EtherType = u16bec<0x8100>))]
#[pun(get(priority: u8 = tci >> 13, drop_eligible: bool = tci & 0x1000 != 0, id: u16 = tci & 0x0fff))]
#[pun(assert((tci & 0x0fff) != 0x0fff))]
pub struct Vlan<P: ?Sized + Tag<EtherType> = [u8]> {
  /// The priority (3 bits), the drop eligible indicator (1 bit) and the VLAN identifier (12 bits).
  ///
  /// The identifier `0xfff` is reserved, and a rule of `Vlan` excludes it.
  pub tci: u16be,
  pub ether_type: P::Tag,
  pub payload: P,
}

/// An IEEE 802.1ad service tag, the outer tag of "Q-in-Q".
#[derive(Pun)]
#[repr(C)]
#[pun(tag(EtherType = u16bec<0x88a8>))]
#[pun(get(id: u16 = tci & 0x0fff))]
pub struct ServiceVlan<P: ?Sized + Tag<EtherType> = [u8]> {
  pub tci: u16be,
  pub ether_type: P::Tag,
  pub payload: P,
}

// -------------------------------------------------------------------------------------------------
// ARP

/// The operation of an ARP message, a request or a reply, in network byte order.
#[derive(Pun, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u16)]
pub enum Operation {
  // The values that a `u16` has in memory when it is big-endian on the wire.
  Request = 1u16.to_be(),
  Reply = 2u16.to_be(),
}

/// An ARP message, for IPv4 over Ethernet.
#[derive(Pun, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C, packed)]
#[pun(tag(EtherType = u16bec<0x0806>))]
pub struct Arp {
  pub hardware_type: u16bec<1>,
  pub protocol_type: u16bec<0x0800>,
  pub hardware_length: u8c<6>,
  pub protocol_length: u8c<4>,
  pub operation: Operation,
  pub sender: (Mac, [u8; 4]),
  pub target: (Mac, [u8; 4]),
}

// -------------------------------------------------------------------------------------------------
// IPv4

/// An IPv4 packet.
///
/// The header holds the size of the packet and the size of its options.
#[derive(Pun)]
#[repr(C)]
#[pun(tag(EtherType = u16bec<0x0800>))]
#[pun(size = total_length)]
#[pun(assert(total_length >= (version_ihl & 0x0f) * 4))]
#[pun(assert(sum16(0, (version_ihl & 0x0f) * 4) == 0xffff))]
// The checksum of UDP (protocol 17, where 0 is "none") and of TCP (protocol 6), unless the packet
// is a fragment: over the addresses, the protocol and the size of what is carried, then over what
// is carried.
#[pun(assert({
  let header = (version_ihl & 0x0f) * 4;
  (protocol != 17 && protocol != 6)
    || flags_fragment_offset & 0x3fff != 0
    || (protocol == 17 && be(header + 6, 2) == 0)
    || (sum16(12, 20) + protocol + (total_length - header) + sum16(header, total_length)) % 0xffff == 0
}))]
#[pun(get(
  version: u8 = version_ihl >> 4,
  header_length: usize = (version_ihl & 0x0f) * 4,
  dont_fragment: bool = flags_fragment_offset & 0x4000 != 0,
  more_fragments: bool = flags_fragment_offset & 0x2000 != 0,
  fragment_offset: u16 = flags_fragment_offset & 0x1fff,
))]
pub struct Ipv4<P: ?Sized + Tag<Protocol> = [u8]> {
  /// The version (4 bits, always 4), then the length of the header in 32-bit words (4 bits, at
  /// least 5).
  #[pun(valid(0x45..=0x4f))]
  pub version_ihl: u8,
  pub type_of_service: u8,
  pub total_length: u16be,
  pub identification: u16be,
  pub flags_fragment_offset: u16be,
  pub time_to_live: u8,
  pub protocol: P::Tag,
  pub checksum: u16be,
  pub source: [u8; 4],
  pub destination: [u8; 4],
  #[pun(regions(options = (version_ihl & 0x0f) * 4 - 20, payload: P))]
  pub data: [u8],
}

/// An IPv6 packet.
///
/// Its extension headers are payloads that have a payload of their own: [`HopByHop`],
/// [`Routing`], [`Authentication`] and [`Fragment`].
#[derive(Pun)]
#[repr(C)]
#[pun(tag(EtherType = u16bec<0x86dd>))]
#[pun(size = payload_length + 40)]
#[pun(assert(version_class_flow >> 28 == 6))]
// The checksum of UDP, TCP and ICMPv6. Its protocol follows the header, or a chain of extension
// headers. Each extension header gives the kind of the next header and its own size, in units of
// 8 bytes after the first 8, or of 4 bytes for authentication. The checksum covers the addresses,
// the protocol, the size of what is carried, and what is carried. The walk stops at a fragment or
// at an encrypted payload, which have no such checksum.
#[pun(assert({
  // The state holds the kind of a header in its high 64 bits and its offset in its low 64 bits. A
  // `u128` is two registers, so each half is read without a shift. The walk stops at the first
  // header that is not an extension header.
  let upper = walk(
    (next_header << 64) + 40,
    |at| {
      let kind = at >> 64;
      kind == 0 || kind == 43 || kind == 60 || kind == 51 || kind == 135 || kind == 139 || kind == 140
    },
    |at| {
      let start = at & 0xffff_ffff_ffff_ffff;
      let size = 8 + be(start + 1, 1) * if at >> 64 == 51 { 4 } else { 8 };
      (be(start, 1) << 64) + start + size
    },
  );
  let protocol = upper >> 64;
  let start = upper & 0xffff_ffff_ffff_ffff;
  let end = 40 + payload_length;
  (protocol != 17 && protocol != 6 && protocol != 58)
    || (sum16(8, 40) + protocol + (end - start) + sum16(start, end)) % 0xffff == 0
}))]
#[pun(get(
  traffic_class: u8 = (version_class_flow >> 20) & 0xff,
  flow_label: u32 = version_class_flow & 0x000f_ffff,
))]
pub struct Ipv6<P: ?Sized + Tag<Protocol> = [u8]> {
  /// The version (4 bits, always 6), the traffic class (8 bits) and the flow label (20 bits).
  pub version_class_flow: u32be,
  pub payload_length: u16be,
  pub next_header: P::Tag,
  pub hop_limit: u8,
  pub source: [u8; 16],
  pub destination: [u8; 16],
  #[pun(regions(payload: P))]
  pub data: [u8],
}

/// The protocol of what an IP packet carries, when the protocol is not known.
///
/// A payload declares its protocol with `#[pun(tag(Protocol = ..))]`.
#[derive(Pun, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct Protocol(pub u8);

/// The hop-by-hop options of IPv6, and the payload that follows them.
///
/// This extension header has its size in its second byte, in units of 8 bytes, not counting the
/// first 8 bytes.
#[derive(Pun)]
#[repr(C)]
#[pun(tag(Protocol = u8c<0>))]
pub struct HopByHop<P: ?Sized + Tag<Protocol> = [u8]> {
  pub next_header: P::Tag,
  pub length: u8,
  #[pun(regions(options = length * 8 + 6, payload: P))]
  pub data: [u8],
}

/// The routing header of IPv6, and the payload that follows it.
///
/// Its size is given as for [`HopByHop`].
#[derive(Pun)]
#[repr(C)]
#[pun(tag(Protocol = u8c<43>))]
pub struct Routing<P: ?Sized + Tag<Protocol> = [u8]> {
  pub next_header: P::Tag,
  pub length: u8,
  pub kind: u8,
  pub segments_left: u8,
  #[pun(regions(route = length * 8 + 4, payload: P))]
  pub data: [u8],
}

/// The authentication header of IPsec, and the payload that follows it.
///
/// Its size is in units of 4 bytes, not counting the first 8 bytes.
#[derive(Pun)]
#[repr(C)]
#[pun(tag(Protocol = u8c<51>))]
pub struct Authentication<P: ?Sized + Tag<Protocol> = [u8]> {
  pub next_header: P::Tag,
  #[pun(valid(1..))]
  pub length: u8,
  pub reserved: u16be,
  pub security_parameters: u32be,
  pub sequence: u32be,
  #[pun(regions(check_value = length * 4 - 4, payload: P))]
  pub data: [u8],
}

/// The fragment header of IPv6, and the fragment.
#[derive(Pun)]
#[repr(C)]
#[pun(tag(Protocol = u8c<44>))]
#[pun(get(offset: u16 = offset_flags >> 3, more: bool = offset_flags & 1 != 0))]
pub struct Fragment<P: ?Sized + Tag<Protocol> = [u8]> {
  pub next_header: P::Tag,
  pub reserved: u8,
  /// The offset of the fragment in units of 8 bytes (13 bits), 2 reserved bits, and the "more
  /// fragments" flag (1 bit).
  pub offset_flags: u16be,
  pub identification: u32be,
  pub payload: P,
}

/// An ICMP message.
///
/// Its checksum covers the whole message, whatever its size.
#[derive(Pun)]
#[repr(C)]
#[pun(tag(Protocol = u8c<1>))]
#[pun(assert(sum16(0, size_of_val(self)) == 0xffff))]
pub struct Icmp {
  pub kind: u8,
  pub code: u8,
  pub checksum: u16be,
  pub rest: [u8; 4],
  pub data: [u8],
}

/// An ICMPv6 message.
///
/// Its checksum also covers the addresses of the packet that carries it. A rule of [`Ipv6`]
/// checks it with that packet.
#[derive(Pun)]
#[repr(C)]
#[pun(tag(Protocol = u8c<58>))]
pub struct Icmp6 {
  pub kind: u8,
  pub code: u8,
  pub checksum: u16be,
  pub body: [u8],
}

/// An IPv4 option that has a length, which is every option but "end of options" and "no
/// operation".
#[derive(Pun)]
#[repr(C)]
#[pun(size = length)]
pub struct Ipv4Option {
  #[pun(valid(2..))]
  pub kind: u8,
  pub length: u8,
  pub data: [u8],
}

/// A UDP datagram.
#[derive(Pun)]
#[repr(C)]
#[pun(tag(Protocol = u8c<17>))]
#[pun(size = length)]
pub struct Udp {
  pub source_port: u16be,
  pub destination_port: u16be,
  pub length: u16be,
  pub checksum: u16be,
  pub payload: [u8],
}

/// A TCP segment.
///
/// The header holds its own size, options included.
#[derive(Pun)]
#[repr(C)]
#[pun(tag(Protocol = u8c<6>))]
#[pun(assert(offset_flags >> 12 >= 5))]
#[pun(get(
  header_length: usize = (offset_flags >> 12) * 4,
  syn: bool = offset_flags & 0x002 != 0,
  ack: bool = offset_flags & 0x010 != 0,
))]
pub struct Tcp {
  pub source_port: u16be,
  pub destination_port: u16be,
  pub sequence: u32be,
  pub acknowledgment: u32be,
  /// The size of the header in 32-bit words (4 bits), 3 reserved bits, and the flags (9 bits).
  pub offset_flags: u16be,
  pub window: u16be,
  pub checksum: u16be,
  pub urgent: u16be,
  #[pun(regions(options = (offset_flags >> 12) * 4 - 20, payload))]
  pub data: [u8],
}

// -------------------------------------------------------------------------------------------------
// Other EtherTypes

/// A MAC control frame that asks its peer to pause.
///
/// It holds an operation and a duration, then 42 zero bytes.
#[derive(Pun)]
#[repr(C)]
#[pun(tag(EtherType = u16bec<0x8808>))]
pub struct Pause {
  pub operation: u16bec<1>,
  pub quanta: u16be,
  pub padding: [u8c<0>; 42],
}

/// An MPLS label stack entry, and what follows it.
///
/// Another entry follows, unless this entry is the bottom of the stack.
#[derive(Pun)]
#[repr(C)]
#[pun(tag(EtherType = u16bec<0x8847>))]
#[pun(get(label: u32 = entry >> 12, bottom: bool = entry & 0x100 != 0, time_to_live: u8 = entry & 0xff))]
pub struct Mpls {
  /// The label (20 bits), the traffic class (3 bits), the bottom of stack flag (1 bit) and the
  /// time to live (8 bits).
  pub entry: u32be,
  pub payload: [u8],
}

/// A PPPoE session frame.
///
/// Its header holds the size of what it carries.
#[derive(Pun)]
#[repr(C)]
#[pun(tag(EtherType = u16bec<0x8864>))]
#[pun(size = length + 6)]
pub struct Pppoe {
  /// The version (4 bits, 1) and the type (4 bits, 1).
  pub version_type: u8c<0x11>,
  pub code: u8c<0>,
  pub session: u16be,
  pub length: u16be,
  pub payload: [u8],
}

/// An IEEE 802.1X frame (EAP over LAN).
#[derive(Pun)]
#[repr(C)]
#[pun(tag(EtherType = u16bec<0x888e>))]
#[pun(size = length + 4)]
pub struct Eapol {
  #[pun(valid(1..=3))]
  pub version: u8,
  pub kind: u8,
  pub length: u16be,
  pub body: [u8],
}

/// An LLDP data unit, a sequence of [`Tlv`]s.
#[derive(Pun)]
#[repr(transparent)]
#[pun(tag(EtherType = u16bec<0x88cc>))]
pub struct Lldp(pub [u8]);

/// An LLDP element: a type (7 bits), the length of a value (9 bits), and that value.
#[derive(Pun)]
#[repr(C)]
#[pun(size = (type_length & 0x01ff) + 2)]
#[pun(get(kind: u8 = type_length >> 9))]
pub struct Tlv {
  pub type_length: u16be,
  pub value: [u8],
}

// -------------------------------------------------------------------------------------------------
// One of several kinds

/// A frame, by what it carries.
///
/// Bytes reinterpreted as an `AnyFrame` are the first variant whose type they are.
#[derive(Pun)]
pub enum AnyFrame<'a> {
  Udp(&'a Frame<Ipv4<Udp>>),
  Tcp(&'a Frame<Ipv4<Tcp>>),
  Icmp(&'a Frame<Ipv4<Icmp>>),
  Ipv4(&'a Frame<Ipv4>),
  Udp6(&'a Frame<Ipv6<Udp>>),
  Icmp6(&'a Frame<Ipv6<Icmp6>>),
  Options6(&'a Frame<Ipv6<HopByHop>>),
  Fragment6(&'a Frame<Ipv6<Fragment>>),
  Ipv6(&'a Frame<Ipv6>),
  Arp(&'a Frame<Arp>),
  Pause(&'a Frame<Pause>),
  Lldp(&'a Frame<Lldp>),
  Mpls(&'a Frame<Mpls>),
  Pppoe(&'a Frame<Pppoe>),
  Eapol(&'a Frame<Eapol>),
  Tagged(&'a Frame<Vlan>),
  ServiceTagged(&'a Frame<ServiceVlan>),
  Length(&'a Frame8023),
  Other(&'a Frame),
}

// -------------------------------------------------------------------------------------------------
// Use

const A: Mac = Mac(u48be::new(0x0200_0000_000a));
const B: Mac = Mac(u48be::new(0x0200_0000_000b));

/// The bytes of an ARP request.
const ARP: [u8; 28] =
  [0, 1, 0x08, 0, 6, 4, 0, 1, 0x02, 0, 0, 0, 0, 0x0a, 10, 0, 0, 1, 0, 0, 0, 0, 0, 0, 10, 0, 0, 2];

/// The bytes of an IPv4 packet with three options and a UDP datagram.
///
/// The first option has a length, and the other two do not.
const UDP: [u8; 41] = [
  0x47, 0x00, 0x00, 0x29, 0x00, 0x01, 0x40, 0x00, 0x40, 0x11, 0x18, 0xb0, 0x0a, 0x00, 0x00, 0x01,
  0x0a, 0x00, 0x00, 0x02, 0x07, 0x06, 0x04, 0x0a, 0x00, 0x00, 0x01, 0x01, 0x30, 0x39, 0x00, 0x35,
  0x00, 0x0d, 0x77, 0x91, 0x68, 0x65, 0x6c, 0x6c, 0x6f,
];

/// The bytes of an IPv4 packet that carries a TCP segment with an option.
const TCP: [u8; 46] = [
  0x45, 0x00, 0x00, 0x2e, 0x00, 0x02, 0x40, 0x00, 0x40, 0x06, 0x26, 0xc6, 0x0a, 0x00, 0x00, 0x01,
  0x0a, 0x00, 0x00, 0x02, 0x30, 0x39, 0x00, 0x50, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
  0x60, 0x02, 0x10, 0x00, 0xdb, 0x2e, 0x00, 0x00, 0x02, 0x04, 0x05, 0xb4, 0x68, 0x69,
];

/// The bytes of an IPv4 packet whose protocol is not known.
const PING: [u8; 24] = [
  0x45, 0, 0, 24, 0, 1, 0, 0, 64, 253, 0x65, 0xe6, 10, 0, 0, 1, 10, 0, 0, 2, b'p', b'i', b'n', b'g',
];

/// The bytes of an IPv6 packet that carries a UDP datagram.
const UDP6: [u8; 53] = [
  0x60, 0x01, 0x23, 0x45, 0x00, 0x0d, 0x11, 0x40, 0x20, 0x01, 0x0d, 0xb8, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x20, 0x01, 0x0d, 0xb8, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x30, 0x39, 0x00, 0x35, 0x00, 0x0d, 0x30, 0x1f,
  0x68, 0x65, 0x6c, 0x6c, 0x6f,
];

/// The bytes of an IPv4 packet that carries an ICMP echo request of an odd size.
const ECHO: [u8; 33] = [
  0x45, 0x00, 0x00, 0x21, 0x00, 0x03, 0x00, 0x00, 0x40, 0x01, 0x66, 0xd7, 0x0a, 0x00, 0x00, 0x01,
  0x0a, 0x00, 0x00, 0x02, 0x08, 0x00, 0xe5, 0xf9, 0x12, 0x34, 0x00, 0x01, 0x70, 0x69, 0x6e, 0x67,
  0x21,
];

/// The bytes of an IPv6 packet with hop-by-hop options (a `PadN` option of 6 bytes), then a UDP
/// datagram.
const OPTIONS6: [u8; 61] = [
  0x60, 0x00, 0x00, 0x00, 0x00, 0x15, 0x00, 0x40, 0x20, 0x01, 0x0d, 0xb8, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x20, 0x01, 0x0d, 0xb8, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x11, 0x00, 0x01, 0x04, 0x00, 0x00, 0x00, 0x00,
  0x30, 0x39, 0x00, 0x35, 0x00, 0x0d, 0x30, 0x1f, 0x68, 0x65, 0x6c, 0x6c, 0x6f,
];

/// The bytes of an IPv6 packet with hop-by-hop options, a routing header and an authentication
/// header, then a TCP segment.
const CHAIN6: [u8; 102] = [
  0x60, 0x00, 0x00, 0x00, 0x00, 0x3e, 0x00, 0x40, 0x20, 0x01, 0x0d, 0xb8, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x20, 0x01, 0x0d, 0xb8, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x2b, 0x00, 0x01, 0x04, 0x00, 0x00, 0x00, 0x00,
  0x33, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x04, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00,
  0x00, 0x00, 0x00, 0x01, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c,
  0x30, 0x39, 0x00, 0x50, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x50, 0x02, 0x10, 0x00,
  0xab, 0x78, 0x00, 0x00, 0x68, 0x69,
];

/// The bytes of an IPv6 packet that carries an ICMPv6 echo request.
const ECHO6: [u8; 52] = [
  0x60, 0x00, 0x00, 0x00, 0x00, 0x0c, 0x3a, 0x40, 0x20, 0x01, 0x0d, 0xb8, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x20, 0x01, 0x0d, 0xb8, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x80, 0x00, 0x33, 0x3e, 0x12, 0x34, 0x00, 0x01,
  0x70, 0x69, 0x6e, 0x67,
];

/// The bytes of an IPv6 packet that carries the first fragment of a UDP datagram.
const FRAGMENT6: [u8; 56] = [
  0x60, 0x00, 0x00, 0x00, 0x00, 0x10, 0x2c, 0x40, 0x20, 0x01, 0x0d, 0xb8, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x20, 0x01, 0x0d, 0xb8, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x11, 0x00, 0x00, 0x01, 0xde, 0xad, 0xbe, 0xef,
  0x66, 0x72, 0x61, 0x67, 0x6d, 0x65, 0x6e, 0x74,
];

/// The bytes of an LLDP data unit: a chassis, a port, a time to live and the end.
const LLDP: [u8; 19] =
  [0x02, 0x07, 4, 0x02, 0, 0, 0, 0, 0x0a, 0x04, 0x02, 5, b'1', 0x06, 0x02, 0, 120, 0x00, 0x00];

/// Returns the bytes of a frame from `A` to `B`, with `ether_type`, then `payload`.
///
/// `ether_type` is an EtherType or, up to 1500, the length of the payload.
fn frame(ether_type: u16, payload: &[u8]) -> Vec<u8> {
  [&B.0.to_bytes()[..], &A.0.to_bytes()[..], &ether_type.to_be_bytes()[..], payload].concat()
}

/// Reads the samples of this file through its types, and asserts the results.
pub fn main() {
  // ---- A frame, of any payload, then of the payload that its EtherType names.
  let bytes = frame(0x0800, &UDP);
  let any: &Frame = try_pun_ref(&bytes[..]).unwrap();
  assert_eq!((any.destination, any.source, any.ether_type.0.get()), (B, A, 0x0800));
  assert_eq!((A.local(), A.multicast(), A.broadcast()), (true, false, false));
  assert!(Mac(u48be::MAX).broadcast() && Mac(u48be::MAX).multicast());
  assert_eq!(any.payload.len(), 41);

  let ip: &Frame<Ipv4> = try_pun_ref(&bytes[..]).unwrap();
  assert_eq!(ip.payload.total_length.get(), 41);
  assert_eq!(ip.payload.protocol, Protocol(17));
  // Bit fields.
  assert_eq!((ip.payload.version(), ip.payload.header_length()), (4, 28));
  assert_eq!((ip.payload.dont_fragment(), ip.payload.more_fragments()), (true, false));
  assert_eq!(ip.payload.fragment_offset(), 0);
  // The regions of the packet: their sizes come from the header.
  assert_eq!(ip.payload.options(), [7, 6, 4, 10, 0, 0, 1, 1]);
  assert_eq!(ip.payload.payload(), Ok(&UDP[28..]));
  // What the packet carries, by its type: the protocol is then checked with the frame.
  let datagram: &Frame<Ipv4<Udp>> = try_pun_ref(&bytes[..]).unwrap();
  let udp = datagram.payload.payload().unwrap();
  assert_eq!((udp.destination_port.get(), &udp.payload), (53, &b"hello"[..]));
  assert_eq!(try_pun_ref::<_, Frame<Ipv4<Tcp>>>(&bytes[..]).err(), Some(Error::Validity));
  // And back to any packet, proven: the rules of `Ipv4<Udp>` are those of `Ipv4`, over the same
  // bytes, and what the protocol is known to be is a `Protocol`. Nothing is checked.
  let any: &Frame<Ipv4> = pun_ref(datagram);
  assert_eq!(any.payload.options(), [7, 6, 4, 10, 0, 0, 1, 1]);
  // The options: one that has a length, then two that are a single byte.
  let (option, rest) = from_head::<Ipv4Option>(ip.payload.options()).unwrap();
  assert_eq!((option.kind, option.length, &option.data), (7, 6, &[4, 10, 0, 0][..]));
  assert_eq!(from_head::<[u8c<1>; 2]>(rest).unwrap().1.len(), 0);
  assert_eq!(from_head::<Ipv4Option>(rest).err(), Some(Error::Validity));

  // ---- It is not something else.
  assert_eq!(try_pun_ref::<_, Frame<Arp>>(&bytes[..]).err(), Some(Error::Size));
  assert_eq!(try_pun_ref::<_, Frame<Vlan>>(&bytes[..]).err(), Some(Error::Validity));
  // Not an EtherType.
  assert_eq!(try_pun_ref::<_, Frame>(&frame(100, &[0; 100])[..]).err(), Some(Error::Validity));
  // Version 6, a header of 16 bytes, a total length that is not the one of the packet, and a
  // time to live that the checksum does not agree with.
  for (at, value) in [(14, 0x67), (14, 0x44), (17, 40), (22, 63)] {
    let mut bytes = bytes.clone();
    bytes[at] = value;
    assert!(try_pun_ref::<_, Frame>(&bytes[..]).is_ok());
    assert_eq!(try_pun_ref::<_, Frame<Ipv4>>(&bytes[..]).err(), Some(Error::Validity));
  }
  // The checksum of UDP covers the addresses of the packet and what the datagram carries.
  for at in [14 + 12, 14 + 28 + 8] {
    let mut bytes = bytes.clone();
    bytes[at] ^= 1;
    if at == 14 + 12 {
      // An address changed: `0x0a00` is now `0x0b00`, so the checksum of the header changes too.
      bytes[14 + 10] = 0x17;
    }
    assert_eq!(try_pun_ref::<_, Frame<Ipv4>>(&bytes[..]).err(), Some(Error::Validity));
    // A UDP checksum of 0 is no checksum.
    bytes[14 + 28 + 6..][..2].copy_from_slice(&[0, 0]);
    assert!(try_pun_ref::<_, Frame<Ipv4>>(&bytes[..]).is_ok());
  }
  // TCP: the same checksum, a header whose size is in the header, options.
  let segment = frame(0x0800, &TCP);
  let ip: &Frame<Ipv4> = try_pun_ref(&segment[..]).unwrap();
  assert_eq!(ip.payload.protocol, Protocol(6));
  let ip: &Frame<Ipv4<Tcp>> = try_pun_ref(&segment[..]).unwrap();
  let tcp = ip.payload.payload().unwrap();
  assert_eq!((tcp.header_length(), tcp.syn(), tcp.ack()), (24, true, false));
  assert_eq!(
    (tcp.destination_port.get(), tcp.options(), tcp.payload()),
    (80, &[2, 4, 5, 0xb4][..], &b"hi"[..])
  );
  let mut corrupted = segment.clone();
  corrupted[14 + 45] ^= 1;
  assert_eq!(try_pun_ref::<_, Frame<Ipv4>>(&corrupted[..]).err(), Some(Error::Validity));
  // Options that are longer than the packet.
  let mut short = frame(0x0800, &UDP[..24]);
  short[17] = 24;
  assert_eq!(try_pun_ref::<_, Frame<Ipv4>>(&short[..]).err(), Some(Error::Validity));
  // Too short for the header.
  assert_eq!(try_pun_ref::<_, Frame<Ipv4>>(&bytes[..20]).err(), Some(Error::Size));

  // ---- On the wire, a small frame is padded to 60 bytes: the packet says where it ends.
  let mut wire = bytes.clone();
  wire.resize(60, 0);
  assert!(try_pun_ref::<_, WireFrame>(&wire[..]).is_ok());
  assert_eq!(try_pun_ref::<_, WireFrame>(&bytes[..]).err(), Some(Error::Validity));
  assert_eq!(try_pun_ref::<_, Frame<Ipv4>>(&wire[..]).err(), Some(Error::Validity));
  let (ip, padding) = from_head::<Frame<Ipv4>>(&wire).unwrap();
  assert_eq!((size_of_val(ip), padding), (55, &[0; 5][..]));
  assert!(try_pun_ref::<_, WireFrame>(&[0xff; 1515][..]).is_err());

  // ---- The frame check sequence, when it is there: the CRC-32 of the frame.
  wire.extend(puns::crc32(&wire).to_le_bytes());
  let checked: &CheckedFrame = try_pun_ref(&wire[..]).unwrap();
  assert_eq!((checked.payload().len(), checked.fcs().len()), (46, 4));
  // The usual check: the CRC of a frame followed by its frame check sequence is a constant.
  assert_eq!(puns::crc32(&wire), 0x2144_df1c);
  for at in [0, 30, 59, 60, 63] {
    let mut wire = wire.clone();
    wire[at] ^= 1;
    assert_eq!(try_pun_ref::<_, CheckedFrame>(&wire[..]).err(), Some(Error::Validity));
  }
  assert!(try_pun_ref::<_, CheckedFrame>(&wire[..63]).is_err());
  // On the medium, after its preamble.
  let mut medium = [&[0x55; 7][..], &[0xd5], &wire].concat();
  let packet: &Packet = try_pun_ref(&medium[..]).unwrap();
  assert_eq!((packet.frame.source, packet.frame.payload().len()), (A, 46));
  medium[7] = 0x55;
  assert_eq!(try_pun_ref::<_, Packet>(&medium[..]).err(), Some(Error::Validity));

  // ---- 802.1Q: a tag, then a frame; twice for "Q-in-Q".
  let tagged = frame(0x8100, &[&[0xb0, 0x2a, 0x08, 0x00][..], &UDP].concat());
  let vlan: &Frame<Vlan<Ipv4>> = try_pun_ref(&tagged[..]).unwrap();
  let tag = &vlan.payload;
  assert_eq!((tag.priority(), tag.drop_eligible(), tag.id()), (5, true, 42));
  assert_eq!(tag.payload.time_to_live, 64);
  assert_eq!(try_pun_ref::<_, Frame<Ipv4>>(&tagged[..]).err(), Some(Error::Validity));
  let reserved = frame(0x8100, &[&[0x2f, 0xff, 0x08, 0x00][..], &UDP].concat());
  assert_eq!(try_pun_ref::<_, Frame<Vlan<Ipv4>>>(&reserved[..]).err(), Some(Error::Validity));
  let double = frame(0x8100, &[&[0, 1, 0x81, 0x00, 0, 2, 0x08, 0x06][..], &ARP[..]].concat());
  let qinq: &Frame<Vlan<Vlan<Arp>>> = try_pun_ref(&double[..]).unwrap();
  assert_eq!((qinq.payload.id(), qinq.payload.payload.id()), (1, 2));
  assert!(try_pun_ref::<_, Frame<Vlan<Vlan<Ipv4>>>>(&double[..]).is_err());
  // With a service tag (802.1ad) as the outer one.
  let service = frame(0x88a8, &[&[0, 1, 0x81, 0x00, 0, 2, 0x08, 0x06][..], &ARP[..]].concat());
  let qinq: &Frame<ServiceVlan<Vlan<Arp>>> = try_pun_ref(&service[..]).unwrap();
  assert_eq!((qinq.payload.id(), qinq.payload.payload.id()), (1, 2));
  assert!(try_pun_ref::<_, Frame<Vlan<Vlan<Arp>>>>(&service[..]).is_err());

  // ---- 802.3: a length instead of an EtherType, and padding after the payload.
  let llc = frame(3, &[0xaa, 0xaa, 0x03, 0, 0, 0, 0]);
  let frame8023: &Frame8023 = try_pun_ref(&llc[..]).unwrap();
  assert_eq!((frame8023.payload(), frame8023.padding()), (&[0xaa, 0xaa, 0x03][..], &[0; 4][..]));
  assert!(try_pun_ref::<_, Frame>(&llc[..]).is_err());
  assert!(try_pun_ref::<_, Frame8023>(&frame(9, &[0; 7])[..]).is_err());
  assert!(try_pun_ref::<_, Frame8023>(&bytes[..]).is_err());
  // LLC and SNAP headers, then what the EtherType of the SNAP header names.
  let snap =
    frame(8 + 28, &[&[0xaa, 0xaa, 0x03, 0, 0, 0, 0x08, 0x06][..], &ARP[..], &[0; 10]].concat());
  let frame8023: &Frame8023 = try_pun_ref(&snap[..]).unwrap();
  let (arp, rest) = from_head::<Snap<Arp>>(frame8023.payload()).unwrap();
  assert_eq!(
    (arp.payload.operation, rest.len(), frame8023.padding().len()),
    (Operation::Request, 0, 10)
  );
  assert!(from_head::<Snap<Ipv4>>(frame8023.payload()).is_err());

  // ---- ARP: a payload of a fixed size, made in safe Rust.
  let request = Frame {
    destination: Mac(u48be::MAX),
    source: A,
    ether_type: u16bec::default(),
    payload: Arp {
      hardware_type: u16bec::default(),
      protocol_type: u16bec::default(),
      hardware_length: u8c::default(),
      protocol_length: u8c::default(),
      operation: Operation::Request,
      sender: (A, [10, 0, 0, 1]),
      target: (Mac(u48be::new(0)), [10, 0, 0, 2]),
    },
  };
  // Nothing to check to see it as bytes, or as a frame of any payload: this is all proven.
  let bytes: &[u8; 42] = pun_ref(&request);
  assert_eq!(bytes[12..14], [0x08, 0x06]);
  assert_eq!(bytes[14..], ARP);
  let any: &Frame = pun_ref(&request);
  assert_eq!((any.ether_type.0.get(), any.payload.len()), (0x0806, 28));
  // And back: only what the bytes do not prove is checked.
  let arp: &Frame<Arp> = try_pun_ref(bytes).unwrap();
  assert_eq!(arp.payload, request.payload);
  let mut reply = *bytes;
  reply[21] = 2;
  // (A field of a packed struct is read by value.)
  let operation = try_pun_ref::<_, Frame<Arp>>(&reply).unwrap().payload.operation;
  assert_eq!(operation, Operation::Reply);
  reply[21] = 3;
  assert_eq!(try_pun_ref::<_, Frame<Arp>>(&reply).err(), Some(Error::Validity));

  // ---- A protocol that is not known: nothing is read where UDP or TCP have their checksum,
  // which is outside this packet.
  let ping = frame(0x0800, &PING);
  let ip: &Frame<Ipv4> = try_pun_ref(&ping[..]).unwrap();
  assert_eq!((ip.payload.protocol, ip.payload.payload()), (Protocol(253), Ok(&b"ping"[..])));
  assert!(try_pun_ref::<_, Frame<Ipv4<Udp>>>(&ping[..]).is_err());

  // ---- IPv6, and the same UDP over it.
  let datagram6 = frame(0x86dd, &UDP6);
  let ip6: &Frame<Ipv6<Udp>> = try_pun_ref(&datagram6[..]).unwrap();
  assert_eq!((ip6.payload.traffic_class(), ip6.payload.flow_label()), (0, 0x12345));
  assert_eq!(ip6.payload.payload().unwrap().payload, *b"hello");
  let mut corrupted = datagram6.clone();
  corrupted[14 + 52] ^= 1;
  assert!(try_pun_ref::<_, Frame<Ipv6>>(&corrupted[..]).is_err());
  assert!(try_pun_ref::<_, Frame<Ipv4>>(&datagram6[..]).is_err());

  // ---- ICMP: a checksum over the whole message, of any size.
  let echo = frame(0x0800, &ECHO);
  let ip: &Frame<Ipv4<Icmp>> = try_pun_ref(&echo[..]).unwrap();
  let icmp = ip.payload.payload().unwrap();
  assert_eq!((icmp.kind, icmp.code, &icmp.data), (8, 0, &b"ping!"[..]));
  let mut corrupted_echo = echo.clone();
  corrupted_echo[14 + 32] ^= 1;
  let ip: &Frame<Ipv4<Icmp>> = try_pun_ref(&corrupted_echo[..]).unwrap();
  assert_eq!(ip.payload.payload().err(), Some(Error::Validity));

  // ---- IPv6 extension headers: each says what follows it, and how long it is.
  let options6 = frame(0x86dd, &OPTIONS6);
  let ip6: &Frame<Ipv6<HopByHop<Udp>>> = try_pun_ref(&options6[..]).unwrap();
  let extension = ip6.payload.payload().unwrap();
  assert_eq!(extension.options(), [1, 4, 0, 0, 0, 0]);
  assert_eq!(extension.payload().unwrap().payload, *b"hello");
  // It is not a datagram right after the header, nor options followed by a segment.
  assert!(try_pun_ref::<_, Frame<Ipv6<Udp>>>(&options6[..]).is_err());
  let ip6: &Frame<Ipv6<HopByHop<Tcp>>> = try_pun_ref(&options6[..]).unwrap();
  assert_eq!(ip6.payload.payload().err(), Some(Error::Validity));
  // The checksum of the datagram is checked with the packet, behind the options: it covers the
  // addresses of the packet, and what the datagram carries.
  for at in [14 + 8, 14 + 60] {
    let mut corrupted = options6.clone();
    corrupted[at] ^= 1;
    assert_eq!(try_pun_ref::<_, Frame<Ipv6>>(&corrupted[..]).err(), Some(Error::Validity));
    assert!(try_pun_ref::<_, Frame>(&corrupted[..]).is_ok());
  }
  // The options are not covered.
  let mut other = options6.clone();
  other[14 + 44] ^= 1;
  assert!(try_pun_ref::<_, Frame<Ipv6<HopByHop<Udp>>>>(&other[..]).is_ok());
  // Options that are longer than the packet: what they are followed by is not in the packet.
  let mut long = options6.clone();
  long[14 + 41] = 2;
  assert_eq!(try_pun_ref::<_, Frame<Ipv6>>(&long[..]).err(), Some(Error::Validity));
  // A chain of headers, of the two units of size: the segment at its end is checked as well.
  let chain6 = frame(0x86dd, &CHAIN6);
  let ip6: &Frame<Ipv6<HopByHop<Routing<Authentication<Tcp>>>>> = try_pun_ref(&chain6[..]).unwrap();
  let routing = ip6.payload.payload().unwrap().payload().unwrap();
  assert_eq!((routing.kind, routing.segments_left, routing.route()), (0, 0, &[0; 4][..]));
  let authentication = routing.payload().unwrap();
  assert_eq!(authentication.security_parameters.get(), 0x100);
  assert_eq!(authentication.check_value(), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
  let tcp = authentication.payload().unwrap();
  assert_eq!((tcp.destination_port.get(), tcp.syn(), tcp.payload()), (80, true, &b"hi"[..]));
  for at in [14 + 39, 14 + 80, 14 + 101] {
    let mut corrupted = chain6.clone();
    corrupted[at] ^= 1;
    assert_eq!(try_pun_ref::<_, Frame<Ipv6>>(&corrupted[..]).err(), Some(Error::Validity));
  }
  // A header of the chain that gives a size which is not its own: the segment is not where it
  // is looked for, or not in the packet.
  for (at, value) in [(14 + 57, 3), (14 + 57, 200)] {
    let mut corrupted = chain6.clone();
    corrupted[at] = value;
    assert_eq!(try_pun_ref::<_, Frame<Ipv6>>(&corrupted[..]).err(), Some(Error::Validity));
  }
  // ICMPv6: the same checksum.
  let echo6 = frame(0x86dd, &ECHO6);
  let ip6: &Frame<Ipv6<Icmp6>> = try_pun_ref(&echo6[..]).unwrap();
  let icmp6 = ip6.payload.payload().unwrap();
  assert_eq!((icmp6.kind, icmp6.code, &icmp6.body[4..]), (128, 0, &b"ping"[..]));
  for at in [14 + 23, 14 + 51] {
    let mut corrupted = echo6.clone();
    corrupted[at] ^= 1;
    assert_eq!(try_pun_ref::<_, Frame<Ipv6>>(&corrupted[..]).err(), Some(Error::Validity));
  }
  // A fragment is not the whole of what is carried: there is no checksum to check with it.
  let fragment6 = frame(0x86dd, &FRAGMENT6);
  let ip6: &Frame<Ipv6<Fragment>> = try_pun_ref(&fragment6[..]).unwrap();
  let fragment = ip6.payload.payload().unwrap();
  assert_eq!((fragment.next_header, fragment.offset(), fragment.more()), (Protocol(17), 0, true));
  assert_eq!((fragment.identification.get(), &fragment.payload), (0xdead_beef, &b"fragment"[..]));

  // ---- MPLS: a stack of labels, down to its bottom.
  let mpls = frame(0x8847, &[&[0x00, 0x01, 0x00, 0x40, 0x00, 0x02, 0x01, 0x3f][..], &UDP].concat());
  let top = &try_pun_ref::<_, Frame<Mpls>>(&mpls[..]).unwrap().payload;
  assert_eq!((top.label(), top.bottom(), top.time_to_live()), (16, false, 64));
  let (next, rest) = from_head::<Mpls>(&top.payload).unwrap();
  assert_eq!((next.label(), next.bottom(), next.time_to_live(), rest.len()), (32, true, 63, 0));
  assert!(try_pun_ref::<_, Ipv4<Udp>>(&next.payload).is_ok());

  // ---- PPPoE and 802.1X: a length in the header, and the padding of the frame after it.
  let mut pppoe = frame(0x8864, &[0x11, 0, 0, 7, 0, 4, 0xc0, 0x21, 9, 1]);
  pppoe.resize(60, 0);
  assert_eq!(try_pun_ref::<_, Frame<Pppoe>>(&pppoe[..]).err(), Some(Error::Validity));
  let (session, padding) = from_head::<Frame<Pppoe>>(&pppoe).unwrap();
  assert_eq!(
    (session.payload.session.get(), &session.payload.payload),
    (7, &[0xc0, 0x21, 9, 1][..])
  );
  assert_eq!(padding.len(), 36);
  let eapol = frame(0x888e, &[2, 1, 0, 0]);
  let start: &Frame<Eapol> = try_pun_ref(&eapol[..]).unwrap();
  assert_eq!((start.payload.version, start.payload.kind, start.payload.body.len()), (2, 1, 0));
  assert!(try_pun_ref::<_, Frame<Eapol>>(&frame(0x888e, &[4, 1, 0, 0])[..]).is_err());

  // ---- MAC control, and LLDP: values that give their type and their length.
  let pause = frame(0x8808, &[&[0, 1, 0, 100][..], &[0; 42]].concat());
  assert_eq!(try_pun_ref::<_, Frame<Pause>>(&pause[..]).unwrap().payload.quanta.get(), 100);
  let lldp = frame(0x88cc, &LLDP);
  let discovery: &Frame<Lldp> = try_pun_ref(&lldp[..]).unwrap();
  let (mut rest, mut values) = (&discovery.payload.0, vec![]);
  while let Ok((tlv, next)) = from_head::<Tlv>(rest) {
    values.push((tlv.kind(), tlv.value.len()));
    rest = next;
  }
  assert_eq!((values, rest.len()), (vec![(1, 7), (2, 2), (3, 2), (0, 0)], 0));

  // ---- One of several kinds. The declaration of `AnyFrame` alone tells the kinds apart, and what
  // follows uses each kind.
  let describe = |bytes: &[u8]| match AnyFrame::from_bytes(bytes) {
    Ok(AnyFrame::Udp(frame)) => match frame.payload.payload() {
      Ok(udp) => format!("UDP to port {}", udp.destination_port),
      Err(error) => format!("UDP, but {error}"),
    },
    Ok(AnyFrame::Tcp(frame)) => match frame.payload.payload() {
      Ok(tcp) => format!("TCP to port {}", tcp.destination_port),
      Err(error) => format!("TCP, but {error}"),
    },
    Ok(AnyFrame::Icmp(frame)) => match frame.payload.payload() {
      Ok(icmp) => format!("ICMP {}", icmp.kind),
      Err(error) => format!("ICMP, but {error}"),
    },
    Ok(AnyFrame::Ipv4(frame)) => format!("IPv4, protocol {}", frame.payload.protocol.0),
    Ok(AnyFrame::Udp6(frame)) => format!("UDP over IPv6, {} hops left", frame.payload.hop_limit),
    Ok(AnyFrame::Icmp6(frame)) => match frame.payload.payload() {
      Ok(icmp6) => format!("ICMPv6 {}", icmp6.kind),
      Err(error) => format!("ICMPv6, but {error}"),
    },
    Ok(AnyFrame::Options6(frame)) => match frame.payload.payload() {
      Ok(options) => format!("IPv6 options, then protocol {}", options.next_header.0),
      Err(error) => format!("IPv6 options, but {error}"),
    },
    Ok(AnyFrame::Fragment6(frame)) => match frame.payload.payload() {
      Ok(fragment) => format!("IPv6 fragment {:#x}", fragment.identification),
      Err(error) => format!("IPv6 fragment, but {error}"),
    },
    Ok(AnyFrame::Ipv6(frame)) => format!("IPv6, next header {}", frame.payload.next_header.0),
    Ok(AnyFrame::Arp(frame)) => format!("ARP {:?}", { frame.payload.operation }),
    Ok(AnyFrame::Pause(frame)) => format!("pause for {}", frame.payload.quanta),
    Ok(AnyFrame::Lldp(frame)) => format!("LLDP, {} bytes", frame.payload.0.len()),
    Ok(AnyFrame::Mpls(frame)) => format!("MPLS label {}", frame.payload.label()),
    Ok(AnyFrame::Pppoe(frame)) => format!("PPPoE session {}", frame.payload.session),
    Ok(AnyFrame::Eapol(frame)) => format!("802.1X, version {}", frame.payload.version),
    Ok(AnyFrame::Tagged(frame)) => format!("VLAN {}", frame.payload.id()),
    Ok(AnyFrame::ServiceTagged(frame)) => format!("service VLAN {}", frame.payload.id()),
    Ok(AnyFrame::Length(frame)) => format!("802.3, {} bytes", frame.payload().len()),
    Ok(AnyFrame::Other(frame)) => format!("EtherType {:#06x}", frame.ether_type.0),
    Err(error) => format!("not a frame: {error}"),
  };
  assert_eq!(describe(&frame(0x0800, &UDP)), "UDP to port 53");
  assert_eq!(describe(&frame(0x0800, &TCP)), "TCP to port 80");
  assert_eq!(describe(&echo), "ICMP 8");
  assert_eq!(describe(&corrupted_echo), "ICMP, but value is not valid for the target type");
  assert_eq!(describe(&ping), "IPv4, protocol 253");
  assert_eq!(describe(&datagram6), "UDP over IPv6, 64 hops left");
  assert_eq!(describe(&echo6), "ICMPv6 128");
  assert_eq!(describe(&options6), "IPv6 options, then protocol 17");
  assert_eq!(describe(&long), "EtherType 0x86dd");
  assert_eq!(describe(&chain6), "IPv6 options, then protocol 43");
  assert_eq!(describe(&fragment6), "IPv6 fragment 0xdeadbeef");
  assert_eq!(describe(&mpls), "MPLS label 16");
  assert_eq!(describe(&eapol), "802.1X, version 2");
  assert_eq!(describe(&frame(0x0806, &ARP)), "ARP Request");
  assert_eq!(describe(&pause), "pause for 100");
  assert_eq!(describe(&lldp), "LLDP, 19 bytes");
  assert_eq!(describe(&tagged), "VLAN 42");
  assert_eq!(describe(&service), "service VLAN 1");
  assert_eq!(describe(&snap), "802.3, 36 bytes");
  assert_eq!(describe(&frame(0x88f7, &[0; 46])), "EtherType 0x88f7");
  assert_eq!(
    describe(&frame(1501, &[0; 46])),
    "not a frame: value is not valid for the target type"
  );
  assert_eq!(describe(&[0; 13]), "not a frame: value is not valid for the target type");
  // A frame that does not hold what its EtherType names is still a frame, of that EtherType.
  assert_eq!(describe(&corrupted), "EtherType 0x86dd");
  // Padded, as on the wire: the packet says where it ends.
  let mut padded = frame(0x0800, &UDP);
  padded.resize(60, 0);
  let Ok((AnyFrame::Udp(frame), padding)) = AnyFrame::from_head(&padded) else { panic!() };
  assert_eq!((frame.payload.total_length.get(), padding.len()), (41, 5));
  let Ok((AnyFrame::Pppoe(frame), padding)) = AnyFrame::from_head(&pppoe) else { panic!() };
  assert_eq!((frame.payload.length.get(), padding.len()), (4, 36));

  println!("ok");
}
