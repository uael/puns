# Compile-time errors

A pun that `puns` [refuses][concepts#proven-checked-and-refused] does not compile. Most refusals are
panics of `const` blocks in the generic functions of `puns` (error E0080), which `rustc` evaluates
only when it generates code for an instance of the function, after monomorphization. `cargo check`
and editors generate no code, so they do not report them, while `cargo build` does. A generic
function that calls `puns` therefore compiles until it is instantiated with a pun that is refused,
and the error then points inside it. The conditions would be trait bounds, which `cargo check`
reports, if stable Rust could bound the value of a constant expression, as `generic_const_exprs`
does on nightly ([Minimum Rust version][msrv]).

## Reading an error

Reinterpreting a struct with padding as bytes is a common mistake with `transmute`. `puns`
refuses it, and the first lines of the error say why:

```rust,compile_fail,E0080
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
...
note: the above error was encountered while instantiating `fn pun_ref::<Header, [u8; 8]>`
 --> src/main.rs:9:25
  |
9 |   let bytes: &[u8; 8] = puns::pun_ref(&Header { kind: 1, length: 2 });
  |                         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

Where `u32` has alignment 4, byte 1 of the target is the first of the three bytes of padding after
`kind`. The last note gives the line of the call, because each function evaluates its constant in
its own body, for the types of that call. Between the first lines and that note, `rustc` 1.100
prints 33 lines that locate the failed constant inside `puns`, which can be skipped.

An optimized build may inline the function into its caller before it evaluates the constant.
The error then has no last note: it names the types, as in `Plan::<Header, [u8; 8]>`, but not
the line of the call. A debug build, such as the one `cargo build` makes, names it.

To fix this pun, declare the padding as a field, so that it is initialized:

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

## Messages

The message of a refused pun starts with `cannot pun:` and says why. When the cause is at one byte,
the message names that byte of the target. A message names only the first cause that `puns` finds,
in this order: an [eager aim](#eager-aims) that the function cannot keep, the sizes, an
unspecified layout, padding, no value of the source valid for the target, a check that the
function cannot make, then the alignment. For a `&mut`, what a write leaves
comes last. Fixing one cause may therefore reveal the next.

### Sizes

A pun of a whole value needs a target of the same size:

```rust,compile_fail,E0080
let n: u32 = puns::pun([1u8, 2, 3]);
```

```text
cannot pun: the source is smaller than the target.
```

A larger source gives another message:

```rust,compile_fail,E0080
let pair: [u8; 2] = puns::pun([1u8, 2, 3]);
```

```text
cannot pun: the source is larger than the target.
Use `head` or `tail` (or their `_ref` and `_mut` forms) to take the first or last bytes of the source.
```

```rust
assert_eq!(puns::head::<[u8; 3], [u8; 2]>([1, 2, 3]), [1, 2]);
```

For a target that ends with a slice:

```rust,compile_fail,E0080
let pairs: &[[u8; 2]] = puns::pun_ref(&[1u8, 2, 3]);
```

```text
cannot pun: the size of the source cannot be reached with elements of the trailing slice of the target.
Use `head` or `tail` (or their `_ref` and `_mut` forms) to take the first or last bytes of the source.
```

```rust
let pairs: &[[u8; 2]] = puns::head_ref(&[1u8, 2, 3]);
assert_eq!(pairs, &[[1, 2]]);
```

For a source that ends with a slice:

```rust,compile_fail,E0080
let bytes: &[u8] = &[1, 2];
let pair: &[u8; 2] = puns::pun_ref(bytes);
```

```text
cannot pun: the size of the source may not be a size of the target, depending on the length of its slice.
Use `try_pun_ref` or `try_pun_mut` to check it at run time.
```

```rust
assert_eq!(puns::try_pun_ref::<[u8], [u8; 2]>(&[1, 2]), Ok(&[1, 2]));
```

[`head_ref`] and [`head_mut`] need every source to be at least as large as the target. To check
the length at run time, read bytes with [`from_head`]:

```rust,compile_fail,E0080
let first: &u8 = puns::head_ref::<[u8], u8>(&[1, 2]);
```

```text
cannot pun: the source can be smaller than the target (if it has a slice, it may be empty).
```

```rust
assert_eq!(puns::from_head::<u8>(&[1, 2]), Ok((&1, &[2][..])));
```

### Unspecified layouts

The fields of a struct of the `Rust` representation have an [unspecified layout]: they may be in
any order, so without the `unstable` feature, `puns` cannot tell which byte is the `bool`. This
pun is then refused:

```rust,compile_fail,E0080
// This example is about the default build, without the `unstable` feature.
const _: () = assert!(!puns::layout::UNSTABLE);

#[derive(puns::Pun)]
struct Flagged {
  value: u8,
  flag: bool,
}

let flagged: &Flagged = puns::from_bytes(&[1u8, 1]).unwrap();
```

```text
cannot pun: the pun depends on a part of the source or of the target with an unspecified layout (a struct or a tuple of the `Rust` representation, or an enum of it, `Option` and `Result` included, whose layout `puns` does not assume): another compiler, or `-Zrandomize-layout`, could arrange it otherwise. Only the same type at the same place proves it.
Add `#[repr(C)]` (or `#[repr(u8)]` to an enum), or enable the `unstable` feature of `puns`.
This is at byte 0 of the target.
```

With the feature, it compiles. With `#[repr(C)]`, the fields are in declaration order, and it
compiles without the feature:

```rust
#[derive(puns::Pun)]
#[repr(C)]
struct Flagged {
  value: u8,
  flag: bool,
}

let flagged: &Flagged = puns::from_bytes(&[7u8, 1]).unwrap();
assert!(flagged.flag);
```

A `&mut` gives another message:

```rust,compile_fail,E0080
let _: &mut [u8; 2] = puns::pun_mut(&mut (true, 1u8));
```

```text
cannot pun: what may be written through the `&mut` must leave a valid source, which has a part with an unspecified layout (a struct, a tuple or an enum of the `Rust` representation): the analysis does not tell what a write leaves there.
Add `#[repr(C)]` (or `#[repr(u8)]` to an enum), or enable the `unstable` feature of `puns`.
```

Here the suggested fixes do not help: `#[repr(C)]` cannot be put on a tuple, and with the feature
the pun is still refused, with the message of [Writing through `&mut`](#writing-through-mut),
since a `u8` written over the `bool` may be 2. No function gives this `&mut`. Reading the bytes
needs no `&mut`:

```rust
let bytes: &[u8; 2] = puns::pun_ref(&(true, 1u8));
assert_eq!(bytes, &[1, 1]);
```

A `&mut` to the first or last bytes of the source gives a third:

```rust,compile_fail,E0080
use core::mem::MaybeUninit;

#[derive(puns::Pun)]
#[repr(C)]
struct Record {
  pair: (u8, u16),
  last: u8,
}

let _ = puns::head_mut::<Record, [MaybeUninit<u8>; 4]>(&mut Record { pair: (1, 2), last: 3 });
```

```text
cannot pun: the `&mut` covers a part of the source with an unspecified layout (a struct, a tuple or an enum of the `Rust` representation), which only a `&mut` to the same type may write.
Add `#[repr(C)]` (or `#[repr(u8)]` to an enum), or enable the `unstable` feature of `puns`.
```

With the feature, this pun is still refused, since uninitialized bytes written over the tuple
would not be a valid `(u8, u16)`. Take the tuple as itself:

```rust
#[derive(puns::Pun)]
#[repr(C)]
struct Record {
  pair: (u8, u16),
  last: u8,
}

let mut record = Record { pair: (1, 2), last: 3 };
let pair: &mut (u8, u16) = puns::head_mut(&mut record);
pair.1 = 7;
assert_eq!(record.pair, (1, 7));
```

### Values and padding

[Reading an error](#reading-an-error) shows the message of a byte that is padding in the
source. An enum gives it too: where `u16` has alignment 2, the byte after the tag of a `Circle` is
padding, and nothing tells a `Circle` from a `Rect` at compile time:

```rust,compile_fail,E0080
#[derive(puns::Pun, Clone, Copy)]
#[repr(u8)]
enum Shape {
  Circle { radius: u16 },
  Rect(u8, bool),
}

let _: [u8; 4] = puns::pun(Shape::Rect(3, true));
```

```text
cannot pun: a byte that the target requires to be initialized is padding in the source.
This is at byte 1 of the target.
```

To fix it, declare the padding of each variant as a field:

```rust
#[derive(puns::Pun, Clone, Copy)]
#[repr(u8)]
enum Shape {
  Circle { reserved: u8, radius: u16 },
  Rect(u8, bool, u8),
}

let bytes: [u8; 4] = puns::pun(Shape::Rect(3, true, 0));
assert_eq!(bytes, [1, 3, 1, 0]);
```

When no value of the source is a valid target, no function can make the pun, and the code that
asks for it is a mistake. A byte that is always 2 is never a `bool`:

```rust,compile_fail,E0080
let never = puns::try_pun::<puns::num::u8c<2>, bool>(Default::default());
```

```text
cannot pun: no value of the source is valid for the target.
This is at byte 0 of the target.
```

A target that is [uninhabited] gives this message too.

A function that checks nothing refuses a pun that needs a check:

```rust,compile_fail,E0080
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

The message says "rules", but a `#[pun(valid(..))]` range of the target that the source does not
prove gives it too, since both are [requirements][concepts#requirements].

### Alignment

A `[u8; 4]` can be at any address, but a `u32` cannot:

```rust,compile_fail,E0080
let n: &u32 = puns::pun_ref(&[0u8; 4]);
```

```text
cannot pun: the target has a greater alignment than the source.
Use `try_pun_ref` or `try_pun_mut` to check the address at run time.
```

[`pun`] takes the value, not a reference to it, so the address does not matter: it copies the
bytes into a `u32`. A [`num::u32le`] has an alignment of 1, so a reference to four bytes can be
reinterpreted as one. Otherwise, [`try_pun_ref`] checks the address at run time:

```rust
use puns::num::u32le;

assert_eq!(puns::pun::<[u8; 4], u32>([1, 0, 0, 0]), u32::from_ne_bytes([1, 0, 0, 0]));
assert_eq!(puns::pun_ref::<[u8; 4], u32le>(&[1, 0, 0, 0]).get(), 1);

// The bytes of two `u32`s: the first four are aligned for a `u32`, and the four after the first
// byte are not.
let all: &[u8; 8] = puns::pun_ref(&[0u32; 2]);
let aligned: &[u8; 4] = all[..4].try_into().unwrap();
let shifted: &[u8; 4] = all[1..5].try_into().unwrap();
assert_eq!(puns::try_pun_ref::<[u8; 4], u32>(aligned), Ok(&0));
assert_eq!(puns::try_pun_ref::<[u8; 4], u32>(shifted), Err(puns::Error::Alignment));
```

The functions of the front and the back check no address, and their message suggests other
functions, since the checked functions of a whole value do not take a part of the source:

```rust,compile_fail,E0080
let n: &u16 = puns::tail_ref(&[0u8; 4]);
```

```text
cannot pun: the target has a greater alignment than the source.
Use `tail` to copy it, or `from_tail` to take it from bytes, whose address is checked at run time.
```

```rust
assert_eq!(puns::tail::<[u8; 4], u16>([0, 0, 1, 0]), u16::from_ne_bytes([1, 0]));
```

[`head_ref`] and [`head_mut`] suggest `head` and `from_head` in the same way:

```rust,compile_fail,E0080
let n: &u16 = puns::head_ref(&[0u8; 4]);
```

```text
cannot pun: the target has a greater alignment than the source.
Use `head` to copy it, or `from_head` to take it from bytes, whose address is checked at run time.
```

```rust
assert_eq!(puns::head::<[u8; 4], u16>([1, 0, 0, 0]), u16::from_ne_bytes([1, 0]));
```

### The last bytes of a value

The functions of the back refuse an unsized source:

```rust,compile_fail,E0080
let last: &u8 = puns::tail_ref::<[u8], u8>(&[1, 2]);
```

```text
cannot pun: the source is unsized: where its last bytes start depends on its length, which is only known at run time.
Use `from_tail` to take a value from the end of bytes.
```

```rust
assert_eq!(puns::from_tail::<u8>(&[1, 2]), Ok((&[1][..], &2)));
```

For an unsized target, name its size, as an array:

```rust,compile_fail,E0080
let last: &[u8] = puns::tail_ref::<[u8; 4], [u8]>(&[1, 2, 3, 4]);
```

```text
cannot pun: the target is unsized: its extent cannot be found from the end of the source.
```

```rust
let last: &[u8; 2] = puns::tail_ref(&[1u8, 2, 3, 4]);
assert_eq!(last, &[3, 4]);
```

[`from_tail`] gives its own message:

```rust,compile_fail,E0080
let _ = puns::from_tail::<[u16]>(&[0; 4]);
```

```text
cannot pun: the target is unsized: where it starts cannot be found from the end of the bytes.
Use `from_head` or `from_bytes`.
```

### Writing through `&mut`

Here, a value of the target written through the `&mut` could be bytes that are not UTF-8:

```rust,compile_fail,E0080
let mut text = String::from("text");
let bytes: &mut [u8] = puns::pun_mut(text.as_mut_str());
```

```text
cannot pun: some values of the target are not valid for the source (or are padding where the source is not), and could be written through the `&mut`.
```

Nothing is written through a shared reference, and every `str` is bytes:

```rust
let text = String::from("text");
let bytes: &[u8] = puns::pun_ref(text.as_str());
assert_eq!(bytes, b"text");
```

A value outside a `#[pun(valid(..))]` range of the source gives this message too, since a write
must stay in such a range ([Writing through `&mut`][functions#writing-through-mut]). An enum
gives it as well: a value written through the `&mut Drawing` made from a `&mut Shape` could be a
`Nothing`, or a `Rect` whose second field is not a `bool`. A shared reference reads it:

```rust,compile_fail,E0080
#[derive(puns::Pun, Clone, Copy)]
#[repr(u8)]
enum Shape {
  Circle { radius: u16 },
  Rect(u8, bool),
}

#[derive(puns::Pun, Clone, Copy)]
#[repr(u8)]
enum Drawing {
  Circle { radius: u16 },
  Rect(u8, u8),
  Nothing,
}

let mut shape = Shape::Rect(3, true);
let _: &mut Drawing = puns::pun_mut(&mut shape);
```

```text
cannot pun: some values of the target are not valid for the source (or are padding where the source is not), and could be written through the `&mut`.
```

```rust
#[derive(puns::Pun, Clone, Copy)]
#[repr(u8)]
enum Shape {
  Circle { radius: u16 },
  Rect(u8, bool),
}

#[derive(puns::Pun, Clone, Copy)]
#[repr(u8)]
enum Drawing {
  Circle { radius: u16 },
  Rect(u8, u8),
  Nothing,
}

let drawing: &Drawing = puns::pun_ref(&Shape::Rect(3, true));
assert!(matches!(drawing, Drawing::Rect(3, 1)));
```

A `&mut` to the first or last bytes is refused where it overlaps a struct that has a rule, except
where [Writes leave a valid source][functions#writes-leave-a-valid-source] allows it:

```rust,compile_fail,E0080
#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(a < b))]
struct Ordered {
  a: u8,
  b: u8,
}

let _: &mut u8 = puns::tail_mut(&mut Ordered { a: 1, b: 2 });
```

```text
cannot pun: a struct of the source that has a rule (a size, a length, an assertion, a region) overlaps the bytes that the `&mut` covers, and a write could break the rule.
```

To fix it, take the struct whole, which keeps its rule a requirement of its type, or only bytes
outside it:

```rust
#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(a < b))]
struct Ordered {
  a: u8,
  b: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
struct Entry {
  range: Ordered,
  flags: u8,
}

let mut entry = Entry { range: Ordered { a: 1, b: 2 }, flags: 0 };
*puns::tail_mut::<_, u8>(&mut entry) = 7;
let range: &mut Ordered = puns::head_mut(&mut entry);
range.b = 9;
assert_eq!((entry.range.b, entry.flags), (9, 7));
```

A `&mut` to the first or last bytes takes an enum whole or not at all:

```rust,compile_fail,E0080
#[derive(puns::Pun, Clone, Copy)]
#[repr(u8)]
enum Shape {
  Circle { radius: u16 },
  Rect(u8, bool),
}

let _: &mut u8 = puns::head_mut(&mut Shape::Rect(1, true));
```

```text
cannot pun: an enum of the source is only partly covered by the `&mut`: what its tag says of the bytes on one side depends on the other.
```

To fix it, take whole enums:

```rust
#[derive(puns::Pun, Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
enum Shape {
  Circle { radius: u16 },
  Rect(u8, bool),
}

let mut shapes = [Shape::Rect(1, true), Shape::Circle { radius: 2 }];
*puns::head_mut::<_, Shape>(&mut shapes) = Shape::Circle { radius: 5 };
assert_eq!(shapes[0], Shape::Circle { radius: 5 });
```

[`head_mut`] gives the same message for a target that takes as many elements as fit, and could
end within an enum, a union or a `str` of the source, though the message names only an enum. For
a `str`, there is no fix: a write of other bytes could leave bytes that are not UTF-8.

```rust,compile_fail,E0080
let text: &mut str = String::from("text").leak();
let _: &mut [[u8; 2]] = puns::head_mut(text);
```

```text
cannot pun: an enum of the source is only partly covered by the `&mut`: what its tag says of the bytes on one side depends on the other.
```

### Eager aims

A value that holds an eager [aim][aims] was checked with its pointee, which code then reads
without a check. So the functions that would make such a value, or give a `&mut` through which a
write could move it away from its pointee, refuse it. The functions of a value require `Copy`,
which a struct with an eager aim is not, so the compiler refuses them first (E0277). A `&mut`
gives this message:

```rust,compile_fail,E0080
use puns::{InPlace, Pun};

/// A pointer to a `u32`, stored as the distance from its own address, in an `i32`.
#[derive(Pun)]
#[repr(transparent)]
struct Ptr {
  #[pun(pointer(pointee: u32))]
  off: InPlace<i32>,
}

let _ = puns::from_bytes_mut::<Ptr>(&mut [0; 4]);
```

```text
cannot pun: the source or the target holds an eager aim, whose pointee is checked with it and read without a check: a write through a `&mut` could move it away from that pointee.
```

Read the value with [`from_bytes`] or [`from_head`] instead, and write the pointee with
[`aim_mut`], from a pointer to the bytes that hold both.

The check of an eager aim reads its pointee in the bytes of the source, so a `try_` function
refuses a source that may have padding:

```text
cannot pun: the target holds an eager aim, whose check reads its pointee in the bytes of the source, and some of them may be padding.
Check the bytes of the source instead, with `from_bytes` or `from_head`.
```

An eager aim is proven only by the same aim at a pointee of the same
[identity][layouts#the-same-type]. Where the pointee has none, the message of a check that the
function cannot make goes on with "The source or the target has an eager aim at a pointee whose type
has no identity, so that not even the same aim proves it", and says which types have none.

### Zeros

[`zeroed`] makes no pun, and its messages do not start with `cannot pun:`. Zero bytes must be a
valid value of the type and meet its `valid(..)` ranges:

```rust,compile_fail,E0080
let _ = puns::zeroed::<core::num::NonZero<u8>>();
```

```text
error[E0080]: evaluation panicked: zero is not a valid value of this type, or does not meet its `valid(..)` ranges
```

Without the `unstable` feature, zero bytes must not be a value only by the tags that `rustc` chose
for an enum of the `Rust` representation, `Option` or `Result` whose layout `puns` does not
[assume][layouts#what-puns-assumes]. `puns` does not assume the layout of an enum whose variant
holds fields of different alignments:

```rust,compile_fail,E0080
// This example is about the default build, without the `unstable` feature.
const _: () = assert!(!puns::layout::UNSTABLE);

#[derive(puns::Pun, Clone, Copy)]
enum Pair {
  Both(u8, u16),
  Neither,
}

let _ = puns::zeroed::<Pair>();
```

```text
error[E0080]: evaluation panicked: zero is a value of this type only by the tags that `rustc` chose for an enum of the `Rust` representation, `Option` or `Result` in it, whose layout `puns` does not assume: another compiler, or `-Zrandomize-layout`, could choose others.
Add `#[repr(u8)]` to the enum, or enable the `unstable` feature of `puns`.
```

Where `puns` assumes the layout, or `core` documents it, zero bytes are a value:

```rust
#[derive(puns::Pun, Clone, Copy, Debug, PartialEq)]
enum Light {
  Off,
  On,
}

assert_eq!(puns::zeroed::<Light>(), Light::Off);
assert_eq!(puns::zeroed::<Option<core::num::NonZero<u8>>>(), None);
```

## Errors of the derive

The derive's [Compile-time errors][macro@Pun#compile-time-errors] say where and when each of its
errors is reported, and its page gives the conditions of each kind of type and of each attribute.
Most of its messages are at the place of the fault, and say what the derive accepts there: an
expression that is not a rule is refused at the part that is not one, with what a rule computes
([The language of rules][rules]). These messages need more:

- "`#[derive(Pun)]` failed": a constant read the layout of a type whose derive failed. It comes
  with the error of the derive, which is the one to fix.
- An option of `#[pun(..)]` that does not parse gets the message of the parser, such as "expected
  `=`" or "expected `,`", at the token where parsing stopped. The derive's page gives the form of
  each option.
- "too many variables": an expression names at most 8 values at once, its `let`s and the
  parameters of its walks. The names of a block end with it, so compute each value in a block of
  its own, or split the expression into several `assert`s.
- A message that ends with "unsupported layout": the derive cannot find the tag of an enum of the
  `Rust` representation in the layout that `rustc` chose ([its
  conditions][macro@Pun#enums-with-fields]). Give the enum a primitive representation, such as
  `#[repr(u8)]`, whose layout RFC 2195 specifies.
- A message that starts with "unsupported compiler", or says that the discriminant of an enum is
  not what `puns` takes it to be: `rustc` does not behave as [What `puns` relies on][rust] says. No
  change of the type fixes it, and the version of `rustc` is the cause.
- "not where the field is", "not the rules of the struct" and "not the operations of the
  expression": the path of [`#[pun(crate = ..)]`][macro@Pun#crate] names items other than those of
  `puns` ([The generated code][trust#the-generated-code]).

A rule that reads a field that is not an integer, or a signed one without a cast, is refused, and
[Fields in a rule][rules#fields-in-a-rule] shows its message and its fixes. A getter of `#[pun(get(..))]` that
reads such a field is refused where it is called, which `cargo check` does not report:

```rust,compile_fail,E0080
#[derive(puns::Pun)]
#[repr(C)]
#[pun(get(negative: bool = level >> 7 == 1))]
struct Level {
  level: i8,
}

let level: &Level = puns::from_bytes(&[0xff]).unwrap();
let _ = level.negative();
```

```text
error[E0080]: evaluation panicked: a getter reads the unsigned integer fields of its struct as `u128`s, and this field is not one (a signed or floating-point number, a `bool`, a `char` or an enum is not)
```

To fix it, write the method by hand:

```rust
#[derive(puns::Pun)]
#[repr(C)]
struct Level {
  level: i8,
}

impl Level {
  const fn negative(&self) -> bool {
    self.level < 0
  }
}

let level: &Level = puns::from_bytes(&[0xff]).unwrap();
assert!(level.negative());
```

[`aim_mut`]: https://docs.rs/puns/0.2.4/puns/fn.aim_mut.html
[`from_bytes`]: https://docs.rs/puns/0.2.4/puns/fn.from_bytes.html
[`from_head`]: https://docs.rs/puns/0.2.4/puns/fn.from_head.html
[`from_tail`]: https://docs.rs/puns/0.2.4/puns/fn.from_tail.html
[`head_mut`]: https://docs.rs/puns/0.2.4/puns/fn.head_mut.html
[`head_ref`]: https://docs.rs/puns/0.2.4/puns/fn.head_ref.html
[`num::u32le`]: https://docs.rs/puns/0.2.4/puns/num/struct.u32le.html
[`pun`]: https://docs.rs/puns/0.2.4/puns/fn.pun.html
[`tail_mut`]: https://docs.rs/puns/0.2.4/puns/fn.tail_mut.html
[`tail_ref`]: https://docs.rs/puns/0.2.4/puns/fn.tail_ref.html
[`try_pun_ref`]: https://docs.rs/puns/0.2.4/puns/fn.try_pun_ref.html
[`zeroed`]: https://docs.rs/puns/0.2.4/puns/fn.zeroed.html
[aims]: aims.md
[concepts#proven-checked-and-refused]: concepts.md#proven-checked-and-refused
[concepts#requirements]: concepts.md#requirements
[functions#writes-leave-a-valid-source]: functions.md#writes-leave-a-valid-source
[functions#writing-through-mut]: functions.md#writing-through-mut
[layouts#the-same-type]: layouts.md#the-same-type
[layouts#what-puns-assumes]: layouts.md#what-puns-assumes
[macro@Pun#compile-time-errors]: https://docs.rs/puns/0.2.4/puns/derive.Pun.html#compile-time-errors
[macro@Pun#crate]: https://docs.rs/puns/0.2.4/puns/derive.Pun.html#crate
[macro@Pun#enums-with-fields]: https://docs.rs/puns/0.2.4/puns/derive.Pun.html#enums-with-fields
[rules]: rules.md
[rules#fields-in-a-rule]: rules.md#fields-in-a-rule
[rust]: rust.md
[trust#the-generated-code]: trust.md#the-generated-code
[uninhabited]: concepts.md#uninhabited-types
[unspecified layout]: concepts.md#unspecified-layouts
[msrv]: target.md#minimum-rust-version
