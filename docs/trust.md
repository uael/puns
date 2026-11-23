# What the derive trusts

`#[derive(Pun)]` writes `unsafe impl`s in the crate that derives it, where safe code can make any
path name something else. `#[pun(crate = ..)]` names any module, `#[pun(crate)]` the root of the
crate, and `extern crate other as core;` makes `::core` another crate. The generated code names
`::core` and the root of `puns`. Yet what a layout is made of depends on no path that the deriving
crate can redefine, except the identity of the type ([The identity of a type and the
compiler](#the-identity-of-a-type-and-the-compiler)). How, and what the derive still trusts,
follow. [Testing][testing] says how this is checked, with a `core` whose items give wrong answers.

## Sizes and offsets

The derive takes sizes and alignments from the compiler. It measures the offset of each field on
the type as compiled, except where the `repr` that it read gives the offset, as stated below. A
measure does not use `offset_of!`, a macro named by a path. It projects a pointer (`&raw const
(*p).field`) and subtracts with `byte_offset_from`, which run only in const evaluation. For a
struct of the `Rust` representation and for a union, which are `Sized`, the pointer is to an
uninitialized value. A `repr(C)` or `repr(transparent)` struct may be unsized, so the pointer is
into 4096 bytes of memory of `puns`, with a tail of no element.

A `repr(C)` or `repr(transparent)` struct whose size with a tail of no element is larger than 4096
bytes keeps the offsets that its `repr` specifies. For `repr(C)`, they are computed from the sizes
and alignments of the fields, and for `repr(transparent)` they are 0.

## Enums with fields

A *sample* of a variant is its [least value][least], made by a function of [`Layout`]. The derive
reads offsets and tags on the samples. A fake `MaybeUninit`, or a fake result type, named by a path
does not have the type that these functions take or return, and does not compile.

With a primitive or `C` representation, the tag and the size are those that [RFC 2195] specifies.
The derive computes the layout from the sizes and alignments of the fields, as `repr(C)` lays out a
struct, and checks it against the size and alignment of the enum. It finds the offsets of the
fields of a variant on its sample, by code that matches on it, with `byte_offset_from`, a pointer
method that no path names. A variant without a sample has the offsets that RFC 2195 specifies.

The tags are those of an enum without fields that the derive declares with the same representation
and discriminants, `Self` replaced with the enum. The derive declares it next to its
implementation, where a name that a macro writes may name another item, so a discriminant may have
another value there. A function of [`Variant`] compares each tag with the first bytes of the sample
of its variant, and fails the build if they differ. So reordered variants fail the build.

A variant without a sample holds an [opaque scalar] or a union none of whose fields has values. If
a value of it may exist, its tag must be known from its declaration ([Enums with fields][enums]
says when). Such a tag has the same value in both enums, because there the derive names only items
whose names start with `__Pun`, which it refuses in the input. Otherwise the build fails (E0080)
with "the tag of a variant without a sample is not known".

With the `Rust` representation, the derive finds the layout that `rustc` chose by
[experiments][rust#the-tag-of-an-enum-of-the-rust-representation] on the samples. Each sample is
written as a constructor expression, `Enum::Variant { field: .. }`, with the names that the derive
read. The derive asserts that the discriminant of each sample is the index of its variant in the
item that it read, so reordered variants fail the build.

## The generated code

The generated code calls associated functions of the types that the implemented traits name
([`Layout`] and [`Spec`]), and of [`Rule`], [`Op`], [`Field`], [`Enum`] and [`Variant`]. A hidden
constant of `Layout` fixes these five with `fn(T)` types, which no path can replace. The generated
code also calls `new` of [`Crc`] and of the identity of a type. Their results go to these
functions, so only the real types compile.

The trait that says where a field is, `Path`, is sealed. The leaf at which a list of fields places
a field is checked against the layout.

The type of each field is written once, as an associated type of a trait that the derive declares
and implements for the type, where `Self` is the type, as in its declaration. The rest of the
generated code names the type of a field there. It also holds a function that is never called,
which compiles only if the type as compiled has the fields and variants that the derive read. Its
exhaustive pattern requires the same names and no other. It requires each field to have exactly
its associated type, since a `&mut` to a `*mut` to the field admits no coercion and no subtype. A
macro in the type of a field expands where `Self` and the names that start with `__Pun` name other
items. So it gives the field one type, or the type does not compile.

## The identity of a type and the compiler

The identity of a type, which says whether two layouts are of the same type, is the one exception.
A macro that the derive names through the path of `puns` writes it where the type is declared, so a
crate that redirects that path can give a type the identity of another. Two layouts count as the
same type only if they are also equal, so this cannot make a wrong layout. It can make a verdict
depend on how `rustc` arranged the two types in this compilation, which without the `unstable`
feature no other verdict does ([The same type][same]).

The rest of `::core` that the derive names is `Option` with `Some` and `None`, `PhantomData`,
`MaybeUninit`, `Result`, `Sized`, `ptr::null`, `panic!`, and the integers `u8`, `u128` and `usize`:

- The types of the traits that it implements, and of the functions of `puns` that it calls, check
  `Option`, `MaybeUninit` and the integers.
- `PhantomData` is only the field of the types that the derive declares as markers, whose bytes
  nothing reads.
- `Result` is only in the signatures of safe functions, the accessors of regions and the functions
  of a view. A fake one changes only these signatures.
- `Sized` is only a bound. An unsized type still fails to build where the generated code passes it
  to a function of `puns` that needs the real `Sized`.
- `::core::ptr::null()` is the probe of a sized type, which nothing reads.
- `::core::panic!` is written only next to a compile error of the derive.

The derive trusts the compiler for what it parses, such as `repr`. From the `repr` that it read, it
takes whether Rust specifies the layout, on which a verdict without the `unstable` feature may rely
([Layout independence][independence]).

## Attribute macros below the derive

An attribute macro written below `#[derive(Pun)]` is expanded after the derive read the item, and
can change what `rustc` compiles. The derive does not compile if such an attribute adds, removes or
renames a field, changes the type of a field, or adds or removes a variant. A field that it adds to
a union is not refused. It adds values that the layout does not accept as a target, which is sound,
and the layout of a source union guarantees nothing.

Where such an attribute changes the order of the fields or the `repr`, the layout is that of the
type as compiled, or the build fails:

- The offsets of the fields are measured, except where the `repr` gives them.
- The tags of a primitive or `C` enum are compared with the samples. In a copy of each sample, the
  byte after the tag that the derive read is set to 0, then to 255, and the variant of the copy
  must stay the same. Const evaluation reads that variant from the tag alone ([The variant read
  from the tag][rust#the-variant-read-from-the-tag]). So a wider tag fails the build. A narrower
  one leaves a byte of padding or of a field where the derive reads its tag, and the comparison
  with the samples or the size of the enum fails the build.
- The variants of a `Rust` enum are checked against their indices.

What such an attribute can still change without failing the build:

- A variant without a sample keeps the offsets that RFC 2195 specifies. So does every variant of an
  enum none of whose variants has a sample, of which no tag width is checked.
- A struct or a union whose `repr(C)` such an attribute removes has the offsets of this
  compilation. It is taken to have them in every compilation, which Rust does not promise.

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
[testing]: testing.md
