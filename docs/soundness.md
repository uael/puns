# Soundness

A pun only ever produces a `D` from bytes that are a valid `D`, and only produces a reference to
a `D` at an address that is [properly aligned] for `D`. Here, a *valid* `D` is a value that is
[valid] for `D` in the sense of the Reference and that also meets the invariants `puns` knows
about: a `str` is UTF-8, a type from [`num`] holds one of its values, and a type that derives
`Pun` meets what the derive was told (the values of fields marked `#[pun(valid(..))]`, and its
rules). The analysis either proves this at compile time, or the function checks it at run time
and returns an [`Error`] if the check fails. A pun with a part that can be neither proven nor
checked, such as a byte that may be padding, does not compile.

What the derive is told is a requirement, which the type does not enforce ([Guarantees and
requirements](#guarantees-and-requirements)). Where the source has the same requirement at the
same place, `puns` takes it as met and does not check it again. If the source does not meet it,
because safe code wrote its fields or [`zeroed`] made it, the target does not meet it either.
Unsafe code may rely on what a type guarantees, but not on what it requires of a value that it
did not check itself.

The analysis reads the layout of each type from its implementation of [`Pun`]: its size, its
alignment, the offset of each field, the values each scalar may hold, and its rules.
`#[derive(Pun)]` takes sizes and alignments from the compiler. It computes offsets as the type's
`repr` specifies them, or measures them on a value when the compiler chooses them. A layout
never depends on a path that the deriving crate could redefine (see [What the derive
trusts][macro@Pun#what-the-derive-trusts]). For the types of `core` that it supports, `puns`
implements `Pun` itself.

`puns` relies on the layouts that the [Reference] specifies: `repr(C)`, `repr(transparent)`, the
primitive representations of enums, `packed` and `align`. It also relies on the layouts of
`Option` and `Result` that `core` documents. [What Rust
documents][unstable#what-rust-documents] lists each of these guarantees with its source.
Where Rust leaves a layout to the compiler, `puns` reads the one that `rustc` chose for the
build. It measures field offsets on a value, and finds the tag of an enum of the `Rust`
representation by experimenting on a value of each variant. An experiment that fails stops the
build, so no wrong layout is used. Beyond that, `puns` relies on three behaviors of `rustc` that
no documentation states:

- A pointer to an unsized type has the representation of a pointer to a slice. This is checked
  at compile time for each unsized type, against a pointer made with an `as` cast.
- Const evaluation reports an error when it reads an uninitialized byte, when it reads the
  variant of a value whose tag is not the tag of a variant, and when it computes the size of a
  slice that is larger than an object can be. The experiments that find the layout of an enum of
  the `Rust` representation rely on the first two, and only run at compile time. The derive
  relies on the first to refuse an enum without fields that has padding, and on the third for an
  unsized struct whose tail is so large that 8 elements overflow an `isize`. An `Option` of a
  scalar that a pun never makes relies on the first to refuse a `None` with an uninitialized
  byte. Compile-fail tests check each of the three errors.
- Const evaluation reads the variant of a value whose tag is the tag of a variant from the tag
  alone, without reading or checking its other bytes, though `mem::discriminant` takes a
  reference to the whole value and the Reference requires a reference to point to a valid value.
  The experiments rely on it when they move the tag of one variant into a value of another. The
  model check compares the layouts that they find with oracles that do not use them.

To check the alignment of an address at run time, `puns` reinterprets the address as an integer,
through a union, since no stable `const fn` gives it on Rust 1.99. At run time, `core` does not
call this undefined behavior, but it advises against it because its semantics are not settled.
No documentation promises that the integer is the address either. In const evaluation, `core`
calls it undefined behavior and says that using the integer stops the evaluation, so a check of
an address does not compile in a constant. The integer is never made into a pointer again, and
the tests run it under Miri with `-Zmiri-strict-provenance`.

Without the `unstable` feature, whether a pun compiles, what it checks and which values a check
accepts do not depend on a layout that Rust leaves to the compiler: the order of the fields of a
struct or tuple of the `Rust` representation, the tag of an enum of that representation, or
niches. One exception is whether an `Option` of a scalar that a pun never makes compiles
([Scalars that a pun never makes][never_made]). With the feature, `puns`
also uses the layout that `rustc` chose. [The `unstable` feature][unstable] states
the rule and [what it relies on][unstable#what-this-relies-on].

The tests run under [Miri], on the targets that [Portability][portability] names. The
analysis itself is checked against oracles that do not use it, both with and without the
`unstable` feature.

## Guarantees and requirements

The analysis relies only on what a type *guarantees*, no matter what safe code does: the valid
values of `bool`, `char`, enums with or without fields, and `NonZero`, the UTF-8 of `str`, and
the invariants of the types in [`num`], such as [`u16bec<V>`][num::u16bec] and [`u8in<MIN,
MAX>`][num::u8in], whose fields are private and whose constructors check them.

A struct that derives `Pun` can only *require* values of its fields. `#[pun(valid(..))]` on a
field is a requirement, and so is a rule, because safe code can write any value to a `pub
version: u8`. `puns` proves or checks a requirement whenever it makes a value of the type,
though [`zeroed`] does not check rules. A requirement never proves what another type guarantees.
For example, a slice of a struct whose only field has `#[pun(valid(..=0x7f))]` is checked when
it is reinterpreted as a `str`, while a `[u8in<0, 0x7f>]` is proven to be one. For the same
reason, the accessors of regions truncate a region rather than trust the fields that give its
size. Two types have the same layout only if they require and guarantee the same values, so a
field `#[pun(valid(0..=1))] u8` is not a `bool`.

A requirement that the source has too proves the same requirement of the target, but a check
of it may still fail on a value of the source. Where a union is proven by a field through a
requirement and checked all the same, its later fields may be tried, and what they read counts
as read (see [Unions](#unions)).

## Padding and `MaybeUninit`

A pun is refused if a byte that the target requires to be initialized may be padding in the
source.

`MaybeUninit<T>` is padding with the size and alignment of `T`, and it is only `Pun` if `T` is,
so that it never holds an `UnsafeCell`. As a target, it accepts anything. As a source, it proves
nothing except a target that accepts anything. So a `&mut MaybeUninit<T>` can only be made from
a source that may hold anything there, since whatever is written through it may be
uninitialized. A `&mut T` is made from a `MaybeUninit<T>` only if `T` requires no byte to be
initialized, since a byte that `T` requires would be read from padding.

A rule that reads the bytes of its value by offset (`le`, `be`, a variable-length integer, a
checksum, a CRC, `matches`, or a `walk` whose expressions do) reads its padding as well. Such a
rule is checked when the source does not prove it. It is also checked with what it shares a
check with: in a variant of an enum that is checked as a whole, in an element of an array of
which another element is checked, and where every part of the target is checked. Its struct
must then have no byte that may be padding in the source.

## Unions

The language has not settled the validity of unions ([`r-undefined.validity.union`][union
validity]). Some of the rules under discussion let a union hold any bytes, while others require
its bytes to be one of its fields, and safe code may write one field over another. So `puns`
accepts only what is sound under every one of these rules:

- As a target, bytes are a union if they are one of its fields. This requires more than some of
  the rules do, and never less. A union is proven by any one field that the source proves, and
  otherwise it is checked field by field at run time.
- As a source, nothing is known about a union, not even that its bytes are initialized. Only the
  same union proves a union. A union may then hold a value even if none of its fields has one,
  so a value that holds it may exist, though no value of it can be made.

A field is only checked if no byte it reads may be padding in the source. If some field might
read padding and no field is proven, the pun is refused. Checking only the other fields would be
sound, but `puns` does not do it yet (see [Limitations][limitations]).

A union that is proven is not read. Some parts are checked together: the elements of an array,
and the fields of a variant of an enum that is checked as a whole. A union that is proven there
would be checked with the others, its fields tried in order. If that would read a byte that may
be padding in the source, or a byte of the same union, of which nothing is known, the pun is
refused. A byte that a field reads is one that it checks, a byte that a rule of a struct in it
loads, or any byte of a struct whose rule reads bytes by offset.

## Types without values

`Infallible` and an enum without variants are types without values. No value of a type that has
values can be one of them, so such a pun is refused as never valid, and a variant or struct that
holds one has no value either. A slice of a type without values has one value, the empty one: a
pun that may give it an element is refused, unless the source ends with a slice of the same
elements at the same place. As a source, a variant without values needs no proof where an
enum is related to a target enum variant by variant, since no value is that variant. Against any
other target, the tag of the enum is read as a scalar that may hold the tag of that variant too,
and a struct that holds a type without values is related field by field, as any struct is.
Without the first rule, a variant of a `repr(u8)` enum whose field has no value would be proven
from a variant whose field does, because no byte of a zero-sized type is ever compared.

No bytes are a variant without values of a target enum, or a field without values of a target
union. The analysis does not look at it, and a check reads nothing of it: bytes whose tag is
that variant's are refused, whatever its other fields would hold.

A union none of whose fields has values is not a type without values as a source: a value of
it may exist ([Unions](#unions)), and so may a variant or a struct that holds it. An enum of the
`Rust` representation with such a variant is refused, since no sample of the variant can be made
to find its tag.

## What deriving `Pun` means

`puns` knows a type by its layout and by what the type system enforces about its bytes, and
nothing more. This has two consequences, both by design.

`PhantomData<T>` is a scalar with no bytes, for any `T`. Types that differ only in phantom
parameters or lifetimes have the same layout, and convert to each other freely. A type must not
derive `Pun` if its parameters mark something about its values: a unit, an invariant lifetime
that ties a value to a scope, or a marker of what a value was checked against.

```
use core::marker::PhantomData;

#[derive(Clone, Copy)]
struct Meters;
#[derive(Clone, Copy)]
struct Feet;

#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
struct Length<U> {
  value: u32,
  unit: PhantomData<U>,
}

// Not what a unit is for: 3 meters are 3 feet.
let feet: Length<Feet> = puns::pun(Length::<Meters> { value: 3, unit: PhantomData });
assert_eq!(feet.value, 3);
```

Deriving `Pun` also makes every field as accessible as if it were public. A value can be made
from anything whose bytes are valid for its fields and rules, reinterpreted as anything its
bytes are, and written through a `&mut` to another type. The functions check what the derive was
told, and nothing that a constructor checks, so a type whose private fields keep an invariant
the derive does not know about must not derive `Pun`.

```
mod even {
  /// Only even numbers, says `new`.
  #[derive(puns::Pun, Clone, Copy)]
  #[repr(transparent)]
  pub struct Even(u32);

  impl Even {
    pub const fn new(value: u32) -> Option<Self> {
      if value % 2 == 0 { Some(Self(value)) } else { None }
    }

    pub const fn get(self) -> u32 {
      self.0
    }
  }
}

assert!(even::Even::new(3).is_none());
// What `new` refuses, `pun` makes.
let odd: even::Even = puns::pun(3u32);
assert_eq!(odd.get(), 3);
```

[valid]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity
[Reference]: https://doc.rust-lang.org/reference/type-layout.html
[Miri]: https://github.com/rust-lang/miri
[union validity]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity.union

[`Error`]: https://docs.rs/puns/0.2.3/puns/enum.Error.html
[`num`]: https://docs.rs/puns/0.2.3/puns/num/index.html
[`Pun`]: https://docs.rs/puns/0.2.3/puns/trait.Pun.html
[`zeroed`]: https://docs.rs/puns/0.2.3/puns/fn.zeroed.html
[limitations]: limitations.md
[macro@Pun#what-the-derive-trusts]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html#what-the-derive-trusts
[never_made]: never_made.md
[num::u16bec]: https://docs.rs/puns/0.2.3/puns/num/struct.u16bec.html
[num::u8in]: https://docs.rs/puns/0.2.3/puns/num/struct.u8in.html
[portability]: portability.md
[properly aligned]: https://doc.rust-lang.org/1.100.0/core/ptr/index.html#alignment
[unstable]: unstable.md
[unstable#what-rust-documents]: unstable.md#what-rust-documents
[unstable#what-this-relies-on]: unstable.md#what-this-relies-on
