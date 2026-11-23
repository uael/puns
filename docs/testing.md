# Testing

`puns` has `unsafe` code, and its analysis decides what that code may do. The tests check both.
[Soundness][soundness] states what a pun guarantees, and what `puns` assumes to guarantee it.

## The model check

The *model check*, the crate [`puns-model`][model], compares every verdict of the analysis with
*oracles*. An oracle is a function written from what the language and the documentation of a type
say, without the analysis, that decides whether some bytes are a valid value of that type. The
model check takes every ordered pair of its types of the same size. It tries every value of the
source up to 2 bytes, and a chosen set of values above. The oracles of enums of the `Rust`
representation are written from how `rustc` lays them out. So they also check the layouts that
`puns` finds for these enums by experiments in const evaluation ([The tag of an enum of the `Rust`
representation][rust#the-tag-of-an-enum-of-the-rust-representation]). The model check runs in full
every night and on demand, and in part on every push, with and without the `unstable` feature. Its
crate documentation describes its types, its oracles and what runs where.

## Miri

The tests of `puns` run under [Miri] on every push, with `-Zmiri-strict-provenance`. They run with
tree borrows on x86-64, and with stacked borrows on two other targets: `i686-unknown-linux-gnu`,
where `usize` has 32 bits and a `u64` is aligned to 4, and `s390x-unknown-linux-gnu`, which is
big-endian. Test files that are not about the `unsafe` code do not run there, such as the examples
other than `ethernet`, which reach the same `unsafe` code.

The model check runs under Miri too, with tree borrows on x86-64 only. It runs a selection of pairs
that reaches each `unsafe` path that the model reaches. [`miri_sample.rs`][sample] names the paths
of `try_pun` and `try_pun_ref`, and `parts.rs`, `dst.rs`, `views.rs` and `shapes.rs` the others.
There, the model check does not check `pun`, `pun_mut`, the checked `_mut` functions or regions,
which the tests of `puns` already run under Miri. It calls `pun_mut` only to fill a buffer.

## The record of verdicts

The tests of `puns` run without the `unstable` feature and with it. Without it, `tests/seeds.rs`
records in `tests/seeds.txt` the verdict of every function on every ordered pair of 33 types, whole
and in part (by `&mut` and from the end). It also records which byte patterns each type accepts
from bytes. The types include structs, tuples and enums of the `Rust` representation, a `repr(u8)`
enum and a `repr(C)` union that hold them, structs with a rule, arrays, an [opaque scalar][opaque],
`Option<bool>`, `Option<NonZero<u16>>` and `Result<NonZero<u8>, ()>`. A rule and an enum checked as
a whole each have a type there. CI makes the record without `-Zrandomize-layout` and under three
seeds of it, and each must equal the one in the repository. This checks, for these pairs, that no
verdict depends on an [unspecified layout][layouts] without the feature.

With the feature, the tests that need it run as well: enums of the `Rust` representation, `Option`
and `Result`, and the tests of the layout that `rustc` chose. CI also builds the example
`codegen`, which needs it. The benchmark needs it too, and is run by hand.

## Compile-fail tests

172 programs in [`puns/tests/`][compile-fail] must fail to compile, each with its expected message:
the 162 of `compile-fail` with and without the feature, the 2 of `compile-fail-kept` without it, and
the 8 of `compile-fail-unstable` with it. 3 others must compile. Three of them check that const
evaluation fails the build where it reads an uninitialized byte (`discover_reads_padding`), reads
the variant of a value whose tag is no variant's (`discover_invalid_tag`), or computes a size
larger than an object can be (`structure_too_large`). `puns` relies on these errors ([Errors of
const evaluation][rust#errors-of-const-evaluation]).

## The derive against a fake `core`

The derive is also built in a crate whose `core` is another crate, `fakecore`, whose items give
wrong answers: its `offset_of!` swaps two fields, and its `PhantomData` is aligned to 16 bytes
(`puns-model/tests/core_shadowed.rs`).

## Commands

```sh
cargo test -p puns --features verdict              # Without the feature, the record included
cargo test -p puns --features unstable             # With the feature
cargo test -p puns-model                           # The sample of the model check
cargo test --profile model -p puns-model --features full   # The full model check
RUSTFLAGS="-Zrandomize-layout -Zlayout-seed=1" cargo +nightly-2026-11-17 test -p puns --features verdict --test seeds
MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-strict-provenance" cargo +nightly-2026-11-17 miri test -p puns --tests
MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly-2026-11-17 miri test -p puns --tests --target i686-unknown-linux-gnu
MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly-2026-11-17 miri test -p puns --tests --target s390x-unknown-linux-gnu
MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-strict-provenance" cargo +nightly-2026-11-17 miri test -p puns-model
```

[Miri]: https://github.com/rust-lang/miri
[compile-fail]: https://github.com/uael/puns/tree/v0.2.3/puns/tests
[model]: https://github.com/uael/puns/blob/v0.2.3/puns-model/src/lib.rs
[sample]: https://github.com/uael/puns/blob/v0.2.3/puns-model/tests/miri_sample.rs

[layouts]: layouts.md
[opaque]: concepts.md#unspecified-layouts-opaque-scalars-and-uninhabited-types
[rust#errors-of-const-evaluation]: rust.md#errors-of-const-evaluation
[rust#the-tag-of-an-enum-of-the-rust-representation]: rust.md#the-tag-of-an-enum-of-the-rust-representation
[soundness]: soundness.md
