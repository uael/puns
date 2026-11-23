# The language of rules

The [rules][concepts#layouts-and-rules] of a struct are the expressions of its
[`size`][macro@Pun#size] and [`assert`][macro@Pun#assert] attributes, and of [`len`][macro@Pun#len]
and [`regions`][macro@Pun#regions] on its last field. They give the [extent](#extent) of a value,
the length of its trailing slice, the size of a [region](#regions), or a condition such as a
checksum, the relations that binary formats store in their headers. [Describing a format][formats]
shows them in use.

A rule is a [requirement][concepts#requirements]. It is checked when a value of the
struct is made from another type, as by [`try_pun`] or [`from_head`], unless the source proves
it ([Why the pair decides][concepts#why-the-pair-decides]). A `size` or `len` rule also
tells `from_head` how long a value is.

A rule is compiled to a constant program of [`layout::Op`]s over a stack of values and a set of
variables, which a `const fn` can evaluate, and which costs what its arithmetic costs. What a
rule computes is built in, and described by data where it varies (walks, variable-length
integers, CRCs, sums), so a rule calls no function by its name alone but [those
below](#functions), and no method. A path to a `const fn`, as `self::f(3)`, is a
[constant](#literals-constants-and-operators).

## Extent

Unlike the [extent][concepts#layouts-and-rules] of a value, [`size_of_val`] counts the padding that
may follow its trailing slice. When a function returns a reference to the bytes of its argument, and
the target ends with a slice, the length of that slice is chosen so that the result covers the
extent of the argument.

## Regions

The methods of [regions][concepts#layouts-and-rules] ([`regions`][macro@Pun#regions]) do not trust
the fields that give the sizes, which are [requirements][concepts#requirements]. A region that does
not fit in the field is truncated, and a region whose expression fails takes the rest of the field.

## Values and types

A rule means what the same Rust expression would mean if every integer were a `u128`, or it is
refused. It is typed as Rust types it: arithmetic is on integers, the operands of `&&` and `||`
and the condition of an `if` are `bool`s, an `assert` is a `bool`, and a `size`, a `len` or a
region is an integer. Rust does not convert an integer to a `bool`, so `flags & 1` is not an
assertion, but `flags & 1 != 0` is.

## Fields in a rule

A rule reads the fields of its struct that are unsigned integers (`u8`, `u16be`,
`NonZero<u32>`...), or sized structs whose only field with a size is one of these and is as
large as the struct. A field is named by its name, or as `self.name` (`self.0` in a tuple
struct). A rule does not name the fields of a field or the elements of an array, but reads them
with `le` and `be`.

A signed or floating-point number, a `bool`, a `char` or an enum is refused, since its bits are
not its value as an unsigned integer, and so is an [opaque
scalar][concepts#unspecified-layouts-opaque-scalars-and-uninhabited-types], which is not
a number:

```rust,compile_fail,E0080
use puns::Pun;

#[derive(Pun)]
#[repr(C)]
#[pun(assert(level < 8))] // Refused: the bits of an `i8` are not its value as a `u128`.
struct Level {
  level: i8,
}
let _ = puns::from_bytes::<Level>(&[1]);
```

```text
error[E0080]: evaluation panicked: a rule reads the unsigned integer fields of its struct, and this field is not one (a signed or floating-point number, a `bool`, a `char` or an enum is not): read its bytes with `le(offset, size)` or `be(offset, size)`
```

A rule that reads bytes at an offset reads the bytes of such a field like any others. To fix
this rule, read the bits of `level` as a byte, in which the negative levels are 128 to 255:

```rust
use puns::Pun;

#[derive(Pun)]
#[repr(C)]
#[pun(assert(be(0, 1) < 8))]
struct Level {
  level: i8,
}

assert!(puns::from_bytes::<Level>(&[7]).is_ok());
assert!(puns::from_bytes::<Level>(&(-1i8).to_ne_bytes()).is_err());
```

`size_of_val(self)` is the extent of the value in bytes. A `size` or `len` rule, which defines
the extent, does not read it, and the derive reports one that does.

## Literals, constants and operators

Besides its fields, a rule is made of:

- integer literals, with no suffix or the suffix of an unsigned type (`1u8`), and byte literals
  (`b'a'`). Floating-point numbers, negative numbers, signed integers and `char`s are refused,
  since they would mean something else.
- constants of an unsigned integer type, which Rust computes: `MAX_LEN`, `u16::MAX`, a const
  parameter `N`, `size_of::<Header>()`, `offset_of!(Self, field)`, or `self::f(3)` for a `const
  fn f`.
- the operators `+ - * / % & | ^ << >> == != < <= > >= && || !`. On an integer, `!` is a bitwise
  not (`!3` is `0xffff..fffc`). On a `bool`, it is a logical not.
- casts: `as u8`, `as u16`, `as u32`, `as u64` and `as usize` keep the low bits, with the width
  of `usize` on the target, `as u128` keeps them all, and `true as u8` is `1`. A cast to another
  type is refused.
- blocks that name values, `{ let a = EXPR; let b = EXPR; EXPR }`. `let name: u128` and `let
  name: bool` state the type.
- choices, `if EXPR { EXPR } else { EXPR }`.
- the [functions](#functions).

## How a rule fails

Where Rust would panic, the rule fails, which means that it does not hold: on an overflow of `+`,
`-` or `*`, a difference below zero included, a division or remainder by zero, a shift by 128 or
more, and a read or a range outside the value. `<<` drops the bits it shifts out, as in Rust, so
`(1 << 127) << 1` is `0`.

As in Rust, the right side of `&&` and `||` is computed only if the left side does not decide,
and the branch of an `if` that is not taken is not computed. What is not computed cannot fail,
and costs nothing. So `kind != 7 || be(40, 2) == 0` holds for a value that is not of kind 7,
however short, while `be(40, 2) == 0 || kind != 7` fails for one that is too short. Also as in
Rust, a `let` is computed whether its name is used or not, and a block fails if one of its
`let`s fails. Everything that is computed counts, so the first failure is the failure of the
expression, and the evaluation stops there:

```rust
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(C)]
// Compiled to `Load` of `end`, `Load` of `start`, `Sub`, `Lit(2)`, `Ge`.
#[pun(assert(end - start >= 2))]
struct Span {
  start: u8,
  end: u8,
}

assert!(puns::try_pun::<_, Span>([1u8, 3]).is_ok());
assert!(puns::try_pun::<_, Span>([1u8, 2]).is_err());
// `end - start` would overflow: where Rust panics, the rule fails.
assert!(puns::try_pun::<_, Span>([3u8, 1]).is_err());
```

## Reading the bytes of the value

A rule can read the bytes of the value beyond its fields, with the [functions](#functions), whose
`start`, `end` and `offset` count bytes from the start of the value. A `size` or `len` rule can read
them too, and [`from_head`] evaluates it over the bytes that are there. An MQTT packet is
`size = 1 + leb128_len(1) + leb128(1)`, and a WebSocket frame from a server, which is not masked,
gives its length in 7, 16 or 64 bits ([`transport.rs`] adds the key of a masked frame):

```rust
use puns::Pun;

#[derive(Pun)]
#[repr(C)]
#[pun(size = 2 + if length & 0x7f == 126 {
  2 + be(2, 2)
} else if length & 0x7f == 127 {
  8 + be(2, 8)
} else {
  length & 0x7f
})]
struct Frame {
  head: u8,
  length: u8,
  rest: [u8],
}

// A frame of 2 bytes of text, then the first byte of the next frame.
let (frame, next) = puns::from_head::<Frame>(&[0x81, 2, b'h', b'i', 0x81]).unwrap();
assert_eq!((&frame.rest, next), (&b"hi"[..], &[0x81][..]));
// A length of 126 says that the length is in the next two bytes.
let mut long = [0; 4 + 300];
long[..4].copy_from_slice(&[0x82, 126, 1, 44]);
assert_eq!(puns::from_head::<Frame>(&long).unwrap().0.rest.len(), 2 + 300);
```

`from_head` then validates the value that it takes as a whole, rule included, over the bytes of
that value only, so a rule that reads beyond what it sizes fails.

A trailer is therefore a region, and "the last 4 bytes are the CRC of what comes before" is an
assertion. A checksum that covers several layers is an assertion of the struct that holds them
all: that of UDP and TCP covers the addresses of the IP header, so it is one of the IPv4 packet.

A rule that reads bytes by offset (`le`, `be`, a variable-length integer, a checksum, a CRC,
`matches`, or a `walk` whose expressions do) reads the padding of its struct as well. It is
checked when the source does not prove it, and with what it shares a check with: in a variant
of an enum checked as a whole, in an element of an array of which another element is checked,
and where every part of the target is checked. The pun is then refused if its struct has a byte
that may be padding in the source.

## Functions

- `le(offset, N)` and `be(offset, N)`: the unsigned integer of `N` bytes at `offset`,
  little-endian or big-endian. `N` is a constant from 1 to 16.
- `leb128(offset)`, `quic(offset)` and `vlq(offset)`: the variable-length integer at `offset`.
  LEB128 has 7 bits per byte, least significant first (Protocol Buffers, WebAssembly, MQTT).
  QUIC (RFC 9000) has 1, 2, 4 or 8 bytes, as its two high bits say. VLQ is big-endian base 128
  (ASN.1 tag numbers and object identifiers). `leb128_len(offset)`, `quic_len(offset)` and
  `vlq_len(offset)` give its length in bytes. They fail if the integer does not end within the
  value, or does not fit in a `u128`. A longer encoding than needed is read as it is, and a rule
  such as `leb128_len(1) <= 4` can refuse it. LEB128 is read up to 19 bytes, the most that a
  `u128` needs, and a longer one fails even if its value fits.
- `crc32(start, end)`: the Ethernet CRC-32 of the bytes from `start` to `end`, as
  [`crc32`][fn@crc32] computes it.
- `crc::<WIDTH, POLY, INIT, REFLECT, XOROUT>(start, end)`: a CRC of 1 to 64 bits whose input and
  output are reflected alike, given by its parameters in the catalogue of parametrised CRC
  algorithms (see [`layout::Crc`]). A CRC that reflects only one of them, such as CRC-12/UMTS, has
  no such parameters. The other parameters fit in `WIDTH` bits. `crc::<16, 0x8005, 0xffff, true, 0>`
  is the Modbus CRC, and `crc::<32, 0x1edc6f41, 0xffffffff, true, 0xffffffff>` is CRC-32C.
- `crc32(start, end, from, to)` and `crc::<..>(start, end, from, to)`: the same, with the bytes
  from `from` to `to` read as zeros, for a CRC over the header that holds it (GPT).
- `sum16(start, end)`: the ones' complement sum of the 16-bit words from `start` to `end`, as
  [`sum16`][fn@sum16] computes it. It is
  `0xffff` over data that holds its own IP checksum.
- `fletcher16(start, end)`: Fletcher-16. It is `0` over data that holds its own ISO 8473
  checksum, as in OSPF and IS-IS.
- `adler32(start, end)`: Adler-32.
- `sum(start, end)`: the sum of the bytes (tar).
- `matches(DFA, start, end)`: whether an automaton accepts the bytes from `start` to `end`. `DFA` is
  a `&'static` [`layout::Dfa`]: `Dfa::ASCII`, `Dfa::DIGITS`, `Dfa::OCTAL`, `Dfa::HEX`, `Dfa::UTF8`,
  `Dfa::UTF8_NO_NUL`, or one of your own. The automaton has 1 to 64 states, and the edges of each
  state are sorted and disjoint. `matches` states a pattern that the struct requires of its bytes,
  and like any rule, it proves nothing about them. A type that guarantees a pattern, as `str` does,
  cannot be derived, since safe code can write its fields.
- `walk(START, |at| COND, |at| NEXT)`: see [Walks](#walks).

## Walks

`walk(START, |at| COND, |at| NEXT)` is what `START` becomes when it is replaced by `NEXT` for as
long as `COND` holds. A walk reaches something at an offset that depends on what comes before
it: after options, at the end of a chain of headers, or at the end of a string. The offset of
the first "type, length, value" whose type is 0 is `walk(0, |at| be(at, 1) != 0, |at| at + 2 +
be(at + 1, 1))`. A walk may be in any rule or in another walk, and one expression may hold
several. It fails if `COND` still holds after its last step, or if `COND` or `NEXT` fails.

A walk takes at most `8 * size + 128` steps, one for each bit of the `size` bytes that it may
read and of a `u128`, and fails beyond that. `size` is `size_of_val(self)`, or, for a `size` or
`len` rule, the number of bytes from the start of the value to the end of the input, which are
then the bytes it reads. So a walk always ends, and a walk over the bits of a field fits any
value. The size of a zero-terminated string is a walk:

```rust
use puns::Pun;

#[derive(Pun)]
#[repr(C)]
#[pun(size = walk(0, |at| be(at, 1) != 0, |at| at + 1) + 1)]
struct Name {
  text: [u8],
}

let (name, rest) = puns::from_head::<Name>(b"abc\0def").unwrap();
assert_eq!((&name.text, rest), (&b"abc\0"[..], &b"def"[..]));
// No zero: the walk reads past the end of the bytes, and fails.
assert!(puns::from_head::<Name>(b"abc").is_err());
```

The state of a walk is one `u128`. A walk that carries two numbers, such as the kind of a header
and where it starts, packs them into it. Pack them at bit 64, as `(kind << 64) + start`: on a
64-bit target the halves are then two registers, and `at >> 64` and `at & 0xffff_ffff_ffff_ffff`
one each, while `kind + (start << 8)` needs a shift across registers to read `start` and a merge
to write it, at each step. [`ethernet.rs`] checks the IPv6 checksum behind any chain of
extension headers this way, and [`codegen.rs`] shows its machine code.

Each operation is computed in a register where the range of its values allows, with the result
it has on a `u128`, and an addition to a low half that cannot carry is computed in that half. A
range comes from how a value is computed and from the reads that succeeded, not from a
comparison that held. So the condition of a walk does not narrow its step, nor that of an `if`
its branches: the states of the walks of `Font` in [`images.rs`], a power of two and its
exponent, are bounded only by their conditions, and computed on 128 bits.

## Limits of the evaluator

The validation applies each operation in a function of its own, reached by recursion over a tree
of the operations, which the first two limits keep shallow. The evaluator holds each value of
its stack and each name in a field of its own, so that the optimizer keeps them in registers,
which the last two limits keep few. An expression has:

- at most 256 operations;
- at most 4 walks nested in each other;
- a stack of at most 16 values. `a + (b + (c + d))` needs one more for each level, 4 in all,
  which a `let` or another order saves: `a + b + c + d` needs 2;
- at most 8 names at once, its `let`s and the parameters of its walks.

The derive reports an expression past one of these limits at the place of the fault, with the
numbers. A walk takes at most `8 * size + 128` steps ([Walks](#walks)).

[`codegen.rs`]: https://github.com/uael/puns/blob/v0.2.3/puns/examples/codegen.rs
[`ethernet.rs`]: https://github.com/uael/puns/blob/v0.2.3/puns/examples/ethernet.rs
[`images.rs`]: https://github.com/uael/puns/blob/v0.2.3/puns/examples/images.rs
[`transport.rs`]: https://github.com/uael/puns/blob/v0.2.3/puns/examples/transport.rs

[`from_head`]: https://docs.rs/puns/0.2.3/puns/fn.from_head.html
[`layout::Crc`]: https://docs.rs/puns/0.2.3/puns/layout/struct.Crc.html
[`layout::Dfa`]: https://docs.rs/puns/0.2.3/puns/layout/struct.Dfa.html
[`layout::Op`]: https://docs.rs/puns/0.2.3/puns/layout/enum.Op.html
[`size_of_val`]: https://doc.rust-lang.org/1.100.0/core/mem/fn.size_of_val.html
[`try_pun`]: https://docs.rs/puns/0.2.3/puns/fn.try_pun.html
[concepts#layouts-and-rules]: concepts.md#layouts-and-rules
[concepts#requirements]: concepts.md#requirements
[concepts#unspecified-layouts-opaque-scalars-and-uninhabited-types]: concepts.md#unspecified-layouts-opaque-scalars-and-uninhabited-types
[concepts#why-the-pair-decides]: concepts.md#why-the-pair-decides
[fn@crc32]: https://docs.rs/puns/0.2.3/puns/layout/fn.crc32.html
[fn@sum16]: https://docs.rs/puns/0.2.3/puns/layout/fn.sum16.html
[formats]: formats.md
[macro@Pun#assert]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html#assert
[macro@Pun#len]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html#len
[macro@Pun#regions]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html#regions
[macro@Pun#size]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html#size
