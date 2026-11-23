# Soundness

A pun gives a value of a type only from bytes that are a [valid] value of that type. The analysis
proves it at compile time, or a check proves it at run time. A check reads only bytes that are
initialized in the source: those that the analysis leaves to it, and the proven parts that share a
check with them ([Reading the bytes of the value][bytes]). `puns` never relies on a [rule] for
memory safety: a rule that no longer holds gives wrong values or a panic, never undefined behavior.

The analysis compares the layouts that the implementations of [`Pun`] give: those of `puns` for
the types of `core` that it supports, and those of the derive for other types ([What the derive
trusts][trust]). [What `puns` relies on][rust] lists what Rust documents of these layouts, and what
no documentation states, each fact with what checks it. [Testing][testing] describes the tests.

## Guarantees and requirements

Unsafe code may rely on what a type guarantees, but never on what it [requires][requirements] of
a value that the unsafe code did not check itself. `puns` proves or checks the requirements of a
type whenever it makes a value of that type, except that [`zeroed`] checks no rule.

A requirement of the target is proven only by the source ([Why the pair decides][pair]), and is
not checked again. So if the source does not meet its own requirements, because safe code wrote
its fields or `zeroed` made it, the target does not meet them either. A part proven this way is
still read where it shares a check with another part ([Reading the bytes of the value][bytes]),
and that check fails on a source whose requirement safe code broke.

## Padding and `MaybeUninit`

A pun is refused if a byte that the target requires to be initialized may be [padding] in the
source.

`MaybeUninit<T>` is padding with the size and the alignment of `T`: its layout is
[`Layout::padding`]. It is `Pun` only if `T` is, so that it never holds an `UnsafeCell`. As a
target, it accepts any bytes. As a source, it proves only a target that accepts any bytes, so a
`&mut T` is made from a `MaybeUninit<T>` only if `T` requires no byte to be initialized. What is
written through a `&mut MaybeUninit<T>` may be uninitialized, so such a reference is made only from
a source that accepts any bytes there, uninitialized ones included.

The tag of an enum is never padding. Rust places the tag of an enum of a primitive or `C`
representation, and the experiments that find the tag of an enum of the `Rust` representation fail
the build on a byte of padding ([The padding of a new value][rust#the-padding-of-a-new-value]).

## Unions

The language has not decided which bytes a union may hold. The Rust Reference states only this
([`r-undefined.validity.union`][union validity]):

> Obviously, all values that can be created entirely in safe code are valid. If the union has a
> zero-sized field, then every possible value is valid.

Some of the proposals under discussion let a union hold any bytes, others require its bytes to be
one of its fields, and safe code may write one field over another. `puns` accepts only what is sound
under every one of these proposals:

- As a target, bytes are a union only if they are one of its fields. Safe code makes such bytes by
  writing that field, so the first sentence above makes them valid under every proposal, though
  some proposals accept more. A union is proven by any one field that the source proves, and is
  otherwise checked field by field at run time.
- As a source, nothing is known of a union, not even that its bytes are initialized, since some
  proposals let it hold any bytes, as the second sentence does where it has a field of size 0. Only
  the same union proves a union. A union may hold a value even if none of its fields has one,
  though safe code cannot make such a value, since it makes a union only from a value of one of its
  fields. Such a union, and a variant or a struct that holds it, is not uninhabited as a source, and
  [`Layout::is_inhabited`] answers for a target only.

A target union that no field proves is checked field by field only if no field requires a byte to
be initialized that may be padding in the source. Otherwise the pun is refused ([Not
yet][limits#not-yet] says what would lift this).

A proven union is not read, except where it shares a check with other parts, as the elements of
an array or the fields of a variant of an enum checked as a whole. The bytes that a field *reads*
are those that it checks, those that a rule of a struct in it loads, and every byte of a struct
whose rule reads bytes by offset. The check tries the fields in order, and stops at the first one
that the source proves whatever safe code writes. A field that only a requirement of the source
proves fails where safe code broke that requirement, so the check may go on past it, and the bytes
that the later fields read count too. The pun is refused if the check would read a byte that may
be padding in the source, or a byte of the same union.

## Uninhabited types

No value of an inhabited type is a value of an [uninhabited] one, so such a pun is refused as never
valid. A variant or a struct that holds an uninhabited type is uninhabited too.

No bytes are an uninhabited variant of a target enum, or an uninhabited field of a target union.
The analysis does not look at such a part, and a check reads nothing of it, so bytes with the tag
of that variant fail the check whatever its other fields hold. Otherwise, a variant of a
`repr(u8)` enum whose field is uninhabited would be proven from a variant whose field is not,
because no byte of a type of size 0 is ever compared.

A slice of an uninhabited type has one value, the empty one. A pun that may give it an element is
refused, unless the source ends with a slice of the same elements at the same place.

As a source, an uninhabited variant needs no proof where the target is an enum, since the two
enums are compared variant by variant and no value is that variant. Against any other target, the
tag of the enum is read as a scalar that may also hold the tag of that variant, and a struct that
holds an uninhabited type is compared field by field, as any struct is.

## What deriving `Pun` means

`PhantomData<T>` is a scalar with no bytes, for any `T`. So types that differ only in phantom
parameters or lifetimes have the same layout, and convert to each other freely. A type must not
derive `Pun` if its parameters mark something about its values: a unit, an invariant lifetime that
ties a value to a scope, or a marker of what a value was checked against. The pun is sound, but the
marker no longer means anything:

```rust
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

Deriving `Pun` also makes every field as accessible as if it were public. A value can be made from
any bytes that are valid for its fields and meet its rules, reinterpreted as any type that its bytes
are, and written through a `&mut` to another type. The functions check what the derive was told,
never what a constructor checks. So a type whose private fields keep an invariant that the derive
does not know must not derive `Pun`, since unsafe code that relies on that invariant has undefined
behavior on a value that a pun made. For the same reason, a derived type cannot guarantee a
pattern, as `str` does.

```rust
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

[union validity]: https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.union

[`Layout::is_inhabited`]: https://docs.rs/puns/0.2.4/puns/layout/struct.Layout.html#method.is_inhabited
[`Layout::padding`]: https://docs.rs/puns/0.2.4/puns/layout/struct.Layout.html#method.padding
[`Pun`]: https://docs.rs/puns/0.2.4/puns/trait.Pun.html
[`zeroed`]: https://docs.rs/puns/0.2.4/puns/fn.zeroed.html
[bytes]: rules.md#reading-the-bytes-of-the-value
[padding]: concepts.md#layouts-and-rules
[limits#not-yet]: limits.md#not-yet
[pair]: concepts.md#why-the-pair-decides
[requirements]: concepts.md#requirements
[rule]: rules.md
[rust]: rust.md
[rust#the-padding-of-a-new-value]: rust.md#the-padding-of-a-new-value
[testing]: testing.md
[trust]: trust.md
[uninhabited]: concepts.md#uninhabited-types
[valid]: concepts.md#what-is-a-valid-value
