# puns

[![CI](https://github.com/uael/puns/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/uael/puns/actions/workflows/ci.yml?query=branch%3Amain)
[![docs.rs](https://img.shields.io/docsrs/puns/0.2.4)](https://docs.rs/puns/0.2.4)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns/0.2.4)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

> **Notes:** `puns` is written using AI, see [How `puns` is written](#how-puns-is-written). And
> the first release came out in November 2026: expect the API to change before 1.0.

Type punning proven at compile time: a safe alternative to `transmute` and to hand-written parsing
of bytes.

- [Overview](#overview): [Reading a packet](#reading-a-packet), [Reading one of several
  types](#reading-one-of-several-types), [Reading an old version of a
  format](#reading-an-old-version-of-a-format)
- [When to use `puns`](#when-to-use-puns): [Compared with other
  crates](#compared-with-other-crates), [Caveats](#caveats), [Performance](#performance), [Safety
  and testing](#safety-and-testing)
- [Going further](#going-further): [Functions and types](#functions-and-types), [The
  reference](#the-reference), [Examples](#examples)
- [About](#about): [History](#history), [How `puns` is written](#how-puns-is-written) (using AI),
  [License](#license)

## Overview

`puns` lets you reinterpret a value of one type, the *source*, as a value of another type, the
*target*, or bytes as a typed value, without writing `unsafe`. `puns` is a `no_std` crate that
never allocates, and it needs Rust 1.99 or later ([why][msrv]). While your code compiles, `puns`
compares the [layouts] of the two types, which include their [valid] values. When it can prove the
whole conversion, there's nothing left to check at run time:

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
compile. Give `Switch` a third variant with the value 2, and some `Switch`es aren't `bool`s
anymore, so the `pun_ref` call above stops with this error:

```text
cannot pun: some values of the source are not valid for the target, or do not prove one of its rules.
Use a `try_*` function to check them at run time.
This is at byte 0 of the target.
```

`try_pun_ref` would check the same conversion at run time instead. `cargo build` reports
refusals, but `cargo check`, and so your editor, doesn't. [Compile-time errors] says why, and
explains each message.

### Reading a packet

Binary formats fix the byte order of their integers, while a `u16` has the byte order of the
machine. So [`puns::num`][num] has a type for each byte order, such as `u16be` for a big-endian
`u16`, and `get` returns its value. These types have an alignment of 1, so a `repr(C)` struct made
of them and of bytes has no padding, and can be at any address in a buffer.

Formats also store the length of their data, and often a checksum of it. You write those relations
on the struct as *rules*:

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

A rule is a [requirement][requirements] of the format, not a guarantee of the type. `puns` checks
it when it reads bytes, but the fields stay writable: if the router set `length` to 10, `payload`
would still have 3 bytes. `puns` never relies on a rule for memory safety, so a rule that no
longer holds gives wrong values or a panic, never undefined behavior ([Soundness]).

Rules can also compute other checksums, read variable-length integers and `walk` a chain of
headers ([The language of rules][rules]). The [derive's page][derive] lists every attribute.

### Reading one of several types

When the data can be one of several types, derive `Pun` on an enum of references, which `puns`
calls a [*view*][view]. Its `from_bytes` and `from_head` return the first variant that the bytes
match:

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

`u8c<1>` is a byte whose only valid value is 1, so `puns` skips a variant as soon as its tag
doesn't match. [`link.rs`][link] reads Bluetooth HCI packets this way.

### Reading an old version of a format

When every value of the old version of a format is also a valid value of the new one, `puns`
proves it, and you can reinterpret old data as the new version without any check. Nothing is moved
or converted, so whatever the two versions share must keep the same layout:

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

If version 2 ever drops a variant or narrows a field, this stops compiling. But `puns` matches
variants only by their tag: if version 2 gave tag 1 to the key, this would still compile, and read
the old potion as a key. So keep the tags of the variants you already have.

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
  and read or write its fields, private ones included, so don't derive it on a type with a private
  invariant ([details][deriving]).
- Without a `repr`, the compiler chooses the layout of a type, and `puns` refuses a pun that
  depends on that choice unless you turn on the [`unstable` feature][unstable]. Even with
  `repr(C)`, the byte order of a `u32` and the alignment of a `u64` vary between targets, and so do
  the offsets of the fields that follow. For data that leaves the program, use the types of
  [`puns::num`][num], like `u32be`.
- A checksum inside a `walk` multiplies the cost of a check by the size of the data, and each
  extra level of nesting multiplies it again. Limit the size of untrusted input for such types
  ([details][costs]).
- An [`Error`][error-type] doesn't tell you which field or rule failed.

### Performance

A checked pun takes about as long as the same check written by hand, and what you pay for is
mostly build time. The chart shows the time of each check relative to the same check by hand,
measured on 2026-11-21 at commit `4176671`. [Measured performance][performance] gives the machine,
how each time was measured, the machine code and the build times.

[![Time of each check by puns, relative to the same check written by hand](https://raw.githubusercontent.com/uael/puns/v0.2.4/docs/charts/checks.svg)][performance]

### Safety and testing

`puns` is built on `unsafe` code so that yours doesn't have to be: crates that use it, derive
included, can keep `#![forbid(unsafe_code)]`. [Soundness] states what a pun guarantees, and [What
`puns` relies on][rust] lists what its code relies on, including behaviors of `rustc` that no
documentation guarantees, and how each one is checked. The analysis is compared with oracles that
don't use it on every pair of test types of the same size, and the tests run under Miri, on 32-bit
and big-endian targets as well ([Testing][testing]). If you find a soundness bug, please [open an
issue][issues].

## Going further

### Functions and types

Each row of this table says what a function reads, a typed value or bytes, and whether it checks
at run time. Each column says how much it reads: the whole, the front, or the back.

| | Whole | Front | Back |
|---|---|---|---|
| A typed value, checking nothing | [`pun`], [`pun_ref`], [`pun_mut`], [`as_bytes`] | [`head`], [`head_ref`], [`head_mut`] | [`tail`], [`tail_ref`], [`tail_mut`] |
| A typed value, checking | [`try_pun`], [`try_pun_ref`], [`try_pun_mut`] | | |
| Bytes, checking | [`from_bytes`], [`from_bytes_mut`] | [`from_head`], [`from_head_mut`] | [`from_tail`], [`from_tail_mut`] |

- [`Pun`][trait-pun] is the trait of the types that `puns` works with: numbers and other scalars,
  arrays, slices, `str`, tuples and other types of `core`, and the types that derive it.
- [`#[derive(Pun)]`][derive] implements `Pun`, and also describes what the bytes of a value say
  about it: its size, the length of its trailing slice, and conditions such as a checksum.
- [`Scalar`][Scalar] is the trait of a type that is one scalar value, such as a number whose
  constructor checks it. [`Tag`][Tag] is the trait of a type carried next to a tag, such as the
  payload of a message.
- [`Error`][error-type] says which check failed: the alignment, the size, or the values.
- [`num`][num] has numbers of a fixed byte order, and integers restricted to some values.
- [`layout`][layout] has the types that describe a layout, and the checksums.

### The reference

The chapters of the [reference], in reading order:

| Chapter | What it holds |
|---|---|
| [How `puns` decides][concepts] | The terms: pun, layout, rule, valid value, requirement, the three kinds of pun, and why the pair of types decides. |
| [The API][functions] | Which function to use, reading bytes, the front and the back, writing through `&mut`, constants, and the conditions of the functions. |
| [In practice][formats] | Describing a binary format with the derive, one relation at a time. |
| [Compile-time errors] | How to read an error, and each message with the code that causes it and its fix. |
| [Unspecified layouts and the `unstable` feature][unstable] | What Rust leaves to the compiler, what `puns` refuses without the feature, and when to turn it on. |
| [The language of rules][rules] | The expressions of rules: fields, values, functions, walks, and how a rule fails. |
| [Limits, and what is not done yet][limits] | What Rust prevents, the bounds of the analysis, what is planned, and what is not decided. |
| [Measured performance][performance] | Machine code, the time of checks next to checks by hand, build times, and costs. |
| [Soundness] | What a pun guarantees, padding, unions, uninhabited types, and what deriving `Pun` means. |
| [Testing][testing] | The model check, Miri, the record of verdicts, the compile-fail tests, the tests of the derive, and the commands that run them. |
| [What the derive trusts][trust] | What the generated code relies on, and what the deriving crate can't redirect. |
| [What `puns` relies on][rust] | What Rust documents, and what no documentation states, each with what checks it. |
| [Rust version and targets][target] | Why Rust 1.99, and what the code assumes of the target. |

### Examples

The tests and the [examples] of the repository show more uses. The examples read parts of more
than 40 formats, from TLS and DNS to ELF and SQLite, and each one says which parts of its format it
doesn't cover, and why. The shortest, [`media.rs`][media], reads WAVE files and MP4 boxes. Run it
with `cargo run --example media` from a clone of the repository.

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
[layouts]: docs/concepts.md#layouts-and-rules
[msrv]: docs/target.md#minimum-rust-version
[rules]: docs/rules.md
[performance]: docs/performance.md
[comparison]: docs/performance.md#compared-with-zerocopy-and-bytemuck
[costs]: docs/performance.md#costs
[rust]: docs/rust.md
[reference]: docs
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
[`pun`]: https://docs.rs/puns/0.2.4/puns/fn.pun.html
[`pun_ref`]: https://docs.rs/puns/0.2.4/puns/fn.pun_ref.html
[`pun_mut`]: https://docs.rs/puns/0.2.4/puns/fn.pun_mut.html
[`as_bytes`]: https://docs.rs/puns/0.2.4/puns/fn.as_bytes.html
[`try_pun`]: https://docs.rs/puns/0.2.4/puns/fn.try_pun.html
[`try_pun_ref`]: https://docs.rs/puns/0.2.4/puns/fn.try_pun_ref.html
[`try_pun_mut`]: https://docs.rs/puns/0.2.4/puns/fn.try_pun_mut.html
[`head`]: https://docs.rs/puns/0.2.4/puns/fn.head.html
[`head_ref`]: https://docs.rs/puns/0.2.4/puns/fn.head_ref.html
[`head_mut`]: https://docs.rs/puns/0.2.4/puns/fn.head_mut.html
[`tail`]: https://docs.rs/puns/0.2.4/puns/fn.tail.html
[`tail_ref`]: https://docs.rs/puns/0.2.4/puns/fn.tail_ref.html
[`tail_mut`]: https://docs.rs/puns/0.2.4/puns/fn.tail_mut.html
[`from_bytes`]: https://docs.rs/puns/0.2.4/puns/fn.from_bytes.html
[`from_bytes_mut`]: https://docs.rs/puns/0.2.4/puns/fn.from_bytes_mut.html
[`from_head`]: https://docs.rs/puns/0.2.4/puns/fn.from_head.html
[`from_head_mut`]: https://docs.rs/puns/0.2.4/puns/fn.from_head_mut.html
[`from_tail`]: https://docs.rs/puns/0.2.4/puns/fn.from_tail.html
[`from_tail_mut`]: https://docs.rs/puns/0.2.4/puns/fn.from_tail_mut.html
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
