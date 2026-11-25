# Aims

This chapter says how `puns` checks and reads an [aim][how_puns_decides#aims] and its pointee. An
aim ([`layout::Aim`]) says three things, and every combination of them is an aim that `puns` checks
and reads:

- Where the pointee is ([`layout::Start`]): at the address of the struct plus an offset that an
  expression over the struct gives, an `i128` in two's complement, as for the self-relative
  pointers of [`nearest`]; or at the *cursor* of the tail of the struct, where the pointee of the
  aim at the cursor before it ends, from the start of the tail, as for regions. The value of a
  pointee at the cursor is aligned for it by its offset from the start of the struct, so that where
  a pointee is depends on the bytes, not on their address.
- How large it is ([`layout::Extent`]): its size is the size of its value with its *metadata*, the
  length of its tail if it is unsized, as `size_of_val` gives. The metadata is what the pointee
  stores, a length before its value or in its sized fields, nothing if it is sized; or a length
  that an expression over the struct gives; or the length of the size in bytes that an expression
  gives, or of what remains of the tail of the struct.
- When it is checked ([`layout::Check`]): with the struct (*eager*), or when it is read (*lazy*).

The pointee is a [`Pointee`]: the value of a [`Pun`][trait-pun] type, or a [`Thin<T, M>`][`Thin`],
which stores the metadata of a pointer to `T` before it, as an `M`: the length of a list or a text,
nothing for a sized `T`, so that a pointer of one format points at any `T` as a `Thin<T, M>`. Its
type is named in the hidden list of the rules of the struct, through which a check reaches the
checks of the pointee, so that a type that points at itself is checked by a recursive call, and
through which a read names the type that it returns.

## Checking an aim

- A check of a struct with an eager aim finds the pointee, in the bytes that the check reads:
  those of [`from_bytes`], [`from_head`] and [`from_tail`], or the source of a `try_` function.
  The pointee must be within them, aligned for its type, of a size that its type has, and every
  part of it is checked, what it stores before its value first, its own eager aims included, one
  level deeper. A pointee may be before the struct, and may be shared by several aims. These
  functions check the pointees in place, one within another, and allocate nothing. They stop at a
  bound ([Bounds of the checks][limits#bounds-of-the-checks]). Deeper data is for [`check`].
- [`check`] and [`check_compact`] check bytes for a value at any depth, with a stack that does not
  grow with the data. The caller gives a [`Worklist`], which holds the pointees still to check,
  each value before its pointees, depth first. They make no reference, so the bytes may be at any
  address, and are checked as they will be once copied to an address of a given alignment.
  [`check_compact`] requires the bytes to be the compact image of the value, each pointee right
  after the one before it in that order, as a writer that copies the value makes them. Both say
  where bytes stop being a value ([`Failure`]).
- So a reference to a struct with an eager aim is a reference to a valid pointee, which code reads
  without a check, through a safe method that the derive writes. No function makes a value that
  holds an eager aim ([`pun`], [`try_pun`], [`zeroed`] and the other functions that return a
  value do not compile), nor gives a `&mut` to or from one ([`pun_mut`], [`try_pun_mut`],
  [`from_bytes_mut`] and the `_mut` parts), nor checks one from a source whose bytes may be
  padding ([`try_pun_ref`] from such a source). Only the same aim, at the same place of the
  source, of a pointee of the same identity, proves one, since its pointee was checked where the
  source was made. An instance of a generic type has an identity only if its parameters are
  bounded by `Pun` ([The same type][layouts#the-same-type]), so a `Pair<O: Pun>` that points at a
  `Pair<O>` proves itself, and a `Pair<O>` with an unbounded `O` does not. A union does not hold an
  eager aim: which of its fields its bytes are would depend on pointees that a check checks after
  it.
- A lazy aim at the cursor requires only that its pointee fits in the tail, since the next one
  starts where it ends, and a lazy aim at a stored place requires nothing: the type of the pointee
  plays no part in the value of the struct, nor in a proof, unless the pointee stores its length,
  which places the next one. What reads the pointee checks it then. Where safe code wrote the
  fields of the struct so that a region does not fit, the read truncates it to the tail, and a size
  whose expression fails takes what remains.

## Declaring aims

The derive declares aims through two options ([Aims][macro@Pun#aims]), one for each place where a
pointee is: `pointer(..)` on a field, whose integer is where the pointee is unless an expression of
the option says otherwise, and `regions(..)` on the last field, whose tail the regions cut. Both
write an aim the same way, eager or lazy. They compose: a struct with regions may hold pointers,
and the pointee of a region may hold pointers too, whose pointees its read looks for within the
bytes of the struct, so that a pointer in a region may point at another region, but not at a byte
outside the struct.

A struct with an eager aim holds its field (the offset of a pointer, or the bytes of eager regions)
in an [`InPlace<T>`][`InPlace`], which no code makes, so no safe code makes a value of the struct,
which exists only where `puns` checked its pointee, or where unsafe code put it, which then
upholds the same. A struct with an eager aim is not `Clone`, `Copy` or `Default`, since an
`InPlace` is none of these. [`tests/support/aims.rs`][support] declares a self-relative pointer
this way, as `nearest` does.

## Reading a pointee

The pointee is read by the methods that the derive writes for an aim with a name, which find it as
the check does. An eager one is safe and checks nothing: the value exists only with a valid pointee.
It reads the pointee through the reference to the value, which has the provenance of the bytes of
the pointee under the tree borrows model of Miri, which `puns` relies on there ([The provenance of a
reference to a value with an eager
aim][what_puns_relies_on#the-provenance-of-a-reference-to-a-value-with-an-eager-aim]). A pointee
outside the value is outside what a reference to the value gives access to under stacked borrows,
where code reads it with [`aim`] from a pointer to the bytes that hold both, or writes it with
[`aim_mut`]. These name the aim by its name, a field of the constant `AIMS` that the derive writes
([`AimOf`]). A lazy one checks the pointee when it reads it.

```rust
use puns::{InPlace, Pun};

/// A pointer to a `u32`, stored as the distance from its own address, in an `i32`.
#[derive(Pun)]
#[repr(transparent)]
struct Ptr {
  #[pun(pointer(pointee: u32))]
  off: InPlace<i32>,
}

// A distance of 4, to the word after the pointer.
let words = [4u32, 7];
let ptr = puns::from_head::<Ptr>(puns::as_bytes(&words)).unwrap().0;
assert_eq!(ptr.pointee(), &7);
// A distance of 8 is past the bytes, and -4 before them.
assert!(puns::from_head::<Ptr>(puns::as_bytes(&[8u32, 7])).is_err());
assert!(puns::from_head::<Ptr>(puns::as_bytes(&[(-4i32).cast_unsigned(), 7])).is_err());
```

[`nearest`]: https://docs.rs/nearest
[support]: https://github.com/uael/puns/blob/v0.2.4/puns/tests/support/aims.rs

[`AimOf`]: https://docs.rs/puns/0.2.4/puns/struct.AimOf.html
[`aim_mut`]: https://docs.rs/puns/0.2.4/puns/fn.aim_mut.html
[`aim`]: https://docs.rs/puns/0.2.4/puns/fn.aim.html
[`check_compact`]: https://docs.rs/puns/0.2.4/puns/fn.check_compact.html
[`check`]: https://docs.rs/puns/0.2.4/puns/fn.check.html
[`Failure`]: https://docs.rs/puns/0.2.4/puns/struct.Failure.html
[`from_bytes_mut`]: https://docs.rs/puns/0.2.4/puns/fn.from_bytes_mut.html
[`from_bytes`]: https://docs.rs/puns/0.2.4/puns/fn.from_bytes.html
[`from_head`]: https://docs.rs/puns/0.2.4/puns/fn.from_head.html
[`from_tail`]: https://docs.rs/puns/0.2.4/puns/fn.from_tail.html
[`InPlace`]: https://docs.rs/puns/0.2.4/puns/struct.InPlace.html
[`layout::Aim`]: https://docs.rs/puns/0.2.4/puns/layout/struct.Aim.html
[`layout::Check`]: https://docs.rs/puns/0.2.4/puns/layout/enum.Check.html
[`layout::Extent`]: https://docs.rs/puns/0.2.4/puns/layout/enum.Extent.html
[`layout::Start`]: https://docs.rs/puns/0.2.4/puns/layout/enum.Start.html
[`Pointee`]: https://docs.rs/puns/0.2.4/puns/trait.Pointee.html
[`pun_mut`]: https://docs.rs/puns/0.2.4/puns/fn.pun_mut.html
[trait-pun]: https://docs.rs/puns/0.2.4/puns/trait.Pun.html
[`pun`]: https://docs.rs/puns/0.2.4/puns/fn.pun.html
[`Thin`]: https://docs.rs/puns/0.2.4/puns/struct.Thin.html
[`try_pun_mut`]: https://docs.rs/puns/0.2.4/puns/fn.try_pun_mut.html
[`try_pun_ref`]: https://docs.rs/puns/0.2.4/puns/fn.try_pun_ref.html
[`try_pun`]: https://docs.rs/puns/0.2.4/puns/fn.try_pun.html
[`Worklist`]: https://docs.rs/puns/0.2.4/puns/trait.Worklist.html
[`zeroed`]: https://docs.rs/puns/0.2.4/puns/fn.zeroed.html
[how_puns_decides#aims]: how_puns_decides.md#aims
[layouts#the-same-type]: layouts.md#the-same-type
[limits#bounds-of-the-checks]: limits.md#bounds-of-the-checks
[macro@Pun#aims]: https://docs.rs/puns/0.2.4/puns/derive.Pun.html#aims
[what_puns_relies_on#the-provenance-of-a-reference-to-a-value-with-an-eager-aim]: what_puns_relies_on.md#the-provenance-of-a-reference-to-a-value-with-an-eager-aim
