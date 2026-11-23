# Unspecified layouts and the `unstable` feature

Rust leaves these choices of layout to the compiler, and a type whose layout depends on them has an
[unspecified layout] ([`r-layout.guarantees`], [`r-layout.repr.rust.unspecified`]):

- The order of the fields of a struct or a tuple of the `Rust` representation, and so its size.
  Where `u16` has alignment 2, `struct Triple(u8, u16, u8)` may be 4 or 6 bytes. `rustc` makes it
  4 bytes, and 6 bytes under [`-Zrandomize-layout`] `-Zlayout-seed=1` on `nightly-2026-11-17`.
  That flag does not move the fields of a tuple, though Rust allows it.
- The tag of an enum of the `Rust` representation: where and how its discriminant is stored, and
  its values ([`r-items.enum.discriminant.repr-rust`], [`mem::discriminant`]).
- The niches of `Option` and `Result`, except where `core` documents them.

`puns` always reads the layout that `rustc` chose for the build. The `unstable` feature decides
whether a pun may also depend on that choice.

## Layout independence

Without the `unstable` feature, `puns` has *layout independence*: whether a pun is [proven,
checked or refused][kinds], what it checks, and which values a check accepts are the same in
every layout that Rust allows. A pun that would depend on an unspecified layout is refused. The
one exception is whether an `Option` of an [opaque scalar] compiles where `core` does not
document its niche ([Opaque scalars][opaque-scalars]). What such a pun checks and accepts does not
depend on the niche.

Layout independence holds of a pun, which is a function and a pair, not of a type: of the rows of
`(bool, u8)` below, some are proven and some refused. It does not cover the message of a
refusal, which may name what `rustc` chose. `pun` of a `Triple` as a `[u8; 4]` is refused for its
unspecified layout, or, under the seed above, because the source is larger than the target. Nor
does it cover which field each byte holds, which is the compiler's choice with or without `puns`.
A pun returns a valid value whose fields depend on the layout, as [`transmute`] does:

```rust
let pair: (u8, u8) = puns::pun([1u8, 2]);
assert!(pair == (1, 2) || pair == (2, 1));
```

| Function and pair, without the feature | Kind | Why |
|---|---|---|
| `pun` of `[u8; 2]` as `(u8, u8)`, and back | proven | any bytes, in either order |
| [`as_bytes`] of a `(bool, u8)`, or `pun` of it as `[u8; 2]` | proven | both bytes are initialized, in either order |
| `pun` of `[u8; 4]` as `(u8, u16)` | proven | it requires only initialized bytes, wherever its padding is |
| `pun` of a `repr(C)` struct that holds a `(bool, u8)` as itself, or `head` of it as that `(bool, u8)` | proven | the same type at the same place |
| `pun` of an enum of the `Rust` representation that is not generic as itself | proven | a type has one layout in a program |
| `pun` of a generic enum of the `Rust` representation, such as `enum E<T> { A(T), B(u16) }`, as itself | refused | it has no [identity](#the-same-type) |
| `pun` of `[bool; 2]` as `(bool, u8)` | proven | either byte may be the `bool`, and both are one |
| `pun` of `[NonZero<u16>; 2]` as `(NonZero<u16>, u16)` | proven | both places hold a `NonZero<u16>` |
| `pun` of `(u8, u8)` as a `repr(C)` union of a struct of the `Rust` representation and a `[u8; 2]` | proven | the `[u8; 2]` takes any bytes |
| `try_pun` of `[u8; 2]` as `(bool, u8)` | refused | the byte that must be 0 or 1 is either byte |
| `head` of `(u16, u8)` as `[u8; 2]` | refused | the second byte is padding where the `u8` comes first, which Rust allows and `rustc` does not choose |
| `try_pun` of bytes as `struct P { a: u8, b: u8 }` with the rule `a < b` | refused | the rule reads `a`, which is either byte |
| `try_pun` of `(u8, u8)` as a `repr(C)` struct of a `bool` and a `u8` | refused | `(0, 2)` passes in one order only |
| `try_pun` of `(u8, u8)` as `NonZero<u16>` | refused | each byte that the check reads may hold either field |
| any function of a `Triple`, or to it, but `pun` to itself | refused | it may be 4 or 6 bytes |
| `try_pun` of bytes as an enum of the `Rust` representation, or as `Option<bool>` | refused | nothing documents where the tag is, nor its values |
| `pun` of `(u8, S)` as itself, for an opaque scalar `S` | refused | a [part] that holds an opaque scalar is proven by nothing |
| [`validate_but_never`] of `(bool, u8)` | refused | the byte that it checks is either byte |

[Unspecified layouts][errors#unspecified-layouts] gives the messages of these refusals and of the
refusals of a `&mut`, and their fixes.

## The `unstable` feature

With the `unstable` feature, `puns` relies on the layout that `rustc` chose, including the tag
that the derive finds for an enum, and [`layout::UNSTABLE`] is `true`. With the same compiler and
target, a pun that compiles without the feature compiles with it and does the same, and more puns
compile.

The feature does not make a pun unsound, since a pun is sound for the build that compiles it,
whose layout `puns` reads. It gives up layout independence: with the next `rustc`, or under
`-Zrandomize-layout`, a pun may stop compiling, or check other bytes. Prefer a `repr` where you
can choose one, and the feature for types that you cannot give one, such as `Option<bool>` or the
types of another crate.

## What a check may read

A pun is refused if its check would read a byte of the source that holds a different field in
some layout that Rust allows. There, the check would read another field of the same value, and
could give a different answer. `puns` does not compare the answers of the orders, so a pun is
refused even where they agree. `try_pun` of `(u8, u8)` as `[bool; 2]` is refused, though each
byte must be 0 or 1 in either order:

```rust,compile_fail,E0080
// This example is about the default build, without the `unstable` feature.
const _: () = assert!(!puns::layout::UNSTABLE);

let _ = puns::try_pun::<(u8, u8), [bool; 2]>((1, 0)); // Refused.
```

```text
cannot pun: the pun depends on a part of the source or of the target with an unspecified layout (a struct, a tuple or an enum of the `Rust` representation, `Option` or `Result` of most types): another compiler, or `-Zrandomize-layout`, could arrange it otherwise. Only the same type at the same place proves it.
Add `#[repr(C)]` (or `#[repr(u8)]` to an enum), or enable the `unstable` feature of `puns`.
This is at byte 0 of the target.
```

With the feature, the pun compiles, and the assertion fails instead. Each byte of an array holds
the same element in every layout, so the same check is allowed from an array:

```rust
assert!(puns::try_pun::<[u8; 2], (bool, bool)>([1, 0]).is_ok());
assert!(puns::try_pun::<[u8; 2], (bool, bool)>([1, 2]).is_err());
```

Only the bytes that a check reads count:

- The check of an enum reads its tag, then the fields of the variant that the tag selects. Where
  the source has an enum whose tag is at the same place, with the same size and byte order, a
  byte under a variant counts only for the values of the source whose tag is that variant's.
- The elements of an array share their check. Where the source has elements of the same size at
  the same places, each element's check reads the same bytes of its element as the first one's.
  Otherwise, every byte under the array counts.
- A checked [rule] may read any byte of its struct, so every byte of that struct counts.

## How a part with an unspecified layout is proven

Without the feature, `puns` uses only what every order of the fields of a struct or a tuple of the
`Rust` representation has in common. Where every order makes the same bytes, as for
`(u8, [u8; 3])` or `(bool, bool)`, it uses every byte. Otherwise, as a source, the struct holds
initialized bytes if it does in every order, and nothing more. As a target, a struct whose size is
the same in every order, with no rule and no opaque scalar, is proven field by field, each field
at every place where it may be, as the rows of `[bool; 2]` and `[NonZero<u16>; 2]` show. A struct
with a rule, which names fields, is proven only by itself. An enum of the `Rust` representation,
with fields or without, proves only the same type, and only the same type proves it.

No check replaces a proof in a part with an unspecified layout, since the check would read where
the compiler may not have put the field. What is not proven there is refused. Two checks still
read such a part, with the same answer in every layout: that of a union that a later field
proves, which accepts whatever the earlier field does not, and that of an enum whose variant
holds an opaque scalar, which no value of the variant passes. The pun then compiles only if every
byte under the part is initialized in the source.

Niches are used only where `core` documents the representation of `Option` and `Result`, not
where it documents only ABI compatibility ([What Rust documents][rust#what-rust-documents]). So
`try_pun` of a `u32` as `enum Ticket { Free, Paid(NonZero<u32>) }` needs the feature, though `rustc` lays
`Ticket` out like `Option<NonZero<u32>>`.

## The same type

Without the feature, a part with an unspecified layout that is not proven field by field is
proven only by the same type at the same place. A type has one layout in a program, but two
types, however alike, may be arranged differently ([`offset_of!`, "Layout is subject to
change"][offset_of layout], [`r-type.struct.layout`]). For `struct Wrapper<A, B> { a: A, b: B,
c: u16 }`, `-Zrandomize-layout` on `nightly-2026-11-17` orders the fields of `Wrapper<u8, u8>` and
`Wrapper<u8, i8>` alike with `-Zlayout-seed=1`, and differently with `-Zlayout-seed=4`.

Stable Rust cannot compare types in `const`, where `TypeId` cannot be compared and `type_name`
cannot be called. So `puns` gives a type an *identity*, a name together with the identities
of the types it is made of. The primitive types, the types of [`num`], arrays, slices, tuples,
`Option`, `Result` and `MaybeUninit` have identities, and `ManuallyDrop<T>` has that of `T`. A
type that derives `Pun` is named by its module, file, line and column, its name, and the number
of its derive in its crate. A generic type that derives `Pun`, a `PhantomData` and a [`Scalar`]
implemented by hand have none, so a part with an unspecified layout that holds one proves itself
only with the feature, unless it is proven field by field. A type whose parts all have the same
layout in every build, such as a generic `repr(C)` struct of a `u8` and a `PhantomData<T>`,
needs no identity. A part that holds a scalar of [`Spec::maybe_uninit`] is the same type as
nothing, itself included. An identity only adds a condition, since the layouts must be the same
as well ([The identity of a type and the compiler][trust-identity]).

[`-Zrandomize-layout`]: https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/randomize-layout.html
[`mem::discriminant`]: https://doc.rust-lang.org/1.100.0/core/mem/fn.discriminant.html
[`r-items.enum.discriminant.repr-rust`]: https://doc.rust-lang.org/1.99.0/reference/items/enumerations.html#r-items.enum.discriminant.repr-rust
[`r-layout.guarantees`]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.guarantees
[`r-layout.repr.rust.unspecified`]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.rust.unspecified
[`r-type.struct.layout`]: https://doc.rust-lang.org/1.99.0/reference/types/struct.html#r-type.struct.layout
[`transmute`]: https://doc.rust-lang.org/1.100.0/core/intrinsics/fn.transmute.html
[offset_of layout]: https://doc.rust-lang.org/1.100.0/core/mem/macro.offset_of.html#layout-is-subject-to-change

[`as_bytes`]: https://docs.rs/puns/0.2.4/puns/fn.as_bytes.html
[`layout::UNSTABLE`]: https://docs.rs/puns/0.2.4/puns/layout/constant.UNSTABLE.html
[`num`]: https://docs.rs/puns/0.2.4/puns/num/index.html
[`Scalar`]: https://docs.rs/puns/0.2.4/puns/trait.Scalar.html
[`Spec::maybe_uninit`]: https://docs.rs/puns/0.2.4/puns/layout/struct.Spec.html#method.maybe_uninit
[`validate_but_never`]: https://docs.rs/puns/0.2.4/puns/fn.validate_but_never.html
[errors#unspecified-layouts]: errors.md#unspecified-layouts
[kinds]: concepts.md#proven-checked-and-refused
[opaque-scalars]: https://docs.rs/puns/0.2.4/puns/trait.Scalar.html#opaque-scalars
[opaque scalar]: concepts.md#unspecified-layouts-opaque-scalars-and-uninhabited-types
[part]: concepts.md#layouts-and-rules
[rule]: concepts.md#layouts-and-rules
[rust#what-rust-documents]: rust.md#what-rust-documents
[trust-identity]: trust.md#the-identity-of-a-type-and-the-compiler
[unspecified layout]: concepts.md#unspecified-layouts-opaque-scalars-and-uninhabited-types
