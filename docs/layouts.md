# Unspecified layouts and the `unstable` feature

Rust leaves these choices of layout to the compiler, and a type whose layout depends on them has an
[unspecified layout] ([`r-layout.guarantees`], [`r-layout.repr.rust.unspecified`]):

- The order of the fields of a struct or a tuple of the `Rust` representation, and so its size.
  Where `u16` has alignment 2, `struct Triple(u8, u16, u8)` may be 4 or 6 bytes. `rustc` makes it
  4 bytes, and [`-Zrandomize-layout`] on `nightly-2026-11-17` makes it 6 bytes under some seeds.
  The order that this flag chooses depends on the seed and on the type's path, its crate
  included. The flag does not move the fields of a tuple, though Rust allows it.
- The tag of an enum of the `Rust` representation: where and how its discriminant is stored, and
  its values ([`r-items.enum.discriminant.repr-rust`], [`mem::discriminant`]).
- The niches of `Option` and `Result`, except where `core` documents them.

`puns` always reads the layout that `rustc` chose for the build. The `unstable` feature decides
whether a pun may also depend on that choice. Without it, the layouts that `puns` relies on are of
two kinds:

- *Specified*: Rust documents them ([What Rust documents][what_puns_relies_on#what-rust-documents]).
- *Assumed and checked when compiling*: `rustc` lays out an enum of the `Rust` representation by
  one algorithm, from the layouts of the fields of its variants, which Rust does not document.
  `puns` assumes that algorithm where its result is the same in every layout that Rust allows for
  the fields, computes the layout that it gives, and fails to compile if it is not the layout
  that `rustc` chose ([What `puns` assumes](#what-puns-assumes)).

Every other unspecified layout is proven only by the same type, or field by field ([How a part
with an unspecified layout is
proven](#how-a-part-with-an-unspecified-layout-is-proven)).

## Layout independence

Without the `unstable` feature, `puns` has *layout independence*: whether a pun is [proven,
checked or refused][kinds], what it checks, and which values a check accepts are the same in
every layout that Rust allows. A pun that would depend on an unspecified layout is refused. The
exception is the layout of most enums of the `Rust` representation, `Option` and `Result`
included, which `puns` assumes and checks when compiling: a compiler that lays one out otherwise
makes it fail to compile, rather than change what a pun does ([What `puns`
assumes](#what-puns-assumes)).

Layout independence assumes that the compiler places the fields of a struct as `rustc` does ([How
`rustc` places fields][what_puns_relies_on#how-rustc-places-fields]). Under another placement, a pun
could only fail to compile. It also assumes that the path of `puns` that the derive names is `puns`
itself: a crate that redirects it can make a verdict depend on how `rustc` arranged two types ([The
identity of a type and the compiler][trust-identity]).

Layout independence holds of a pun, which is a function and a pair, not of a type: of the rows of
`(bool, u8)` below, some are proven and some refused. It does not cover the message of a refusal,
which may name what `rustc` chose. `pun` of a `Triple` as a `[u8; 4]` is refused for its unspecified
layout, or, where `Triple` has 6 bytes, because the source is larger than the target. Nor does it
cover which field each byte holds, which is the compiler's choice with or without `puns`. A pun
returns a valid value whose fields depend on the layout, as [`transmute`] does:

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
| `pun` of a generic enum of the `Rust` representation, such as `enum E<T> { A(T), B(u16) }`, as itself | proven | the layout that `puns` assumes, or else the same type with the same parameters ([identity](#the-same-type)) |
| `pun` of a generic enum of the `Rust` representation whose parameter is only in a `PhantomData`, such as `enum P<T> { A(u8, PhantomData<T>), B(u16) }`, as itself | refused | the derive cannot name the identity of `T` ([identity](#the-same-type)) |
| `pun` of `[bool; 2]` as `(bool, u8)` | proven | either byte may be the `bool`, and both are one |
| `pun` of `[NonZero<u16>; 2]` as `(NonZero<u16>, u16)` | proven | both places hold a `NonZero<u16>` |
| `pun` of `(u8, u8)` as a `repr(C)` union of a struct of the `Rust` representation and a `[u8; 2]` | proven | the `[u8; 2]` takes any bytes |
| `try_pun` of `[u8; 2]` as `(bool, u8)` | refused | the byte that must be 0 or 1 is either byte |
| `head` of `(u16, u8)` as `[u8; 2]` | refused | the second byte is padding where the `u8` comes first, which Rust allows and `rustc` does not choose |
| `try_pun` of bytes as `struct P { a: u8, b: u8 }` with the rule `a < b` | refused | the rule reads `a`, which is either byte |
| `try_pun` of `(u8, u8)` as a `repr(C)` struct of a `bool` and a `u8` | refused | `(0, 2)` passes in one order only |
| `try_pun` of `(u8, u8)` as `NonZero<u16>` | refused | each byte that the check reads may hold either field |
| any pun of a `Triple` as another type, or of another type as a `Triple` | refused | it may be 4 or 6 bytes |
| `try_pun` of bytes as `Option<bool>`, or as an enum of the `Rust` representation of which `puns` assumes the layout | checked | the layout is assumed, and checked when compiling ([What `puns` assumes](#what-puns-assumes)) |
| `pun` of `Option<u16>` as a `repr(C)` struct of a `u16` tag and a `MaybeUninit<u16>` | proven | the tag 0 or 1, then the value, as assumed |
| `try_pun` of bytes as `Option<(bool, u16)>`, or as an enum with a variant `A(bool, bool)` | refused | where the niche is depends on the order of the fields |

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
can choose one, and the feature for types that you cannot give one, such as an `Option` of a
tuple whose niche depends on the order of its fields, or the types of another crate.

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
cannot pun: the pun depends on a part of the source or of the target with an unspecified layout (a struct or a tuple of the `Rust` representation, or an enum of it, `Option` and `Result` included, whose layout `puns` does not assume): another compiler, or `-Zrandomize-layout`, could arrange it otherwise. Only the same type at the same place proves it.
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
the same in every order, with no rule, is proven field by field, each field at every place where
it may be, as the rows of `[bool; 2]` and `[NonZero<u16>; 2]` show. A struct with a rule, which
names fields, is proven only by itself. An enum of the `Rust` representation, with fields or
without, whose layout `puns` does not assume, proves only the same type, and only the same type
proves it.

No check replaces a proof in a part with an unspecified layout, since the check would read where
the compiler may not have put the field. What is not proven there is refused. One check still
reads such a part, because its answer is the same in every layout: the check of a target union of
which the source proves a field after the first. Whatever the earlier fields do not accept, that
field does, so the union accepts every value of the source. The pun then compiles only if every
byte under the part is initialized in the source.

Niches are not guaranteed beyond what `core` documents of the representation of `Option` and
`Result` ([What Rust documents][what_puns_relies_on#what-rust-documents]). `Option<bool>` is one
byte with `None` at 2 only because `rustc` does it so today, so `puns` assumes it, and checks it
when compiling ([What `puns` assumes](#what-puns-assumes)). Niches that Rust documents only as
ABI-compatible are assumed in the same way, not taken from that documentation: `enum Ticket { Free,
Paid(NonZero<u32>) }` is laid out like `Option<NonZero<u32>>`, and the check compares it with the
layout that the assumption gives.

## What `puns` assumes

Rust documents neither where the tag of an enum of the `Rust` representation is, nor its values,
nor how an enum without fields is stored ([`r-layout.repr.rust.unspecified`]). `rustc` lays them
out by one algorithm, which has given the same layouts since Rust 1.65, from the layouts of the
fields of the variants. Without the `unstable` feature, `puns` assumes this algorithm, as follows,
where `-Zrandomize-layout` cannot change its result. `Option`, `Result` and the enums that derive
`Pun` follow it alike.

- *An enum without fields* is its discriminant, in the least integer of 1, 2, 4, 8 or 16 bytes
  that holds every discriminant, signed if one is negative, in the byte order of the target. It
  has no byte if it has one variant or none.
- A variant is *absent* if its fields have no values and each has size 0 and alignment 1. It
  takes no byte and no value of the tag. If one variant is not absent, the enum is its fields.
- *A tag* is at offset 0, an unsigned integer as wide as the least alignment of the first field
  of each variant, among the fields that do not have both size 0 and alignment 1, if an integer
  of that width has that alignment on the target, and at least wide enough for the index of every
  variant. Each variant is its index there, and its fields start at the first offset after the
  tag that their alignment allows. So `Option<u16>` is a `u16` tag, 0 for `None` and 1 for
  `Some`, then the value, and `Result<u32, u8>` a `u8` tag with `Ok` at 4 and `Err` at 1.
- *A niche* is a scalar in the fields of the largest variant (the last one, if several are as
  large), whose values Rust restricts. The other variants from the first to the last take
  consecutive values outside its *valid range*, the shortest range of values, wrapping past the
  greatest one to 0, that holds every value that Rust allows. One variant takes the value next to
  the range that is nearest to zero as a signed integer, the one before it on a tie, except after
  the range 0 to 1 of a `bool`, where it takes 2. Several variants take the values after the
  range, unless the range does not wrap and they fit before it, nearer to zero, or the values
  after it would wrap into it. So `None` is 2 in `Option<bool>`, 255 in `Option<Option<bool>>`,
  and `0xFFFF_FFFF` in `Option<char>`. Each other variant starts at 0 if it ends before the niche,
  and otherwise after it, where its alignment allows, and all must fit in the size of the largest
  variant. The niche of a type is the scalar with the most free values; the niche of an array is
  that of its first element.
- *Which one*: the niche if it makes the enum smaller than the tag, or as large with more free
  values left, and the tag otherwise.

The assumption applies to an enum whose every variant has at most one field, or fields that all
have the same alignment and give the same bytes in every order (all bytes of any value without
rules, or all of the same layout, fixed at any depth), and none of which has values that Rust
restricts. The niche must be in a scalar whose values for Rust `puns` knows (a `bool`, a `char`, a
`NonZero`, an enum without fields, or the tag of an enum whose layout Rust specifies or `puns`
assumes), at the same place in every layout that Rust allows, and no other scalar of its type may
have as many free values. On stable Rust, only `core` and `std` make niches (`bool`, `char`,
`NonZero`, references and other pointers that are never null, the tags of enums, and a few types
of `std`), and any other type has the niches of its fields. So the values that a scalar declared
by hand ([`Scalar`]) accepts never give a niche: its `Spec` may declare the type that its bytes
are ([`Spec::stored_as`]), such as the `NonZero` that a `repr(transparent)` struct holds, and the
niche is then that type's, in its own bytes. Otherwise the enum that would hold it there is not
assumed. A variant whose fields have no value but a size is not assumed either. Every other enum
of the `Rust` representation keeps a layout that Rust leaves to the compiler.

Where the assumption applies, the layout of the enum is computed from the layouts of its fields
and the alignments of the integers of the target, and compared in a `const` with the one that the
experiments found (`Layout::assumed`, and for an enum without fields, the `SPECIFIED` of the
`Scalar` that the derive implements): the size, the alignment, the place and the width of the
tag, the value of each variant, and where the fields of each variant start. If they differ,
`Pun::LAYOUT` of the enum fails to compile, with the message "an enum of the `Rust`
representation is not laid out as `puns` assumes", and the note of `rustc` names the enum. Enable
the feature then, which uses the layout that `rustc` chose.

The assumption never gives a layout that `puns` then uses. The layout of the enum is always the
one that the experiments found; the assumption only decides whether that layout counts as the
same in every layout that Rust allows, after the check shows that it is the predicted one. So a
wrong assumption, or a false [`Spec::stored_as`], fails to compile, and is not unsound.

The rule was measured on Rust 1.100.0 and `nightly-2026-11-17`, without `-Zrandomize-layout` and
with it under 16 seeds, and on `aarch64-apple-darwin`, `i686-unknown-linux-gnu` and
`s390x-unknown-linux-gnu`. The layouts that vary are those of variants of several fields: the
order of their fields, and the width of the tag where their alignments differ, which is why the
rule requires one alignment. The targets differ only by the alignments of their integers, which
the rule reads.

## The same type

Without the feature, a part with an unspecified layout that is proven neither field by field nor by
the layout that `puns` assumes is proven only by the same type at the same place. A type has one
layout in a program, but two types, however alike, may be arranged differently
([`offset_of!`, "Layout is subject to change"][offset_of layout], [`r-type.struct.layout`]). For
`struct Wrapper<A, B> { a: A, b: B, c: u16 }`, `-Zrandomize-layout` on `nightly-2026-11-17` orders
the fields of `Wrapper<u8, u8>` and `Wrapper<u8, i8>` alike under some seeds and differently under
others, depending on the crate and the path of the type as well.

Stable Rust cannot compare types in `const`, where `TypeId` cannot be compared and `type_name`
cannot be called. So `puns` gives a type an *identity*, a name together with the identities
of the types it is made of. An identity has two uses, and no other. It tells whether two parts
with an unspecified layout, proven neither field by field nor by the layout that `puns` assumes,
are of the same type. And it names a type without its layout, so that the identity of a generic
type is made of the identities of its parameters, and a type that holds itself through a generic
type has one. A layout that Rust specifies, or that `puns` assumes, is compared by its parts,
never by an identity. The primitive types, the types of [`num`], arrays, slices, tuples,
`Option`, `Result`, `MaybeUninit` and `ManuallyDrop` have identities. A part that is a
`ManuallyDrop<T>` is of the same type as a part that is a `T`, since `core` gives them one layout,
but a type made of a `ManuallyDrop<T>` is not made of a `T`: `-Zrandomize-layout` arranges
`Wrapper<ManuallyDrop<u8>, u16, u32>` and `Wrapper<u8, u16, u32>` differently. A type that derives
`Pun` is named by its module, file, line and column, its name, and the number of its derive in its
crate. An instance of a generic type that derives `Pun` is named, in addition, by the identity of
each of its type parameters and the value of each of its const parameters, in order, so a
`Pair<i32>` proves a `Pair<i32>`, and not a `Pair<u32>`. Its lifetimes do not count. The derive
can name the identity of a type parameter only if the parameter is bounded by `Pun` where it sees
it: in its bounds (`<T: Pun>`), in the `where` clause, in `#[pun(bound(..))]`, or by the bound
that it adds for a field of type `T`. A generic type with a type parameter that is not, such as
one that is only in a `PhantomData<T>`, has no identity, nor have a `PhantomData` and a
[`Scalar`] implemented by hand, nor a type with a parameter that has none. A part with an
unspecified layout that holds one of them proves itself only with the feature, unless it is proven
field by field, and the refusal says so. A type whose parts all have the same
layout in every build, such as a generic `repr(C)` struct of a `u8` and a `PhantomData<T>`,
needs no identity. A part that holds a scalar of [`Spec::maybe_uninit`] is the same type as
nothing, itself included. An identity only adds a condition, since the layouts must be the same
as well ([The identity of a type and the compiler][trust-identity]).

[`-Zrandomize-layout`]: https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/randomize-layout.html
[`mem::discriminant`]: https://doc.rust-lang.org/1.99.0/core/mem/fn.discriminant.html
[`r-items.enum.discriminant.repr-rust`]: https://doc.rust-lang.org/1.99.0/reference/items/enumerations.html#r-items.enum.discriminant.repr-rust
[`r-layout.guarantees`]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.guarantees
[`r-layout.repr.rust.unspecified`]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.rust.unspecified
[`r-type.struct.layout`]: https://doc.rust-lang.org/1.99.0/reference/types/struct.html#r-type.struct.layout
[`transmute`]: https://doc.rust-lang.org/1.99.0/core/mem/fn.transmute.html
[offset_of layout]: https://doc.rust-lang.org/1.99.0/core/mem/macro.offset_of.html#layout-is-subject-to-change

[`as_bytes`]: https://docs.rs/puns/0.2.4/puns/fn.as_bytes.html
[`layout::UNSTABLE`]: https://docs.rs/puns/0.2.4/puns/layout/constant.UNSTABLE.html
[`num`]: https://docs.rs/puns/0.2.4/puns/num/index.html
[`Scalar`]: https://docs.rs/puns/0.2.4/puns/trait.Scalar.html
[`Spec::maybe_uninit`]: https://docs.rs/puns/0.2.4/puns/layout/struct.Spec.html#method.maybe_uninit
[`Spec::stored_as`]: https://docs.rs/puns/0.2.4/puns/layout/struct.Spec.html#method.stored_as
[errors#unspecified-layouts]: errors.md#unspecified-layouts
[kinds]: how_puns_decides.md#proven-checked-and-refused
[rule]: how_puns_decides.md#layouts-and-rules
[what_puns_relies_on#how-rustc-places-fields]: what_puns_relies_on.md#how-rustc-places-fields
[what_puns_relies_on#what-rust-documents]: what_puns_relies_on.md#what-rust-documents
[trust-identity]: trust.md#the-identity-of-a-type-and-the-compiler
[unspecified layout]: how_puns_decides.md#unspecified-layouts
