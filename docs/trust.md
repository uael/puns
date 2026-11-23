# What the derive trusts

`#[derive(Pun)]` writes `unsafe impl`s in the crate that derives it, where safe code can make any
path name something else, with `#[pun(crate = ..)]`, `#[pun(crate)]` or `extern crate other as
core;`. The generated code names `::core` and the root of `puns`, yet what a layout is made of
depends on no path that the deriving crate can redefine, except the identity of the type ([The
identity of a type and the compiler](#the-identity-of-a-type-and-the-compiler)). [The derive
against a fake `core`][testing#the-derive-against-a-fake-core] checks this.

## Sizes and offsets

The derive takes sizes and alignments from the compiler, and measures the offset of each field on
the type as compiled, except where the `repr` that it read gives the offset. It does not use
`offset_of!`, a macro named by a path, but projects a pointer (`&raw const (*p).field`) and
subtracts with `byte_offset_from`, which run only in const evaluation. For a struct of the `Rust`
representation and for a union, which are `Sized`, the pointer is to an uninitialized value. A
`repr(C)` or `repr(transparent)` struct may be unsized, so the pointer is into 4096 bytes of memory
of `puns`, with a tail of no element. A struct that is larger than that with such a tail keeps the
offsets that its `repr` specifies: those computed from the sizes and alignments of the fields for
`repr(C)`, and 0 for `repr(transparent)`.

## Enums with fields

The derive reads offsets and tags on the *sample* of each variant, its [least value][least], which
a function of [`Layout`] makes. A fake `MaybeUninit`, or a fake result type, named by a path does
not have the type that the functions of `Layout` take or return, and does not compile.

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
tag has the same value in both enums, because there the derive names only items whose names start
with `__Pun`, which it refuses in the input.

With the `Rust` representation, the derive finds the layout that `rustc` chose by
[experiments][rust#the-tag-of-an-enum-of-the-rust-representation] on the samples. Each sample is
written as a constructor expression, `Enum::Variant { field: .. }`, with the names that the derive
read, and the derive asserts that its discriminant is the index of its variant in the item that it
read. So reordered variants fail the build.

## The generated code

The generated code calls associated functions of the types that the implemented traits name
([`Layout`] and [`Spec`]), and of [`Rule`], [`Op`], [`Field`], [`Enum`] and [`Variant`], which a
hidden constant of `Layout` fixes with `fn(T)` types that no path can replace. It also calls `new`
of [`Crc`] and of the identity of a type, whose results go to these functions, so only the real
types compile.

The trait that says where a field is, `Path`, is sealed, and the leaf at which a list of fields
places a field is checked against the layout.

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

- `Option` with `Some` and `None`, `MaybeUninit`, and the integers `u8`, `u128` and `usize`, which
  are checked by the types of the traits that it implements and of the functions of `puns` that it
  calls.
- `PhantomData`, only as the field of the types that the derive declares as markers, whose bytes
  nothing reads.
- `Result`, only in the signatures of safe functions, the accessors of regions and the functions of
  a view. A fake one changes only these signatures.
- `Sized`, only as a bound. An unsized type still fails to build where the generated code passes it
  to a function of `puns` that needs the real `Sized`.
- `ptr::null()`, the probe of a sized type, which nothing reads, and `panic!`, written only next to
  a compile error of the derive.

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
the variants on the samples, as above. It also checks the width of the tag of a primitive or `C`
enum. In a copy of each sample, the byte after the tag that the derive read is set to 0, then to
255, and the variant of the copy, which const evaluation reads from the tag alone ([The variant
read from the tag][rust#the-variant-read-from-the-tag]), must stay the same. So a wider tag fails
the build. A narrower one leaves a byte of padding or of a field where the derive reads its tag, so
the comparison with the samples or the size of the enum fails the build.

What such an attribute can still change without failing the build:

- The offsets of a variant without a sample, and the width of the tag of an enum none of whose
  variants has a sample, which are not checked.
- The offsets of a struct or a union whose `repr(C)` such an attribute removes. They are those of
  this compilation, and are taken to hold in every compilation, which Rust does not promise.

[RFC 2195]: https://rust-lang.github.io/rfcs/2195-really-tagged-unions.html
[enums]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html#enums-with-fields
[least]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html#enums-with-fields
[`Crc`]: https://docs.rs/puns/0.2.3/puns/layout/struct.Crc.html
[`Enum`]: https://docs.rs/puns/0.2.3/puns/layout/struct.Enum.html
[`Field`]: https://docs.rs/puns/0.2.3/puns/layout/struct.Field.html
[`Layout`]: https://docs.rs/puns/0.2.3/puns/layout/struct.Layout.html
[`Op`]: https://docs.rs/puns/0.2.3/puns/layout/enum.Op.html
[`Rule`]: https://docs.rs/puns/0.2.3/puns/layout/enum.Rule.html
[`Spec`]: https://docs.rs/puns/0.2.3/puns/layout/struct.Spec.html
[`Variant`]: https://docs.rs/puns/0.2.3/puns/layout/struct.Variant.html
[independence]: layouts.md#layout-independence
[opaque scalar]: concepts.md#unspecified-layouts-opaque-scalars-and-uninhabited-types
[rust#the-tag-of-an-enum-of-the-rust-representation]: rust.md#the-tag-of-an-enum-of-the-rust-representation
[rust#the-variant-read-from-the-tag]: rust.md#the-variant-read-from-the-tag
[same]: layouts.md#the-same-type
[testing#the-derive-against-a-fake-core]: testing.md#the-derive-against-a-fake-core
