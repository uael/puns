//! Benchmarks of the checks that `puns` makes at run time, next to the same checks by hand.
//!
//! ```text
//! cargo bench -p puns --features unstable --bench checks              # every case
//! cargo bench -p puns --features unstable --bench checks -- ipv6      # the cases whose name has `ipv6`
//! cargo bench -p puns --features unstable --bench checks -- '=sum16, 1500 bytes'  # the case of that name only
//! ```
//!
//! Each case is a check made by `puns` and the same check written by hand, as a careful programmer
//! writes it with the same algorithm. The checksums are computed the same way, so the comparison
//! measures everything else, except in the cases of CRC-16, which the hand-written side computes a
//! byte at a time while `puns` computes 16 bytes at a time. Both sides must agree on the input
//! before they are timed. A time is the best of 7 samples of about 20 ms each, in nanoseconds per
//! check. The machine should be idle.
//!
//! With `PUNS_BENCH_RUNS=n`, each chosen case runs `n` times without timing, on the side that
//! `PUNS_BENCH_SIDE` names (`puns`, the default, or `hand`). This gives a count of instructions
//! under `valgrind --tool=callgrind --toggle-collect='*checks*runs*'`.
//!
//! This is not a test, and `cargo test` does not run it. It needs the `unstable` feature, because
//! two of its cases read slices of enums of the `Rust` representation (`[Small]`, `[MaybeFlag]`).
//!
//! # Instructions
//!
//! The tables give the instructions of each check, counted with `valgrind --tool=callgrind` as
//! above, next to the same check written by hand.
//!
//! | Check                                  | `puns` | by hand |
//! |----------------------------------------|-------:|--------:|
//! | IPv4, UDP, 64 bytes                    |    241 |     217 |
//! | IPv4, protocol 253                     |    120 |     106 |
//! | IPv6, TCP behind 3 extension headers   |     75 |     106 |
//! | IPv6, TCP behind 12 extension headers  |    210 |     322 |
//! | IPv6, UDP behind 3, 64 bytes           |    209 |     229 |
//! | IPv6, UDP, 1400 bytes                  |    803 |     798 |
//! | a frame of 6 things, UDP over IPv4     |    267 |     244 |
//!
//! The IPv6 packet packs the state of its walk at bit 64 (see [`Ipv6`]). Behind 12 extension
//! headers, its check is then 210 instructions against 322 for the same check by hand.
//!
//! The cases named `aims` check the pointees of eager aims, which `from_head` checks with the value
//! that holds them: 1 M items, each with a name and tags, wide data whose pointees hold no pointer,
//! then trees of a type that points at itself, wide, small, and a chain of 100,000 nodes, which
//! the check follows past `AIM_PARTS` with a loop. Their sides by hand follow the offsets with a
//! list of what is still to check.
//!
//! The second table is of the checks of the `AnyFrame` view enum (6 variants). The column "a call"
//! is `from_bytes` behind a function that is not inlined, as the check by hand is. Where the bench
//! calls the view directly, the optimizer inlines it, and its checksums stay calls.
//!
//! | `AnyFrame` of the bench (6 variants)   | `puns` | `puns`, a call | by hand (a call) |
//! |----------------------------------------|-------:|---------------:|-----------------:|
//! | UDP over IPv4 (the first variant)      |    283 |            273 |              244 |
//! | TCP over IPv4                          |    302 |            290 |              286 |
//! | UDP over IPv4, a wrong checksum        |    266 |            255 |              246 |
//! | ARP                                    |     25 |             41 |               46 |
//! | VLAN                                   |     25 |             41 |               45 |
//! | another EtherType (the last variant)   |     21 |             36 |               40 |
//!
//! A wrong checksum is computed once, for `Frame<Ipv4>`, the base of the first three variants. The
//! UDP variant is then its protocol alone. The EtherType is one decision, a `cmp` of its 16 bits
//! for each value, then the variant.

use std::{hint::black_box, time::Instant};

#[path = "../tests/support/aims.rs"]
#[expect(dead_code, reason = "of the types of the tests, the bench reads lists and trees")]
mod aims;

use aims::{Near, Tree};
use puns::{
  Pun, Tag, from_bytes, from_head,
  layout::Crc,
  num::{u8c, u16be, u16bec},
  try_pun_ref,
};

// -------------------------------------------------------------------------------------------------
// Declarations

#[derive(Pun, Clone, Copy)]
#[repr(transparent)]
pub struct EtherType(#[pun(valid(0x0600..))] pub u16be);

#[derive(Pun)]
#[repr(C)]
pub struct Frame<P: ?Sized + Tag<EtherType> = [u8]> {
  pub destination: [u8; 6],
  pub source: [u8; 6],
  pub ether_type: P::Tag,
  pub payload: P,
}

#[derive(Pun, Clone, Copy)]
#[repr(transparent)]
pub struct Protocol(pub u8);

/// An IPv4 packet, as in `examples/ethernet.rs`, with the checksum of its header and the
/// checksums of UDP and TCP.
#[derive(Pun)]
#[repr(C)]
#[pun(tag(EtherType = u16bec<0x0800>))]
#[pun(size = total_length)]
#[pun(assert(total_length >= (version_ihl & 0x0f) * 4))]
#[pun(assert(sum16(0, (version_ihl & 0x0f) * 4) == 0xffff))]
#[pun(assert({
  let header = (version_ihl & 0x0f) * 4;
  (protocol != 17 && protocol != 6)
    || flags_fragment_offset & 0x3fff != 0
    || (protocol == 17 && be(header + 6, 2) == 0)
    || (sum16(12, 20) + protocol + (total_length - header) + sum16(header, total_length)) % 0xffff == 0
}))]
pub struct Ipv4<P: ?Sized + Tag<Protocol> = [u8]> {
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

#[derive(Pun)]
#[repr(C)]
#[pun(tag(Protocol = u8c<6>))]
#[pun(assert(offset_flags >> 12 >= 5))]
pub struct Tcp {
  pub source_port: u16be,
  pub destination_port: u16be,
  pub sequence: [u8; 4],
  pub acknowledgment: [u8; 4],
  pub offset_flags: u16be,
  pub window: u16be,
  pub checksum: u16be,
  pub urgent: u16be,
  #[pun(regions(options = (offset_flags >> 12) * 4 - 20, payload))]
  pub data: [u8],
}

#[derive(Pun)]
#[repr(C)]
#[pun(tag(EtherType = u16bec<0x8100>))]
#[pun(assert((tci & 0x0fff) != 0x0fff))]
pub struct Vlan<P: ?Sized + Tag<EtherType> = [u8]> {
  pub tci: u16be,
  pub ether_type: P::Tag,
  pub payload: P,
}

#[derive(Pun)]
#[repr(C)]
#[pun(tag(EtherType = u16bec<0x0806>))]
pub struct Arp {
  pub hardware_type: u16bec<1>,
  pub protocol_type: u16bec<0x0800>,
  pub hardware_length: u8c<6>,
  pub protocol_length: u8c<4>,
  pub operation: u16be,
  pub addresses: [u8; 20],
}

/// A frame, by what it carries. The variant is the first of these types that the bytes are.
#[derive(Pun)]
pub enum AnyFrame<'a> {
  Udp(&'a Frame<Ipv4<Udp>>),
  Tcp(&'a Frame<Ipv4<Tcp>>),
  Ipv4(&'a Frame<Ipv4>),
  Arp(&'a Frame<Arp>),
  Tagged(&'a Frame<Vlan>),
  Other(&'a Frame),
}

/// An IPv6 packet, with the checksum of UDP behind any chain of extension headers, found by a
/// walk.
///
/// The state of the walk holds the kind of a header in its high 64 bits, and where the header
/// starts in its low 64 bits. The two halves are two registers (see the documentation of `walk`
/// in `#[derive(Pun)]`).
#[derive(Pun)]
#[repr(C)]
#[pun(size = payload_length + 40)]
#[pun(assert({
  let upper = walk(
    (next_header << 64) + 40,
    |at| { let kind = at >> 64; kind == 0 || kind == 43 || kind == 60 },
    |at| {
      let start = at & 0xffff_ffff_ffff_ffff;
      (be(start, 1) << 64) + start + 8 + be(start + 1, 1) * 8
    },
  );
  let protocol = upper >> 64;
  let start = upper & 0xffff_ffff_ffff_ffff;
  let end = 40 + payload_length;
  protocol != 17 || (sum16(8, 40) + protocol + (end - start) + sum16(start, end)) % 0xffff == 0
}))]
pub struct Ipv6 {
  pub version_class_flow: [u8; 4],
  pub payload_length: u16be,
  pub next_header: u8,
  pub hop_limit: u8,
  pub source: [u8; 16],
  pub destination: [u8; 16],
  pub data: [u8],
}

/// An enum of the `Rust` representation, with a direct tag followed by what the variant holds.
#[derive(Pun, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Small {
  Byte(u8),
  Flag(bool),
  Word(u16),
}

/// An enum of the `Rust` representation, with a niche in the `bool`.
#[derive(Pun, Clone, Copy, PartialEq, Eq, Debug)]
pub enum MaybeFlag {
  Nothing,
  Flag(bool),
}

#[derive(Pun, Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Kind {
  A = 1,
  B = 2,
  C = 3,
  D = 7,
}

/// Items, each with a name and tags: pointees whose values hold no pointer.
#[derive(Pun)]
#[repr(C)]
pub struct Item {
  pub id: u32,
  pub name: Near<str>,
  pub tags: Near<[u16]>,
}

/// A list of items, the root of an image.
#[derive(Pun)]
#[repr(transparent)]
pub struct Items {
  pub items: Near<[Item]>,
}

static CASTAGNOLI: Crc = Crc::new(32, 0x1edc_6f41, 0xffff_ffff, true, 0xffff_ffff);
static MODBUS: Crc = Crc::new(16, 0x8005, 0xffff, true, 0);
static XMODEM: Crc = Crc::new(16, 0x1021, 0, false, 0);

// -------------------------------------------------------------------------------------------------
// By hand

mod hand {
  /// Returns the big-endian `u16` at `at`.
  ///
  /// # Panics
  ///
  /// Panics if `b` has fewer than `at + 2` bytes. The callers check the length first.
  #[inline(always)]
  pub fn be16(b: &[u8], at: usize) -> u128 {
    u128::from(u16::from_be_bytes([b[at], b[at + 1]]))
  }

  /// Computes what `puns::sum16` computes, with the same algorithm, eight bytes at a time.
  #[inline(always)]
  pub fn sum16(bytes: &[u8]) -> u128 {
    let (blocks, rest) = bytes.as_chunks::<8>();
    let mut wide = 0u64;
    for block in blocks {
      let block = u64::from_ne_bytes(*block);
      wide += (block & 0xffff_ffff) + (block >> 32);
    }
    let mut sum = u64::from(fold(wide));
    if cfg!(target_endian = "little") {
      sum = u64::from((sum as u16).swap_bytes());
    }
    for word in rest.chunks(2) {
      sum += u64::from(word[0]) << 8 | u64::from(*word.get(1).unwrap_or(&0));
    }
    u128::from(fold(sum))
  }

  #[inline(always)]
  const fn fold(mut sum: u64) -> u16 {
    while sum > 0xffff {
      sum = (sum & 0xffff) + (sum >> 16);
    }
    sum as u16
  }

  /// Returns whether `b` is an `Ipv4`, by checking the version, the sizes, the checksum of the
  /// header, and the checksums of UDP and TCP.
  #[inline(always)]
  pub fn ipv4(b: &[u8]) -> bool {
    if b.len() < 20 || !(0x45..=0x4f).contains(&b[0]) || be16(b, 2) != b.len() as u128 {
      return false;
    }
    let header = (usize::from(b[0]) & 0x0f) * 4;
    if b.len() < header || sum16(&b[..header]) != 0xffff {
      return false;
    }
    let protocol = b[9];
    if protocol != 17 && protocol != 6 || be16(b, 6) & 0x3fff != 0 {
      return true;
    }
    if protocol == 17 && (b.len() < header + 8 || be16(b, header + 6) == 0) {
      return b.len() >= header + 8;
    }
    let total = sum16(&b[12..20]) + u128::from(protocol) + (b.len() - header) as u128;
    (total + sum16(&b[header..])).is_multiple_of(0xffff)
  }

  /// Returns whether `b` is an `Ipv6`, by walking its extension headers.
  #[inline(always)]
  pub fn ipv6(b: &[u8]) -> bool {
    if b.len() < 40 || be16(b, 4) + 40 != b.len() as u128 {
      return false;
    }
    let (mut kind, mut start) = (b[6], 40usize);
    let mut steps = 0;
    while matches!(kind, 0 | 43 | 60) {
      if start + 2 > b.len() || steps == (b.len() + 16) * 8 {
        return false;
      }
      (kind, start, steps) = (b[start], start + 8 + usize::from(b[start + 1]) * 8, steps + 1);
    }
    if kind != 17 {
      return true;
    }
    if start > b.len() {
      return false;
    }
    let total = sum16(&b[8..40]) + 17 + (b.len() - start) as u128;
    (total + sum16(&b[start..])).is_multiple_of(0xffff)
  }

  /// Returns the index of the variant of `AnyFrame` that `b` is, or `None` if it is none.
  ///
  /// The payload of an `Ipv4<Udp>` is a region, which is not checked. Only the protocol is.
  #[inline(always)]
  pub fn any_frame(b: &[u8]) -> Option<usize> {
    if b.len() < 14 {
      return None;
    }
    let (ether_type, payload) = (be16(b, 12), &b[14..]);
    if ether_type == 0x0800 && ipv4(payload) {
      return Some(match payload[9] {
        17 => 0,
        6 => 1,
        _ => 2,
      });
    }
    if ether_type == 0x0806 && payload.len() == 28 && payload[..6] == [0, 1, 8, 0, 6, 4] {
      return Some(3);
    }
    if ether_type == 0x8100
      && payload.len() >= 4
      && be16(payload, 0) & 0x0fff != 0x0fff
      && be16(payload, 2) >= 0x0600
    {
      return Some(4);
    }
    (ether_type >= 0x0600).then_some(5)
  }

  /// Returns where the value of the `Thin<T, i32>` that the `Near` at `at` points at starts, and
  /// its length, if the offset is not 0, the length is in `b`, at an address aligned to 4, not
  /// negative, and `len` elements of `size` bytes follow it in `b`.
  #[inline(always)]
  fn near(b: &[u8], at: usize, size: usize) -> Option<(usize, usize)> {
    let word =
      |at: usize| Some(i32::from_ne_bytes(b.get(at..at.checked_add(4)?)?.try_into().ok()?));
    let offset = word(at)?;
    let to = at.checked_add_signed(isize::try_from(offset).ok()?).filter(|_| offset != 0)?;
    if !(b.as_ptr().addr() + to).is_multiple_of(4) {
      return None;
    }
    let len = usize::try_from(word(to)?).ok()?;
    let value = to + 4;
    (value.checked_add(len.checked_mul(size)?)? <= b.len()).then_some((value, len))
  }

  /// Returns whether `b` starts with a list of items, each with a name of UTF-8 and tags.
  pub fn items(b: &[u8]) -> bool {
    let Some((list, n)) = near(b, 0, 12) else { return false };
    (0..n).all(|i| {
      let at = list + 12 * i;
      near(b, at + 4, 1).is_some_and(|(text, len)| str::from_utf8(&b[text..text + len]).is_ok())
        && near(b, at + 8, 2).is_some()
    })
  }

  /// Returns whether `b` starts with a tree, each node with a list of children, depth first with
  /// a list of the lists still to check.
  pub fn tree(b: &[u8]) -> bool {
    let mut lists = vec![0];
    while let Some(at) = lists.pop() {
      let Some((kids, n)) = near(b, at + 4, 8) else { return false };
      lists.extend((0..n).map(|i| kids + 8 * i));
    }
    true
  }

  /// Returns the table of the CRC of each byte, for the reflected polynomial `poly`.
  const fn table(poly: u32) -> [u32; 256] {
    let (mut table, mut i) = ([0u32; 256], 0);
    while i < 256 {
      let (mut value, mut bit) = (i as u32, 0);
      while bit < 8 {
        value = if value & 1 != 0 { (value >> 1) ^ poly } else { value >> 1 };
        bit += 1;
      }
      table[i] = value;
      i += 1;
    }
    table
  }

  /// Computes a reflected CRC-32 of the reversed polynomial `POLY`, a byte at a time with a table.
  pub fn crc32_bytewise<const POLY: u32>(bytes: &[u8]) -> u32 {
    let table = const { &table(POLY) };
    let mut crc = !0u32;
    for &byte in bytes {
      crc = table[((crc ^ u32::from(byte)) & 0xff) as usize] ^ (crc >> 8);
    }
    !crc
  }

  /// Computes a reflected CRC-32, 16 bytes at a time, by slicing by 16 with 16 KiB of tables.
  pub fn crc32_slice16<const POLY: u32>(bytes: &[u8]) -> u32 {
    let tables = const {
      &{
        let mut tables = [[0u32; 256]; 16];
        tables[0] = table(POLY);
        let mut k = 1;
        while k < 16 {
          let mut i = 0;
          while i < 256 {
            let previous = tables[k - 1][i];
            tables[k][i] = (previous >> 8) ^ tables[0][(previous & 0xff) as usize];
            i += 1;
          }
          k += 1;
        }
        tables
      }
    };
    let (blocks, rest) = bytes.as_chunks::<16>();
    let mut crc = !0u32;
    for block in blocks {
      let low = crc ^ u32::from_le_bytes([block[0], block[1], block[2], block[3]]);
      let mut next = 0;
      for (i, byte) in low.to_le_bytes().iter().chain(&block[4..]).enumerate() {
        next ^= tables[15 - i][usize::from(*byte)];
      }
      crc = next;
    }
    for &byte in rest {
      crc = tables[0][((crc ^ u32::from(byte)) & 0xff) as usize] ^ (crc >> 8);
    }
    !crc
  }

  /// Computes CRC-16/MODBUS, which is reflected, a byte at a time with a table.
  pub fn modbus(bytes: &[u8]) -> u16 {
    let table = const { &table(0xa001) };
    let mut crc = 0xffffu32;
    for &byte in bytes {
      crc = table[((crc ^ u32::from(byte)) & 0xff) as usize] ^ (crc >> 8);
    }
    crc as u16
  }

  /// Computes CRC-16/XMODEM, which is not reflected, a byte at a time with a table.
  pub fn xmodem(bytes: &[u8]) -> u16 {
    let table = const {
      &{
        let (mut table, mut i) = ([0u16; 256], 0);
        while i < 256 {
          let (mut value, mut bit) = ((i as u16) << 8, 0);
          while bit < 8 {
            value = if value & 0x8000 != 0 { (value << 1) ^ 0x1021 } else { value << 1 };
            bit += 1;
          }
          table[i] = value;
          i += 1;
        }
        table
      }
    };
    let mut crc = 0u16;
    for &byte in bytes {
      crc = table[usize::from((crc >> 8) as u8 ^ byte)] ^ (crc << 8);
    }
    crc
  }
}

// -------------------------------------------------------------------------------------------------
// Inputs

/// Returns `len` pseudo-random bytes, the same on every run.
fn noise(len: usize) -> Vec<u8> {
  let mut seed = 0x9e37_79b9_7f4a_7c15u64;
  (0..len)
    .map(|_| {
      seed ^= seed << 13;
      seed ^= seed >> 7;
      seed ^= seed << 17;
      (seed >> 24) as u8
    })
    .collect()
}

/// Returns the ones' complement of `sum` folded to 16 bits, as a checksum field holds it.
const fn checksum(sum: u128) -> [u8; 2] {
  let mut sum = sum;
  while sum > 0xffff {
    sum = (sum & 0xffff) + (sum >> 16);
  }
  (!(sum as u16)).to_be_bytes()
}

/// Returns an IPv4 packet of `protocol` that carries `payload` bytes, with correct checksums.
fn ipv4_packet(protocol: u8, payload: usize) -> Vec<u8> {
  let mut upper = match protocol {
    17 => [0x30, 0x39, 0x00, 0x35, 0, 0, 0, 0].to_vec(),
    6 => [0x30, 0x39, 0, 80, 0, 0, 0, 1, 0, 0, 0, 0, 0x50, 0x10, 0xff, 0xff, 0, 0, 0, 0].to_vec(),
    _ => vec![],
  };
  upper.extend(noise(payload));
  if protocol == 17 {
    let length = (upper.len() as u16).to_be_bytes();
    upper[4..6].copy_from_slice(&length);
  }
  let mut packet = vec![0x45, 0, 0, 0, 0, 1, 0x40, 0, 64, protocol, 0, 0, 10, 0, 0, 1, 10, 0, 0, 2];
  packet[2..4].copy_from_slice(&((20 + upper.len()) as u16).to_be_bytes());
  let sum = checksum(hand::sum16(&packet));
  packet[10..12].copy_from_slice(&sum);
  if matches!(protocol, 6 | 17) {
    let at = if protocol == 17 { 6 } else { 16 };
    let sum = hand::sum16(&packet[12..20])
      + u128::from(protocol)
      + upper.len() as u128
      + hand::sum16(&upper);
    upper[at..at + 2].copy_from_slice(&checksum(sum));
  }
  packet.extend(upper);
  packet
}

/// Returns an IPv6 packet that carries `protocol` behind `headers` extension headers. For UDP,
/// the checksum does not hold, but the check computes it all the same.
fn ipv6_packet(headers: usize, protocol: u8, payload: usize) -> Vec<u8> {
  let mut p = vec![0x60, 0, 0, 0];
  p.extend(((headers * 8 + payload) as u16).to_be_bytes());
  p.push(if headers == 0 { protocol } else { 0 });
  p.push(64);
  p.extend([0x20; 32]);
  for i in 0..headers {
    p.push(if i + 1 == headers { protocol } else { [43, 60, 0][i % 3] });
    p.extend([0; 7]);
  }
  p.extend(noise(payload));
  p
}

/// Returns an Ethernet frame of `ether_type` around `payload`.
fn frame(ether_type: u16, payload: &[u8]) -> Vec<u8> {
  let mut frame = vec![2, 0, 0, 0, 0, 0xa, 2, 0, 0, 0, 0, 0xb];
  frame.extend(ether_type.to_be_bytes());
  frame.extend(payload);
  frame
}

/// An image under construction, as a writer of pointers makes it: each value at the next place
/// aligned for it, depth first, each value before its pointees.
struct Image(Vec<u8>);

impl Image {
  /// Reserves `size` bytes aligned to 4, and returns where they are.
  fn reserve(&mut self, size: usize) -> usize {
    let at = self.0.len().next_multiple_of(4);
    self.0.resize(at + size, 0);
    at
  }

  /// Writes the `i32` `value` at `at`.
  fn put(&mut self, at: usize, value: i32) {
    self.0[at..at + 4].copy_from_slice(&value.to_ne_bytes());
  }

  /// Reserves a list of `n` elements of `size` bytes after its length, points the `Near` at `at`
  /// to it, and returns where its first element is.
  fn list(&mut self, at: usize, n: usize, size: usize) -> usize {
    let list = self.reserve(4 + n * size);
    self.put(list, i32::try_from(n).unwrap());
    self.put(at, i32::try_from(list - at).unwrap());
    list + 4
  }

  /// `n` items, each with a name and 4 tags.
  fn items(n: usize) -> Self {
    let mut image = Self(vec![]);
    let root = image.reserve(4);
    let list = image.list(root, n, 12);
    for i in 0..n {
      let at = list + 12 * i;
      image.put(at, i32::try_from(i).unwrap());
      let name = format!("item-{i}");
      let text = image.list(at + 4, name.len(), 1);
      image.0[text..text + name.len()].copy_from_slice(name.as_bytes());
      image.list(at + 8, 4, 2);
    }
    image
  }

  /// A tree of `levels` levels below the root, each node with `fan` children.
  fn tree(levels: usize, fan: usize) -> Self {
    fn kids(image: &mut Image, at: usize, levels: usize, fan: usize) {
      let n = if levels == 0 { 0 } else { fan };
      let list = image.list(at + 4, n, 8);
      for i in 0..n {
        kids(image, list + 8 * i, levels - 1, fan);
      }
    }
    let mut image = Self(vec![]);
    let root = image.reserve(8);
    kids(&mut image, root, levels, fan);
    image
  }

  /// A chain of `n` nodes below the root, each the one child of the node before it.
  fn chain(n: usize) -> Self {
    let mut image = Self(vec![]);
    let mut at = image.reserve(8);
    for _ in 0..n {
      at = image.list(at + 4, 1, 8);
    }
    image.list(at + 4, 0, 8);
    image
  }

  /// The bytes at an address aligned to 8.
  fn aligned(&self) -> Vec<u64> {
    let mut words = vec![0u64; self.0.len().div_ceil(8)];
    puns::pun_mut::<[u64], [u8]>(&mut words[..])[..self.0.len()].copy_from_slice(&self.0);
    words
  }
}

// -------------------------------------------------------------------------------------------------
// Timing

/// Returns the best time of `f` in nanoseconds, over 7 samples of about 20 ms each.
#[expect(clippy::cast_precision_loss, reason = "a number of iterations, far below 2^52")]
fn time<R>(mut f: impl FnMut() -> R) -> f64 {
  let mut iterations = 1u64;
  loop {
    let start = Instant::now();
    for _ in 0..iterations {
      black_box(f());
    }
    if start.elapsed().as_millis() >= 20 {
      break;
    }
    iterations *= 2;
  }
  (0..7)
    .map(|_| {
      let start = Instant::now();
      for _ in 0..iterations {
        black_box(f());
      }
      start.elapsed().as_secs_f64() * 1e9 / iterations as f64
    })
    .fold(f64::INFINITY, f64::min)
}

/// Times the case `name`, a check by `puns` and the same check by hand, unless `filter` leaves
/// it out.
///
/// # Panics
///
/// Panics if the two checks disagree on `input`.
fn case<I: ?Sized, R: PartialEq + std::fmt::Debug>(
  filter: &[String],
  name: &str,
  input: &I,
  puns: impl Fn(&I) -> R,
  hand: impl Fn(&I) -> R,
) {
  let chosen =
    |f: &String| f.strip_prefix('=').map_or_else(|| name.contains(f.as_str()), |f| f == name);
  if !filter.is_empty() && !filter.iter().any(chosen) {
    return;
  }
  assert_eq!(puns(input), hand(input), "{name}: puns and the hand-written check disagree");
  // `PUNS_BENCH_RUNS=n`: no timing, but `n` runs of each side in `runs`, for a count of the
  // instructions of each (`valgrind --tool=callgrind --toggle-collect='*checks*runs*'`, with
  // `PUNS_BENCH_SIDE=puns` or `hand`).
  if let Ok(n) = std::env::var("PUNS_BENCH_RUNS") {
    let n = n.parse().expect("PUNS_BENCH_RUNS is a number");
    match std::env::var("PUNS_BENCH_SIDE").as_deref() {
      Ok("hand") => runs(|| hand(black_box(input)), n),
      _ => runs(|| puns(black_box(input)), n),
    }
    return;
  }
  let (a, b) = (time(|| puns(black_box(input))), time(|| hand(black_box(input))));
  println!("{name:44} {a:10.2} {b:10.2} {:8.2}", a / b);
}

/// Calls `f` `n` times.
#[inline(never)]
fn runs<R>(mut f: impl FnMut() -> R, n: u64) {
  for _ in 0..n {
    black_box(f());
  }
}

fn main() {
  // `cargo bench` passes `--bench`; anything else filters the cases by name.
  let filter: Vec<String> = std::env::args().skip(1).filter(|a| !a.starts_with("--")).collect();
  println!("{:44} {:>10} {:>10} {:>8}", "ns per check", "puns", "by hand", "ratio");

  // IPv4: the checksum of the header, and of UDP and TCP over the whole packet.
  let ipv4 = |b: &[u8]| try_pun_ref::<[u8], Ipv4>(b).is_ok();
  for (name, protocol, payload) in [
    ("ipv4, udp, 64 bytes", 17, 56),
    ("ipv4, udp, 1472 bytes", 17, 1464),
    ("ipv4, tcp, 1460 bytes", 6, 1440),
    ("ipv4, protocol 253, 1480 bytes", 253, 1480),
  ] {
    let packet = ipv4_packet(protocol, payload);
    case(&filter, name, &packet[..], ipv4, hand::ipv4);
  }
  let mut bad = ipv4_packet(17, 56);
  bad[30] ^= 1;
  case(&filter, "ipv4, udp, 64 bytes, bad checksum", &bad[..], ipv4, hand::ipv4);

  // IPv6: a walk down the extension headers, then the checksum of UDP.
  let ipv6 = |b: &[u8]| try_pun_ref::<[u8], Ipv6>(b).is_ok();
  for (name, headers, protocol, payload) in [
    ("ipv6, tcp, 0 headers", 0, 6, 64),
    ("ipv6, tcp, 3 headers", 3, 6, 64),
    ("ipv6, tcp, 12 headers", 12, 6, 64),
    ("ipv6, udp, 3 headers, 64 bytes", 3, 17, 64),
    ("ipv6, udp, 0 headers, 1400 bytes", 0, 17, 1400),
  ] {
    case(&filter, name, &ipv6_packet(headers, protocol, payload)[..], ipv6, hand::ipv6);
  }

  // UDP: a size in the value, taken from the front of bytes.
  let mut datagram = vec![0x30, 0x39, 0, 0x35, 0, 72, 0, 0];
  datagram.extend(noise(80));
  case(
    &filter,
    "udp, from_head",
    &datagram[..],
    |b: &[u8]| from_head::<Udp>(b).map(|(udp, rest)| (udp.payload.len(), rest.len())).ok(),
    |b: &[u8]| {
      if b.len() < 8 {
        return None;
      }
      let length = usize::from(u16::from_be_bytes([b[4], b[5]]));
      (8..=b.len()).contains(&length).then(|| (length - 8, b.len() - length))
    },
  );

  // Views of one thing or another: the first variant that the bytes are.
  let any = |b: &[u8]| {
    AnyFrame::from_bytes(b).ok().map(|frame| match frame {
      AnyFrame::Udp(_) => 0,
      AnyFrame::Tcp(_) => 1,
      AnyFrame::Ipv4(_) => 2,
      AnyFrame::Arp(_) => 3,
      AnyFrame::Tagged(_) => 4,
      AnyFrame::Other(_) => 5,
    })
  };
  let arp =
    [0, 1, 8, 0, 6, 4, 0, 1, 2, 0, 0, 0, 0, 0xa, 10, 0, 0, 1, 0, 0, 0, 0, 0, 0, 10, 0, 0, 2];
  for (name, frame) in [
    ("view enum, udp over ipv4 (first)", frame(0x0800, &ipv4_packet(17, 56))),
    ("view enum, tcp over ipv4", frame(0x0800, &ipv4_packet(6, 40))),
    ("view enum, udp, bad checksum", frame(0x0800, &bad)),
    ("view enum, arp", frame(0x0806, &arp)),
    ("view enum, vlan", frame(0x8100, &[0, 5, 0x08, 0x00, 1, 2, 3, 4])),
    ("view enum, other ethertype (last)", frame(0x88b5, &noise(46))),
  ] {
    case(&filter, name, &frame[..], any, hand::any_frame);
  }

  // Slices of validated elements.
  let bools: Vec<u8> = noise(64 << 10).iter().map(|b| b & 1).collect();
  case(
    &filter,
    "[u8] -> [bool], 64 KiB",
    &bools[..],
    |b: &[u8]| from_bytes::<[bool]>(b).is_ok(),
    |b: &[u8]| b.iter().all(|&b| b <= 1),
  );
  let kinds: Vec<u8> = noise(4 << 10).iter().map(|b| [1, 2, 3, 7][usize::from(b & 3)]).collect();
  case(
    &filter,
    "[u8] -> [Kind] (1, 2, 3, 7), 4 KiB",
    &kinds[..],
    |b: &[u8]| from_bytes::<[Kind]>(b).is_ok(),
    |b: &[u8]| b.iter().all(|&b| matches!(b, 1..=3 | 7)),
  );
  // Enums of the `Rust` representation: a direct tag, then what its variant requires; a niche in a
  // `bool`.
  let smalls: Vec<u8> =
    noise(4 << 10).chunks(4).flat_map(|c| [c[0] % 3, c[1] & 1, c[2], c[3]]).collect();
  case(
    &filter,
    "[u8] -> [Small] (repr(Rust): a tag, a bool), 4 KiB",
    &smalls[..],
    |b: &[u8]| from_bytes::<[Small]>(b).is_ok(),
    |b: &[u8]| {
      // Where `rustc` put them (`examples/codegen.rs` has the same enum): the tag in the first
      // byte, the `bool` of `Flag` in the second.
      b.len().is_multiple_of(4)
        && b.chunks(4).all(|c| c[0] == 0 || c[0] == 2 || (c[0] == 1 && c[1] <= 1))
    },
  );
  let flags: Vec<u8> = noise(64 << 10).iter().map(|b| b % 3).collect();
  case(
    &filter,
    "[u8] -> [MaybeFlag] (repr(Rust): a niche), 64 KiB",
    &flags[..],
    |b: &[u8]| from_bytes::<[MaybeFlag]>(b).is_ok(),
    |b: &[u8]| b.iter().all(|&b| b <= 2),
  );
  let chars: Vec<u32> =
    noise(4 << 10).chunks(4).map(|c| u32::from(c[0] & 0x7f) << 8 | u32::from(c[1])).collect();
  case(
    &filter,
    "[u32] -> [char], 4 KiB",
    &chars[..],
    |w: &[u32]| try_pun_ref::<[u32], [char]>(w).is_ok(),
    |w: &[u32]| w.iter().all(|&w| char::from_u32(w).is_some()),
  );

  // Pointers: the pointees of eager aims, checked with the value that holds them. Wide data, whose
  // pointees hold no pointer, then a tree of a type that points at itself, wide, small, and as deep
  // as a chain, past `AIM_PARTS`.
  for (name, image, n) in [
    ("aims, 1 M items (a name, 4 tags)", Image::items(1 << 20), 1 << 20),
    ("aims, a tree of 1,000 x 1,000", Image::tree(2, 1000), 0),
    ("aims, a tree of 3 levels of 4", Image::tree(3, 4), 0),
    ("aims, a chain of 100,000", Image::chain(100_000), 0),
  ] {
    let words = image.aligned();
    let bytes = &puns::as_bytes(&words[..])[..image.0.len()];
    if n > 0 {
      case(&filter, name, bytes, |b: &[u8]| from_head::<Items>(b).is_ok(), hand::items);
    } else {
      case(&filter, name, bytes, |b: &[u8]| from_head::<Tree>(b).is_ok(), hand::tree);
    }
  }

  // Checksums, as functions.
  for len in [64, 1518] {
    let bytes = noise(len);
    case(&filter, &format!("crc32, {len} bytes"), &bytes[..], puns::crc32, |b: &[u8]| {
      hand::crc32_slice16::<0xedb8_8320>(b)
    });
    case(
      &filter,
      &format!("crc32, {len} bytes (by hand: a byte at a time)"),
      &bytes[..],
      puns::crc32,
      |b: &[u8]| hand::crc32_bytewise::<0xedb8_8320>(b),
    );
    case(
      &filter,
      &format!("crc-32c (a Crc), {len} bytes"),
      &bytes[..],
      |b: &[u8]| CASTAGNOLI.check(b) as u32,
      hand::crc32_slice16::<0x82f6_3b78>,
    );
    case(
      &filter,
      &format!("crc-16/modbus (a Crc), {len} bytes"),
      &bytes[..],
      |b: &[u8]| MODBUS.check(b) as u16,
      hand::modbus,
    );
    case(
      &filter,
      &format!("crc-16/xmodem (a Crc), {len} bytes"),
      &bytes[..],
      |b: &[u8]| XMODEM.check(b) as u16,
      hand::xmodem,
    );
  }
  let bytes = noise(1500);
  case(
    &filter,
    "sum16, 1500 bytes",
    &bytes[..],
    |b: &[u8]| u128::from(puns::sum16(b)),
    hand::sum16,
  );
}
