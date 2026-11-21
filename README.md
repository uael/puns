# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns)](https://docs.rs/puns)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)][mit]

**Type punning, proven at compile time: a safe alternative to `transmute`, and to hand-written
parsing of bytes.**

`puns` reads a value of one type as a value of another, in place: a struct as bytes, bytes as a
packet, a `[bool; 4]` as a `u32`. It compares the two types when the program is built (their
sizes, their alignments, their padding, the values that each byte can take), and the pun is

- **proven** when every value of the source is a valid target: it compiles to no instruction;
- **checked** when only some are: a `try_` or a `from_` function checks at run time what could
  not be proven, and nothing else;
- **refused** when it can never be valid, or would read padding, or would let safe code write an
  invalid value through a `&mut`: it does not compile.

You write no `unsafe` and implement no trait: `#[derive(Pun)]` is all. It works on stable Rust,
without `std`, for dynamically sized types too.

```rust
use puns::Pun;

#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
struct Position {
  x: u8,
  visible: bool,
}

// Proven: every `Position` is two initialized bytes. No instruction.
let bytes: &[u8; 2] = puns::pun_ref(&Position { x: 7, visible: true });
assert_eq!(bytes, &[7, 1]);

// Checked: not every pair of bytes is a `Position`. Only the `bool` is looked at: one `cmp`.
assert_eq!(puns::try_pun_ref(&[7u8, 1]), Ok(&Position { x: 7, visible: true }));
assert_eq!(puns::try_pun_ref::<_, Position>(&[7u8, 2]), Err(puns::Error::Validity));
```

```rust,compile_fail
let mut visible = true;
// Refused: through a `&mut u8`, safe code could write a 2 where a `bool` is.
let byte: &mut u8 = puns::pun_mut(&mut visible);
*byte = 2;
```

```text
error[E0080]: evaluation panicked:
              cannot pun: some values of the target are not valid for the source (or are
              padding where the source is not), and could be written through the `&mut`.
...
note: the above error was encountered while instantiating `fn pun_mut::<bool, u8>`
 --> src/main.rs:4:23
  |
4 |   let byte: &mut u8 = puns::pun_mut(&mut visible);
  |                       ^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

[What is refused](#what-is-refused) shows a whole message, and what to write instead.

## Types that look nothing alike

Two types need not be declared alike to hold the same bytes. What is compared is what each byte
can be, through structs, arrays, tuples, enums and slices:

```rust
use core::num::NonZero;

use puns::{
  Pun,
  num::{u16be, u32le},
};

#[derive(Pun, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
enum Kind {
  Text = 1,
  Image = 2,
}

#[derive(Pun, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
enum Shape {
  Circle { radius: u16 },
  Rect(u8, bool),
}

#[derive(Pun, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
enum Drawing {
  Circle { radius: u16 },
  Rect(u8, u8),
  Nothing,
}

/// An entry, as the program that writes it knows it.
#[derive(Pun, Debug, PartialEq, Clone, Copy)]
#[repr(C)]
struct Entry {
  kind: Kind,            // 1 or 2
  visible: bool,         // 0 or 1
  port: u16be,           // two bytes, of any alignment
  id: NonZero<u32>,      // not 0
  name: [char; 2],       // Unicode scalar values
  range: (u32le, u32le), // eight bytes
  shape: Shape,          // a tag, then a `u16`, or a `u8` and a `bool`
}

/// The same 28 bytes, as a program that knows less declares them.
#[derive(Pun, Debug, PartialEq, Clone, Copy)]
#[repr(C)]
struct Raw {
  kind: u8,
  flags: u8,
  port: [u8; 2],
  id: u32,
  name: [u32; 2],
  range: [[u8; 4]; 2],
  shape: Drawing, // more variants, and a `u8` where the `bool` is
}

/// A table of them: a header, then entries to the end.
#[derive(Pun)]
#[repr(C)]
struct Table<E: ?Sized> {
  count: u32le,
  entries: E,
}

let entry = Entry {
  kind: Kind::Image,
  visible: true,
  port: u16be::new(443),
  id: NonZero::new(7).unwrap(),
  name: ['p', 'n'],
  range: (u32le::new(1), u32le::new(2)),
  shape: Shape::Rect(3, true),
};

// Proven, field by field and byte by byte: 1 or 2 is a `u8`, a `bool` is a `u8`, a `NonZero` is a
// `u32`, a `char` is a `u32`, two `u32le` are two arrays of four bytes, a `Shape` is a `Drawing`.
let raw: Raw = puns::pun(entry);
assert_eq!((raw.kind, raw.flags, raw.port, raw.id), (2, 1, [1, 187], 7));
assert_eq!((raw.name, raw.shape), ([112, 110], Drawing::Rect(3, 1)));

// So is a slice of them, and a struct that ends with one: the same pointer, the same length.
let table = Table { count: u32le::new(2), entries: [entry; 2] };
let typed: &Table<[Entry]> = &table;
let loose: &Table<[Raw]> = puns::pun_ref(typed);
assert_eq!((loose.entries.len(), loose.entries[1]), (2, raw));

// The other way is checked, and only what a `Raw` does not prove: the kind, the flags, the
// identifier, the two characters and the shape. The port and the range are not looked at.
assert_eq!(puns::try_pun_ref::<Raw, Entry>(&raw), Ok(&entry));
let surrogate = Raw { name: [0xd800, 110], ..raw };
assert_eq!(puns::try_pun_ref::<Raw, Entry>(&surrogate), Err(puns::Error::Validity));
let nothing = Raw { shape: Drawing::Nothing, ..raw };
assert!(puns::try_pun_ref::<Raw, Entry>(&nothing).is_err());

// By `&mut`, both must accept what the other holds: a `(u32le, u32le)` and a `[[u8; 4]; 2]` do.
let mut range = (u32le::new(1), u32le::new(2));
puns::pun_mut::<_, [[u8; 4]; 2]>(&mut range)[1] = [9, 0, 0, 0];
assert_eq!(range.1.get(), 9);
```

What is not compatible is found as precisely. Reading these entries as bytes is refused, "at byte
25 of the target": the one byte of padding of the 28, after the tag of a `Shape::Circle`. Reading
a `&mut Entry` as a `&mut Raw` is refused too: through it, a kind of 9 could be written.

## A format is a type

What the type of a field cannot say (a size that is in a header, a checksum, a value that tells
what follows) is told to the derive. This is UDP over IPv4 over Ethernet. There is no parsing
code below, and none is hidden: the declarations are the format.

```rust
use puns::{
  Pun, Tag,
  num::{u8c, u16be, u16bec},
};

/// An Ethernet frame. A `Frame` carries bytes, whatever its EtherType; a `Frame<Ipv4>` is a
/// frame whose EtherType is the one of IPv4.
#[derive(Pun)]
#[repr(C)]
struct Frame<P: ?Sized + Tag<EtherType> = [u8]> {
  destination: [u8; 6],
  source: [u8; 6],
  ether_type: P::Tag,
  payload: P,
}

#[derive(Pun, Clone, Copy)]
#[repr(transparent)]
struct EtherType(u16be);

#[derive(Pun)]
#[repr(C)]
#[pun(tag(EtherType = u16bec<0x0800>))] // the EtherType it comes with
#[pun(size = total_length)] // its size is in its header
#[pun(assert(sum16(0, (version_ihl & 0x0f) * 4) == 0xffff))] // the checksum of the header
#[pun(get(header_length: usize = (version_ihl & 0x0f) * 4))] // a bit field, as a method
struct Ipv4<P: ?Sized + Tag<Protocol> = [u8]> {
  #[pun(valid(0x45..=0x4f))] // version 4, a header of 5 to 15 words
  version_ihl: u8,
  type_of_service: u8,
  total_length: u16be,
  identification: u16be,
  flags_fragment_offset: u16be,
  time_to_live: u8,
  protocol: P::Tag,
  checksum: u16be,
  source: [u8; 4],
  destination: [u8; 4],
  // The options, whose size is in the header too, then what the packet carries.
  #[pun(regions(options = (version_ihl & 0x0f) * 4 - 20, payload: P))]
  data: [u8],
}

#[derive(Pun, Clone, Copy)]
#[repr(transparent)]
struct Protocol(u8);

#[derive(Pun)]
#[repr(C)]
#[pun(tag(Protocol = u8c<17>))]
#[pun(size = length)]
struct Udp {
  source_port: u16be,
  destination_port: u16be,
  length: u16be,
  checksum: u16be,
  payload: [u8],
}

/// A frame is one thing or another: the first of these that the bytes are.
#[derive(Pun)]
enum Packet<'a> {
  Udp(&'a Frame<Ipv4<Udp>>),
  Ipv4(&'a Frame<Ipv4>),
  Other(&'a Frame),
}

let bytes: [u8; 47] = [
  2, 0, 0, 0, 0, 2, 2, 0, 0, 0, 0, 1, 0x08, 0x00, // Ethernet: to, from, the EtherType of IPv4
  0x45, 0, 0, 33, 0, 1, 0x40, 0, 64, 17, 0x26, 0xc9, 10, 0, 0, 1, 10, 0, 0, 2, // IPv4
  0x30, 0x39, 0, 53, 0, 13, 0, 0, b'h', b'e', b'l', b'l', b'o', // UDP, to port 53
];

// One call checks the EtherType, the version, the sizes, the checksum of the header and the
// protocol. The frame is read where it is: nothing is copied, nothing is allocated.
let frame: &Frame<Ipv4<Udp>> = puns::from_bytes(&bytes).unwrap();
let ip = &frame.payload;
assert_eq!((ip.header_length(), ip.time_to_live), (20, 64));
assert!(ip.options().is_empty());
let udp = ip.payload().unwrap();
assert_eq!((udp.destination_port.get(), &udp.payload), (53, &b"hello"[..]));

// What is known is not checked again. A frame that carries UDP is a frame: no instruction.
let any: &Frame = puns::pun_ref(frame);
assert_eq!(any.payload.len(), 33);
// And an IPv4 frame of any protocol: the rules of `Ipv4<Udp>` (its size, its checksum) are those
// of `Ipv4`, over the same bytes, so they prove them. No instruction either.
let ip: &Frame<Ipv4> = puns::pun_ref(frame);
assert_eq!(ip.payload.header_length(), 20);

// One bit of the header is flipped: its checksum does not hold, this is not IPv4.
let mut corrupted = bytes;
corrupted[22] ^= 1;
assert_eq!(puns::from_bytes::<Frame<Ipv4>>(&corrupted).err(), Some(puns::Error::Validity));
assert!(matches!(Packet::from_bytes(&corrupted), Ok(Packet::Other(_))));
assert!(matches!(Packet::from_bytes(&bytes), Ok(Packet::Udp(_))));

// From a stream: the header tells where the packet ends, `from_head` returns what follows.
let stream = [&bytes[14..], &[0xaa, 0xbb]].concat();
let (packet, rest) = puns::from_head::<Ipv4>(&stream).unwrap();
assert_eq!((size_of_val(packet), rest), (33, &[0xaa, 0xbb][..]));
```

| What the format says                                    | Told by                                      |
|---------------------------------------------------------|----------------------------------------------|
| numbers are big-endian                                  | `u16be`                                      |
| the EtherType tells what the frame carries              | `P::Tag`, `tag(EtherType = u16bec<0x0800>)`  |
| version 4, a header of 5 to 15 words                    | `valid(0x45..=0x4f)`                         |
| the size of a packet is in its header                   | `size = total_length`                        |
| the header has a checksum                               | `assert(sum16(..) == 0xffff)`                |
| options, whose size is in the header, then the payload  | `regions(options = .., payload: P)`          |
| bit fields                                              | `get(header_length: usize = ..)`             |
| a frame is one thing or another                         | an enum of references                        |

[`puns/examples/ethernet.rs`][ethernet] is the complete version: the checksums
of UDP and TCP, IPv6 behind any chain of extension headers, VLAN tags, the frame check sequence.

## What the derive is told

| Option                                     | What it tells                                              |
|--------------------------------------------|------------------------------------------------------------|
| `u16be`, `u32le`, `i64be`, `f32le`...      | the byte order of a field (types of `puns::num`)           |
| `u8c<17>`, `u32bec<0x89504e47>`...         | a field that has one value: a magic number, a tag          |
| `#[pun(valid(1..=9, 0x80))]`               | the values that an integer field takes                     |
| `#[pun(size = EXPR)]`                      | the size of the value, in bytes                            |
| `#[pun(len = EXPR)]`                       | the number of elements of the slice that ends it           |
| `#[pun(assert(EXPR))]`                     | anything else: bits, relations between fields, checksums   |
| `#[pun(regions(a = EXPR, b: Type))]`       | consecutive regions of the tail, and a method for each     |
| `#[pun(get(name: Type = EXPR))]`           | a method that computes from fields: a bit field            |
| `#[pun(tag(Key = Type))]`                  | the value of the `Key` that comes with this type           |
| an enum whose variants hold references     | one thing or another: `from_bytes`, `from_head`            |

An `EXPR` is a Rust expression over the unsigned integer fields of the struct and
`size_of_val(self)`: operators, `if`, `let`, constants. It can also read the value itself:

- `le(offset, n)`, `be(offset, n)`: the integer of `n` bytes at an offset;
- `sum16`, `crc32`, `crc::<WIDTH, POLY, INIT, REFLECT, XOROUT>`, `fletcher16`, `adler32`, `sum`:
  checksums over a range of bytes;
- `leb128`, `quic`, `vlq`: variable-length integers;
- `matches(DFA, start, end)`: a pattern of bytes (ASCII, digits, UTF-8, or an automaton of yours);
- `walk(start, |at| condition, |at| next)`: what is reached step by step, such as the end of a
  chain of headers or of a list of options.

Where Rust would panic (an overflow, a division by zero, a read past the end), the rule does not
hold: a rule cannot fail in any other way. The
[documentation of the derive](https://docs.rs/puns/latest/puns/derive.Pun.html) is the reference.

## The functions

What is read (a typed value, or bytes), by how much of it:

|               | Whole                                   | Front                          | Back                           |
|---------------|-----------------------------------------|--------------------------------|--------------------------------|
| a typed value | `pun`, `pun_ref`, `pun_mut`, `try_pun*` | `head`, `head_ref`, `head_mut` | `tail`, `tail_ref`, `tail_mut` |
| bytes         | `from_bytes`, `as_bytes`                | `from_head`                    | `from_tail`                    |

- **A typed value** is read by value (`pun`), by reference (`pun_ref`) or by mutable reference
  (`pun_mut`). These are proven, or refused; `try_pun`, `try_pun_ref` and `try_pun_mut` are
  checked, and return a `Result`.
- **Bytes** are checked: the size, the address, then what bytes do not prove of the type.
  `from_bytes` wants all the bytes to be the value; `from_head` and `from_tail` take a value
  from the front or from the back, and return what remains. The three have a `_mut` twin, and
  `as_bytes` is the way back, proven.
- **A part** is the first or the last bytes of a value: `head_ref::<Ipv4, [u8; 20]>` is the
  fixed header of a packet.

```rust
let mut bytes = [0u8, 1, 2, 3, 4];
let (first, rest) = puns::from_head::<[u8; 2]>(&bytes).unwrap();
let (_, last) = puns::from_tail::<[u8; 2]>(rest).unwrap();
assert_eq!((first, last), (&[0, 1], &[3, 4]));

assert_eq!(puns::tail::<[u8; 5], [u8; 2]>(bytes), [3, 4]);
*puns::head_mut::<_, [u8; 2]>(&mut bytes) = [7, 7];
assert_eq!(bytes, [7, 7, 2, 3, 4]);

// By value, sizes are equal; by reference, a slice is as long as its bytes allow.
assert_eq!(puns::pun::<[bool; 4], u32>([true; 4]), 0x0101_0101);
assert_eq!(puns::pun_ref::<[u16], [[u8; 2]]>(&[1, 2, 3]).len(), 3);
assert_eq!(puns::from_bytes::<str>(b"caf\xc3\xa9"), Ok("café"));
```

All are `const fn`s.

## What it knows

- Integers, floating-point numbers, `bool`, `char`, `NonZero`, and enums without fields, whatever
  their `repr`: each has a set of valid values, and so has each of their bytes.
- Structs of any `repr` (`C`, `transparent`, `packed`, `align`, or none), tuples, arrays, slices,
  and structs that end with a slice: their padding, and what each of their bytes can be. Of a
  struct or a tuple without `repr`, what every order of its fields has (see below).
- Enums with fields, of any `repr` or of none: their tag, and what each variant is made of.
  Without `repr`, the layout that `rustc` chose (a tag, or a niche in a field) is found when the
  program is built, by experiments on a value of each variant; an experiment that goes wrong is
  a compile error, never a wrong layout. `Option<T>` and `Result<T, E>` are such enums. What is
  found is used with the `unstable` feature; without it, such an enum only proves itself (but
  `Option` and `Result` where `core` documents them: of a `NonZero`).
- `repr(C)` unions: as a target, one of their fields; as a source, nothing is known of them.
- `MaybeUninit<T>`: bytes of which nothing is known, which take anything. `Infallible`, and enums
  without variants: no value.
- `str`: bytes that are UTF-8.
- What the derive is told, above.

### What Rust keeps, and the `unstable` feature

Rust lets the compiler choose the order of the fields of a struct or of a tuple without `repr`,
and where the tag of an enum without `repr` is, and its values; the next version of `rustc` may
choose otherwise, and `-Zrandomize-layout` chooses at random. A pun is always right for the build
that makes it, but without the `unstable` feature, **`-Zrandomize-layout` changes nothing that
`puns` does**: a pun compiles only if what it does, what it checks and which values it accepts are
the same in every layout that Rust allows.

It is a matter of the pun, not of the type: whatever byte the `bool` of a `(bool, u8)` is, its
bytes are initialized, but which one bytes read as a `(bool, u8)` must check is not known.

```rust
// Bytes of any value, or both `bool`s, in either order.
assert_eq!(puns::pun::<[u8; 2], (u8, u8)>([7, 7]), (7, 7));
assert!(puns::try_pun::<[u8; 2], (bool, bool)>([1, 2]).is_err());
// Initialized bytes, wherever the `bool` is.
let _: &[u8] = puns::as_bytes(&(true, 1u8));
// The `bool` may be either byte: two `bool`s prove it wherever it is.
let _: (bool, u8) = puns::pun([true, false]);
// A value that `core` documents: `None` is zeros.
assert_eq!(puns::pun::<u32, Option<core::num::NonZero<u32>>>(0), None);
```

```rust,ignore
// Without `unstable`: does not compile, the byte that must be 0 or 1 is one or the other.
let _ = puns::try_pun::<[u8; 2], (bool, u8)>([1, 1]);
```

With `features = ["unstable"]`, `puns` takes the layout that `rustc` chose, as it is: what
compiles without the feature compiles with it and does the same, and more compiles. What Rust
documents, and where, is in [`puns/DESIGN.md`][design], "What Rust keeps of a layout".

An enum is known by its tag, and a variant proves the variant of the same tag:

```rust
use puns::Pun;

#[derive(Pun, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
enum Shape {
  Circle { radius: u16 },
  Rect(u8, bool),
}

#[derive(Pun, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
enum Drawing {
  Circle { radius: u16 },
  Rect(u8, u8),
  Nothing,
}

// Proven: every `Shape` is a `Drawing`, variant by variant. So is a slice of them.
let shapes = [Shape::Rect(3, true), Shape::Circle { radius: 9 }];
let drawings: &[Drawing] = puns::pun_ref(&shapes[..]);
assert_eq!(drawings, [Drawing::Rect(3, 1), Drawing::Circle { radius: 9 }]);

// Checked: the tag, then the `bool` of a `Rect`.
assert_eq!(puns::try_pun(Drawing::Rect(3, 1)), Ok(Shape::Rect(3, true)));
assert_eq!(puns::try_pun::<_, Shape>(Drawing::Rect(3, 2)), Err(puns::Error::Validity));
assert_eq!(puns::try_pun::<_, Shape>(Drawing::Nothing), Err(puns::Error::Validity));
```

With the `unstable` feature (see above), the same of enums without `repr`, `Option` included,
whose layout is found rather than declared:

```rust
# #[cfg(feature = "unstable")]
# fn main() {
use core::num::NonZero;

use puns::Pun;

#[derive(Pun, Debug, PartialEq, Clone, Copy)]
enum Ticket {
  Free,
  Paid(NonZero<u32>),
}

// A niche: `Free` is the 0 that no `NonZero` is. Every `u32` is a `Ticket`, both ways proven.
assert_eq!(puns::pun::<Ticket, u32>(Ticket::Free), 0);
assert_eq!(puns::pun::<u32, Ticket>(7), Ticket::Paid(NonZero::new(7).unwrap()));
assert_eq!(puns::pun::<u32, Option<NonZero<u32>>>(7), NonZero::new(7));

// Not every byte is an `Option<bool>`: 0 and 1 are `Some`, `None` is where `rustc` put it.
let none: u8 = puns::pun(None::<bool>);
assert_eq!(puns::try_pun::<u8, Option<bool>>(none), Ok(None));
assert_eq!(puns::try_pun::<u8, Option<bool>>(1), Ok(Some(true)));
assert!(puns::try_pun::<u8, Option<bool>>(none + 1).is_err());
# }
# #[cfg(not(feature = "unstable"))]
# fn main() {}
```

Two things follow from "a type is its bytes", by design. Types that differ only by a
`PhantomData` or a lifetime convert to each other freely, and deriving `Pun` makes fields as
accessible as if they were public. A type whose parameter is a brand, or whose private fields
keep an invariant that the derive is not told of, should not derive `Pun`: see
[What deriving `Pun` means](https://docs.rs/puns/latest/puns/#what-deriving-pun-means).

## What is refused

A refusal is a compile error that tells why, at which byte, and often what to write instead. Here
is a classic mistake with `transmute`, a struct that has padding read as bytes:

```rust,compile_fail
#[derive(puns::Pun)]
#[repr(C)]
struct Header {
  kind: u8,
  length: u32,
}

let bytes: &[u8; 8] = puns::pun_ref(&Header { kind: 1, length: 2 });
```

```text
error[E0080]: evaluation panicked:
              cannot pun: a byte that the target requires to be initialized is padding in the source.
              This is at byte 1 of the target.

   --> puns-0.1.0/src/lib.rs
    |
    |   const BY_REF: () = Self::require(Self::check(Need::REF));
    |                      ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ evaluation of `puns::Plan::<Header, [u8; 8]>::BY_REF` failed inside this call
    |
note: inside `puns::Plan::<Header, [u8; 8]>::require`
   --> puns-0.1.0/src/lib.rs
    |
    |       message.panic();
    |       ^^^^^^^^^^^^^^^
note: inside `puns::Message::panic`
   --> core/src/panic.rs
    |
    = note: the failure occurred here

note: erroneous constant encountered
   --> puns-0.1.0/src/lib.rs
    |
    |   const { Plan::<S, D>::BY_REF };
    |   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

note: the above error was encountered while instantiating `fn pun_ref::<Header, [u8; 8]>`
  --> src/main.rs:10:25
   |
10 |   let bytes: &[u8; 8] = puns::pun_ref(&Header { kind: 1, length: 2 });
   |                         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

The first lines tell why, and where in the type: byte 1 is the first of the three bytes of padding
after `kind`. The last note tells where in your code. Between them, `rustc` shows the constant of
`puns` whose evaluation failed, with the two types it was evaluated for. What to write instead is
to declare the bytes, which are then initialized:

```rust
#[derive(puns::Pun)]
#[repr(C)]
struct Header {
  kind: u8,
  reserved: [u8; 3],
  length: u32,
}

let bytes: &[u8; 8] = puns::pun_ref(&Header { kind: 1, reserved: [0; 3], length: 2 });
assert_eq!(bytes[..4], [1, 0, 0, 0]);
```

The other refusals, each with its message and with what works:

**A target that needs a greater alignment than the source has.** A `[u8; 4]` can be at any
address, a `u32` cannot.

```rust,compile_fail
let n: &u32 = puns::pun_ref(&[0u8; 4]);
```

```text
cannot pun: the target has a greater alignment than the source.
Use `try_pun_ref` or `try_pun_mut` to check the address at run time.
```

```rust
use puns::num::u32le;

// By value there is no address: the four bytes are copied into a `u32`.
assert_eq!(puns::pun::<[u8; 4], u32>([1, 0, 0, 0]), u32::from_ne_bytes([1, 0, 0, 0]));
// By reference, to a type that has no alignment: a `u32le` is four bytes.
assert_eq!(puns::pun_ref::<[u8; 4], u32le>(&[1, 0, 0, 0]).get(), 1);
// Or the address is checked at run time: it is aligned, or it is not.
match puns::try_pun_ref::<[u8; 4], u32>(&[0; 4]) {
  Ok(n) => assert_eq!(*n, 0),
  Err(error) => assert_eq!(error, puns::Error::Alignment),
}
```

**A value that may not be valid, without a check.**

```rust,compile_fail
let text: &str = puns::pun_ref(&[0xffu8; 4]);
```

```text
cannot pun: some values of the source are not valid for the target, or do not prove one of its rules.
Use a `try_*` function to check them at run time.
This is at byte 0 of the target.
```

```rust
assert_eq!(puns::try_pun_ref::<[u8; 4], str>(b"text"), Ok("text"));
assert_eq!(puns::try_pun_ref::<[u8; 4], str>(&[0xff; 4]), Err(puns::Error::Validity));
```

**A `&mut` through which an invalid value could be written.** Here, bytes that are not UTF-8.

```rust,compile_fail
let mut text = String::from("text");
let bytes: &mut [u8] = puns::pun_mut(text.as_mut_str());
```

```text
cannot pun: some values of the target are not valid for the source (or are padding where the
source is not), and could be written through the `&mut`.
```

```rust
// By shared reference nothing is written: every `str` is bytes.
let text = String::from("text");
let bytes: &[u8] = puns::pun_ref(text.as_str());
assert_eq!(bytes, b"text");
```

**An enum, read as bytes.** Its padding is known.

```rust,compile_fail
#[derive(puns::Pun, Clone, Copy)]
#[repr(u8)]
enum Small {
  A(u8),
  B(u16),
}

let bytes: [u8; 4] = puns::pun(Small::B(1));
```

```text
cannot pun: a byte that the target requires to be initialized is padding in the source.
This is at byte 1 of the target.
```

**A check that can never succeed.** A byte that is always 2 is never a `bool`.

```rust,compile_fail
let never = puns::try_pun::<puns::num::u8c<2>, bool>(Default::default());
```

```text
cannot pun: no value of the source is valid for the target.
This is at byte 0 of the target.
```

A refusal is an error of the monomorphization of a generic function: **`cargo build` reports it,
`cargo check` does not.**

## Performance

What a pun costs is decided when the program is built. A proven pun is no instruction: the
analysis ran in the compiler. A checked pun runs the checks that the pair of types leaves, and
nothing else: reading a `[u8; 2]` as a `Position` looks at one byte, reading a `Raw` as an
`Entry` at the fields that can be wrong, and not at its port or its range.

### Machine code

On x86-64, `opt-level = 3` (the functions are in [`puns/examples/codegen.rs`][codegen], the whole
table is in [`puns/DESIGN.md`][design]):

| Function                                           | Machine code                         |
|----------------------------------------------------|--------------------------------------|
| `&Position -> &[u8; 2]`, `[bool; 4] -> u32`        | `mov rax, rdi; ret`                  |
| `&[Shape] -> &[Drawing]` (enums with fields)       | `mov rdx, rsi; mov rax, rdi; ret`    |
| `&Frame<Ipv4> -> &[u8]`                            | `lea rdx, [rsi + 34]` (the length)   |
| `&Frame<Ipv4<Udp>> -> &Frame<Ipv4>` (same rules)   | `mov rdx, rsi; mov rax, rdi; ret`    |
| `&[u8; 2] -> Option<&Position>`                    | one `cmp byte ptr [rdi + 1], 2`      |
| `&Drawing -> Option<&Shape>`                       | the tag, then one `cmp` for a `bool` |
| `from_head::<Frame<Ipv4>>(&[u8])`                  | 29 instructions, no loop, no call    |
| `&[u8] -> Option<&[bool]>`                         | 64 bytes at a time, without a branch |

### Time

[`puns/benches/checks.rs`][checks] times each check next to the same check written by hand, with
the same algorithm for the checksums (`cargo bench -p puns --features unstable --bench checks`).
Nanoseconds per check, release with LTO, on a Xeon at 2.1 GHz; each number is the median of 5
runs (3 for the rows of IPv4 in 64 bytes, IPv6 in 0 and 3 headers and the view enum, measured
again when rules were narrowed), a run the best of 7 samples:

| Check                                               | `puns` | by hand | ratio |
|-----------------------------------------------------|-------:|--------:|------:|
| IPv4, UDP, 64 bytes (two checksums)                 |     11 |      10 |   1.1 |
| IPv4, UDP, 1472 bytes                               |     53 |      51 |   1.0 |
| IPv4, TCP, 1460 bytes                               |     53 |      53 |   1.0 |
| IPv6, TCP, no extension header                      |    1.1 |     1.5 |   0.8 |
| IPv6, TCP behind 3 extension headers                |    3.4 |     4.8 |   0.7 |
| IPv6, TCP behind 12 extension headers               |     12 |      15 |   0.7 |
| IPv6, UDP, 1400 bytes (its checksum)                |     50 |      42 |   1.2 |
| UDP, `from_head`                                    |    1.3 |     1.3 |   1.0 |
| a frame that is one of 6 things: UDP over IPv4      |     13 |      11 |   1.1 |
| a frame that is one of 6 things: ARP                |    1.3 |     2.2 |   0.6 |
| a frame that is one of 6 things: the last one       |    1.0 |     2.1 |   0.5 |
| a frame that is one of 6 things: a wrong checksum   |     12 |      11 |   1.1 |
| `[u8]` as `[bool]`, 64 KiB                          |  1 550 |  21 980 |  0.07 |
| `[u8]` as `[Kind]` (1, 2, 3 or 7), 4 KiB            |    152 |   5 241 |  0.03 |
| `[u32]` as `[char]`, 4 KiB                          |    186 |     336 |   0.6 |
| CRC-32, 1518 bytes (by hand: 16 bytes at a time)    |    710 |     594 |   1.2 |
| CRC-16/Modbus, 1518 bytes (by hand: a byte at once) |    701 |   3 711 |   0.2 |
| `sum16`, 1500 bytes                                 |     48 |      48 |   1.0 |

By hand, a slice is checked the way one writes it first, `bytes.iter().all(|&b| b <= 1)`: `puns`
checks 64 bytes at a time, without a branch in a block. A rule is compiled to its arithmetic, one
function per operation that the optimizer inlines; no expression is interpreted at run time.

A view enum answers what trying its variants in order would, but decides each variant once:
bytes whose EtherType is not the one of a variant do not get to it, and a variant whose values are
all values of a later one (`Frame<Ipv4<Udp>>` of `Frame<Ipv4>`) is decided after that one, then
checked only for what it adds (the protocol). A UDP packet whose checksum is wrong computes it
once, for `Frame<Ipv4>`, rather than for each variant that has it. The optimizer now inlines the
view where it is called (its checksums stay calls), and not the check by hand: hence frames that
take less time than by hand ([the design][design], "Views").

### What to keep in mind

- **A check never allocates, never panics, and its time is bounded by the size of the value**:
  where Rust would panic, the rule fails, and a `walk` takes at most `8 * size + 128` steps.
  Untrusted bytes can be given to `from_bytes` and `from_head`.
- **A slice is checked by blocks** of 64 bytes, without a branch inside a block: an element that
  is not valid is found when its block is done, not at its byte.
- **A CRC is 32 KiB of tables in the binary** for each set of parameters that is used (16 bytes
  at a time; 16 KiB of them are read for a CRC of up to 32 bits); `sum16` adds 8 bytes at a time
  and needs none.
- **A rule means what it means on 128-bit integers, and computes in a register what fits one.**
  The range of each of its values is known when the program is built, from the widths of the
  fields and of what it reads: a sum of two fields of 16 bits is one addition of a register,
  without a test of overflow, and an offset that was read at is in the value. A `walk` that
  carries two things in its state (what a header is, where it starts) keeps each in a half,
  `(kind << 64) + start`: what is added to the low half is computed there alone when it cannot
  carry into the high one ([the design][design], "Walks" and "Narrowing"). A counter that only a
  comparison bounds is computed on 128 bits.
- **Each checked pun is code**, specialized for its pair of types: a proven pun adds nothing to
  the binary, a checked one adds its checks, inlined where it is called.
- **Measure in release.** In a debug build nothing is inlined: each operation of a rule is a
  call.
- **The analysis is build time**: it runs in the compiler, for each new pair of types. The
  Ethernet example (32 types, about 80 puns, a view enum of 19 variants) builds in a few seconds
  in debug, and in about a minute in release with LTO and one codegen unit.
- **The functions of the grid are `const fn`s**: a pun of constants is done when the program is
  built.

## Next to `transmute`, `bytemuck` and `zerocopy`

`transmute` checks sizes, and the rest is yours to prove in a comment.

[`bytemuck`](https://crates.io/crates/bytemuck) and [`zerocopy`](https://crates.io/crates/zerocopy)
describe each type on its own, by marker traits (roughly: "it can be read as bytes", "any bytes
make one", "bytes can be checked to make one"), and allow a conversion when the source has one
trait and the target another. `puns` compares the two types with each other, byte by byte. What
this changes:

- **What the source proves is not checked again.** A `[bool; 2]` is a `Position`, a `Shape` is
  a `Drawing`, a frame of UDP is a frame: proven, for no instruction, where a trait of the target
  alone asks for a check of its bytes.
- **Types that have invalid values convert to each other**, by `&mut` too: `&mut [bool; 4]` as
  `&mut [[bool; 2]; 2]`, an enum as another when each accepts what the other holds.
- **Enums without `repr` are known**, `Option` and `Result` among them: their layout, which
  `rustc` chose, is found when the program is built, where a derive of a marker trait asks an
  enum with fields for a `repr` that specifies it. A `bool` is an `Option<bool>`, proven (with the
  `unstable` feature: Rust does not keep where the `None` of an `Option<bool>` is).
- **The rules of a format are in its type**: sizes and lengths that are in the data, values that
  tell what follows, checksums. With a marker trait, a cast gives a header, and the code that
  finds where the packet ends is still to be written, and to be trusted.

What they have and `puns` does not: years of use and of review, an older minimum Rust version,
helpers to allocate, and refusals that are trait bounds, which `cargo check` reports and an
editor shows. The analysis of `puns` is run by the compiler when it evaluates constants, which
takes build time: a fraction of a second for each new pair of types.

## How it is verified

- **A model check.** Every verdict of the analysis is compared with oracles that do not use it:
  about a hundred types of 1 to 8 bytes, every ordered pair of the same size, every value of
  the source; every part, by writing every value through every `&mut`. It runs in full every
  night, a sample on every push.
- **Miri**, on every push, with tree borrows and with stacked borrows, on x86-64, on a 32-bit
  target (`i686`) and on a big-endian one (`s390x`).
- **Compile-fail tests**: more than eighty programs that must not build, each with its message.
- **A `core` that lies**: the derive is run in a crate whose `core` is another crate, with an
  `offset_of!` that swaps fields: what it generates does not trust a name that safe code can
  redefine.

[`puns/DESIGN.md`][design] tells how it works, what its soundness rests on, and what is
not done yet.

## Self-relative pointers

A crate that has self-relative pointers (offsets from their own address to a target in the same
buffer, as in the regions of [`nearest`](https://crates.io/crates/nearest)) declares them with
`puns::SelfRelative`. `puns::view_ref` then reads a value that holds them as another type, with
everything that they aim at proven at compile time: a tree in a buffer, read as the next
version of its type, for no instruction.

## Examples

Each file of [`puns/examples/`][examples] declares formats with the derive only (no `impl`,
no `unsafe`), reads them from bytes, and checks that corrupted ones are rejected. Each tells
what is specific to its formats and the option that tells it, and what is not covered and why.
They run as tests (`cargo test --test ethernet`), or one by one (`cargo run --example ethernet`).

| Example          | Formats                                                                        |
|------------------|--------------------------------------------------------------------------------|
| `ethernet.rs`    | Ethernet II and 802.3, VLANs, ARP, IPv4, IPv6 and its extension headers, UDP, TCP, ICMP, MPLS, PPPoE, 802.1X, LLDP |
| `ip_services.rs` | DNS, DHCP, NTP, RADIUS, NetFlow v5, STUN, RTP and RTCP                         |
| `transport.rs`   | TLS records and handshakes, QUIC long headers, HTTP/2 frames, WebSocket, SSH, MQTT, CoAP, SCTP |
| `routing.rs`     | BGP-4, OSPFv2 and its Fletcher checksum, GRE, VXLAN, Geneve, IGMP, ICMPv6 neighbor discovery |
| `link.rs`        | IEEE 802.11, Bluetooth HCI and L2CAP, CAN and CAN FD, Modbus RTU and TCP, SLIP, PPP |
| `images.rs`      | PNG, BMP, the table directory of TrueType and OpenType fonts                  |
| `media.rs`       | RIFF and WAVE, MP4 (ISO-BMFF) boxes                                            |
| `archives.rs`    | ZIP, tar (ustar), gzip                                                         |
| `executables.rs` | ELF (32 and 64 bits, either byte order), PE/COFF, Mach-O, Java class files, WebAssembly |
| `storage.rs`     | the SQLite header, FAT boot sectors, GPT partition tables, USB descriptors     |
| `encodings.rs`   | ASN.1 DER (SNMP, Kerberos), Protocol Buffers, MessagePack, CBOR, UTF-8 fields, C strings, an HTTP/1.1 request line |

## Getting started

```toml
[dependencies]
puns = "0.1"
```

Rust 1.99 or later: the first stable release where `size_of_val_raw` and `align_of_val_raw` are
`const`, which is how `puns` knows the layout of a type that ends with a slice. `puns` is `no_std`
and depends on nothing but its derive. Build with `cargo build` to see refusals: `cargo check`
does not evaluate what decides them.

Without features, nothing that `puns` does depends on how `rustc` lays out what Rust lets it lay
out otherwise; `features = ["unstable"]` takes the layout that `rustc` chose (see "What Rust
keeps, and the `unstable` feature").

`0.1`: the API may still change before `1.0`. "Not done yet", in
[`puns/DESIGN.md`][design], lists what is missing.

## License

Licensed under either of [Apache License, Version 2.0][apache] or [MIT license][mit] at your
option.

[ethernet]: https://github.com/uael/puns/blob/main/puns/examples/ethernet.rs
[codegen]: https://github.com/uael/puns/blob/main/puns/examples/codegen.rs
[checks]: https://github.com/uael/puns/blob/main/puns/benches/checks.rs
[examples]: https://github.com/uael/puns/tree/main/puns/examples
[design]: https://github.com/uael/puns/blob/main/puns/DESIGN.md
[apache]: https://github.com/uael/puns/blob/main/LICENSE-APACHE
[mit]: https://github.com/uael/puns/blob/main/LICENSE-MIT
