# Unspecified layouts and the `unstable` feature

Rust specifies some layouts, and leaves parts of others to the compiler
([`r-layout.guarantees`], [`r-layout.repr.rust.unspecified`]). Such a part is an *[unspecified
layout]*: the next `rustc` may choose it differently, and [`-Zrandomize-layout`] chooses it at
random. These parts are:

- The order of the fields of a struct or a tuple of the `Rust` representation, and so its size.
  Where `u16` has alignment 2, `(u8, u16, u8)` is 4 or 6 bytes.
- The tag of an enum of the `Rust` representation: where and how its discriminant is stored, and
  its values ([`r-items.enum.discriminant.type`], [`mem::discriminant`]).
- The niches of `Option` and `Result`, except where `core` documents them.

`puns` always reads the layout that `rustc` chose for the build. The `unstable` feature decides
whether a pun may also depend on it.

## Layout independence

Without the `unstable` feature, `puns` has *layout independence*: whether a pun compiles, what
it checks and which values a check accepts are the same in every layout that Rust allows, so
`-Zrandomize-layout` changes none of them. A pun that would depend on an unspecified layout is
refused. The one exception is whether an `Option` of an [opaque scalar] compiles, which depends
on a niche that `rustc` chooses, though what the pun checks and accepts does not, since nothing
of it is checked.

Layout independence holds of a pun, not of a type: the [table](#puns-without-the-feature)
proves `(bool, u8)` as bytes, and refuses bytes as `(bool, u8)`. It does not cover the message
of a refusal, which may name what `rustc` chose: `(u8, u16, u8)` as `[u8; 4]` is refused for its
layout, or for its size when it is 6 bytes. Nor does it cover which field a byte holds. That is
the compiler's choice, with or without `puns`, and a pun promises a valid value and nothing
more, as [`transmute`] does:

```
let pair: (u8, u8) = puns::pun([1u8, 2]);
assert!(pair == (1, 2) || pair == (2, 1));
```

## The `unstable` feature

With the `unstable` feature, `puns` relies on the layout that `rustc` chose, as it is, including
the tag that the derive finds for an enum. Within one build, a pun that compiles without the
feature compiles with it and does the same, and more puns compile. [`layout::UNSTABLE`] is `true`
when the feature is enabled.

The feature does not make a pun unsound: with or without it, `puns` reads the layout of the
build that compiles the pun, so the pun is sound for that build. What the feature gives up is
that the verdict stays the same. With the next `rustc`, or under `-Zrandomize-layout`, a pun may
stop compiling, or check other bytes. Prefer a `repr` where you can choose one, and the feature
for types that you cannot give one, such as `Option<bool>` or the types of another crate.

## What a check may read

A check never reads a byte of the source that holds another field in another layout that Rust
allows, since it would read another part of the value there, and could give another answer. A
check that reads a byte of a struct whose fields Rust may reorder is therefore refused, even
where every order would give the same answer, because `puns` does not compare them. In `(u8, u8)`
as `[bool; 2]`, each byte must be 0 or 1 in either order, but the pun is refused. Each byte of an
array holds the same element in every layout, so from an array the check is allowed:

```compile_fail,E0080
// This example is about the default build, without the `unstable` feature.
const _: () = assert!(!puns::layout::UNSTABLE);

let _ = puns::try_pun::<(u8, u8), [bool; 2]>((1, 0)); // Refused.
```

```
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

## Puns without the feature

| Pun, without `unstable` | | Why |
|---|---|---|
| `[u8; 2]` as `(u8, u8)`, and back | proven | any bytes, in either order |
| `[u8; 2]` as `(bool, bool)` | checked | both bytes are a `bool`, in either order |
| `(bool, u8)` as `[u8; 2]`, `as_bytes` | proven | both bytes are initialized, in either order |
| `[u8; 4]` as `(u8, u16)` | proven | it requires only initialized bytes, wherever its padding is |
| a `repr(C)` struct that holds a `(bool, u8)` as itself, or its first bytes as the `(bool, u8)` | proven | the same type at the same place |
| an enum of the `Rust` representation as itself | proven | a type has one layout in a program |
| `[bool; 2]` as `(bool, u8)` | proven | the `bool` may be either byte, and both are a `bool` |
| `[NonZero<u16>; 2]` as `(NonZero<u16>, u16)` | proven | either field may be at either place, and both hold a `NonZero<u16>` |
| `[u8; 2]` as `(bool, u8)` | refused | the byte that must be 0 or 1 is either byte |
| `(u8, u8)` as `[bool; 2]` | refused | each element's check reads a byte that holds the other field in another layout |
| the first two bytes of `(u8, u16)` | refused | the second byte is padding in one layout and not in another |
| bytes as a `struct P { a: u8, b: u8 }` with the rule `a < b` | refused | the rule reads `a`, which is either byte |
| `(u8, u8)` as a `repr(C)` struct of a `bool` and a `u8` | refused | the check reads either field, and `(0, 2)` passes in one order only |
| `(u8, u8)` as `NonZero<u16>` | refused | the check reads both bytes, and each may hold either field |
| a pun from `(u8, u16, u8)` or to it, other than to itself | refused | it is 4 or 6 bytes |
| bytes as an enum of the `Rust` representation, or as `Option<bool>` | refused | nothing documents where the tag is, nor its values |
| `validate_but_never::<(bool, u8)>` | refused | the byte that it checks is either byte |

A struct or a tuple of the `Rust` representation is known by what all orders of its fields have
in common. If every order makes the same bytes, as for `(u8, [u8; 3])` or `(bool, bool)`, a pun
relies on every byte. Otherwise, as a source, it holds initialized bytes if it does in every
order, and nothing more. As a target, if its size is the same in every order and it has no rule
and no opaque scalar, it is proven field by field, each field at every place where it may be. A
struct with a rule, which names fields, is proven only by itself, and so is an enum of the `Rust`
representation. A [part] that holds an opaque scalar is proven by nothing.

No check replaces a proof in a part with an unspecified layout, since the check would read where
the compiler may not have put the field: what is not proven there is refused. The check of a
union that a later field proves, or of an enum whose variant holds an opaque scalar, still reads
such a part. Its answer is then the same in every layout, and the pun compiles only if every byte
under the part is initialized in the source.

A niche is used only where `core` documents the representation of `Option` or `Result`, not where
Rust documents only that an enum is "ABI-compatible". So
`enum Ticket { Free, Paid(NonZero<u32>) }` of the `Rust` representation needs the feature, though
`rustc` lays it out like `Option<NonZero<u32>>`.

## The same type

Without the feature, a part with an unspecified layout that is not proven field by field is
proven only by the same type at the same place. A type has one layout in a program, but two
types, however alike, may be arranged differently ([`offset_of!`, "Layout is subject to
change"][offset_of layout], [`r-type.struct.layout`]): `-Zrandomize-layout` arranges
`Wrapper<u8, u8>` and `Wrapper<u8, i8>` differently.

Stable Rust cannot compare types in `const`, since `TypeId` cannot be compared there and
`type_name` is not a `const fn`. So `puns` identifies a type by a name, with the identities of the
types it is made of. The primitive types, the types of [`num`], arrays, slices, tuples, `Option`,
`Result` and `MaybeUninit` have identities, and `ManuallyDrop<T>` is `T`. A type that derives
`Pun` is named by its module, file, line and column, its name, and the number of its derive in
its crate. A generic type that derives `Pun`, a `PhantomData` and a [`Scalar`] implemented by
hand have none, so a part with an unspecified layout that holds one proves itself only with the
feature, unless it is proven field by field. A type whose parts all have the same layout in every
build, such as a generic `repr(C)` struct of a `u8` and a `PhantomData<T>`, needs no identity. An
identity can only refuse a pun, since layouts are compared as well: two types that share a name
are still told apart by their layouts.

[`-Zrandomize-layout`]: https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/randomize-layout.html
[`mem::discriminant`]: https://doc.rust-lang.org/1.100.0/core/mem/fn.discriminant.html
[`r-items.enum.discriminant.type`]: https://doc.rust-lang.org/1.99.0/reference/items/enumerations.html#r-items.enum.discriminant.type
[`r-layout.guarantees`]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.guarantees
[`r-layout.repr.rust.unspecified`]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.rust.unspecified
[`r-type.struct.layout`]: https://doc.rust-lang.org/1.99.0/reference/types/struct.html#r-type.struct.layout
[`transmute`]: https://doc.rust-lang.org/1.100.0/core/intrinsics/fn.transmute.html
[offset_of layout]: https://doc.rust-lang.org/1.100.0/core/mem/macro.offset_of.html#layout-is-subject-to-change

[`layout::UNSTABLE`]: https://docs.rs/puns/0.2.3/puns/layout/constant.UNSTABLE.html
[`num`]: https://docs.rs/puns/0.2.3/puns/num/index.html
[`Scalar`]: https://docs.rs/puns/0.2.3/puns/trait.Scalar.html
[opaque scalar]: concepts.md#unspecified-layouts-opaque-scalars-and-uninhabited-types
[part]: concepts.md#layouts-and-rules
[rule]: concepts.md#layouts-and-rules
[unspecified layout]: concepts.md#unspecified-layouts-opaque-scalars-and-uninhabited-types
