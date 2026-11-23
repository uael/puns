# Describing a format

A binary format is a layout of fields, and relations between them: a header gives the length
of what follows, a byte gives the kind of a message, a checksum covers the data. With
`#[derive(Pun)]`, the fields of a struct describe the layout, and attributes describe the
relations. This chapter builds formats from these parts, one at a time. The
[derive][macro@Pun] lists every attribute with its conditions, and [The language of
rules][rules] specifies the expressions that the attributes take.

## A header and its data

A record of this format is a kind from 1 to 3, a big-endian length of at most 64, and that many
bytes. [`valid`][macro@Pun#valid] restricts the kind to its values, [`len`][macro@Pun#len]
gives the length of the trailing slice, and [`assert`][macro@Pun#assert] states the bound:

```rust
use puns::{Pun, num::u16be};

#[derive(Pun)]
#[repr(C)]
#[pun(assert(length <= 64))]
struct Record {
  #[pun(valid(1..=3))]
  kind: u8,
  length: u16be,
  #[pun(len = length)]
  data: [u8],
}

// The record ends after its 3 bytes of data, and `from_head` returns what follows it.
let (record, rest) = puns::from_head::<Record>(&[2, 0, 3, b'a', b'b', b'c', 9]).unwrap();
assert_eq!((record.kind, &record.data, rest), (2, &b"abc"[..], &[9][..]));

// A kind that is not one of its values.
assert_eq!(puns::from_head::<Record>(&[4, 0, 0]).err(), Some(puns::Error::Validity));
// A length over 64, though its 65 bytes are there.
let mut long = [b'x'; 3 + 65];
long[..3].copy_from_slice(&[1, 0, 65]);
assert_eq!(puns::from_head::<Record>(&long).err(), Some(puns::Error::Validity));
// Fewer bytes than the length says.
assert_eq!(puns::from_head::<Record>(&[2, 0, 3, b'a']).err(), Some(puns::Error::Size));
```

The expressions of `len` and `assert` are [rules][concepts#layouts-and-rules]. A record
has the length that its rule gives, so [`from_head`] can take it from the front of a stream. A
rule can also read bytes beyond the fields, such as a checksum over the data, as the packet of
the [crate page][crate#reading-a-packet] does.

`len` counts elements, not bytes. Two numbers of two bytes each are four bytes:

```rust
use puns::{Pun, num::u16be};

#[derive(Pun)]
#[repr(C)]
struct List {
  count: u8,
  #[pun(len = count)]
  numbers: [u16be],
}

let (list, after) = puns::from_head::<List>(&[2, 0, 1, 0, 2, 9]).unwrap();
assert_eq!((list.numbers.len(), after), (2, &[9][..]));
// Two numbers need four bytes after the count.
assert!(puns::from_bytes::<List>(&[2, 0, 1]).is_err());
```

## A header with options

Where the size of a value is not the length of its slice, [`size`][macro@Pun#size] gives its
[extent][concepts#layouts-and-rules] instead, and the slice takes the bytes that remain.
The low half of the first byte of this header is its length in words of 4 bytes, and what
follows the two fixed bytes is options:

```rust
use puns::Pun;

#[derive(Pun)]
#[repr(C)]
#[pun(size = (version_ihl & 0x0f) * 4)]
struct Header {
  version_ihl: u8,
  service: u8,
  options: [u8],
}

// A length of 2 words: 8 bytes, of which 6 are options.
let (header, payload) = puns::from_head::<Header>(&[0x42, 0, 1, 2, 3, 4, 5, 6, 0xaa]).unwrap();
assert_eq!((&header.options, payload), (&[1, 2, 3, 4, 5, 6][..], &[0xaa][..]));
```

## Bit fields and regions

A byte that holds several values is read by getters, which [`get`][macro@Pun#get] generates
from expressions on the fields. A field that holds several parts is split into
[regions][rules#regions] by [`regions`][macro@Pun#regions], whose methods return the
bytes of each part:

```rust
use puns::Pun;

#[derive(Pun)]
#[repr(C)]
#[pun(get(version: u8 = flags >> 4, urgent: bool = flags & 1 != 0))]
struct Packet {
  flags: u8,
  header_len: u8,
  #[pun(regions(header = header_len, body))]
  rest: [u8],
}

let packet = puns::from_bytes::<Packet>(&[0x21, 2, 7, 7, 1, 2, 3]).unwrap();
assert_eq!((packet.version(), packet.urgent()), (2, true));
assert_eq!((packet.header(), packet.body()), (&[7, 7][..], &[1, 2, 3][..]));
```

## A region with a type

A region with a type returns the region as a value of that type, if its bytes are one:

```rust
use puns::{Pun, num::u16be};

#[derive(Pun)]
#[repr(C)]
struct Point {
  x: u16be,
  #[pun(valid(..=1000))]
  y: u16be,
}

#[derive(Pun)]
#[repr(C)]
struct Shape {
  kind: u8,
  #[pun(regions(origin: Point = 4, rest))]
  data: [u8],
}

let shape = puns::from_bytes::<Shape>(&[1, 0, 2, 0, 3, 9]).unwrap();
let origin = shape.origin().unwrap();
assert_eq!((origin.x.get(), origin.y.get(), shape.rest()), (2, 3, &[9][..]));
// The bytes of `origin` are checked when it is read: a `y` over 1000 is not a `Point`.
let far = puns::from_bytes::<Shape>(&[1, 0, 2, 0xff, 0xff]).unwrap();
assert_eq!(far.origin().err(), Some(puns::Error::Validity));
```

## A byte order that the value gives

A field has the byte order of its type, and the numbers of [`num`] have a fixed one: `u32be`,
`i16le` and `f64be`. Some formats give their byte order in the value, as ELF does in `EI_DATA`
and TIFF in its first two bytes. Such a format has one type for each byte order, and a
[view][concepts#layouts-and-rules] chooses between them:

```rust
use puns::{Pun, num::{u16bec, u16lec, u32be, u32le}};

#[derive(Pun)]
#[repr(C)]
struct LittleTiff {
  order: u16bec<0x4949>, // "II"
  magic: u16lec<42>,
  first: u32le,
}

#[derive(Pun)]
#[repr(C)]
struct BigTiff {
  order: u16bec<0x4d4d>, // "MM"
  magic: u16bec<42>,
  first: u32be,
}

#[derive(Pun)]
enum Tiff<'a> {
  Little(&'a LittleTiff),
  Big(&'a BigTiff),
}

let header = [b'M', b'M', 0, 42, 0, 0, 0, 8];
assert!(matches!(Tiff::from_bytes(&header), Ok(Tiff::Big(tiff)) if tiff.first.get() == 8));
```

## A tag

A message often starts with a kind, which selects the type of what follows. Make the message
generic over its payload, with a kind of type `P::Tag`, and give each payload the values of the
kind that select it with [`tag`][macro@Pun#tag]. A view then reads a message of any kind:

```rust
use puns::{Pun, Tag, num::{u8c, u16be}};

/// The kind of a message, when its payload is not known.
#[derive(Pun, Clone, Copy)]
#[repr(transparent)]
struct Kind(u8);

#[derive(Pun)]
#[repr(C)]
struct Message<P: ?Sized + Tag<Kind> = [u8]> {
  kind: P::Tag,
  payload: P,
}

#[derive(Pun)]
#[repr(C)]
#[pun(tag(Kind = u8c<1>))]
struct Ping {
  sequence: u16be,
}

#[derive(Pun)]
enum Any<'a> {
  Ping(&'a Message<Ping>),
  Other(&'a Message),
}

let ping = [1, 0, 7];
assert!(matches!(Any::from_bytes(&ping), Ok(Any::Ping(m)) if m.payload.sequence.get() == 7));
assert!(matches!(Any::from_bytes(&[2, 0, 7]), Ok(Any::Other(m)) if m.kind.0 == 2));
```

Every `Message<Ping>` is also a `Message`, so the view checks a ping only for its kind.

## Reading again after a write

The attributes describe the bytes that `puns` reads. They are
[requirements][concepts#requirements], and the fields stay writable. So the bytes of a
value that [`as_bytes`] returns may fail the check of [`from_bytes`] for the same type, once a
write has broken a requirement:

```rust
use puns::{Pun, num::u16be};

#[derive(Pun)]
#[repr(C)]
struct Record {
  #[pun(valid(1..=3))]
  kind: u8,
  length: u16be,
  #[pun(len = length)]
  data: [u8],
}

let mut bytes = [2, 0, 3, b'a', b'b', b'c'];
let record = puns::from_bytes_mut::<Record>(&mut bytes).unwrap();
record.kind = 7;
assert!(puns::from_bytes::<Record>(puns::as_bytes(&*record)).is_err());
```

Check bytes again when they come from code that may have written them.

[`as_bytes`]: https://docs.rs/puns/0.2.3/puns/fn.as_bytes.html
[`from_bytes`]: https://docs.rs/puns/0.2.3/puns/fn.from_bytes.html
[`from_head`]: https://docs.rs/puns/0.2.3/puns/fn.from_head.html
[`num`]: https://docs.rs/puns/0.2.3/puns/num/index.html
[concepts#layouts-and-rules]: concepts.md#layouts-and-rules
[concepts#requirements]: concepts.md#requirements
[crate#reading-a-packet]: ../README.md#reading-a-packet
[macro@Pun]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html
[macro@Pun#assert]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html#assert
[macro@Pun#get]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html#get
[macro@Pun#len]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html#len
[macro@Pun#regions]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html#regions
[macro@Pun#size]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html#size
[macro@Pun#tag]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html#tag
[macro@Pun#valid]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html#valid
[rules]: rules.md
[rules#regions]: rules.md#regions
