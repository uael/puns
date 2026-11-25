# The API

Choose a function by what it reads, a typed value or bytes, and by how much of it: the whole, the
front or the back ([the table of functions][crate#going-further]). The choice also decides whether
a check at run time is possible. [`pun`], [`pun_ref`], [`pun_mut`], [`as_bytes`], [`head`],
[`tail`] and their `_ref` and `_mut` forms check nothing, and refuse a pair that needs a check. The
`try_` and `from_` functions check at run time what the analysis leaves to them ([Proven, checked
and refused][concepts#proven-checked-and-refused]).

## Reinterpreting a value

For a value that already has a type, [`pun_ref`] and [`pun_mut`] return a reference to the same
bytes, and [`pun`] returns a copy. Where one of them is refused because some values of the source
are not valid targets, its `try_` form does the same, and checks those values at run time. [`pun`],
[`try_pun`], [`head`] and [`tail`] take and return values, and require both types to be `Copy`:

```rust
// Reinterpret an `f32` as its bits. A `u32` is a `char` only if it is a Unicode scalar value.
assert_eq!(puns::pun::<f32, u32>(1.0), 0x3f80_0000);
assert_eq!(puns::try_pun::<u32, char>(0xe9), Ok('é'));
assert_eq!(puns::try_pun::<u32, char>(0xd800), Err(puns::Error::Validity));
```

## Reading bytes

Read bytes from outside the program, such as a file or a packet, with the `from_` functions. Read
bytes that already have a type that proves the target with [`pun_ref`] instead.

[`from_bytes`], [`from_bytes_mut`] and [`as_bytes`] are `try_pun_ref::<[u8], D>`,
`try_pun_mut::<[u8], D>` and `pun_ref::<S, [u8]>` with a single type parameter. Prefer them to
the `try_` functions when you read bytes: `try_pun_ref(&[1, 0])` infers its source as an
`[i32; 2]`, because an integer literal without a suffix is an `i32` unless something says
otherwise.

[`from_head`] and [`from_head_mut`] take a value from the front of the bytes, whose length the
`size` or `len` rule of the target gives ([`from_head`] says how many bytes it takes), and return
the bytes that follow:

```rust
use puns::Pun;

/// A string of UTF-8, after its length in one byte.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 1 + length)]
struct Text {
  length: u8,
  text: str,
}

// Read strings from the front of a stream, until the bytes are not one.
let mut stream: &[u8] = b"\x05hello\x00\x05world\xff";
let mut words = vec![];
while let Ok((word, rest)) = puns::from_head::<Text>(stream) {
  words.push(&word.text);
  stream = rest;
}
assert_eq!((words, stream), (vec!["hello", "", "world"], &[0xff][..]));
```

[`from_tail`] and [`from_tail_mut`] take a sized value from the end of bytes, and return the bytes
before it:

```rust
use puns::num::u32le;

// Read a checksum from the back of a file.
let file = [b'd', b'a', b't', b'a', 0x78, 0x56, 0x34, 0x12];
let (data, checksum) = puns::from_tail::<u32le>(&file).unwrap();
assert_eq!((data, checksum.get()), (&b"data"[..], 0x1234_5678));
```

The `from_` functions check the address only if the target needs an alignment greater than 1.
[`from_bytes`] and [`from_bytes_mut`] check the address, then the size. [`from_head`] and
[`from_head_mut`] check that the bytes hold the fields of the target before its slice, then the
address, then the extent that its rules give. [`from_tail`] and [`from_tail_mut`] check the size
first, since it decides where the last bytes start, then the address. All of them then check the
values of the target that the bytes do not prove, requirements included, and the pointees of its
eager [aims][aims] in the same bytes, a few levels deep. [`check`] and [`check_compact`] check data
whose pointees lead deeper, with a list of the pointees still to check that you give. A view has a
`from_bytes` and a `from_head` of its own ([Reading one of several
types][crate#reading-one-of-several-types]).

## Taking the front or the back

[`head`], [`head_ref`] and [`head_mut`] take the front of a typed value, and [`tail`],
[`tail_ref`] and [`tail_mut`] its back. For a sized target, the front or the back is the first
or last `size_of::<D>()` bytes. If the target of [`head_ref`] or [`head_mut`] is a slice, it
holds as many elements as fit. The functions of the back need a [sized source and
target](#sizes). Each byte order keeps the low half of a number at one end:

```rust
use puns::num::{u16be, u32be, u32le};

// The low half of a big-endian number is its last two bytes.
assert_eq!(puns::tail::<u32be, u16be>(0x1234_5678.into()).get(), 0x5678);
assert_eq!(puns::head::<u32le, u8>(0x1234_5678.into()), 0x78);
```

If you read the size of the front from the bytes, or the front needs a check, read the bytes with
[`from_head`] instead of [`head_ref`].

## Writing through `&mut`

A function that returns a `&mut D` also needs the pun to hold in the other direction: every `D`
that may be written through the reference must leave a valid source ([Writes leave a valid
source](#writes-leave-a-valid-source)).

The analysis compares what may be written with the source scalar by scalar. A `#[pun(valid(..))]`
range is part of the layout of its scalar, so it is compared like any value, and a write must stay
in it. A rule is an expression on the whole struct, which this comparison does not read. So
[`pun_mut`] and [`try_pun_mut`] let a write break a rule of the source, as a write to a field may,
since a rule is a [requirement][concepts#requirements]. To check the value again, pass its bytes,
from [`as_bytes`], to [`from_bytes`]. [`head_mut`] and [`tail_mut`] are stricter about rules
([Writes leave a valid source](#writes-leave-a-valid-source)). A more precise analysis is [not
done yet][limits#not-yet].

Where [`pun_mut`] is refused because a `D` written through it could leave an invalid source, take
a `&mut` to only the bytes that you write, with [`head_mut`] or [`tail_mut`]. A `&mut [u8; 4]` to
this note could write a 2 over its `bool`:

```rust,compile_fail,E0080
use puns::Pun;

/// A note: whether it is urgent, then three bytes of text.
#[derive(Pun)]
#[repr(C)]
struct Note {
  urgent: bool,
  text: [u8; 3],
}

let mut note = Note { urgent: true, text: *b"abc" };
let bytes: &mut [u8; 4] = puns::pun_mut(&mut note); // Refused.
```

```text
cannot pun: some values of the target are not valid for the source (or are padding where the source is not), and could be written through the `&mut`.
```

A `&mut` to the last three bytes leaves the `bool` as it is:

```rust
use puns::Pun;

/// A note: whether it is urgent, then three bytes of text.
#[derive(Pun)]
#[repr(C)]
struct Note {
  urgent: bool,
  text: [u8; 3],
}

let mut note = Note { urgent: true, text: *b"abc" };
puns::tail_mut::<_, [u8; 3]>(&mut note).make_ascii_uppercase();
assert_eq!((note.urgent, &note.text), (true, b"ABC"));
```

## In a constant

Every function of `puns` is a `const fn`, and so is every function that the derive generates, so you
can check bytes in a constant while the program compiles. Only the implementations of standard
traits, such as `From` and `Display`, are not ([Limits of Rust][limits#limits-of-rust]). `unwrap` is
not `const` either: match on the result instead.

A check of an address does not compile in a constant, because `core` calls reading an address as an
integer undefined behavior in const evaluation, and says that using the integer stops the evaluation
([`transmute`][core::mem::transmute#transmutation-between-pointers-and-integers]). A checked
function therefore compiles in a constant only where it checks no address:

- [`try_pun_ref`] and [`try_pun_mut`] where `D` is no more aligned than `S`;
- [`from_bytes`] and [`from_bytes_mut`] where `D` has an alignment of 1;
- [`from_head`] and [`from_head_mut`] where `D` has an alignment of 1, or where the bytes are too
  short for the fields of `D` before its slice, since this is checked before the address;
- [`from_tail`] and [`from_tail_mut`] where `D` has an alignment of 1, or where the bytes are
  shorter than a `D`, for the same reason.

The save of a game, whose target has an alignment of 1, is read in a constant:

```rust
use puns::{
  Pun,
  num::{u16le, u32bec},
};

#[derive(Pun)]
#[repr(C)]
#[pun(assert(hp <= level * 10))]
struct Save {
  magic: u32bec<0x5341_5645>,
  version: u8,
  #[pun(valid(1..=99))]
  level: u8,
  hp: u16le,
  bag: [u8],
}

// A new game starts from a save in the binary, which the build checks.
const NEW_GAME: &Save = match puns::from_bytes(b"SAVE\x02\x01\x0a\x00") {
  Ok(save) => save,
  Err(_) => panic!("the built-in save is invalid"),
};
assert_eq!((NEW_GAME.level, NEW_GAME.hp.get()), (1, 10));
```

With 11 hp at level 1, the rule does not hold, and the build fails:

```rust,compile_fail,E0080
use puns::{Pun, num::{u16le, u32bec}};
#[derive(Pun)]
#[repr(C)]
#[pun(assert(hp <= level * 10))]
struct Save {
  magic: u32bec<0x5341_5645>,
  version: u8,
  #[pun(valid(1..=99))]
  level: u8,
  hp: u16le,
  bag: [u8],
}
const NEW_GAME: &Save = match puns::from_bytes(b"SAVE\x02\x01\x0b\x00") {
  Ok(save) => save,
  Err(_) => panic!("the built-in save is invalid"),
};
let _ = NEW_GAME;
```

```text
error[E0080]: evaluation panicked: the built-in save is invalid
```

## The conditions of the functions

The page of each function lists, under "Compile-time errors", the conditions under which a call
compiles. These are checked after monomorphization ([Compile-time errors][errors]), and those that
several functions share are defined here. The *covered bytes* are the bytes of a value of `S` that
the target takes: all of them for [`pun`] and the other functions of a whole value, and the first or
last bytes for [`head`], [`tail`] and their `_ref` and `_mut` forms, as each page says.

### Sizes

- *Same extent*: whatever the length of its slice, every value of `S` has an
  [extent][concepts#layouts-and-rules] that a `D` can have. For sized types,
  `size_of::<S>() == size_of::<D>()`.
- *An extent in common*: some length of the slice of `S` gives it an extent that a `D` can have.
  The check at run time tells whether the value has such an extent, and whether the `D` of that
  extent, with the padding that may follow its slice, fits in the value. For sized types,
  `size_of::<S>() == size_of::<D>()`.
- *Room for the target*: every value of `S` has an extent at least as large as that of the
  smallest `D`. For sized types, `size_of::<S>() >= size_of::<D>()`.
- *Sized source and target*: `S` and `D` are sized, because where the last bytes of an unsized
  value start is known only at run time, and an unsized target has no size to count from the end
  of `S`. [`from_tail`] takes a value from the end of bytes.

### Alignment

`D` needs no greater alignment than `S`, so that a reference to a `D` at the address of an `S` is
aligned. The `try_` and `from_` functions check the address at run time instead, except in a
constant ([In a constant](#in-a-constant)).

### Initialized bytes

Every byte that `D` requires to be initialized is initialized in the covered bytes of every value
of `S`.

### Proven target

In every value of `S`, the covered bytes are a [valid][concepts#what-is-a-valid-value] `D`, and
`S` proves the [requirements][concepts#requirements] of `D` ([Why the pair
decides][concepts#why-the-pair-decides]).

### Accepted by a check

Each [part][concepts#layouts-and-rules] of `D` accepts what some value of `S` holds at its
place: each scalar outside an enum or a union, some variant of each enum, and some field of each
union. The check at run time then tests that the bytes are a valid `D` and meet the requirements
of `D`. A pair where no value of `S` is a valid `D` is refused rather than checked, so `D` is
inhabited if `S` is. The analysis does not solve rules, so a pair whose rules no value meets, such
as `assert(a > 255)` on a `u8`, compiles, and its check always fails.

### Writes leave a valid source

Any `D` may be written through a `&mut D`, so what is written must leave a valid source that
meets the `valid(..)` ranges of the source, though not always its rules ([Writing through
`&mut`](#writing-through-mut)).

- For [`pun_mut`] and [`try_pun_mut`], every value of `D` is such an `S`. In particular, every
  byte that `S` requires to be initialized is initialized in every value of `D`.
- For [`head_mut`] and [`tail_mut`], every value of `D`, written over the covered bytes, leaves
  such an `S`, whatever its other bytes hold. Moreover, a struct of `S` that has a rule and
  overlaps the covered bytes must lie within them, be in no enum, and have the same rules at the
  same place in `D`. Each enum of `S` lies wholly within the covered bytes or wholly outside
  them, since its tag tells what its other bytes may hold. For [`head_mut`], this holds
  whatever the length of the slice of `S`, and if `D` ends with a slice whose extent may differ
  from that of `S`, `S` holds no enum with fields, union or `str`, since the end of `D` could fall
  within one.
- For [`from_bytes_mut`], [`from_head_mut`] and [`from_tail_mut`], every byte of every value of
  `D` is initialized, as bytes are.

### No eager aim

`D` holds no eager [aim][aims], since no function makes a value that holds one: its pointee was
checked where the value was. For a `&mut`, `S` holds none either: no function gives a `&mut` to
or from a value that holds one, since a write could move it away from its pointee. A reference to
a value that holds one is made by the functions that return a `&D`, which check its pointee, or
prove it by the same aim at the same place of the source.

### Bytes for a pointee

If `D` holds an eager [aim][aims], every byte of every value of `S` is initialized, since the
check looks for its pointee in the bytes of the source.

[`as_bytes`]: https://docs.rs/puns/0.2.4/puns/fn.as_bytes.html
[`check_compact`]: https://docs.rs/puns/0.2.4/puns/fn.check_compact.html
[`check`]: https://docs.rs/puns/0.2.4/puns/fn.check.html
[`from_bytes_mut`]: https://docs.rs/puns/0.2.4/puns/fn.from_bytes_mut.html
[`from_bytes`]: https://docs.rs/puns/0.2.4/puns/fn.from_bytes.html
[`from_head_mut`]: https://docs.rs/puns/0.2.4/puns/fn.from_head_mut.html
[`from_head`]: https://docs.rs/puns/0.2.4/puns/fn.from_head.html
[`from_tail_mut`]: https://docs.rs/puns/0.2.4/puns/fn.from_tail_mut.html
[`from_tail`]: https://docs.rs/puns/0.2.4/puns/fn.from_tail.html
[`head_mut`]: https://docs.rs/puns/0.2.4/puns/fn.head_mut.html
[`head_ref`]: https://docs.rs/puns/0.2.4/puns/fn.head_ref.html
[`head`]: https://docs.rs/puns/0.2.4/puns/fn.head.html
[`pun_mut`]: https://docs.rs/puns/0.2.4/puns/fn.pun_mut.html
[`pun_ref`]: https://docs.rs/puns/0.2.4/puns/fn.pun_ref.html
[`pun`]: https://docs.rs/puns/0.2.4/puns/fn.pun.html
[`tail_mut`]: https://docs.rs/puns/0.2.4/puns/fn.tail_mut.html
[`tail_ref`]: https://docs.rs/puns/0.2.4/puns/fn.tail_ref.html
[`tail`]: https://docs.rs/puns/0.2.4/puns/fn.tail.html
[`try_pun_mut`]: https://docs.rs/puns/0.2.4/puns/fn.try_pun_mut.html
[`try_pun_ref`]: https://docs.rs/puns/0.2.4/puns/fn.try_pun_ref.html
[`try_pun`]: https://docs.rs/puns/0.2.4/puns/fn.try_pun.html
[aims]: aims.md
[concepts#layouts-and-rules]: concepts.md#layouts-and-rules
[concepts#proven-checked-and-refused]: concepts.md#proven-checked-and-refused
[concepts#requirements]: concepts.md#requirements
[concepts#what-is-a-valid-value]: concepts.md#what-is-a-valid-value
[concepts#why-the-pair-decides]: concepts.md#why-the-pair-decides
[core::mem::transmute#transmutation-between-pointers-and-integers]: https://doc.rust-lang.org/1.99.0/core/mem/fn.transmute.html#transmutation-between-pointers-and-integers
[crate#going-further]: ../README.md#going-further
[crate#reading-one-of-several-types]: ../README.md#reading-one-of-several-types
[errors]: errors.md
[limits#limits-of-rust]: limits.md#limits-of-rust
[limits#not-yet]: limits.md#not-yet
