//! Functions whose machine code shows what `puns` checks at run time, and what it does not.
//!
//! Each function makes one call to `puns`: a pun, a view, a part, a read from the head or the tail
//! of bytes, or a region. A pun that is proven compiles to no check. A pun that is checked
//! compiles to the checks of what the analysis could not prove. Each function is `#[no_mangle]`,
//! so that its name appears in the assembly.
//!
//! The following command writes the assembly:
//!
//! ```text
//! cargo rustc --release --features unstable --example codegen -- --emit asm -C llvm-args=-x86-asm-syntax=intel
//! ```
//!
//! The functions are in `target/release/**/codegen*.s`, under their names. The table below records
//! the machine code of most of them. A change of the library must not make any of them longer, so
//! compare the assembly before and after it, and update the table.
//!
//! The table was measured on `x86_64-unknown-linux-gnu` with `rustc` 1.100.0, the release profile
//! of the workspace (`opt-level = 3`, LTO, one codegen unit) and the `unstable` feature, with the
//! command above. A count of instructions excludes labels and directives.
//!
//! | Function                                              | Machine code                                  |
//! |-------------------------------------------------------|-----------------------------------------------|
//! | `&Position -> &[u8; 2]`                               | `mov rax, rdi; ret`                           |
//! | `[bool; 4] -> u32`                                    | `mov eax, edi; ret`                           |
//! | `&Frame<Ipv4<[u8]>> -> &[u8]`                         | `lea rdx, [rsi + 34]` (the length)            |
//! | `&Packet<Datagram> -> &Packet` (the same rules)       | `mov rdx, rsi; mov rax, rdi; ret`             |
//! | `&[u8; 2] -> Option<&Position>` (a `u8`, a `bool`)    | one `cmp byte ptr [rdi + 1], 2`               |
//! | `&[u8] -> Option<&Header>`                            | the size, a 4-byte compare, `bt`, `cmp`       |
//! | `from_head::<Frame<Ipv4>>(&[u8])`                     | 29 instructions, no loop, no call             |
//! | `Ipv4::options()` (a region)                          | 10 instructions, no branch                    |
//! | `&[u8] -> Option<&[bool]>`                            | 64 bytes at a time, no branch in a block, 46 instructions |
//! | `&[Shape] -> &[Drawing]` (enums with fields)          | `mov rdx, rsi; mov rax, rdi; ret`             |
//! | `&Drawing -> Option<&Shape>`                          | the tag, then one `cmp` for a `bool`, 12 instructions |
//! | `[u8; 4] -> Option<Shape>`                            | 14 instructions, no loop, no call             |
//! | `Ticket <-> u32` (`repr(Rust)`, a niche in `NonZero`) | `mov eax, edi; ret`, both ways                |
//! | `[u8; 4] -> Option<Small>` (`repr(Rust)`, direct tag) | the tag, one `cmp` for the `bool`, 13 instructions |
//! | `u8 -> Option<Choice>` (a union of `bool`, `u8in`)    | one `cmp cl, 4`, 7 instructions               |
//! | `&[u8] -> Option<&Ipv6>` (a walk, two checksums)      | 207 instructions, no call                     |
//! | `head_ref::<Ipv6, [u8; 40]>`                          | `mov rax, rdi; ret`                           |
//! | `tail_ref::<[u32; 4], u32>`                           | `lea rax, [rdi + 12]; ret`                    |
//! | `from_head::<Position>(&[u8])`                        | the size, one `cmp` for the `bool`, 7 instructions |
//! | `from_head::<Udp>(&[u8])` (a size in the value)       | 15 instructions                               |
//! | `from_tail::<Trailer>(&[u8])` (an enum, a magic)      | the size, `bt`, one `cmp`, 18 instructions, no loop |
//!
//! A count is not a time. The check of a slice of `bool`s takes 64 bytes at a time with no branch
//! inside a block. It has more instructions than a loop of one `cmp` per byte that stops at the
//! first invalid byte, and takes a fraction of its time on a long slice. `benches/checks.rs` times
//! the checks next to the same checks written by hand.
//!
//! The example requires the `unstable` feature, since some of its types have a layout that Rust
//! leaves to the compiler (`Ticket`, `Small` and a tuple). `main` is empty, because the example is
//! compiled to be read, not run.
#![expect(
  clippy::no_mangle_with_rust_abi,
  reason = "the code of the Rust ABI is what is measured; `no_mangle` only names it in the assembly"
)]

use puns::{
  Pun, Tag, from_head, from_tail, head_ref,
  num::{u16be, u16bec, u32bec},
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
  // The state holds the kind of a header in its high 64 bits and its offset in its low 64 bits,
  // so each is one register.
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

/// A trailer of a kind and a magic number, which `back_trailer` reads from the end of bytes.
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
// Two enums of the `Rust` representation. `Ticket` has a niche, and the pun is proven both ways,
// since every `u32` is a `Ticket`. `Small` has a direct tag, and the pun is checked: the tag first,
// then what its variant requires.
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

// Parts. The first bytes of a value start at its address, and the last bytes start at that address
// plus a constant.
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

/// A packet whose payload is a typed region of its bytes, like `Ipv4` in `ethernet.rs`.
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
