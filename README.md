# puns

[![CI](https://github.com/uael/puns/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/uael/puns/actions/workflows/ci.yml?query=branch%3Amain)
[![docs.rs](https://img.shields.io/docsrs/puns/0.2.4)](https://docs.rs/puns/0.2.4)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns/0.2.4)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

> **Notes:** `puns` is written using AI, see [How `puns` is written](#how-puns-is-written). And
> the first release came out in November 2026: expect the API to change before 1.0.

Type punning proven at compile time: a safe alternative to `transmute` and to hand-written parsing
of bytes.

- [Overview](#overview)
  - [Reading a packet](#reading-a-packet)
  - [Reading one of several types](#reading-one-of-several-types)
  - [Reading an old version of a format](#reading-an-old-version-of-a-format)
  - [Going further](#going-further)
- [When to use `puns`](#when-to-use-puns)
  - [Compared with other crates](#compared-with-other-crates)
  - [Caveats](#caveats)
  - [Performance](#performance)
  - [Safety and testing](#safety-and-testing)
- [The guide][guide]
  - [How `puns` decides][concepts]
  - [Choosing a function][functions]
  - [Describing a format][formats]
  - [Compile-time errors]
  - [Unspecified layouts and the `unstable` feature][unstable]
  - [The language of rules][rules]
  - [Limits, and what is not done yet][limits]
  - [Measured performance][performance]
  - [Soundness]
  - [Testing][testing]
  - [What the derive trusts][trust]
  - [What `puns` relies on][rust]
  - [Rust version and targets][target]
- [About](#about)
  - [History](#history)
  - [How `puns` is written](#how-puns-is-written) (using AI)
  - [License](#license)

## Overview

`puns` lets you reinterpret a value of one type, the *source*, as a value of another type, the
*target*, or bytes as a typed value, without writing `unsafe`. `puns` is `no_std`, never allocates,
and needs Rust 1.99 or later (see [Minimum Rust version][msrv]). While your code compiles, it
compares the [layouts] of the two types, which include their [valid][valid] values. When it can
prove the whole conversion, nothing is left to check at run time:

```rust
use puns::Pun;

#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Switch {
  Off = 0,
  On = 1,
}

// Every `Switch` is 0 or 1, and both are valid `bool`s, so there's nothing to check.
let switches = [Switch::On, Switch::Off, Switch::On];
let bools: &[bool; 3] = puns::pun_ref(&switches);
assert_eq!(bools, &[true, false, true]);

// A byte is only a `Switch` if it's 0 or 1, so `from_bytes` checks each one.
assert_eq!(puns::from_bytes::<[Switch; 2]>(&[1, 0]), Ok(&[Switch::On, Switch::Off]));
assert_eq!(puns::from_bytes::<[Switch; 2]>(&[1, 2]), Err(puns::Error::Validity));
```

`pun_ref` returns the same memory with a new type, and `from_bytes` returns a `Result`. `puns` calls
these puns [*proven* and *checked*][kinds]. A pun that can be neither is *refused*, and doesn't
compile. If you give `Switch` a third variant with the value 2, some `Switch`es aren't `bool`s, and
the `pun_ref` call above stops with this error:

```text
cannot pun: some values of the source are not valid for the target, or do not prove one of its rules.
Use a `try_*` function to check them at run time.
This is at byte 0 of the target.
```

`try_pun_ref` would check the same conversion at run time instead. [Compile-time errors] explains
each message.

`cargo build` reports refusals, but `cargo check`, and therefore your editor, doesn't
([Compile-time errors] says why). Both report the errors of the derive and of types.

The three examples below show what `puns` is for. Going further, after them, lists the functions,
traits and modules.

### Reading a packet

Binary formats fix the byte order of their integers, while a `u16` has the byte order of the
machine. [`puns::num`][num] therefore has a type for each byte order: `u16be` is a big-endian `u16`,
`u16le` a little-endian one, and the same goes for integers of 24 to 128 bits, signed integers and
floats (`i32le`, `f64be`). Each one is a byte array with an alignment of 1. A `repr(C)` struct
made only of them and of bytes has no padding, and can be at any address in a buffer, unless
`align(..)` raises its alignment. `get` returns the value, and `new` makes one from a value.

Binary formats also usually store the length of their data and a checksum of it. With `puns`, you
write those relations on the struct as *rules*:

```rust
use puns::{Pun, num::{u16be, u32be}};

#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Kind {
  Ping = 1,
  Pong = 2,
}

// An 8-byte header, then the payload. The packet is `8 + length` bytes long, and `crc` is the
// CRC-32 of the payload, which is the trailing slice.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 8 + length)]
#[pun(assert(crc32(8, 8 + length) == crc))]
struct Packet {
  kind: Kind,
  ttl: u8,
  length: u16be,
  crc: u32be,
  payload: [u8],
}

// A ping carrying "abc", followed by one unrelated byte.
let bytes = [1, 64, 0, 3, 0x35, 0x24, 0x41, 0xc2, b'a', b'b', b'c', 0xff];

// `from_head` borrows a `&Packet` from the start of the bytes, and returns what follows it.
let (packet, rest) = puns::from_head::<Packet>(&bytes).unwrap();
assert_eq!(packet.kind, Kind::Ping);
assert_eq!(packet.length.get(), 3);
assert_eq!(&packet.payload, b"abc");
assert_eq!(rest, [0xff]);

// The same packet with a wrong CRC.
let wrong = [1, 64, 0, 3, 0, 0, 0, 0, b'a', b'b', b'c'];
assert_eq!(puns::from_head::<Packet>(&wrong).err(), Some(puns::Error::Validity));

// A router lowers the TTL in place. The CRC only covers the payload, so it stays valid.
let mut buffer = [1, 64, 0, 3, 0x35, 0x24, 0x41, 0xc2, b'a', b'b', b'c'];
let packet = puns::from_bytes_mut::<Packet>(&mut buffer).unwrap();
packet.ttl -= 1;
assert_eq!(buffer[1], 63);
```

Rules can also compute other checksums, read variable-length integers and `walk` a chain of
headers ([The language of rules][rules]). Other attributes cover bit fields, restricted values and
regions, and the [derive's documentation][derive] lists them all.

A rule is a [requirement][requirements] of the format, not a guarantee of the type: it holds when
`puns` reads the bytes, but the fields stay writable. If the router set `length` to 10, `payload`
would still have 3 bytes. `puns` never relies on a rule for memory safety: a rule that no longer
holds gives wrong values or a panic, never undefined behavior ([Soundness]).

`puns` [can't build][unsized] a new value of an unsized type like `Packet`. To send one, you write
its bytes yourself.

### Reading one of several types

When the data can be one of several types, derive `Pun` on an enum of references, which `puns` calls
a [*view*][view]. Its `from_bytes` and `from_head` return the first variant that the bytes match:

```rust
use puns::{Pun, num::{u8c, u16le}};

// A move: tag 1, then two coordinates.
#[derive(Pun)]
#[repr(C)]
struct Move {
  tag: u8c<1>,
  x: u16le,
  y: u16le,
}

// A chat line: tag 2, a length, then that many bytes of text.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 2 + length)]
struct Say {
  tag: u8c<2>,
  length: u8,
  text: str,
}

#[derive(Pun)]
enum Message<'a> {
  Move(&'a Move),
  Say(&'a Say),
}

// A move to (2, 3), then "hi".
let stream = [1, 2, 0, 3, 0, 2, 2, b'h', b'i'];
let (first, rest) = Message::from_head(&stream).unwrap();
assert!(matches!(first, Message::Move(m) if (m.x.get(), m.y.get()) == (2, 3)));
let (second, rest) = Message::from_head(rest).unwrap();
assert!(matches!(second, Message::Say(s) if &s.text == "hi"));
assert!(rest.is_empty());

// Tag 3 is neither.
assert!(Message::from_bytes(&[3]).is_err());
```

`u8c<1>` is a byte whose only valid value is 1, and `puns` skips a variant as soon as its tag
doesn't match. [`link.rs`][link] reads Bluetooth HCI packets this way.

### Reading an old version of a format

Here, every value of the old version of a format is also a valid value of the new one, and `puns`
proves it, so you can reinterpret old data as the new version without any check. Nothing is moved
or converted, which means that whatever the two versions share must keep the same layout:

```rust
use puns::{Pun, num::u16le};

// Version 1: each item is a 1-byte tag and 2 bytes of fields.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum OldItem {
  Sword { damage: u16le } = 0,
  Potion { heal: u8, cursed: bool } = 1,
}

// Version 2 keeps the tags and fields of version 1, and adds a key of the same size.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum Item {
  Sword { damage: u16le } = 0,
  Potion { heal: u8, cursed: bool } = 1,
  Key { floor: u8, door: u8 } = 2,
}

// A version 1 file: a sword that deals 300 damage, and a potion that heals 25.
let file = [0, 0x2c, 0x01, 1, 25, 0];
let old: &[OldItem] = puns::from_bytes(&file).unwrap();

// Proven: an `OldItem` is always a valid `Item`.
let items: &[Item] = puns::pun_ref(old);
assert_eq!(items[1], Item::Potion { heal: 25, cursed: false });
```

If version 2 ever drops a variant or narrows a field, this stops compiling. (`pun_mut` won't accept
it, since you could then write a key into an `OldItem`.) `puns` can't check what a variant means,
because it matches variants by their tag. If version 2 gave tag 1 to the key, this would still
compile and reinterpret the old potion as a key, so keep the tags of existing variants.

### Going further

The functions of `puns` are in this table. Each row says what is read, a typed value or bytes, and
whether the function checks at run time. Each column says how much is read: the whole, the front,
or the back.

|                                  | Whole                              | Front                          | Back                           |
|----------------------------------|------------------------------------|--------------------------------|--------------------------------|
| a typed value, checking nothing  | [`pun`][pun], [`pun_ref`][pun_ref], [`pun_mut`][pun_mut], [`as_bytes`][as_bytes] | [`head`][head], [`head_ref`][head_ref], [`head_mut`][head_mut] | [`tail`][tail], [`tail_ref`][tail_ref], [`tail_mut`][tail_mut] |
| a typed value, checking          | [`try_pun`][try_pun], [`try_pun_ref`][try_pun_ref], [`try_pun_mut`][try_pun_mut] | | |
| bytes, checking                  | [`from_bytes`][from_bytes], [`from_bytes_mut`][from_bytes_mut] | [`from_head`][from_head], [`from_head_mut`][from_head_mut] | [`from_tail`][from_tail], [`from_tail_mut`][from_tail_mut] |

Two functions make no pun. [`zeroed`][zeroed] returns a value whose fields are all zero bytes.
[`validate_but_never`][validate_but_never] checks bytes for a type that holds [opaque
scalars][opaque], but makes no reference: the crate that declares these scalars proves them itself,
and then makes the reference.

- [`Pun`][trait-pun] is the trait of the types that `puns` works with: numbers and other scalars,
  arrays, slices, `str`, tuples and other types of `core`, and the types that derive it. Its page
  has the full list.
- [`#[derive(Pun)]`][derive] implements `Pun`, and also describes what the bytes of a value say
  about it: its size, the length of its trailing slice, and conditions such as a checksum.
- [`Scalar`][Scalar] is the trait of a type that is one scalar value, such as a number whose
  constructor checks it. [`Tag`][Tag] is the trait of a type carried next to a tag, such as the
  payload of a message, and gives the values of the tag that select it.
- [`Error`][error-type] says which check failed: the alignment, the size, or the values.
- [`num`][num] has numbers of a fixed byte order, and integers restricted to some values.
- [`layout`][layout] has the types that describe a layout, and the checksums.
- The [guide] explains how `puns` decides, teaches the attributes and the rules from examples, and
  says what its soundness rests on.

The tests and the [examples] of the repository show more uses. The examples read parts of more
than 40 formats, from TLS and DNS to ELF and SQLite, and each one says which parts of its format it
doesn't cover, and why. The shortest, [`media.rs`][media], reads WAVE files and MP4 boxes. Run it
with `cargo run --example media` from a clone of the repository.

## When to use `puns`

### Compared with other crates

Use `puns` to read binary formats in place, or to reinterpret plain data as another type, when you
want the compiler to prove what it can. Use [`zerocopy`][zerocopy] or [`bytemuck`][bytemuck] for
what `puns` doesn't do, for an older Rust, or when you need a crate that has been reviewed for
years. To decode data into owned values, [`binrw`][binrw], [`deku`][deku] or [`nom`][nom] are a
better fit.

`puns` takes many ideas and implementation details from [`zerocopy`][zerocopy] and
[`bytemuck`][bytemuck]. Their traits describe what each type accepts on its own, while `puns`
compares the two types of each conversion. Read on 2026-11-23, with `zerocopy` 0.8.64 and
`bytemuck` 1.25.2, except the machine code of the first row, which was compiled with `zerocopy`
0.8.63:

| | `puns` | `zerocopy` and `bytemuck` |
|---|---|---|
| `&[Switch]` as `&[bool]` | free ([code and machine code][comparison]) | checks every byte, and returns a `Result` |
| A checksum | declared on the type, and checked by `from_bytes` | computed and compared by your code |
| The rules of one type prove those of another | [yes][pairs] | no |
| Errors | refusals, which your editor doesn't report | mostly trait bounds, which your editor reports right away |
| `Vec` | [not yet][not-yet] | `bytemuck` casts a `Vec` |
| Atomics | [not decided][not-decided] | `zerocopy` handles atomics |
| Rust | [1.99][msrv] or later | 1.56 or later (`zerocopy`), 1.34 or later without features (`bytemuck`) |

Rust's own Safe Transmute project compares the two types as well, inside the compiler, with
[`TransmuteFrom`][transmute-from], but it's only on nightly.

### Caveats

- Deriving `Pun` bypasses your constructors. Any code can then make a value of the type from bytes
  and read or write its fields, private ones included: don't derive it on a type with a private
  invariant ([details][deriving]).
- Without a `repr`, the compiler is free to change the layout of a type, and `puns` refuses
  anything that depends on it unless you turn on the [`unstable` feature][unstable]. Even
  `repr(C)` only places the fields in order, each at the next offset that its alignment allows:
  the byte order of a `u32` and the alignment of a `u64` still vary between targets, and so do the
  offsets that follow from them. For data that leaves the program, use the integers of
  [`puns::num`][num], like `u32be`.
- In a release build, the error may not name the line of your code. A debug build names it.
- A generic function that calls `pun_ref` compiles fine on its own, and only fails when you call
  it with two types that don't pun. The error then points inside the generic function.
- A checksum inside a `walk` multiplies the cost of a check by the size of the data, and each
  extra level of nesting multiplies it again. Limit the size of untrusted input for such types
  ([details][costs]).
- An [`Error`][error-type] doesn't tell you which field or rule failed.

### Performance

Checked puns take about as long as the same checks written by hand, and what you pay for is
mostly build time. The times of the checks and of the builds in [Performance][performance] were
measured on 2026-11-21 at commit `4176671`, and the chapter also shows the machine code and how
each was measured. The chart below shows the time of each check relative to the same check written
by hand.

[![Time of each check by puns, relative to the same check written by hand](https://raw.githubusercontent.com/uael/puns/v0.2.4/docs/charts/checks.svg)][performance]

### Safety and testing

`puns` is built on `unsafe` code so that yours doesn't have to be: crates that use it, derive
included, can keep `#![forbid(unsafe_code)]`. The [Soundness] chapter of the guide states what a
pun guarantees, and [What `puns` relies on][rust] what that code relies on, including behaviors of
`rustc` that no documentation guarantees, and how each is checked. On every pair of test types of
the same size, the analysis is compared with oracles that don't use it, and the tests run under
Miri, on 32-bit and big-endian targets as well. Programs that must fail to compile are tested with
their expected messages ([Testing][testing]). If you find a soundness bug, please [open an
issue][issues].

## About

### History

I started this crate in November 2023 under the name `mutant`. I was working on Bluetooth at the
time, which involves a lot of packing and unpacking of protocol data, and I was impressed by how
much crates like [`typic`][typic] or [`typenum`][typenum] do with the type system. Stable Rust had
just gained more compile-time evaluation and inline `const` was about to be stabilized, so I wanted
to see how far that could go.

But by then compile-time evaluation was too limited, types were worse, and after many prototypes
that mixed the two I stopped working on the project for a long time.

Recent stabilizations and the progress of AI models made me come back to it. These are the
stabilizations that `puns` could not do without:

- `&raw const` (Rust 1.82), which gives the place of a field without a macro whose path the user's
  crate could redefine
- `size_of_val_raw` and `align_of_val_raw` in `const` (1.99), which give the layout of unsized types

`mutant` was taken, so the crate is now `puns`, and the [changelog] lists everything since.

### How `puns` is written

The current code, tests and documentation of `puns` were all written using AI (Claude models),
under my supervision, based on prototypes I wrote by hand between 2023 and 2025. The [`wip`][wip]
branch keeps those prototypes for history.

I haven't read every line, and I don't claim to understand every part. I understand the
architecture and the design. What I trust is the process: the code is checked by tools that don't
depend on the model that wrote it ([Safety and testing](#safety-and-testing)). I use `puns` myself
because I trust these checks. That's my own judgement, and yours may differ.

`puns` is published as it is, and I work on it alongside other projects. If people find it useful,
I hope more of it gets reviewed by people over time, and that some of them help maintain it.
Reviews, questions and tests that break it are all welcome. If that doesn't happen, `puns` stays as
it is, and this section will say so.

### License

Licensed under either of [Apache License, Version 2.0][apache] or [MIT license][mit] at your
option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
`puns` by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.

[valid]: docs/concepts.md#what-is-a-valid-value
[kinds]: docs/concepts.md#proven-checked-and-refused
[pairs]: docs/concepts.md#why-the-pair-decides
[requirements]: docs/concepts.md#requirements
[opaque]: docs/concepts.md#opaque-scalars
[layouts]: docs/concepts.md#layouts-and-rules
[unsized]: docs/limits.md#limits-of-rust
[msrv]: docs/target.md#minimum-rust-version
[rules]: docs/rules.md
[performance]: docs/performance.md
[comparison]: docs/performance.md#compared-with-zerocopy-and-bytemuck
[costs]: docs/performance.md#costs
[rust]: docs/rust.md
[guide]: docs/README.md
[concepts]: docs/concepts.md
[functions]: docs/functions.md
[formats]: docs/formats.md
[limits]: docs/limits.md
[trust]: docs/trust.md
[target]: docs/target.md
[not-decided]: docs/limits.md#not-decided
[not-yet]: docs/limits.md#not-yet
[Compile-time errors]: docs/errors.md
[Soundness]: docs/soundness.md
[testing]: docs/testing.md
[deriving]: docs/soundness.md#what-deriving-pun-means
[view]: https://docs.rs/puns/0.2.4/puns/derive.Pun.html#views
[unstable]: docs/layouts.md
[num]: https://docs.rs/puns/0.2.4/puns/num/index.html
[layout]: https://docs.rs/puns/0.2.4/puns/layout/index.html
[derive]: https://docs.rs/puns/0.2.4/puns/derive.Pun.html
[trait-pun]: https://docs.rs/puns/0.2.4/puns/trait.Pun.html
[Scalar]: https://docs.rs/puns/0.2.4/puns/trait.Scalar.html
[Tag]: https://docs.rs/puns/0.2.4/puns/trait.Tag.html
[error-type]: https://docs.rs/puns/0.2.4/puns/enum.Error.html
[pun]: https://docs.rs/puns/0.2.4/puns/fn.pun.html
[pun_ref]: https://docs.rs/puns/0.2.4/puns/fn.pun_ref.html
[pun_mut]: https://docs.rs/puns/0.2.4/puns/fn.pun_mut.html
[as_bytes]: https://docs.rs/puns/0.2.4/puns/fn.as_bytes.html
[try_pun]: https://docs.rs/puns/0.2.4/puns/fn.try_pun.html
[try_pun_ref]: https://docs.rs/puns/0.2.4/puns/fn.try_pun_ref.html
[try_pun_mut]: https://docs.rs/puns/0.2.4/puns/fn.try_pun_mut.html
[head]: https://docs.rs/puns/0.2.4/puns/fn.head.html
[head_ref]: https://docs.rs/puns/0.2.4/puns/fn.head_ref.html
[head_mut]: https://docs.rs/puns/0.2.4/puns/fn.head_mut.html
[tail]: https://docs.rs/puns/0.2.4/puns/fn.tail.html
[tail_ref]: https://docs.rs/puns/0.2.4/puns/fn.tail_ref.html
[tail_mut]: https://docs.rs/puns/0.2.4/puns/fn.tail_mut.html
[from_bytes]: https://docs.rs/puns/0.2.4/puns/fn.from_bytes.html
[from_bytes_mut]: https://docs.rs/puns/0.2.4/puns/fn.from_bytes_mut.html
[from_head]: https://docs.rs/puns/0.2.4/puns/fn.from_head.html
[from_head_mut]: https://docs.rs/puns/0.2.4/puns/fn.from_head_mut.html
[from_tail]: https://docs.rs/puns/0.2.4/puns/fn.from_tail.html
[from_tail_mut]: https://docs.rs/puns/0.2.4/puns/fn.from_tail_mut.html
[zeroed]: https://docs.rs/puns/0.2.4/puns/fn.zeroed.html
[validate_but_never]: https://docs.rs/puns/0.2.4/puns/fn.validate_but_never.html
[changelog]: https://github.com/uael/puns/blob/v0.2.4/CHANGELOG.md
[examples]: https://github.com/uael/puns/tree/v0.2.4/puns/examples
[media]: https://github.com/uael/puns/blob/v0.2.4/puns/examples/media.rs
[link]: https://github.com/uael/puns/blob/v0.2.4/puns/examples/link.rs
[issues]: https://github.com/uael/puns/issues
[wip]: https://github.com/uael/puns/tree/wip
[apache]: https://github.com/uael/puns/blob/v0.2.4/LICENSE-APACHE
[mit]: https://github.com/uael/puns/blob/v0.2.4/LICENSE-MIT
[zerocopy]: https://crates.io/crates/zerocopy
[bytemuck]: https://crates.io/crates/bytemuck
[binrw]: https://crates.io/crates/binrw
[deku]: https://crates.io/crates/deku
[nom]: https://crates.io/crates/nom
[typic]: https://github.com/jswrenn/typic
[typenum]: https://github.com/paholg/typenum
[transmute-from]: https://doc.rust-lang.org/nightly/core/mem/trait.TransmuteFrom.html
