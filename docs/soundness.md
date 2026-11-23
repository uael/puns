# Soundness

A pun gives a value of a type only from bytes that are a [valid] value of that type. The analysis
proves it at compile time, or a check proves it at run time, and a check reads only the bytes
that the analysis leaves to it. `puns` never relies on a [rule] for memory safety: a rule that no
longer holds gives wrong values, never undefined behavior.

The analysis compares the layouts that the implementations of [`Pun`] give. `puns` implements
`Pun` itself for the types of `core` that it supports, and the derive implements it for other
types ([What the derive trusts][trust]). [What `puns` relies on][rust] lists what Rust documents
of these layouts and the behaviors of `rustc` that no documentation states, and
[Testing][testing] says how each is checked.

## Guarantees and requirements

Unsafe code may rely on what a type guarantees, but never on what it [requires][requirements] of
a value that the unsafe code did not check itself. `puns` proves or checks the requirements of a
type whenever it makes a value of that type, except that [`zeroed`] checks no rule.

A requirement of the target is proven only by the source, as [Why the pair decides][pair] says:
a `#[pun(valid(..))]` by a source whose values it accepts, and rules by the same rules. Such a
requirement is not checked again. So if the source does not meet its own requirements, because
safe code wrote its fields or `zeroed` made it, the target does not meet them either. A part
proven this way is still read where it shares a check with another part ([Reading the bytes of
the value][bytes]), and that check fails on a source whose requirement safe code broke. Where a
union is proven by a field through a requirement and checked all the same, its later fields may
be tried, and what they read counts as read ([Unions](#unions)).

## Padding and `MaybeUninit`

A byte of a struct that no field covers is *padding*, and may be uninitialized. A pun is refused
if a byte that the target requires to be initialized may be padding in the source.

`MaybeUninit<T>` is padding with the size and the alignment of `T`: its layout is
[`Layout::padding`]. It is `Pun` only if `T` is, so that it never holds an `UnsafeCell`. As a
target, it accepts any bytes. As a source, it proves only a target that accepts any bytes. So a
`&mut MaybeUninit<T>` is made only from a source that may hold any bytes there, since what is
written through it may be uninitialized. A `&mut T` is made from a `MaybeUninit<T>` only if `T`
requires no byte to be initialized, since such a byte would be read from padding.

The tag of an enum is never a scalar that may be uninitialized, since the tags that `puns`
discovers are exact.

## Unions

The language has not decided which bytes a union may hold ([`r-undefined.validity.union`][union
validity]). Some of the rules under discussion let a union hold any bytes, others require its
bytes to be one of its fields, and safe code may write one field over another. `puns` accepts only
what is sound under every one of these rules:

- As a target, bytes are a union only if they are one of its fields, which requires more than some
  of the rules and never less. A union is proven by any one field that the source proves, and is
  otherwise checked field by field at run time.
- As a source, nothing is known of a union, not even that its bytes are initialized, and only the
  same union proves a union. A union may hold a value even if none of its fields has one, though
  no value of it can be made, so [`Layout::is_inhabited`] answers for a target only.

The bytes that a field *reads* are those that it checks, those that a rule of a struct in it
loads, and every byte of a struct whose rule reads bytes by offset. A field of a target union is
checked only if no byte that it reads may be padding in the source. If some field may read
padding and no field is proven, the pun is refused ([Not yet][limits#not-yet] says what would lift
this).

A proven union is not read. But where parts are checked together, as the elements of an array or
the fields of a variant of an enum checked as a whole, a proven union is checked with them, its
fields tried in order. The pun is refused if that would read a byte that may be padding in the
source, or a byte of the same union, of which nothing is known.

## Uninhabited types

No value of an inhabited type is a value of an [uninhabited] one, so such a pun is refused as never
valid. A variant or a struct that holds an uninhabited type is uninhabited too. No bytes are an
uninhabited variant of a target enum, or an uninhabited field of a target union: the analysis does
not look at it, and a check reads nothing of it, so bytes with the tag of that variant fail the
check whatever its other fields hold. Without this rule, a variant of a `repr(u8)` enum whose
field is uninhabited would be proven from a variant whose field is not, because no byte of a type
of size 0 is ever compared.

A slice of an uninhabited type has one value, the empty one. A pun that may give it an element is
refused, unless the source ends with a slice of the same elements at the same place.

As a source, an uninhabited variant needs no proof where the target is an enum, since the two
enums are compared variant by variant and no value is that variant. Against any other target, the
tag of the enum is read as a scalar that may also hold the tag of that variant, and a struct that
holds an uninhabited type is compared field by field, as any struct is.

A union none of whose fields has values is not uninhabited as a source ([Unions](#unions)), and
neither is a variant or a struct that holds it.

## What deriving `Pun` means

`PhantomData<T>` is a scalar with no bytes, for any `T`. So types that differ only in phantom
parameters or lifetimes have the same layout, and convert to each other freely. A type must not
derive `Pun` if its parameters mark something about its values: a unit, an invariant lifetime that
ties a value to a scope, or a marker of what a value was checked against. The pun itself is sound,
but the marker no longer says anything:

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
are, and written through a `&mut` to another type. The functions check what the derive was told, and
nothing that a constructor checks. So a type whose private fields keep an invariant that the derive
does not know must not derive `Pun`: unsafe code that relies on that invariant has undefined
behavior on a value that a pun made. For the same reason, the derive cannot make a type that
guarantees a pattern, as `str` does: safe code can write its fields.

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

[`Layout::is_inhabited`]: https://docs.rs/puns/0.2.3/puns/layout/struct.Layout.html#method.is_inhabited
[`Layout::padding`]: https://docs.rs/puns/0.2.3/puns/layout/struct.Layout.html#method.padding
[`Pun`]: https://docs.rs/puns/0.2.3/puns/trait.Pun.html
[`zeroed`]: https://docs.rs/puns/0.2.3/puns/fn.zeroed.html
[bytes]: rules.md#reading-the-bytes-of-the-value
[limits#not-yet]: limits.md#not-yet
[pair]: concepts.md#why-the-pair-decides
[requirements]: concepts.md#requirements
[rule]: rules.md
[rust]: rust.md
[testing]: testing.md
[trust]: trust.md
[uninhabited]: concepts.md#unspecified-layouts-opaque-scalars-and-uninhabited-types
[valid]: concepts.md#what-is-a-valid-value
