# Testing

`puns` has `unsafe` code, and its analysis decides what that code may do. The tests check both.
[Soundness] states what a pun guarantees, and what `puns` assumes to guarantee it.

## The model check

The analysis is compared with oracles that do not use it. This model check takes every ordered pair
of its test types that have the same size. For types of up to 2 bytes it tries every value of the
source, and for larger ones a chosen set of values. It runs in full every night, and in part on
every push, with and without the `unstable` feature. The [crate documentation of
`puns-model`][model] describes its types, its oracles and what runs where.

## Miri

The tests of `puns` run under [Miri] on every push: with tree borrows on x86-64, and with stacked
borrows on a 32-bit target (`i686`) and on a big-endian one (`s390x`). Test files that are not about
the `unsafe` code, such as the examples other than `ethernet`, do not run there. The safe reads of
the pointees of aims, through a reference to a value that holds the aim, give access to bytes
outside the value. Tree borrows allows this, and stacked borrows does not. So their tests,
[`aim_reads.rs`][aim-reads], run with tree borrows only, as `.github/tests.sh` lists them. The
other aims tests read the same pointees from pointers, under both models.

The model check runs under Miri too, with tree borrows on x86-64 only. It runs a few pairs for each
`unsafe` path that it reaches instead of every pair, and [`miri_sample.rs`][sample] names the unsafe
path that each pair reaches. Under Miri, it does not check `pun`, `pun_mut`, the checked `_mut`
functions, or regions, because the tests of `puns` already run these there. It calls `pun_mut` only
to fill a buffer.

## Layouts that Rust leaves to the compiler

`puns/tests/seeds.rs` records the verdict of every function on pairs of types, and which bytes each
type accepts, in `puns/tests/seeds.txt`. CI makes the record without `-Zrandomize-layout`, and with
it under three seeds, and each time it must equal the one in the repository. This checks, for these
pairs, that no verdict depends on a layout that Rust leaves to the compiler when the `unstable`
feature is off.

## Targets

CI builds `puns` for two targets without `std`: `thumbv7em-none-eabihf` and `msp430-none-elf`,
whose `usize` has 16 bits. It also builds the library and its examples on Rust 1.99, the minimum
version.

## Compile errors

More than seventy programs must fail to compile, each with its expected message. They are in
[`puns/tests/compile-fail/`][compile-fail] and the folders next to it.

## The derive

The derive is also built in a crate whose `core` is another crate, with an `offset_of!` that
swaps fields. This checks that the layouts the derive makes do not depend on any name that the
user's crate can redefine.

[Miri]: https://github.com/rust-lang/miri
[model]: ../puns-model/src/lib.rs
[sample]: ../puns-model/tests/miri_sample.rs
[aim-reads]: ../puns/tests/aim_reads.rs
[compile-fail]: ../puns/tests/compile-fail
[Soundness]: https://docs.rs/puns/0.2.4/puns/#soundness
