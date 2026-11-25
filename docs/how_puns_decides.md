# How `puns` decides

[`transmute`][core::mem::transmute] reinterprets the bytes of a value as another type, and the
compiler checks only that the two types have the same size. Whether every value of the first
type is a valid value of the second is left to the caller. `puns` answers that question while
your program compiles, and leaves to run time only what it cannot answer there.

## Pun, source and target

A *pun* reinterprets the bytes of a value of one type, the *source* `S`, as a value of another
type, the *target* `D`, through a function of `puns`. The two types are the pun's *pair*.

## Layouts and rules

`puns` knows your types only by their [layouts][layout] and by what the type system enforces
about their bytes, never by what their constructors check ([What deriving `Pun`
means][soundness#what-deriving-pun-means]).

The layout that `puns` reads holds more than the [layout that the Rust Reference
defines][type-layout]. Besides the size, the alignment and the offset of each field, it holds the
values that each scalar may hold, and the rules of each struct. A byte that may be uninitialized,
such as a byte of a struct that no field covers, is *padding*. The type's implementation of [`Pun`]
gives this layout as a `const` value, `Pun::LAYOUT`. The analysis is ordinary `const fn` code that
compares the two layouts of a pair *part* by part. A part is a scalar, an element of an array, a
field, a variant or a trailing slice. The compiler runs the analysis once for each pair that a
function of `puns` is used with.

A struct that derives `Pun` may state *[rules][rules]*: expressions on its fields and its
bytes that its values must meet. A rule is an assertion, the size of a value, the length of its
trailing slice, or the size of a region. The *extent* of a value is its size up to the end of its
trailing slice. [`size_of_val`] also counts the padding that may follow that slice, and the extent
does not. A `size` rule gives the extent. A *[region][rules#regions]* is a named range of bytes of
the last field of a struct. A rule gives its size, or its type does, and a method of the same name
reads it. A region is one kind of [aim](#aims).

## What is a valid value?

A *valid* value of a type is [valid] in the sense of the Rust Reference, and meets the invariants
that its type *guarantees*, whatever code does. A `str` is UTF-8, a [`u8in<MIN, MAX>`][num::u8in]
holds a value from `MIN` to `MAX`, and a [`u16bec<V>`][num::u16bec] holds `V`. The types of [`num`]
keep their fields private, and their constructors check these invariants.

## Requirements

What a struct asks of its values with `#[pun(valid(..))]` and with its rules is a *requirement*. The
type does not enforce it: code can write its fields directly or through a pun, private fields
included. `puns` checks a requirement only when it reads bytes. After a write, a getter of
`#[pun(get(..))]` may panic, and a [region][rules#regions] may no longer fit in its field.

The analysis relies only on validity. A pun makes a `D` only from bytes that are a valid `D`, and a
reference to a `D` only at an address [properly aligned] for `D`. The requirements of `D` hold of
the result if those of the source held: the pun checks them, or proves them from the source ([Why
the pair decides](#why-the-pair-decides)).

A requirement never proves what another type guarantees. Two types have the same layout only if
they require and guarantee the same values, and a field `#[pun(valid(0..=1))] u8` is not a
`bool`.

## Proven, checked and refused

At compile time, `puns` puts each pun in one of three kinds:

- *Proven*: every value of the source is a valid target, and the source proves the requirements
  of the target. Nothing is checked at run time.
- *Checked*: some values of the source are not valid targets, or may not meet the requirements of
  the target. The `try_` and `from_` functions check at run time what the analysis could not
  prove, and return an [`Error`] if the bytes fail the check. A part that the analysis proves is
  checked too where it shares a check with a part that it does not prove, as the elements of an
  array do ([Not yet][limits#not-yet] lists these parts). A check never allocates and never
  panics.
- *Refused*: the pun does not compile. Either the pair can never be valid, or some part of it
  can be neither proven nor checked, such as a byte that may be padding in the source, or the
  pair needs a check and the function checks nothing.

A proven pun compiles to no check, though it may still compute an address or a length:

```rust
// Proven, since every `u16` is two initialized bytes. The length of the result is computed.
let bytes: &[u8] = puns::pun_ref(&[1u16, 2, 3][..]);
assert_eq!(bytes.len(), 6);
```

## Why the pair decides

The kind of a pun depends on its function and on both of its types, and never on one type alone.
With the `Switch` of the [crate page][crate], `&[Switch]` as `&[bool]` is proven by [`pun_ref`] and
[`try_pun_ref`] alike: a pair that is proven is proven with either function. `&[u8]` as `&[bool]` is
refused by `pun_ref`, which checks nothing, and checked by `try_pun_ref`, since a byte may hold the
value 2. At compile time, the two layouts tell what holds of every value of the source. What depends
on one value, such as its bytes or a length that a rule reads from them, is left to a check at run
time.

A `#[pun(valid(..))]` of the target is proven by any source whose values it accepts, as a `bool`
proves `#[pun(valid(0..=1))]`. A rule of a struct is proven only by the same rule of the source, at
the same place, each rule on its own. The two structs must have the same sized part and the same
trailing slice. Without the `unstable` feature, they must also be the same type, or both have a
layout that Rust specifies. Their other rules do not matter. The exception is an [aim](#aims) at the
cursor: the aims at the cursor before it place it, so the source must have the same aims at the
cursor before it. The names of their fields do not matter either:

```rust
use puns::Pun;

#[derive(Pun)]
#[repr(C)]
#[pun(assert(low <= high))]
struct Range {
  low: u8,
  high: u8,
}

/// The same rule as `Range`, on fields of other names.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(a <= b))]
struct Pair {
  a: u8,
  b: u8,
}

// From bytes, the rule is checked.
assert!(puns::from_bytes::<Range>(&[5, 1]).is_err());
let range: &Range = puns::from_bytes(&[1, 5]).unwrap();

// From a `Range`, the rule of `Pair` is proven.
let pair: &Pair = puns::pun_ref(range);
assert_eq!((pair.a, pair.b), (1, 5));
```

Bytes that a struct requires to be ASCII are not a `str`, because code may have written 0xff to one
of them since they were checked:

```rust,compile_fail,E0080
use puns::Pun;

#[derive(Pun)]
#[repr(transparent)]
struct Required(#[pun(valid(..=0x7f))] pub u8);

let required: &[Required] = puns::from_bytes(b"hi").unwrap();
let text: &str = puns::pun_ref(required); // Refused.
```

```text
cannot pun: some values of the source are not valid for the target, or do not prove one of its rules.
Use a `try_*` function to check them at run time.
This is at byte 0 of the target.
```

To fix this pun, you can check it at run time, or read the bytes as a type that guarantees ASCII:

```rust
use puns::{Pun, num::u8in};

#[derive(Pun)]
#[repr(transparent)]
struct Required(#[pun(valid(..=0x7f))] pub u8);

let required: &[Required] = puns::from_bytes(b"hi").unwrap();
assert_eq!(puns::try_pun_ref::<[Required], str>(required), Ok("hi"));

// Reading bytes as `u8in<0, 0x7f>` is checked. Reading that as a `str` is proven.
let guaranteed: &[u8in<0, 0x7f>] = puns::from_bytes(b"hi").unwrap();
let text: &str = puns::pun_ref(guaranteed);
assert_eq!(text, "hi");
```

## Unspecified layouts

A part of a type has an *unspecified layout* where Rust leaves its layout to the compiler, which may
choose differently in its next version, or at random under `-Zrandomize-layout`. Only what the Rust
Reference lists is guaranteed ([`r-layout.guarantees`]). [Unspecified layouts and the `unstable`
feature][layouts] lists these choices, and what `puns` does about them.

## Aims

An *aim* is a rule of a struct that points at a place of the checked bytes. The value there is its
*pointee*, such as the value that a self-relative pointer points at, or a region of the trailing
bytes of the struct. A check of the struct checks the pointee of an eager aim with it, and a read
checks the pointee of a lazy one. [Aims][aims] says how.

## Uninhabited types

A type, a variant or a field is *[uninhabited]* if it has no values. `Infallible` and an enum
without variants are uninhabited.

[valid]: https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity

[`r-layout.guarantees`]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.guarantees
[uninhabited]: https://doc.rust-lang.org/1.99.0/reference/glossary.html#uninhabited
[`size_of_val`]: https://doc.rust-lang.org/1.99.0/core/mem/fn.size_of_val.html
[type-layout]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html

[`Error`]: https://docs.rs/puns/0.2.4/puns/enum.Error.html
[`num`]: https://docs.rs/puns/0.2.4/puns/num/index.html
[`pun_ref`]: https://docs.rs/puns/0.2.4/puns/fn.pun_ref.html
[`Pun`]: https://docs.rs/puns/0.2.4/puns/trait.Pun.html
[`try_pun_ref`]: https://docs.rs/puns/0.2.4/puns/fn.try_pun_ref.html
[core::mem::transmute]: https://doc.rust-lang.org/1.99.0/core/mem/fn.transmute.html
[crate]: ../README.md
[num::u16bec]: https://docs.rs/puns/0.2.4/puns/num/struct.u16bec.html
[num::u8in]: https://docs.rs/puns/0.2.4/puns/num/struct.u8in.html
[properly aligned]: https://doc.rust-lang.org/1.99.0/core/ptr/index.html#alignment
[aims]: aims.md
[rules]: rules.md
[rules#regions]: rules.md#regions
[soundness#what-deriving-pun-means]: soundness.md#what-deriving-pun-means
[layout]: https://docs.rs/puns/0.2.4/puns/layout/index.html
[layouts]: layouts.md
[limits#not-yet]: limits.md#not-yet
