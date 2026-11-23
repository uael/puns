# What `puns` relies on

`puns` builds each layout in `const fn`s from what the compiler reports: `size_of` and `align_of`,
the offsets of fields, and `size_of_val_raw` for an unsized type. What it concludes from them relies
on what Rust documents, and on a few behaviors of `rustc` that no documentation states. This chapter
lists both, with how each behavior is checked.

## What Rust documents

Each row is a guarantee that `puns` relies on, and where Rust states it: "R" is the
[Reference][reference] of Rust 1.99, and "C" the documentation of [`core`] 1.99. Two rules are
only in the nightly Reference, as marked.

| Guarantee | Where |
|---|---|
| A size is a multiple of the alignment, and is the distance between the elements of an array | R [`r-layout.properties.size`][size] |
| The sizes of `bool`, integers, floats and `char`, and `usize` and `isize` have the same size | R [`r-layout.primitive.size`][primitive size], [`r-layout.primitive.size-align`][primitive size-align] |
| The alignment of a primitive is that of the platform, so `puns` reads it | R [`r-layout.primitive.platform-specific-alignment`][primitive align] |
| Any initialized bytes are an integer or a float | R [`r-type.numeric.validity`][numeric validity] |
| `bool` is one byte, 0 or 1 | R [`r-type.bool.layout`][bool layout], [`r-type.bool.repr`][bool repr] |
| `char` is a `u32` that is a Unicode scalar value | R [`r-type.char.value`][char value], [`r-type.char.layout`][char layout] |
| `[T; N]` is `N` elements one after the other, `[T]` is laid out as the array, and `str` as `[u8]` | R [`r-layout.array`][array], [`r-layout.slice`][slice], [`r-layout.str`][str] |
| `()` has size 0 and alignment 1. `!` has the same, and no value | R [`r-layout.tuple.unit`][unit], [`r-undefined.validity.never`][validity never], nightly [`r-type.never.layout`][never layout] |
| A value is valid if its fields or its elements are, and padding may be uninitialized | R [`r-undefined.validity.struct`][validity struct], [`r-undefined.validity.undef`][validity undef] |
| An enum has a valid discriminant, and the fields of its variant are valid | R [`r-undefined.validity.enum`][validity enum] |
| Which bytes a union may hold is not decided ([Unions][soundness#unions] says what `puns` does) | R [`r-undefined.validity.union`][validity union] |
| A `repr(C)` struct has its fields in order, each at the next offset that its alignment allows | R [`r-layout.repr.c.struct`][c struct] |
| A `repr(C)` union has every field at 0 | R [`r-layout.repr.c.union.size-align`][c union] |
| An enum without fields, `repr(C)` or of a primitive representation, has the size of that integer, and a primitive one holds its discriminant there | R [`r-layout.repr.c.enum`][c enum], [`r-layout.repr.primitive.enum`][primitive enum], [`r-items.enum.discriminant.access-memory`][discriminant memory] |
| An enum with fields, `repr(C)`, of a primitive representation or both, is made of `repr(C)` structs that start with the tag ([RFC 2195]) | R [`r-layout.repr.c.adt`][c adt], [`r-layout.repr.primitive.adt`][primitive adt], [`r-layout.repr.primitive-c`][primitive c] |
| `repr(transparent)` has the layout of its field | R [`r-layout.repr.transparent.layout-abi`][transparent] |
| `packed(N)` places each field at the smaller of `N` and its alignment, and `align(N)` raises the alignment to `N` | R [`r-layout.repr.alignment`][alignment] |
| The `Rust` representation puts each field at a multiple of its alignment, and the type is at least as aligned as its fields | R [`r-layout.repr.rust.layout`][rust layout] |
| The fields of a struct of the `Rust` representation do not overlap, in an order that is not specified | R [`r-layout.repr.rust.layout.struct`][rust struct] |
| A struct of the `Rust` representation whose fields have size 0 has size 0, and so does an enum of one variant whose fields have size 0, or without variants | R [`r-layout.repr.rust.struct-zst`][struct zst], [`r-layout.repr.rust.enum-struct-like-zst`][enum struct-like], nightly [`r-layout.repr.rust.enum-empty-zst`][enum empty] |
| The `Rust` representation guarantees nothing else | R [`r-layout.repr.rust.unspecified`][rust unspecified] |
| A tuple has the `Rust` representation | R [`r-layout.tuple.def`][tuple] |
| The representation of a type does not change the layout of its fields | R [`r-layout.repr.inter-field`][inter-field] |
| `NonZero<T>` is `T`, except zero | C [`NonZero`, "Layout"][nonzero layout] |
| `Option<T>` is `T`, and `None` is zeros, for a `NonZero` and a `repr(transparent)` struct around one | C [`option`, "Representation"][option representation] |
| `Result<T, E>` has the size of `T`, and `Ok(t)` the bytes of `t`, when `T` is such a type and `E` has size 0 and alignment 1, or the reverse | C [`result`, "Representation"][result representation] |
| `ManuallyDrop<T>` has the layout and the valid values of `T` | C [`ManuallyDrop`] |
| `MaybeUninit<T>` has the size and the alignment of `T`, and any bytes are a value | C [`MaybeUninit`, "Layout"][maybe-uninit layout], [`MaybeUninit`, "Validity"][maybe-uninit validity] |
| `PhantomData<T>` has size 0 and alignment 1 | C [`PhantomData`, "Layout"][phantom layout] |

Rust also documents that an enum of two variants, one that holds a type of the `Option` row and one
with no field of nonzero size, is [ABI-compatible][abi compatibility] with that type. `puns` does
not use it, since it gives the size of the enum but not which bytes its other variant is.
`Option<bool>` is one byte with `None` at 2 only because `rustc` does it today, and the
documentation of `MaybeUninit` and `UnsafeCell` says that such layouts vary.

## Behaviors of `rustc` that no documentation states

### Pointers to unsized types

A pointer to an unsized type, a struct that ends with a slice or a `str`, has the representation of
a pointer to a slice: an address and the length of the tail. This is checked at compile time for
each unsized type: a pointer made from a pointer to a slice by `as` casts must give back its address
and its length, or the build fails.

### Errors of const evaluation

Const evaluation reports an error where it would otherwise have undefined behavior, in three cases:
when it reads an uninitialized byte, when it reads the variant of a value whose tag is not the tag
of a variant, and when it computes the size of a slice larger than an object can be. Code that
relies on them runs only at compile time:

- The experiments that find the layout of an enum of the `Rust` representation rely on the first
  two.
- The derive relies on the first to refuse an enum without fields whose `align` gives it padding,
  since it reads each variant from all of its bytes.
- An `Option` of an [opaque scalar] relies on the first to refuse a `None` with an uninitialized
  byte.
- The derive relies on the third for an unsized struct whose tail is so large that 8 elements of
  it overflow an `isize`, since the layout of an unsized struct is compared with the sizes that
  `rustc` gives for tails of up to 8 elements.

Compile-fail tests check each of the three errors ([Testing][testing]).

### The variant read from the tag

Const evaluation reads the variant of a value from its tag alone, when the tag is that of a variant,
without reading or checking the other bytes of the value. Yet `mem::discriminant` takes a reference
to the whole value, and the Reference requires a reference to point to a valid value
([`r-undefined.validity.reference-box`][validity reference]). The experiments rely on it when they
move the tag of one variant into a value of another. The [model check][testing#the-model-check]
compares the layouts that they find with its oracles.

### The tag of an enum of the `Rust` representation

`rustc` tells the variant of such an enum by one scalar at a fixed place: a direct tag before the
fields of every variant, or a niche in the fields of one variant. `puns` finds it by experiments in
const evaluation on a value of each variant, so the tag is where `rustc` put it. The experiments
verify what they assume, and one that fails stops the build, so no wrong layout is used. The direct
tag, all the bytes before the first field of any variant, must have 1, 2, 4, 8 or 16 bytes, since
`rustc` makes no other size, or the enum is refused. If `rustc` places the fields of an uninhabited
variant before those of the other variants, the build fails, since `puns` cannot observe where those
fields are and does not guess.

### The address read as an integer

To check the alignment of an address at run time, `puns` reinterprets the pointer as a `usize`
through a union, since no stable `const fn` gives the address on Rust 1.99. Of this, `core` says
([`transmute`][transmute]):

> Transmuting pointers to integers in a const context is undefined behavior, unless the pointer
> was originally created from an integer. [...] (And even outside `const`, such transmutation is
> touching on many unspecified aspects of the Rust memory model and should be avoided. [...])

So it is not undefined behavior at run time, but no documentation promises that the integer is the
address. The integer is never made into a pointer again, and a check of an address does not compile
in a constant ([In a constant][functions#in-a-constant]). The tests run it under Miri with strict
provenance ([Testing][testing]).

### How `rustc` places fields

`rustc` places the fields of a struct of the `Rust` representation one after another, each at the
first offset its alignment allows, in an order of its choosing. The sizes it may give are those of
the orders of the fields, which `puns` tries up to a [bound][limits#bounds-of-the-analysis]. No
documentation states this: Rust only bounds the offsets ([`r-layout.repr.rust.layout`][rust
layout]), and a compiler could put padding where no alignment needs it. The size that `puns` reads
would then differ from that of every order, the struct would no longer count as the same in every
layout, and the pun would fail to compile rather than change what it does.
[`-Zrandomize-layout`][randomize], under which the record of verdicts runs, rearranges fields
"within the degrees of freedom provided by the largely unspecified default representation".

[reference]: https://doc.rust-lang.org/1.99.0/reference/
[`core`]: https://doc.rust-lang.org/1.99.0/core/
[RFC 2195]: https://rust-lang.github.io/rfcs/2195-really-tagged-unions.html
[randomize]: https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/randomize-layout.html
[transmute]: https://doc.rust-lang.org/1.99.0/core/mem/fn.transmute.html#transmutation-between-pointers-and-integers
[size]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.properties.size
[primitive size]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.primitive.size
[primitive size-align]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.primitive.size-align
[primitive align]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.primitive.platform-specific-alignment
[numeric validity]: https://doc.rust-lang.org/1.99.0/reference/types/numeric.html#r-type.numeric.validity
[bool layout]: https://doc.rust-lang.org/1.99.0/reference/types/boolean.html#r-type.bool.layout
[bool repr]: https://doc.rust-lang.org/1.99.0/reference/types/boolean.html#r-type.bool.repr
[char value]: https://doc.rust-lang.org/1.99.0/reference/types/char.html#r-type.char.value
[char layout]: https://doc.rust-lang.org/1.99.0/reference/types/char.html#r-type.char.layout
[array]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.array
[slice]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.slice
[str]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.str
[unit]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.tuple.unit
[validity never]: https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.never
[never layout]: https://doc.rust-lang.org/nightly/reference/types/never.html#r-type.never.layout
[validity struct]: https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.struct
[validity undef]: https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.undef
[validity enum]: https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.enum
[validity union]: https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.union
[validity reference]: https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.reference-box
[c struct]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.c.struct
[c union]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.c.union.size-align
[c enum]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.c.enum
[primitive enum]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.primitive.enum
[discriminant memory]: https://doc.rust-lang.org/1.99.0/reference/items/enumerations.html#r-items.enum.discriminant.access-memory
[c adt]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.c.adt
[primitive adt]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.primitive.adt
[primitive c]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.primitive-c
[transparent]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.transparent.layout-abi
[alignment]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.alignment
[rust layout]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.rust.layout
[rust struct]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.rust.layout.struct
[struct zst]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.rust.struct-zst
[enum struct-like]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.rust.enum-struct-like-zst
[enum empty]: https://doc.rust-lang.org/nightly/reference/type-layout.html#r-layout.repr.rust.enum-empty-zst
[rust unspecified]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.rust.unspecified
[tuple]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.tuple.def
[inter-field]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.inter-field
[nonzero layout]: https://doc.rust-lang.org/1.99.0/core/num/struct.NonZero.html#layout-1
[option representation]: https://doc.rust-lang.org/1.99.0/core/option/index.html#representation
[result representation]: https://doc.rust-lang.org/1.99.0/core/result/index.html#representation
[`ManuallyDrop`]: https://doc.rust-lang.org/1.99.0/core/mem/struct.ManuallyDrop.html
[maybe-uninit layout]: https://doc.rust-lang.org/1.99.0/core/mem/union.MaybeUninit.html#layout-1
[maybe-uninit validity]: https://doc.rust-lang.org/1.99.0/core/mem/union.MaybeUninit.html#validity
[phantom layout]: https://doc.rust-lang.org/1.99.0/core/marker/struct.PhantomData.html#layout-1
[abi compatibility]: https://doc.rust-lang.org/1.99.0/core/primitive.fn.html#abi-compatibility

[functions#in-a-constant]: functions.md#in-a-constant
[limits#bounds-of-the-analysis]: limits.md#bounds-of-the-analysis
[opaque scalar]: concepts.md#unspecified-layouts-opaque-scalars-and-uninhabited-types
[soundness#unions]: soundness.md#unions
[testing]: testing.md
[testing#the-model-check]: testing.md#the-model-check
