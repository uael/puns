# The `unstable` feature

Rust leaves parts of the layout of some types to the compiler
([`r-layout.guarantees`][layout guarantees],
[`r-layout.repr.rust.unspecified`][rust unspecified]), which may choose differently in its next
version, or at random under [`-Zrandomize-layout`]. These parts are:

- The order of the fields of a struct or tuple of the `Rust` representation, and therefore its
  size, so `(u8, u16, u8)` is 4 or 6 bytes.
- The position and the values of the tag, for an enum of the `Rust` representation.
- The niches of `Option` and `Result`, except where `core` documents them.

`puns` reads what the compiler chose, so a pun is sound for the build that compiles it, whatever
the choice was. The `unstable` feature decides whether a pun must also behave the same under any
other choice. [`layout::UNSTABLE`] is `true` if the feature is enabled.

## The rule

Without the `unstable` feature, a pun compiles only if `-Zrandomize-layout` cannot change what
it does (whether it compiles, what it checks, and which values a check accepts). The message of
a refusal is not part of this, and may name what the compiler chose. For example, reinterpreting
`(u8, u16, u8)` as `[u8; 4]` is refused because of its layout, or because of its size when it is
6 bytes.

With the feature, `puns` uses the layout that `rustc` chose. Within one build, a pun that
compiles without the feature also compiles with it, and does the same thing.

The rule applies to a pun, not to a type. Reinterpreting `(bool, u8)` as bytes is proven
whichever byte holds the `bool`, but reinterpreting bytes as `(bool, u8)` would need a check of
one byte or the other, so that pun is refused.

## What a check may read

A check never reads a byte of the source that holds a different field in another layout that
Rust allows. Such a check would read a different part of the same value, and could give a
different answer, so a check that reads a byte of a struct whose fields Rust may reorder is
refused. Only the bytes that a check reads count, as follows:

- The check of an enum reads its tag, then the fields of the variant that the tag selects. A
  byte under a variant therefore counts only for the values of the source whose tag is that
  variant's. This holds where the source has an enum whose tag is at the same place, with the
  same size and byte order.
- The elements of an array share their checks. Where the source has elements of the same size at
  the same places, the check of every element reads the same bytes of its element as the check
  of the first one. Otherwise, every byte under the array counts.
- A checked rule may read any byte of its struct, so every byte of that struct counts.

Which field a byte holds is not part of what a pun does. `pun::<[u8; 2], (u8, u8)>([1, 2])` is
`(1, 2)` or `(2, 1)`. Either is a valid value, and a pun promises a valid value and nothing
more, as `transmute` does. Which bytes a value of a struct of the `Rust` representation has is
the compiler's choice, with or without `puns`. But `puns` decides the result of a check on given
bytes, so that result is the same in every layout. For example, whether `try_pun::<(u8, u8),
F>((0, 2))` is `Ok`, for `F` a `repr(C)` struct of a `bool` and a `u8`, must not depend on which
byte holds the `0`.

## Puns without the feature

| Pun, without `unstable` | | Why |
|---|---|---|
| `[u8; 2]` as `(u8, u8)`, and back | proven | bytes of any value, in either order |
| `[u8; 2]` as `(bool, bool)` | checked | both bytes are a `bool`, in either order |
| `(bool, u8)` as `[u8; 2]`, `as_bytes` | proven | both bytes are initialized, in either order |
| `[u8; 4]` as `(u8, u16)` | proven | it only requires initialized bytes, wherever its padding is |
| a `repr(C)` struct that holds a `(bool, u8)` as itself, or its first bytes as the `(bool, u8)` | proven | the same type at the same place |
| an enum of the `Rust` representation as itself | proven | a type has one layout in a program |
| `[bool; 2]` as `(bool, u8)` | proven | the `bool` may be either byte, and both bytes are a `bool` |
| `[u8; 2]` as `(bool, u8)` | refused | the byte that must be 0 or 1 is either byte |
| `(u8, u8)` as `[bool; 2]` | refused | the check of each element reads a byte that, in another layout, holds the other field |
| the first two bytes of `(u8, u16)` | refused | the second byte is padding in one layout and not in another |
| bytes as a `struct P { a: u8, b: u8 }` with the rule `a < b` | refused | the rule reads `a`, which is either byte |
| `(u8, u8)` as a `repr(C)` struct of a `bool` and a `u8` | refused | the check reads one field or the other, and `(0, 2)` passes in one order only |
| `(u8, u8)` as `NonZero<u16>` | refused | the check reads both bytes, and each may hold either field |
| a pun from `(u8, u16, u8)` or to it, other than to itself | refused | it is 4 or 6 bytes |
| bytes as an enum of the `Rust` representation, or as `Option<bool>` | refused | nothing documents where its tag is, nor its values |
| `validate_but_never::<(bool, u8)>` | refused | the byte that it checks is either byte |

A struct or tuple of the `Rust` representation whose size is the same in every layout, and that
has no rule and no scalar that a pun never makes, is proven field by field. Since a field may be
anywhere, it must be proven at every place where it may be. So two `bool`s prove a `(bool, u8)`,
four initialized bytes prove a `(u8, u16)`, and two `NonZero<u16>` prove a `(NonZero<u16>,
u16)`. A struct with a rule, which names fields, is proven only by itself, as an enum is. A part
that holds a scalar that a pun never makes is not proven at all. No check replaces a proof in a
part whose layout Rust leaves to the compiler, because the check would read where the compiler
may not have put the field, so whatever is not proven there does not compile. Such a part is
still read by the check of a union that a later field proves, or of an enum whose variant holds
a scalar that a pun never makes. The answer is then the same in every layout, and the pun
compiles only if every byte under the part is initialized in the source.

## The same type

Without the feature, a part whose layout Rust leaves to the compiler, unless it is proven field
by field as above, is proven only by the same type at the same place. A type has one layout in a
program, but two types, however alike, may each be arranged differently ([`offset_of!`, "Layout
is subject to change"][offset_of layout]). `-Zrandomize-layout`, for example, arranges
`Wrapper<u8, u8>` and `Wrapper<u8, i8>` differently. Stable Rust cannot compare types in
`const`, since `TypeId` cannot be compared there and `type_name` is not a `const fn`.

So `puns` identifies a type by a name, together with the identities of the types it is made of.
The primitive types, the types of [`num`], arrays, slices, tuples, `Option`, `Result` and
`MaybeUninit` have identities, and `ManuallyDrop<T>` is `T`. A type that derives `Pun` is named
by where it is declared (the module, file, line and column), by its name, and by the number of
its derive in its crate. A generic type that derives `Pun`, a `PhantomData`, and a [`Scalar`]
implemented by hand have no identity. A part whose layout Rust leaves to the compiler and that
holds one of them therefore proves itself only with the feature, unless it is proven field by
field. A type whose every part has the same layout in every build, such as a generic `repr(C)`
struct of a `u8` and a `PhantomData<T>`, needs no identity. An identity can only refuse a pun,
because the layouts are compared as well, so two types that happened to share a name would still
be told apart by their layouts.

## What this relies on

`rustc` places the fields of a struct of the `Rust` representation one after another, each at
the first offset its alignment allows, in an order of its choosing. The sizes it may give are
therefore the sizes of the possible orders of the fields. `puns` tries them all for a struct of
up to 6 fields that have a size or an alignment greater than 1. With more such fields, it
assumes that the size depends on the order, unless the fields need no padding in any order.
`-Zrandomize-layout` draws the order at random and does nothing else. No documentation states
that `rustc` places fields this way. Rust only bounds the offsets
([`r-layout.repr.rust.layout`][rust layout]), and a compiler could put padding where no
alignment needs it. In that case the size that `puns` reads would differ from the size that
every order gives. The struct would no longer count as the same in every layout, so the pun
would fail to compile rather than change what it does.

The alignment of a type is read, not assumed. Rust gives the alignment of each primitive per
platform, and makes a struct at least as aligned as its fields. `-Zrandomize-layout` does not
change alignment.

## What Rust documents

The table lists each guarantee that `puns` relies on, and where Rust states it. "R" is the
[Reference][reference root], and "C" the documentation of [`core`].

| Guarantee | Where |
|---|---|
| Layout may change with each compilation, and only what is listed is guaranteed | R [`r-layout.guarantees`][layout guarantees] |
| A size is a multiple of the alignment, and the stride of an array | R [`r-layout.properties.size`][size] |
| The sizes of `bool`, integers, floats and `char`, and of `usize` and `isize` | R [`r-layout.primitive.size`][primitive size], [`r-layout.primitive.size-align`][primitive size-align] |
| The alignment of a primitive is that of the platform, read and not assumed | R [`r-layout.primitive.platform-specific-alignment`][primitive align] |
| Any initialized bytes are an integer or a float | R [`r-type.numeric.validity`][numeric validity] |
| `bool` is one byte, 0 or 1 | R [`r-type.bool.layout`][bool layout], [`r-type.bool.repr`][bool repr] |
| `char` is a `u32` that is a Unicode scalar value | R [`r-type.char.value`][char value], [`r-type.char.layout`][char layout] |
| `[T; N]` is `N` elements one after the other, `[T]` is laid out as the array, and `str` as `[u8]` | R [`r-layout.array`][array], [`r-layout.slice`][slice], [`r-layout.str`][str] |
| `()` has size 0 and alignment 1. `!` has the same, and no value | R [`r-layout.tuple.unit`][unit], [`r-type.never.layout`][never layout] (nightly Reference) |
| A value is valid if its fields, or its elements, are. Padding may be uninitialized | R [`r-undefined.validity.struct`][validity struct], [`r-undefined.validity.undef`][validity undef] |
| An enum has a valid discriminant, and the fields of its variant are valid | R [`r-undefined.validity.enum`][validity enum] |
| A `repr(C)` struct has its fields in order, each at the next offset that its alignment allows | R [`r-layout.repr.c.struct`][c struct] |
| A `repr(C)` union has every field at 0 | R [`r-layout.repr.c.union.size-align`][c union] |
| The validity of a union is not decided, so a target is one of its fields, and a source proves nothing | R [`r-undefined.validity.union`][union validity] |
| An enum without fields, `repr(C)` or of a primitive representation, is that integer, which holds the discriminant | R [`r-layout.repr.c.enum`][c enum], [`r-layout.repr.primitive.enum`][primitive enum], [`r-items.enum.discriminant.access-memory`][discriminant memory] |
| An enum with fields, `repr(C)`, of a primitive representation, or both ([RFC 2195]) | R [`r-layout.repr.c.adt`][c adt], [`r-layout.repr.primitive.adt`][primitive adt], [`r-layout.repr.primitive-c`][primitive c] |
| `repr(transparent)` has the layout of its field | R [`r-layout.repr.transparent.layout-abi`][transparent] |
| `packed(N)` and `align(N)` | R [`r-layout.repr.alignment`][alignment] |
| The `Rust` representation puts each field at a multiple of its alignment, and the type is at least as aligned as its fields | R [`r-layout.repr.rust.layout`][rust layout] |
| The fields of a struct of the `Rust` representation do not overlap, in an order that is not specified | R [`r-layout.repr.rust.struct`][rust struct] |
| A struct of the `Rust` representation whose fields have a size of 0 has none. So does an enum without variants, or of one variant whose fields have no size | R [`r-layout.repr.rust.struct-zst`][struct zst], [`r-layout.repr.rust.enum-empty-zst`][enum empty], [`r-layout.repr.rust.enum-struct-like-zst`][enum struct-like] |
| The `Rust` representation guarantees nothing else | R [`r-layout.repr.rust.unspecified`][rust unspecified] |
| A tuple has the `Rust` representation | R [`r-layout.tuple.def`][tuple] |
| The representation of a type does not change the layout of its fields | R [`r-layout.repr.inter-field`][inter-field] |
| A type has one layout in a program, and a similar type may have another | C [`offset_of!`, "Layout is subject to change"][offset_of layout], R [`r-type.struct.layout`][struct layout] |
| Where and how the discriminant of an enum of the `Rust` representation is stored is not defined | R [`r-items.enum.discriminant.type`][discriminant type], C [`mem::discriminant`] |
| The fields of a union of the `Rust` representation need not be at 0, so such a union is refused | R [`r-type.union.layout`][union layout] |
| `NonZero<T>` is `T`, except zero | C [`NonZero`, "Layout"][nonzero layout] |
| `Option<T>` is `T`, and `None` is zeros, for a `NonZero` and a `repr(transparent)` struct around one | C [`option`, "Representation"][option representation] |
| `Result<T, E>` has the size of `T`, and `Ok(t)` the bytes of `t`, when `T` is such a type and `E` has size 0 and alignment 1, or the reverse | C [`result`, "Representation"][result representation] |
| `ManuallyDrop<T>` has the layout and the valid values of `T` | C [`ManuallyDrop`] |
| `MaybeUninit<T>` has the size and the alignment of `T`, and any bytes are a value | C [`MaybeUninit`, "Layout"][maybe-uninit layout], [`MaybeUninit`, "Validity"][maybe-uninit validity] |
| `PhantomData<T>` has size 0 and alignment 1 | C [`PhantomData`, "Layout"][phantom layout] |

Rust also documents that an enum with two variants, one holding a type that `Option` is
documented for and one with no sized field, is [ABI-compatible][abi compatibility] with that
type. `puns` does not use it: it gives the size of the enum, but not which bytes its other
variant is.

Niches are not guaranteed beyond the rows for `Option` and `Result` above. `Option<bool>` is one
byte with `None` at 2, but only because that is how `rustc` does it today, and the documentation
of `MaybeUninit` and `UnsafeCell` says that such optimizations vary. So `Option<bool>` needs the
`unstable` feature. Niches that Rust documents only as "ABI-compatible" are not used either, so
an enum of the `Rust` representation with a variant that holds a `NonZero` and a variant without
fields, such as `enum Ticket { Free, Paid(NonZero<u32>) }`, needs the feature, even though
`rustc` lays it out like the corresponding `Option`.

## How it is tested

The tests of `puns` run both without the feature and with it.

- Without it, a test records the verdict of every function on every pair of 33 types. These are
  the types of the table in [Puns without the feature](#puns-without-the-feature), along with
  what holds them: an enum and a union of a specified representation, a struct with a rule,
  arrays, and a scalar that a pun never makes. The record covers each pair whole, and in part
  (by `&mut` and from the end). It also records which byte patterns each type accepts from
  bytes, but not the message of a refusal, which may name what `rustc` chose. A rule and an enum
  checked as a whole each have a type in the record. CI compares the record with the one in the
  repository, without `-Zrandomize-layout` and with it under three seeds, and it must be the
  same each time.
- Compile-fail tests check each refusal without the feature, along with its message, and
  compile-pass tests check what compiles. Other compile-fail tests run with the feature, and the
  rest are the same both ways.
- With the feature, the tests that need it run as well: enums of the `Rust` representation,
  `Option` and `Result`, and the tests of the layout that `rustc` chose. CI also builds the
  example `codegen`, which needs it. The benchmark needs it too, and is run by hand.

[reference root]: https://doc.rust-lang.org/reference/
[`core`]: https://doc.rust-lang.org/core/
[RFC 2195]: https://rust-lang.github.io/rfcs/2195-really-tagged-unions.html
[`-Zrandomize-layout`]: https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/randomize-layout.html
[offset_of layout]: https://doc.rust-lang.org/core/mem/macro.offset_of.html#layout-is-subject-to-change
[`mem::discriminant`]: https://doc.rust-lang.org/core/mem/fn.discriminant.html
[layout guarantees]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.guarantees
[size]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.properties.size
[primitive size]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.primitive.size
[primitive size-align]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.primitive.size-align
[primitive align]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.primitive.platform-specific-alignment
[numeric validity]: https://doc.rust-lang.org/reference/types/numeric.html#r-type.numeric.validity
[bool layout]: https://doc.rust-lang.org/reference/types/boolean.html#r-type.bool.layout
[bool repr]: https://doc.rust-lang.org/reference/types/boolean.html#r-type.bool.repr
[char value]: https://doc.rust-lang.org/reference/types/char.html#r-type.char.value
[char layout]: https://doc.rust-lang.org/reference/types/char.html#r-type.char.layout
[array]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.array
[slice]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.slice
[str]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.str
[unit]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.tuple.unit
[never layout]: https://doc.rust-lang.org/nightly/reference/types/never.html#r-type.never.layout
[validity struct]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity.struct
[validity undef]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity.undef
[validity enum]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity.enum
[union validity]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity.union
[c struct]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.c.struct
[c union]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.c.union.size-align
[c enum]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.c.enum
[primitive enum]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.primitive.enum
[discriminant memory]: https://doc.rust-lang.org/reference/items/enumerations.html#r-items.enum.discriminant.access-memory
[c adt]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.c.adt
[primitive adt]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.primitive.adt
[primitive c]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.primitive-c
[transparent]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.transparent.layout-abi
[alignment]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.alignment
[rust layout]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.rust.layout
[rust struct]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.rust.struct
[struct zst]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.rust.struct-zst
[enum empty]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.rust.enum-empty-zst
[enum struct-like]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.rust.enum-struct-like-zst
[rust unspecified]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.rust.unspecified
[tuple]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.tuple.def
[inter-field]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.inter-field
[struct layout]: https://doc.rust-lang.org/reference/types/struct.html#r-type.struct.layout
[discriminant type]: https://doc.rust-lang.org/reference/items/enumerations.html#r-items.enum.discriminant.type
[union layout]: https://doc.rust-lang.org/reference/types/union.html#r-type.union.layout
[nonzero layout]: https://doc.rust-lang.org/core/num/struct.NonZero.html#layout-1
[option representation]: https://doc.rust-lang.org/core/option/index.html#representation
[result representation]: https://doc.rust-lang.org/core/result/index.html#representation
[`ManuallyDrop`]: https://doc.rust-lang.org/core/mem/struct.ManuallyDrop.html
[maybe-uninit layout]: https://doc.rust-lang.org/core/mem/union.MaybeUninit.html#layout-1
[maybe-uninit validity]: https://doc.rust-lang.org/core/mem/union.MaybeUninit.html#validity
[phantom layout]: https://doc.rust-lang.org/core/marker/struct.PhantomData.html#layout-1
[abi compatibility]: https://doc.rust-lang.org/core/primitive.fn.html#abi-compatibility

[`layout::UNSTABLE`]: https://docs.rs/puns/0.2.3/puns/layout/constant.UNSTABLE.html
[`num`]: https://docs.rs/puns/0.2.3/puns/num/index.html
[`Scalar`]: https://docs.rs/puns/0.2.3/puns/trait.Scalar.html
