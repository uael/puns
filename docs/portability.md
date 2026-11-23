# Portability

`puns` builds for every width of `usize` that Rust supports, including 16 bits, and CI builds it
for `msp430-none-elf`, a 16-bit target. Miri cannot run a 16-bit target, so what must hold there
is asserted at compile time. The code assumes the following about `usize`:

- A value is at most `isize::MAX` bytes, as Rust guarantees. So a size plus a small constant
  does not overflow, and an offset that a rule reads at is at most `isize::MAX` too. A `const`
  assertion checks that `isize::MAX` plus 16 does not overflow a `usize`.
- An offset, a length or a range that a rule reads is compared with the size of the value as a
  `u128`, on every target, before it becomes a `usize`.
- The tables of a [`layout::Crc`] fit in a value. They take 32 KiB, or a single table of 2 KiB
  where `usize` is 16 bits, and a `const` assertion checks that they are at most `isize::MAX`
  bytes. The CRCs are the same either way, and tests compare them with reference values. The
  table of [`crc32`], 16 KiB, fits on every width, since `isize::MAX` is 32 KiB minus 1 on 16
  bits.
- [`sum16`] folds its sum before it can overflow a `u64`. It folds every 2^20 blocks, or only at
  the end where `usize` cannot count that far. A `const` assertion checks that the blocks
  between two folds cannot overflow.
- An operation of a rule is computed in a register where its values fit one, and on 128 bits
  otherwise. The register has 64 bits where `usize` has 64 bits, and 32 bits otherwise. Every
  width computes the same values, so the choice only changes speed, and tests check both.

What `unsafe` code assumes about the target (the alignment of a primitive, the width of `usize`,
the byte order) is asserted where it is used, or derived from the target. The tests run under
Miri on `i686-unknown-linux-gnu`, where `usize` is 32 bits and a `u64` is aligned to 4, and on
`s390x-unknown-linux-gnu`, which is big-endian.

[`crc32`]: https://docs.rs/puns/0.2.3/puns/layout/fn.crc32.html
[`layout::Crc`]: https://docs.rs/puns/0.2.3/puns/layout/struct.Crc.html
[`sum16`]: https://docs.rs/puns/0.2.3/puns/layout/fn.sum16.html
