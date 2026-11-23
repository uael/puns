# What `puns` relies on

`puns` builds each layout in `const fn`s from what the compiler reports: `size_of` and `align_of`,
the offsets of fields, and `size_of_val_raw` for an unsized type. What it concludes from them relies
on what Rust documents, and on seven facts that no documentation states: that a value whose fields
are valid is valid, and six behaviors of `rustc`.

## What Rust documents

Each row is a guarantee that `puns` relies on, and where Rust states it: "R" is the
[Reference][reference] and "C" the documentation of [`core`], both of Rust 1.99, the minimum
version of `puns`, since the guarantee must hold there. One rule is only in the nightly Reference,
and one only from 1.100, as marked.

| Guarantee | Where |
|---|---|
| A size is a multiple of the alignment, and the stride of an array | R [`r-layout.properties.size`][size] |
| `bool`, the integers, the floats and `char` have the sizes that the Reference lists, and `usize` and `isize` the same size | R [`r-layout.primitive.size`][primitive size], [`r-layout.primitive.size-align`][primitive size-align] |
| The alignment of a primitive is that of the platform, so `puns` reads it | R [`r-layout.primitive.platform-specific-alignment`][primitive align] |
| Any initialized bytes are an integer or a float | R [`r-type.numeric.validity`][numeric validity] |
| `bool` is one byte, 0 or 1 | R [`r-type.bool.layout`][bool layout], [`r-type.bool.repr`][bool repr] |
| `char` is a `u32` that is a Unicode scalar value | R [`r-type.char.value`][char value], [`r-type.char.layout`][char layout] |
| `[T; N]` is `N` elements one after the other, `[T]` is laid out as the array, and `str` as `[u8]` | R [`r-layout.array`][array], [`r-layout.slice`][slice], [`r-layout.str`][str] |
| `()` has size 0 and alignment 1. `!` has the same, and no value | R [`r-layout.tuple.unit`][unit], [`r-undefined.validity.never`][validity never], nightly [`r-type.never.layout`][never layout] |
| A struct, a tuple and an array require their fields or elements to be valid, and padding may be uninitialized | R [`r-undefined.validity.struct`][validity struct], [`r-undefined.validity.undef`][validity undef] |
| An enum requires a valid discriminant, and valid fields of the variant that it indicates | R [`r-undefined.validity.enum`][validity enum] |
| A `repr(C)` struct has its fields in order, each at the next offset that its alignment allows | R [`r-layout.repr.c.struct`][c struct] |
| A `repr(C)` union has every field at 0 | R [`r-layout.repr.c.union.size-align`][c union] |
| An enum without fields of `repr(C)` has the size of a C enum of the target. One of a primitive representation has the size of that integer, and holds its discriminant there | R [`r-layout.repr.c.enum`][c enum], [`r-layout.repr.primitive.enum`][primitive enum], [`r-items.enum.discriminant.access-memory`][discriminant memory] |
| An enum with fields, `repr(C)`, of a primitive representation or both, is made of `repr(C)` structs that start with the tag ([RFC 2195]) | R [`r-layout.repr.c.adt`][c adt], [`r-layout.repr.primitive.adt`][primitive adt], [`r-layout.repr.primitive-c`][primitive c] |
| `repr(transparent)` has the layout of its field | R [`r-layout.repr.transparent.layout-abi`][transparent] |
| `packed(N)` lowers the alignment of each field to at most `N`, and `align(N)` raises the alignment to `N` | R [`r-layout.repr.alignment`][alignment] |
| The `Rust` representation puts each field at a multiple of its alignment, and the type is at least as aligned as its fields | R [`r-layout.repr.rust.layout`][rust layout] |
| The fields of a struct of the `Rust` representation do not overlap, in an order that is not specified | R [`r-layout.repr.rust.layout.struct`][rust struct] |
| A struct of the `Rust` representation whose fields have size 0 has size 0, as does an enum without variants or of one such variant | R [`r-layout.repr.rust.struct-zst`][struct zst], [`r-layout.repr.rust.enum-struct-like-zst`][enum struct-like], 1.100 [`r-layout.repr.rust.enum-empty-zst`][enum empty] |
| The `Rust` representation guarantees nothing else | R [`r-layout.repr.rust.unspecified`][rust unspecified] |
| A tuple has the `Rust` representation | R [`r-layout.tuple.def`][tuple] |
| The representation of a type does not change the layout of its fields | R [`r-layout.repr.inter-field`][inter-field] |
| `NonZero<T>` is `T`, except zero | C [`NonZero`, "Layout"][nonzero layout] |
| `Option<T>` is `T`, and `None` is zeros, for a `NonZero` and a `repr(transparent)` struct around one | C [`option`, "Representation"][option representation] |
| `Result<T, E>` has the size of `T`, and `Ok(t)` the bytes of `t`, when `T` is such a type and `E` has size 0 and alignment 1, or the reverse | C [`result`, "Representation"][result representation] |
| `ManuallyDrop<T>` has the layout and the valid values of `T` | C [`ManuallyDrop`] |
| `MaybeUninit<T>` has the size and the alignment of `T`, and any bytes are a value | C [`MaybeUninit`, "Layout"][maybe-uninit layout], [`MaybeUninit`, "Validity"][maybe-uninit validity] |
| `PhantomData<T>` has size 0 and alignment 1 | C [`PhantomData`, "Layout"][phantom layout] |

Which bytes a union may hold is not decided ([`r-undefined.validity.union`][validity union]), and
[Unions][soundness#unions] says what `puns` does.

Rust also documents that an "option-like" enum, as [`core` defines it][abi compatibility], is
ABI-compatible with the type of the `Option` row that it holds. `puns` does not use it, because it
gives the size of the enum but not which bytes its other variant is. `Option<bool>` is one byte
with `None` at 2 only because `rustc` does it today, and the documentation of `MaybeUninit` and
`UnsafeCell` says that such layouts vary.

## What no documentation states

### A value whose fields are valid

The Reference states what validity requires of a struct, a tuple, an array and an enum (the rows
above), and warns that its list of undefined behavior "is not exhaustive; it may grow or shrink"
([`r-undefined`][undefined]). `puns` takes these conditions to be all that validity requires. A
struct, a tuple, an array or a slice is valid if its fields or its elements are, whatever its
padding holds, and an enum if its tag is that of a variant whose fields are valid.

This does not hold of every type of `core`. `NonZero` restricts the values of its field with an
attribute that stable Rust does not accept, so `puns` describes it by its documentation (the
`NonZero` row). `puns` describes by their fields only arrays, slices, tuples, `Option`, `Result`,
and the types that derive `Pun`.

No test checks this rule of the language. The oracles of the model check take it too
([`types.rs`][model types]), and so does Miri when it checks that a value is valid
([Miri][testing#miri]).

### Pointers to unsized types

A pointer to an unsized type, a struct that ends with a slice or a `str`, has the representation of
a pointer to a slice: an address and the length of the tail. For each unsized type, a pointer made
from a pointer to a slice by `as` casts must give back its address and its length, or the build
fails.

### The tag of an enum of the `Rust` representation

`rustc` tells the variant of such an enum by one scalar at a fixed place, a direct tag or a niche,
as [Enums with fields][derive#enums-with-fields] requires. The direct tag is all the bytes before
the first field of any variant. The derive finds the tag by *experiments* in const evaluation on a
value of each variant, so the tag is where `rustc` put it. The experiments verify what they assume,
and one that fails stops the build, so no wrong layout is used. If `rustc` places the fields of an
uninhabited variant before those of the other variants, the build fails, because `puns` cannot
observe where those fields are and does not guess.

### Errors of const evaluation

Const evaluation reports an error where it would otherwise have undefined behavior, in three cases:
when it reads an uninitialized byte, when it reads the variant of a value whose tag is not the tag
of a variant, and when it computes the size of a slice larger than an object can be. Code that
relies on them runs only at compile time:

- The experiments rely on the first two.
- The derive relies on the first to refuse an enum without fields whose `align` gives it padding,
  since it reads each variant from all of its bytes.
- An `Option` of an [opaque scalar] relies on the first to refuse a `None` with an uninitialized
  byte.
- The derive relies on the third for an unsized struct whose tail is so large that 8 elements of
  it overflow an `isize`. It compares the layout of an unsized struct with the sizes that `rustc`
  gives for tails of up to 8 elements.

[Compile-fail tests][testing#compile-fail-tests] check each of the three errors.

### The variant read from the tag

Const evaluation reads the variant of a value from its tag alone, when the tag is that of a
variant, and reads no other byte of the value. Yet `mem::discriminant` takes a reference to the
whole value, and the Reference requires a reference to point to a valid value
([`r-undefined.validity.reference-box`][validity reference]). The experiments rely on it when they
move the tag of one variant into a value of another. The derive also relies on it to check the
width of the tag of a primitive or `C` enum ([Attribute macros below the
derive][trust#attribute-macros-below-the-derive]). The [model check][testing#the-model-check]
checks the layouts that the experiments find.

### The address read as an integer

To check the alignment of an address at run time, `puns` reinterprets the pointer as a `usize`
through a union, since no stable `const fn` gives the address on Rust 1.99. Of this, `core` says
([`transmute`][transmute]):

> Transmuting pointers to integers in a const context is undefined behavior, unless the pointer
> was originally created from an integer. [...] (And even outside `const`, such transmutation is
> touching on many unspecified aspects of the Rust memory model and should be avoided. [...])

At run time, `core` does not call it undefined behavior, but says to avoid it, and no documentation
promises that the integer is the address. The integer is never made into a pointer again. A check
of an address does not compile in a constant ([In a constant][functions#in-a-constant]). The tests
run it under Miri with strict provenance ([Miri][testing#miri]).

### How `rustc` places fields

`rustc` places the fields of a struct of the `Rust` representation one after another, in an order
of its choosing, each at the first offset that its alignment allows. So the sizes that it may give
are those of the orders of the fields, which `puns` tries up to a
[bound][limits#bounds-of-the-analysis]. No documentation states this. Rust only bounds the offsets
([`r-layout.repr.rust.layout`][rust layout]), and a compiler could put padding where no alignment
needs it. The size that `puns` reads would then match no order, so the struct would no longer count
as the same in every layout, and the pun would fail to compile rather than change what it does.

The [record of verdicts][testing#the-record-of-verdicts] runs under
[`-Zrandomize-layout`][randomize]. This flag rearranges fields "within the degrees of freedom
provided by the largely unspecified default representation", which "also affects type sizes and
padding". On `nightly-2026-11-17`, it shuffles the order of the fields and does nothing else
([`rustc_abi`][shuffle]), so it changes no alignment. Its comment says that it may do more in the
future.

[reference]: https://doc.rust-lang.org/1.99.0/reference/
[`core`]: https://doc.rust-lang.org/1.99.0/core/
[RFC 2195]: https://rust-lang.github.io/rfcs/2195-really-tagged-unions.html
[randomize]: https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/randomize-layout.html
[shuffle]: https://github.com/rust-lang/rust/blob/5df8c02afdf6b882bc301442cea70f026e3d3974/compiler/rustc_abi/src/layout.rs#L616-L638
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
[undefined]: https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined
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
[enum empty]: https://doc.rust-lang.org/1.100.0/reference/type-layout.html#r-layout.repr.rust.enum-empty-zst
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

[derive#enums-with-fields]: https://docs.rs/puns/0.2.4/puns/derive.Pun.html#enums-with-fields
[model types]: https://github.com/uael/puns/blob/v0.2.4/puns-model/src/types.rs

[functions#in-a-constant]: functions.md#in-a-constant
[limits#bounds-of-the-analysis]: limits.md#bounds-of-the-analysis
[opaque scalar]: concepts.md#unspecified-layouts-opaque-scalars-and-uninhabited-types
[soundness#unions]: soundness.md#unions
[testing#compile-fail-tests]: testing.md#compile-fail-tests
[testing#miri]: testing.md#miri
[testing#the-model-check]: testing.md#the-model-check
[testing#the-record-of-verdicts]: testing.md#the-record-of-verdicts
[trust#attribute-macros-below-the-derive]: trust.md#attribute-macros-below-the-derive
