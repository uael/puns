# What the derive trusts

`#[derive(Pun)]` writes `unsafe impl`s in the crate that derives it, where safe code can make any
path name something else, with `#[pun(crate = ..)]`, `#[pun(crate)]` or `extern crate other as
core;`. The generated code names `::core` and the root of `puns`, yet what a layout is made of
depends on no path that the deriving crate can redefine, except the identity of the type ([The
identity of a type and the compiler](#the-identity-of-a-type-and-the-compiler)). [The derive
against other paths and rewritten items][testing#the-derive-against-other-paths-and-rewritten-items]
says how each of these claims is tested.

## Sizes and offsets

The derive takes sizes and alignments from the compiler, and measures the offset of each field on
the type as compiled, except in a variant without a sample ([Enums with
fields](#enums-with-fields)). It does not use `offset_of!`, a macro named by a path, but projects a
pointer (`&raw const (*p).field`) and subtracts with `byte_offset_from`, which run only in const
evaluation. For a struct of the `Rust` representation and for a union, which are `Sized`, the
pointer is to an uninitialized value. A `repr(C)` or `repr(transparent)` struct may be unsized, so
the pointer is into uninitialized bytes that hold it with a trailing slice of no element: the
smallest power of two from 4096 bytes up, a local of const evaluation. A struct of any size is
measured, and if const evaluation cannot allocate those bytes, the build fails.

## Enums with fields

The derive reads offsets and tags on the [sample] of each variant, which a function of [`Layout`]
makes. A fake `MaybeUninit` or `Option`, named by a path, does not have the type that the functions
of `Layout` take or return, and does not compile.

With a primitive or `C` representation, the derive computes the layout that [RFC 2195] specifies
from the sizes and alignments of the fields, and checks it against the size and alignment of the
enum. It finds the offsets of the fields of a variant on its sample, by code that matches on it
with `byte_offset_from`, a pointer method that no path names. A variant without a sample has the
offsets that RFC 2195 specifies.

The tags are those of an enum without fields that the derive declares next to its implementation,
with the same representation and discriminants, `Self` replaced with the enum. There, a name that a
macro writes may name another item, so a discriminant may have another value. A function of
[`Variant`] compares each tag with the first bytes of the sample of its variant, so such a
discriminant, or reordered variants, fail the build.

A variant without a sample holds an [opaque scalar] or a union none of whose fields has values. If
a value of it may exist, its tag must be known from its declaration, as [Enums with fields][enums]
says, or the build fails (E0080) with "the tag of a variant without a sample is not known". Such a
tag has the same value in both enums, because the items that the derive declares there all have
names that start with `__Pun`, which it refuses in the input.

With the `Rust` representation, the derive finds the layout that `rustc` chose by
[experiments][rust#the-tag-of-an-enum-of-the-rust-representation] on the samples. Each sample is
written as a constructor expression, `Enum::Variant { field: .. }`, with the names that the derive
read, and the derive asserts that its discriminant is the index of its variant in the item that it
read. So reordered variants fail the build.

## The generated code

The functions that the generated code calls to make a layout are associated functions of
[`Layout`], [`Spec`], [`Rule`], [`Op`], [`Field`], [`Enum`], [`Variant`], [`Crc`], [`Checksum`],
[`Varint`] and the identity of a type, and methods of the values that they return. A hidden constant
of `Layout` fixes these types with `fn(T)` types that no path can replace, so a path that names a
module or another type for one of them does not compile. The description of an enum of the `Rust`
representation, whose type the generated code names, is what a function of `Layout` returns, and
the samples of an enum (`layout::Samples`) are what functions of `Layout` take, so only the real
types compile.

Nothing else that the derive names through the path of `puns` makes a layout. A trait that it
implements through the path is the trait of `puns`: otherwise the type does not implement that
trait of `puns`, or the implementation of `Pun` does not compile. The lists of fields and of rules
that it writes as types are checked against the layout. The functions of a view, the methods of
the regions and the getters call functions named through the path, in code that holds no `unsafe`
block. A path that names other functions there changes what they return, but whatever it is, it is
a valid value of its type.

The trait that says where a field is, `Path`, is sealed, and where a list of fields says that a
field is, counted in the parts of the layout that a check may read, is checked against the layout.
A list of fields, of rules or of the operations of an expression that is not the layout's fails the
build, with "not where the field is", "not the rules of the struct" or "not the operations of the
expression".

The derive writes the type of each field once, as an associated type of a trait that it declares
and implements for the type, where `Self` is the type, as in its declaration. The rest of the
generated code names that associated type. A function that is never called compiles only if the
type as compiled has the fields and variants that the derive read and no other, by an exhaustive
pattern, and if each field has exactly its associated type, since a `&mut` to a `*mut` to the field
admits no coercion and no subtype. So a macro in the type of a field, which expands where `Self`
and the names that start with `__Pun` name other items, gives the field one type, or the type does
not compile.

## The identity of a type and the compiler

The identity of a type, which says whether two layouts are of the same type, is the one exception.
A macro that the derive names through the path of `puns` writes it where the type is declared, so a
crate that redirects that path can give a type the identity of another. Two layouts count as the
same type only if they are also equal, so this cannot make a wrong layout. It can make a verdict
depend on how `rustc` arranged the two types in this compilation, which without the `unstable`
feature no other verdict does ([The same type][same]).

The derive names no other item of `::core` than these:

- `Option` with `Some` and `None`, `MaybeUninit`, and the integers `u8`, `u128` and `usize`, as
  types only, which are checked by the types of the traits that it implements and of the functions
  of `puns` that it calls.
- `PhantomData`, only as the field of the types that the derive declares as markers, whose bytes
  nothing reads.
- `Result`, only in the signatures of safe functions, the methods of regions and the functions of
  a view. A fake one changes only these signatures.
- `Sized`, only as a bound. An unsized type still fails to build where the generated code passes it
  to a function of `puns` that needs the real `Sized`.
- `ptr::null()`, the probe of a sized type, which nothing reads, and `panic!`, written only next to
  a compile error of the derive.

It also names `bool`, `u128` and `usize` without a path, where an item of the deriving crate may
have one of these names: they are checked by the types as the integers above. And it derives
`Clone` and `Copy`, by these names, for the enum of tags that it declares next to the
implementation, of which no function of `puns` requires either trait.

The derive trusts the compiler for what it parses, such as `repr`. From the `repr` that it read, it
takes whether Rust specifies the layout, on which a verdict without the `unstable` feature may rely
([Layout independence][independence]).

## Attribute macros below the derive

An attribute macro written below `#[derive(Pun)]` is expanded after the derive read the item, and
can change what `rustc` compiles. The derive does not compile if such an attribute adds, removes or
renames a field, changes the type of a field, or adds or removes a variant. A field that it adds to
a union is not refused, which is sound: the layout does not accept the field's values in a target
union, and guarantees nothing of a source union.

Where such an attribute changes the order of the fields or the `repr`, the layout is that of the
type as compiled, or the build fails, since the derive measures the offsets and checks the tags and
the variants on the samples, as above. It also checks the width of the tag of an enum of a
primitive or `C` representation. In a copy of each sample, the byte after the tag that the derive
read is set to 0, then to 255, and the variant of the copy, which const evaluation reads from the
tag alone ([The variant read from the tag][rust#the-variant-read-from-the-tag]), must stay the
same. So a wider tag fails the build. A narrower one leaves a byte of padding or of a field where
the derive reads its tag, so the comparison with the samples or the size of the enum fails the
build.

What such an attribute can still change without failing the build:

- The offsets of a variant without a sample, and the width of the tag of an enum none of whose
  variants has a sample, which are not checked.
- The offsets of a struct or a union whose `repr(C)` such an attribute removes. They are those of
  this compilation, and are taken to hold in every compilation, which Rust does not promise.

[RFC 2195]: https://rust-lang.github.io/rfcs/2195-really-tagged-unions.html
[enums]: https://docs.rs/puns/0.2.4/puns/derive.Pun.html#enums-with-fields
[sample]: https://docs.rs/puns/0.2.4/puns/derive.Pun.html#enums-with-fields
[`Checksum`]: https://docs.rs/puns/0.2.4/puns/layout/enum.Checksum.html
[`Crc`]: https://docs.rs/puns/0.2.4/puns/layout/struct.Crc.html
[`Enum`]: https://docs.rs/puns/0.2.4/puns/layout/struct.Enum.html
[`Field`]: https://docs.rs/puns/0.2.4/puns/layout/struct.Field.html
[`Layout`]: https://docs.rs/puns/0.2.4/puns/layout/struct.Layout.html
[`Op`]: https://docs.rs/puns/0.2.4/puns/layout/enum.Op.html
[`Rule`]: https://docs.rs/puns/0.2.4/puns/layout/enum.Rule.html
[`Spec`]: https://docs.rs/puns/0.2.4/puns/layout/struct.Spec.html
[`Varint`]: https://docs.rs/puns/0.2.4/puns/layout/enum.Varint.html
[`Variant`]: https://docs.rs/puns/0.2.4/puns/layout/struct.Variant.html
[independence]: layouts.md#layout-independence
[opaque scalar]: concepts.md#opaque-scalars
[rust#the-tag-of-an-enum-of-the-rust-representation]: rust.md#the-tag-of-an-enum-of-the-rust-representation
[rust#the-variant-read-from-the-tag]: rust.md#the-variant-read-from-the-tag
[same]: layouts.md#the-same-type
[testing#the-derive-against-other-paths-and-rewritten-items]: testing.md#the-derive-against-other-paths-and-rewritten-items
