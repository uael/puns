# Rust version and targets

## Minimum Rust version

`puns` needs Rust 1.99 or later, because its design depends on what stable Rust can do in
`const`. The table lists what such a design can use, and the release that made it stable. `puns`
does without the first three rows, which are not stable.

| Feature | Stable |
|---|---|
| [`TransmuteFrom`], [`ptr::metadata`] | no |
| [const traits], [`generic_const_exprs`] | no |
| reflection, [`offset_of!`][offset_of] on enums and slices | no |
| loops in a `const fn` | [1.46][1.46] |
| dereferencing a raw pointer in a `const fn` | [1.58][1.58] |
| [`offset_of!`][offset_of] on sized fields, also in unsized structs | [1.77][1.77] |
| inline `const { .. }` in generic code | [1.79][1.79] |
| `&raw const`, which [the derive][trust#sizes-and-offsets] uses instead of `offset_of!` | [1.82][1.82] |
| `&mut` in a `const fn` | [1.83][1.83] |
| [`size_of_val_raw`] and [`align_of_val_raw`] in `const` | [1.99][1.99] |

The last row sets the minimum version. `puns` reads the layout of an unsized type, a struct that
ends with a slice or a `str`, in `const` from a raw pointer with an arbitrary length.
`size_of_val` cannot do this, because it needs a reference, and a reference needs a valid value.
On 1.98, `puns` does not build, because `layout_for_ptr` and its use in `const` are unstable there.
CI builds the library and its examples on 1.99, so the constants that make and compare their
layouts are evaluated by that version too. Const evaluation is also bounded ([Limits of
Rust][limits#limits-of-rust]).

## Targets

`puns` builds for every width of `usize` that Rust supports, 16 bits included. CI builds it for
two targets without `std`: `thumbv7em-none-eabihf`, and, on the pinned nightly, `msp430-none-elf`,
whose `usize` has 16 bits. Miri cannot run a 16-bit target, so what must hold there is asserted at
compile time. The code assumes the following of `usize`:

- A value is at most `isize::MAX` bytes, as Rust guarantees. So a size plus a small constant does
  not overflow, and an offset that a rule reads at is at most `isize::MAX` too. A `const`
  assertion checks that `isize::MAX` plus 16 does not overflow a `usize`.
- An offset, a length or a range that a rule reads is compared with the size of the value as a
  `u128`, on every target, before it becomes a `usize`.
- A [`layout::Crc`] has one table instead of sixteen where `usize` has 16 bits, since sixteen
  would be larger than a value can be there (the page of [`layout::Crc`] gives their sizes). A
  `const` assertion checks that its tables are at most `isize::MAX` bytes. The CRCs are the same
  with one table or sixteen, and tests compare them with reference values. The tables of
  [`crc32`], 16 KiB, fit on every width, since `isize::MAX` is 32 KiB minus 1 on 16 bits.
- [`sum16`] folds its sum before it can overflow a `u64`: every 2^20 blocks, or only at the end
  where `usize` cannot count that far. A `const` assertion checks that the blocks between two
  folds cannot overflow.
- An operation of a rule is computed on the narrowest of `u32`, `u64` and `u128` that holds its
  values, but not narrower than a register, which has 64 bits where `usize` has 64, and 32 bits
  otherwise. Every width computes the same values, so the choice changes only the speed, and tests
  check both.

What `unsafe` code assumes of the target (the alignment of a primitive, the width of `usize` and
of `isize`, the byte order) is asserted in a `const` where it is used, or derived from the target.
[Miri][testing#miri] says on which targets the tests run.

[`TransmuteFrom`]: https://doc.rust-lang.org/nightly/core/mem/trait.TransmuteFrom.html
[`ptr::metadata`]: https://doc.rust-lang.org/nightly/core/ptr/fn.metadata.html
[const traits]: https://doc.rust-lang.org/nightly/unstable-book/language-features/const-trait-impl.html
[`generic_const_exprs`]: https://doc.rust-lang.org/nightly/unstable-book/language-features/generic-const-exprs.html
[offset_of]: https://doc.rust-lang.org/1.99.0/core/mem/macro.offset_of.html
[`size_of_val_raw`]: https://doc.rust-lang.org/1.99.0/core/mem/fn.size_of_val_raw.html
[`align_of_val_raw`]: https://doc.rust-lang.org/1.99.0/core/mem/fn.align_of_val_raw.html
[1.46]: https://blog.rust-lang.org/2020/08/27/Rust-1.46.0/
[1.58]: https://blog.rust-lang.org/2022/01/13/Rust-1.58.0/
[1.77]: https://blog.rust-lang.org/2024/03/21/Rust-1.77.0/
[1.79]: https://blog.rust-lang.org/2024/06/13/Rust-1.79.0/
[1.82]: https://blog.rust-lang.org/2024/10/17/Rust-1.82.0/
[1.83]: https://blog.rust-lang.org/2024/11/28/Rust-1.83.0/
[1.99]: https://blog.rust-lang.org/2026/10/01/Rust-1.99.0/

[`crc32`]: https://docs.rs/puns/0.2.4/puns/layout/fn.crc32.html
[`layout::Crc`]: https://docs.rs/puns/0.2.4/puns/layout/struct.Crc.html
[`sum16`]: https://docs.rs/puns/0.2.4/puns/layout/fn.sum16.html
[limits#limits-of-rust]: limits.md#limits-of-rust
[trust#sizes-and-offsets]: trust.md#sizes-and-offsets
[testing#miri]: testing.md#miri
