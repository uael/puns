# puns

[![CI](https://github.com/uael/puns/workflows/CI/badge.svg)](https://github.com/uael/puns/actions)
[![docs.rs](https://img.shields.io/docsrs/puns/0.2.0-beta.2)](https://docs.rs/puns/0.2.0-beta.2)
[![crates.io](https://img.shields.io/crates/v/puns)](https://crates.io/crates/puns/0.2.0-beta.2)
[![license](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue)](#license)

Type punning proven at compile time: a safe alternative to `transmute` and to hand-written parsing
of bytes.

`puns` lets you reinterpret a value as another type, or bytes as a typed value, without writing
`unsafe`. It compares the layouts and valid values of the two types while your code compiles, and
whatever it can prove, it doesn't check at run time:

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

The first conversion costs nothing at run time. Every `Switch` is already a valid `bool`, so
`pun_ref` returns the same memory with a new type. The second can't be proven, because a byte can
hold any value, so `from_bytes` checks each one and returns a `Result`. `puns` calls these *proven*
and *checked* puns. A conversion that could never be valid, or that would read padding bytes, is
*refused*, and doesn't compile.

If you give `Switch` a third variant with the value 2, the `pun_ref` call above stops compiling:

```text
cannot pun: some values of the source are not valid for the target, or do not prove one of its rules.
Use a `try_*` function to check them at run time.
This is at byte 0 of the target.
```

Switching to `try_pun_ref` moves the check to run time. `cargo build` reports these errors, but
`cargo check` and your editor mostly don't, because the comparison happens while the compiler
generates code.

```sh
cargo add puns
```

`puns` is `no_std`, never allocates, and needs Rust 1.99 or later ([why][msrv]). The first release
came out in November 2026, so expect the API to change before 1.0.

## Reading a packet

Binary formats usually store the length of their data and a checksum of it. With `puns`, you write
those relations on the struct as *rules*, and `from_bytes` and the other `from_` functions check
them along with the fields:

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

`u16be` and `u32be` are big-endian integers with an alignment of 1, which is why `Packet` has no
padding and can be at any address in a buffer. Rules can also compute other checksums, read
variable-length integers and `walk` a chain of headers. Other attributes cover bit fields,
restricted values and regions, and the [derive's documentation][derive] lists them all.

A rule describes the format, and Rust knows nothing of it. It's checked when the bytes are read, and
nothing keeps it true afterwards. If the router had set `length` to 10, the payload would still have
3 bytes, and a later pun to a type with the same rule would trust the old check. `puns` doesn't
build new values of an unsized type like `Packet`, so to send one, you write its bytes yourself.
`as_bytes` gives you the bytes of any value.

When the data can be one of several types, derive `Pun` on an enum of references, which `puns` calls
a *view*. Its `from_bytes` returns the first variant that the bytes match, and [`link.rs`][link]
reads Bluetooth HCI packets this way.

`from_bytes`, `from_head` and `from_tail` are `const fn`, so you can check bytes at compile time.
`unwrap` isn't `const`, so [match on the result][const] there instead.

## Reading an old version of a format

If every value of an old version of a format is also a valid value of the new one, `puns` can
prove it, and you can reinterpret old data as the new version without any check. Nothing is moved or
converted, so whatever the two versions share must keep the same layout:

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

## Compared with other crates

`puns` takes many ideas and implementation details from [`zerocopy`][zerocopy] and
[`bytemuck`][bytemuck]. Their traits describe what each type accepts on its own. A `bool` doesn't
accept every byte, so with them, reinterpreting a `&[Switch]` as a `&[bool]` checks every byte and
returns a `Result`. With `puns`, it's free ([code and machine code][comparison]). Both can check
conditions like a checksum at run time, but neither uses the rules of one type to prove those of
another.

They also do things that `puns` doesn't: `bytemuck` casts a `Vec`, `zerocopy` handles atomics, and
their errors are mostly trait bounds that your editor reports right away. Use them for these, for an
older Rust, or when you need a crate that has been reviewed for years. To decode data into owned
values, [`binrw`][binrw], [`deku`][deku] or [`nom`][nom] are a better fit. Rust's own [Safe
Transmute][safe transmute] project compares the two types as well, inside the compiler, but it's
only on nightly.

## Caveats

- Deriving `Pun` bypasses your constructors. Any code can then make a value of the type from bytes
  and read or write its fields, private ones included, so don't derive it on a type with a private
  invariant ([details][deriving]).
- Without a `repr`, the compiler is free to change the layout of a type, and `puns` refuses
  anything that depends on it unless you turn on the [`unstable` feature][unstable]. Even
  `repr(C)` only fixes the order of the fields: the byte order of a `u32` and the alignment of a
  `u64` still vary between targets, so for data that leaves the program, use the integers of
  [`puns::num`][num], like `u32be`.
- In a release build, the error doesn't name the line of your code, so build in debug to find it.
  [Compile-time errors] explains the messages.
- A generic function that calls `pun_ref` compiles fine on its own, and only fails when you call
  it with two types that don't pun. The error then points inside the generic function.
- A checksum inside a `walk` multiplies the cost of a check by the size of the data, and each
  extra level of nesting multiplies it again. Limit the size of untrusted input for such types
  ([details][costs]).
- An [`Error`][error-type] doesn't tell you which field or rule failed.

## Performance

A proven pun compiles to no check at all, and checked puns are about as fast as the same checks
written by hand: on an Apple M4 Max, checking Ethernet, IPv4, IPv6, UDP and TCP packets takes
between half and 1.1 times as long. What you pay for is mostly build time: about 17 ms per struct
in a debug build, and up to 25 s in release for a view of 64 overlapping variants.
[`docs/performance.md`][performance] has the code, the method and the other cases.

![Time of each check by puns, relative to the same check written by hand](https://raw.githubusercontent.com/uael/puns/v0.2.0-beta.2/docs/charts/checks.svg)

## Safety and testing

`puns` is built on `unsafe` code so that yours doesn't have to be: crates that use it, derive
included, can keep `#![forbid(unsafe_code)]`. The [Soundness] section lists everything that code
relies on, including three behaviors of `rustc` that no documentation guarantees, and how each
is checked.

The analysis is compared with hand-written oracles on every pair of test types of the same size,
and the tests run under Miri, on 32-bit and big-endian targets as well. More than 100 programs must
fail to compile, each with its expected message ([`docs/testing.md`][testing]). If you find a
soundness bug, please [open an issue][issues].

## Learn more

The [reference][api] has a table of every function, and [Limitations] lists what `puns` can't do
yet. The [examples] read parts of more than 40 formats, from TLS and DNS to ELF and SQLite. The
shortest one, [`media.rs`][media], reads WAVE files and MP4 boxes. Run it with
`cargo run --example media` from a clone of the repository.

## History

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

## License

Licensed under either of [Apache License, Version 2.0][apache] or [MIT license][mit] at your
option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
`puns` by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.

[msrv]: https://docs.rs/puns/0.2.0-beta.2/puns/#minimum-rust-version
[api]: https://docs.rs/puns/0.2.0-beta.2/puns/#reference
[Limitations]: https://docs.rs/puns/0.2.0-beta.2/puns/#limitations
[num]: https://docs.rs/puns/0.2.0-beta.2/puns/num/index.html
[error-type]: https://docs.rs/puns/0.2.0-beta.2/puns/enum.Error.html
[derive]: https://docs.rs/puns/0.2.0-beta.2/puns/derive.Pun.html
[Soundness]: https://docs.rs/puns/0.2.0-beta.2/puns/#soundness
[deriving]: https://docs.rs/puns/0.2.0-beta.2/puns/#what-deriving-pun-means
[Compile-time errors]: https://docs.rs/puns/0.2.0-beta.2/puns/#compile-time-errors
[const]: https://docs.rs/puns/0.2.0-beta.2/puns/#using-the-functions
[unstable]: https://docs.rs/puns/0.2.0-beta.2/puns/#the-unstable-feature
[performance]: https://github.com/uael/puns/blob/v0.2.0-beta.2/docs/performance.md
[comparison]: https://github.com/uael/puns/blob/v0.2.0-beta.2/docs/performance.md#compared-with-zerocopy-and-bytemuck
[costs]: https://github.com/uael/puns/blob/v0.2.0-beta.2/docs/performance.md#costs
[testing]: https://github.com/uael/puns/blob/v0.2.0-beta.2/docs/testing.md
[changelog]: https://github.com/uael/puns/blob/v0.2.0-beta.2/CHANGELOG.md
[examples]: https://github.com/uael/puns/tree/v0.2.0-beta.2/puns/examples
[media]: https://github.com/uael/puns/blob/v0.2.0-beta.2/puns/examples/media.rs
[link]: https://github.com/uael/puns/blob/v0.2.0-beta.2/puns/examples/link.rs
[issues]: https://github.com/uael/puns/issues
[apache]: https://github.com/uael/puns/blob/v0.2.0-beta.2/LICENSE-APACHE
[mit]: https://github.com/uael/puns/blob/v0.2.0-beta.2/LICENSE-MIT
[zerocopy]: https://crates.io/crates/zerocopy
[bytemuck]: https://crates.io/crates/bytemuck
[binrw]: https://crates.io/crates/binrw
[deku]: https://crates.io/crates/deku
[nom]: https://crates.io/crates/nom
[typic]: https://github.com/jswrenn/typic
[typenum]: https://github.com/paholg/typenum
[safe transmute]: https://rust-lang.github.io/rfcs/2835-project-safe-transmute.html
