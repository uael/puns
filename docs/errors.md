# Compile-time errors

A pun that `puns` [refuses][concepts#proven-checked-and-refused] does not compile. This chapter
shows how to read the error, then each message, with code that causes it and its fix. Build with
`cargo build` to see these errors. Most of them are panics of `const` blocks in the generic
functions of `puns` (error E0080), which `rustc` evaluates only when it generates code for an
instance of the function, after monomorphization. `cargo check` and editors generate no code, so
they do not report them. A generic function that calls `puns` therefore compiles until it is
instantiated with a pun that is refused, and the error then points inside it. The conditions
would be trait bounds, which `cargo check` reports, if stable Rust could bound the value of a
constant expression, as `generic_const_exprs` does on nightly ([Minimum Rust version][msrv]).

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

Byte 1 of the target is the first of the three bytes of padding after `kind`. The last note
gives the line of the call. Between the first lines and that note, `rustc` 1.100 prints 33 lines
that locate the failed constant inside `puns`, which can be skipped.

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

The message of a refused pun starts with `cannot pun:` and says why. When the cause is at one
byte, the message names that byte of the target. A message names only the first cause that
`puns` finds, in this order: the sizes, an unspecified layout, padding, a source with no valid
value, a check that the function cannot make, then the alignment. For a `&mut`, what a write
leaves comes last. Fixing one cause may therefore reveal the next. Each message follows, with
code that causes it and its fix, or the reason why there is none.

### Sizes

A pun of a whole value needs a target of the same size:

```rust,compile_fail,E0080
let n: u32 = puns::pun([1u8, 2, 3]);
```

```text
cannot pun: the source is smaller than the target.
```

For a larger source, the message suggests the functions that take the first or last bytes of it:

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

A target that ends with a slice must reach the size of the source with whole elements:

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

A source that ends with a slice may have a length that gives no size of the target:

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
cannot pun: the pun depends on a part of the source or of the target with an unspecified layout (a struct, a tuple or an enum of the `Rust` representation, `Option` or `Result` of most types): another compiler, or `-Zrandomize-layout`, could arrange it otherwise. Only the same type at the same place proves it.
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

[`validate_but_never`] checks every part of its target, and gives its own message for the same
struct, with the same fix:

```rust,compile_fail,E0080
// This example is about the default build, without the `unstable` feature.
const _: () = assert!(!puns::layout::UNSTABLE);

#[derive(puns::Pun)]
struct Flagged {
  value: u8,
  flag: bool,
}

let _ = puns::validate_but_never::<Flagged>(&[1, 1]);
```

```text
cannot pun: the type has a part with an unspecified layout (a struct, a tuple or an enum of the `Rust` representation), and something to check in it, which would be at another place with another compiler.
Add `#[repr(C)]` (or `#[repr(u8)]` to an enum), or enable the `unstable` feature of `puns`.
```

A `&mut` is refused when what is written through it would have to leave a valid source in a
part with an unspecified layout:

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

A `&mut` to the first or last bytes of the source can cover a part with an unspecified layout
only if the target has the same type at the same place:

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
source. An enum gives it too: the byte after the tag of a `Circle` is padding, and nothing tells
a `Circle` from a `Rect` at compile time:

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

A target that is [uninhabited] gives this message too, and so does a target that holds an [opaque
scalar] where a pun would make it, since `puns` makes no value of one. The bytes may still be a
valid value of such a target: [`validate_but_never`] checks the rest of it, and the crate that
declares the scalar makes the value.

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

[`tail_ref`] and [`tail_mut`] check no address. Their message suggests [`tail`], which copies the
value and needs no aligned address, and [`from_tail`], which checks the address of bytes at run
time:

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

### The last bytes of a value

Where the last bytes of an unsized source start depends on its length, which is only known at
run time:

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

An unsized target has no size to count from the end. Name its size, as an array:

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

[`from_tail`] gives its own message, which names [`from_head`] and [`from_bytes`]:

```rust,compile_fail,E0080
let _ = puns::from_tail::<[u16]>(&[0; 4]);
```

```text
cannot pun: the target is unsized: where it starts cannot be found from the end of the bytes.
Use `from_head` or `from_bytes`.
```

### Writing through `&mut`

Any value of the target may be written through a `&mut`, so every value of the target must be a
valid source. Here, it could be bytes that are not UTF-8:

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

A `&mut` to the first or last bytes cannot cover a struct that has a rule, unless the target has
the same struct there. A `&mut` to the whole struct, from [`pun_mut`], is allowed, and a write
through it may break the rule ([Writing through `&mut`][functions#writing-through-mut]). Telling
which fields a rule reads, so as to allow the first case too, is [not yet][limits#not-yet] done:

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

A `&mut` to the first or last bytes takes an enum whole or not at all, since its tag tells what
its other bytes may hold:

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

### Zeros

[`zeroed`] makes no pun, and its message does not start with `cannot pun:`:

```rust,compile_fail,E0080
let _ = puns::zeroed::<core::num::NonZero<u8>>();
```

```text
error[E0080]: evaluation panicked: zero is not a valid value of this type
```

```rust
assert_eq!(puns::zeroed::<Option<core::num::NonZero<u8>>>(), None);
```

## Errors of the derive

The derive reports most of its errors where the fault is, and the others when the layout of the
type is evaluated. The derive's [Compile-time errors][macro@Pun#compile-time-errors] say which
are which, and when `cargo check` reports them. A rule that reads a field that is not an unsigned
integer is refused, and [Fields in a rule][rules#fields-in-a-rule] shows its message and its fix.
A getter of `#[pun(get(..))]` that reads such a field is refused where it is called, which `cargo
check` does not report:

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

[`from_bytes`]: https://docs.rs/puns/0.2.3/puns/fn.from_bytes.html
[`from_head`]: https://docs.rs/puns/0.2.3/puns/fn.from_head.html
[`from_tail`]: https://docs.rs/puns/0.2.3/puns/fn.from_tail.html
[`head_mut`]: https://docs.rs/puns/0.2.3/puns/fn.head_mut.html
[`head_ref`]: https://docs.rs/puns/0.2.3/puns/fn.head_ref.html
[`num::u32le`]: https://docs.rs/puns/0.2.3/puns/num/struct.u32le.html
[`pun_mut`]: https://docs.rs/puns/0.2.3/puns/fn.pun_mut.html
[`pun`]: https://docs.rs/puns/0.2.3/puns/fn.pun.html
[`tail_mut`]: https://docs.rs/puns/0.2.3/puns/fn.tail_mut.html
[`tail_ref`]: https://docs.rs/puns/0.2.3/puns/fn.tail_ref.html
[`tail`]: https://docs.rs/puns/0.2.3/puns/fn.tail.html
[`try_pun_ref`]: https://docs.rs/puns/0.2.3/puns/fn.try_pun_ref.html
[`validate_but_never`]: https://docs.rs/puns/0.2.3/puns/fn.validate_but_never.html
[`zeroed`]: https://docs.rs/puns/0.2.3/puns/fn.zeroed.html
[concepts#proven-checked-and-refused]: concepts.md#proven-checked-and-refused
[concepts#requirements]: concepts.md#requirements
[functions#writing-through-mut]: functions.md#writing-through-mut
[limits#not-yet]: limits.md#not-yet
[macro@Pun#compile-time-errors]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html#compile-time-errors
[opaque scalar]: concepts.md#unspecified-layouts-opaque-scalars-and-uninhabited-types
[rules#fields-in-a-rule]: rules.md#fields-in-a-rule
[uninhabited]: concepts.md#unspecified-layouts-opaque-scalars-and-uninhabited-types
[unspecified layout]: concepts.md#unspecified-layouts-opaque-scalars-and-uninhabited-types
[msrv]: target.md#minimum-rust-version
