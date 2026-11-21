//! Functions whose machine code is worth looking at:
//!
//! ```text
//! cargo rustc --release --features unstable --example codegen -- --emit asm -C llvm-args=-x86-asm-syntax=intel
//! ```
//!
//! and look for their names in `target/release/**/codegen*.s`. See `DESIGN.md` for what to expect.
#![expect(
  clippy::no_mangle_with_rust_abi,
  reason = "the code of the Rust ABI is what is measured; `no_mangle` only names it in the assembly"
)]

use core::marker::PhantomData;

use puns::{
  Pun, SelfRelative, Tag, from_head, from_tail, head_ref,
  layout::{Endian, Pointer},
  num::{i32le, u16be, u16bec, u32bec, u32le},
  pun, pun_ref, tail_ref, try_pun, try_pun_ref,
};

#[derive(Pun)]
#[repr(C)]
pub struct Position {
  pub x: u8,
  pub visible: bool,
}

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
pub enum Kind {
  A = 1,
  B = 2,
  C = 3,
  D = 7,
}

#[derive(Pun)]
#[repr(C)]
pub struct Header {
  pub magic: u32bec<0xcafe_babe>,
  pub kind: Kind,
  pub flag: bool,
  pub len: u16be,
  pub tail: [u8],
}

#[derive(Pun)]
#[repr(C)]
#[pun(size = length)]
pub struct Udp {
  pub source: u16be,
  pub destination: u16be,
  pub length: u16be,
  pub checksum: u16be,
  pub payload: [u8],
}

#[derive(Pun)]
#[repr(C)]
pub struct Frame<P: ?Sized + Tag<u16be> = [u8]> {
  pub dst: [u8; 6],
  pub src: [u8; 6],
  pub ether_type: P::Tag,
  pub payload: P,
}
#[derive(Pun)]
#[repr(C)]
#[pun(size = total_length, tag(u16be = u16bec<0x0800>))]
pub struct Ipv4<P: ?Sized + Pun = [u8]> {
  #[pun(valid(0x45..=0x4f))]
  pub version_ihl: u8,
  pub tos: u8,
  pub total_length: u16be,
  pub id: u16be,
  pub frag: u16be,
  pub ttl: u8,
  pub protocol: u8,
  pub checksum: u16be,
  pub source: [u8; 4],
  pub destination: [u8; 4],
  #[pun(regions(options = (version_ihl & 0x0f) * 4 - 20, payload))]
  pub data: P,
}

// The checksum of what is behind a chain of extension headers: a rule that walks.
#[derive(Pun)]
#[repr(C)]
#[pun(size = payload_length + 40)]
#[pun(assert({
  // What a header is in the high 64 bits, where it starts in the low ones: two registers.
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

/// A trailer: a kind, and a magic number.
#[derive(Pun)]
#[repr(C)]
pub struct Trailer {
  pub kind: Kind,
  pub magic: u16bec<0xcafe>,
}

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
pub enum Shape {
  Circle { radius: u16 },
  Rect(u8, bool),
  Empty,
}
#[derive(Pun, Clone, Copy)]
#[repr(u8)]
pub enum Drawing {
  Circle { radius: u16 },
  Rect(u8, u8),
  Empty,
  Line(u8, u8) = 9,
}

/// Self-relative pointers, declared as a crate of its own would: an offset of 4 bytes,
/// little-endian, of no alignment. No value of them is made here, only the views are compiled.
macro_rules! pointers {
  ($($(#[$doc:meta])* $name:ident[$($param:ident)?] $target:ty, $pointer:expr;)+) => {$(
    $(#[$doc])*
    #[repr(transparent)]
    pub struct $name<$($param)?>(i32le, PhantomData<($($param,)?)>);

    // SAFETY: the type is its offset, a byte array; no value of it is made.
    unsafe impl<$($param: Pun)?> SelfRelative for $name<$($param)?> {
      type Target = $target;
      const POINTER: Pointer = $pointer;
    }

    puns::self_relative!(impl[$($param: Pun)?] $name<$($param)?>);
  )+};
}

const SCHEME: &str = "codegen";

/// The alignment of the first element of a sequence of `T`.
const fn first<T>() -> usize {
  if align_of::<T>() > 4 { align_of::<T>() } else { 4 }
}

pointers! {
  /// To one `T`.
  Rel[T] T, Pointer::one::<T>(SCHEME, 4, Endian::Little);
  /// To one `T`, or none.
  RelOpt[T] T, Pointer::one::<T>(SCHEME, 4, Endian::Little).or_null();
  /// To a sequence of `T`, at least 4-aligned.
  RelSeq[T] T, Pointer::sequence::<T>(SCHEME, 4, Endian::Little, first::<T>());
  /// To text.
  RelStr[] (), Pointer::text(SCHEME, 4, Endian::Little);
}

/// An expression, and the next version of it: a `bool` read as a `u8`, one more variant.
#[derive(Pun)]
#[repr(u8)]
pub enum Old {
  Leaf(bool),
  Neg(Rel<Old>),
  Call(RelStr, RelSeq<Old>),
}

#[derive(Pun)]
#[repr(u8)]
pub enum New {
  Leaf(u8),
  Neg(Rel<New>),
  Call(RelStr, RelSeq<New>),
  Pair(Rel<New>, Rel<New>),
}

/// A pointer that may be null, read as one that is not.
#[derive(Pun)]
#[repr(C)]
pub struct Maybe {
  pub id: u32le,
  pub owner: RelOpt<Old>,
}

#[derive(Pun)]
#[repr(C)]
pub struct Owned {
  pub id: u32le,
  pub owner: Rel<New>,
}

// Nothing to check.
#[unsafe(no_mangle)]
pub const fn position_bytes(p: &Position) -> &[u8; 2] {
  pun_ref(p)
}
#[unsafe(no_mangle)]
pub const fn bools_to_u32(b: [bool; 4]) -> u32 {
  pun(b)
}
#[unsafe(no_mangle)]
pub const fn frame_bytes(f: &Frame<Ipv4>) -> &[u8] {
  pun_ref(f)
}
#[unsafe(no_mangle)]
pub const fn words_bytes(w: &[u32]) -> &[u8] {
  pun_ref(w)
}
// One comparison.
#[unsafe(no_mangle)]
pub fn bytes_position(b: &[u8; 2]) -> Option<&Position> {
  try_pun_ref(b).ok()
}
#[unsafe(no_mangle)]
pub fn u8_bool(b: u8) -> Option<bool> {
  try_pun(b).ok()
}
// The `bool` only: the `u8` and the `u16` are not looked at.
#[unsafe(no_mangle)]
pub fn mixed(b: &(u8, u8, u16)) -> Option<&(u8, bool, u16)> {
  try_pun_ref(b).ok()
}
// A size, a magic number, an enum, a `bool`.
#[unsafe(no_mangle)]
pub fn header(b: &[u8]) -> Option<&Header> {
  try_pun_ref(b).ok()
}
#[unsafe(no_mangle)]
pub fn udp(b: &[u8]) -> Option<&Udp> {
  from_head::<Udp>(b).ok().map(|(u, _)| u)
}
#[unsafe(no_mangle)]
pub fn ipv4(b: &[u8]) -> Option<&Frame<Ipv4>> {
  from_head::<Frame<Ipv4>>(b).ok().map(|(u, _)| u)
}
#[unsafe(no_mangle)]
pub const fn options(p: &Ipv4) -> &[u8] {
  p.options()
}
#[unsafe(no_mangle)]
pub fn bools(b: &[u8]) -> Option<&[bool]> {
  try_pun_ref(b).ok()
}
// Enums with fields: nothing when every variant is one of the target, else the tag, then what
// the variant of that tag requires.
#[unsafe(no_mangle)]
pub const fn shapes_drawings(s: &[Shape]) -> &[Drawing] {
  pun_ref(s)
}
#[unsafe(no_mangle)]
pub fn drawing_shape(d: &Drawing) -> Option<&Shape> {
  try_pun_ref(d).ok()
}
#[unsafe(no_mangle)]
pub fn bytes_shape(b: &[u8; 4]) -> Option<Shape> {
  try_pun(*b).ok()
}
// Enums without `repr`: a niche, proven both ways (every `u32` is a `Ticket`); a direct tag,
// checked: the tag, then what its variant requires.
#[derive(Pun, Clone, Copy)]
pub enum Ticket {
  Free,
  Paid(core::num::NonZero<u32>),
}
#[derive(Pun, Clone, Copy)]
pub enum Small {
  Byte(u8),
  Flag(bool),
  Word(u16),
}
#[unsafe(no_mangle)]
pub const fn ticket_u32(t: Ticket) -> u32 {
  pun(t)
}
#[unsafe(no_mangle)]
pub const fn u32_ticket(w: u32) -> Ticket {
  pun(w)
}
#[unsafe(no_mangle)]
pub fn bytes_small(b: [u8; 4]) -> Option<Small> {
  try_pun(b).ok()
}
// A union: one of its fields, here the `bool` or a number up to 3.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
pub union Choice {
  pub flag: bool,
  pub small: puns::num::u8in<0, 3>,
}
#[unsafe(no_mangle)]
pub fn byte_choice(b: u8) -> Option<Choice> {
  try_pun(b).ok()
}

// A rule that walks: a loop over the headers, then the sums.
#[unsafe(no_mangle)]
pub fn ipv6(b: &[u8]) -> Option<&Ipv6> {
  try_pun_ref(b).ok()
}

// Parts: the first bytes of a value are where it is, the last an addition away.
#[unsafe(no_mangle)]
pub const fn ipv6_header(p: &Ipv6) -> &[u8; 40] {
  head_ref(p)
}
#[unsafe(no_mangle)]
pub const fn last_word(w: &[u32; 4]) -> &u32 {
  tail_ref(w)
}
// From the front or the back of bytes: the size, then the checks of the target.
#[unsafe(no_mangle)]
pub fn front_position(b: &[u8]) -> Option<&Position> {
  from_head::<Position>(b).ok().map(|(p, _)| p)
}
#[unsafe(no_mangle)]
pub fn back_trailer(b: &[u8]) -> Option<&Trailer> {
  from_tail::<Trailer>(b).ok().map(|(_, t)| t)
}

// Proven, pointers followed through a recursive type: nothing is compiled.
#[unsafe(no_mangle)]
pub fn view_new(old: &Old) -> &New {
  puns::view_ref(old)
}
#[unsafe(no_mangle)]
pub fn view_slice(old: &[Old]) -> &[New] {
  puns::view_ref(old)
}
// What is not proven of the value itself: here that a pointer is not null.
#[unsafe(no_mangle)]
pub fn try_view_owned(maybe: &Maybe) -> Option<&Owned> {
  puns::try_view_ref(maybe).ok()
}

/// A packet of a protocol, whose payload is a typed region of its bytes, as `Ipv4` in the README.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 4 + length)]
#[pun(assert(sum16(0, size_of_val(self)) == 0xffff))]
pub struct Packet<P: ?Sized + Tag<u8> = [u8]> {
  pub protocol: P::Tag,
  pub length: u8,
  pub checksum: u16be,
  #[pun(regions(payload: P))]
  pub data: [u8],
}

#[derive(Pun)]
#[repr(C)]
#[pun(tag(u8 = puns::num::u8c<17>))]
pub struct Datagram {
  pub ports: [u8; 4],
  pub rest: [u8],
}

// The rules of a packet of a protocol are those of any packet: proven by the same rules.
#[unsafe(no_mangle)]
pub const fn any_packet(packet: &Packet<Datagram>) -> &Packet {
  pun_ref(packet)
}

fn main() {}
