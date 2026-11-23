# The `unstable` feature: what remains to move

This file holds the statements of the former chapter "The `unstable` feature" whose homes are the
chapters "What `puns` relies on" and "Testing", and two statements of the former chapter
"Limitations" whose home is "What `puns` relies on". Those chapters take them, and this file is
then deleted.

## How `rustc` lays out the tag of an enum

The direct tag of an enum of the `Rust` representation, which is all the bytes before the first
field of any variant, must be 1, 2, 4, 8 or 16 bytes, since `rustc` makes no other size, or the
enum is refused. If `rustc` places the fields of a variant without values before the fields of the
other variants, the build fails, because `puns` cannot observe where those fields are and does
not guess.

## What this relies on

`rustc` places the fields of a struct of the `Rust` representation one after another, each at
the first offset its alignment allows, in an order of its choosing. The sizes it may give are
therefore the sizes of the possible orders of the fields, which `puns` tries up to a
[bound](limits.md#bounds-of-the-analysis). `-Zrandomize-layout` draws the order at random and does nothing else. No documentation states
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

`Option<bool>` is one byte with `None` at 2, but only because that is how `rustc` does it today,
and the documentation of `MaybeUninit` and `UnsafeCell` says that such optimizations vary. So
`Option<bool>` needs the `unstable` feature.

## How it is tested

The tests of `puns` run both without the feature and with it.

- Without it, a test records the verdict of every function on every pair of 33 types. These are
  the types of the table in [Puns without the feature](layouts.md#puns-without-the-feature),
  along with what holds them: an enum and a union of a specified representation, a struct with a rule,
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
