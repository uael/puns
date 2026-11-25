# Testing

`puns` has `unsafe` code, and its analysis decides what that code may do, so the tests check both.
[Soundness][soundness] states what a pun guarantees, and what `puns` assumes to guarantee it.

## The model check

The *model check*, the crate [`puns-model`][model], compares every verdict of the analysis with
*oracles*. An oracle decides whether some bytes are a valid value of a type from what the language
and the type's documentation say, without the analysis. The model check takes every ordered pair
of its types of the same size, and tries every value of the source up to 2 bytes and a chosen set
of values above. Its oracles of enums of the `Rust` representation follow how `rustc` lays them
out, so they also check the layouts that the
[experiments][rust#the-tag-of-an-enum-of-the-rust-representation] find. The model check runs in
full every night and on demand, and in part on every push, with and without the `unstable`
feature. Its crate documentation describes its types, its oracles and what runs where.

## Miri

The tests of `puns` run under [Miri] on every push, with `-Zmiri-strict-provenance`. They run with
tree borrows on x86-64, and with stacked borrows on `i686-unknown-linux-gnu`, where `usize` has 32
bits and a `u64` is aligned to 4, and on `s390x-unknown-linux-gnu`, which is big-endian. A test
file that reaches no `unsafe` code that the other files do not reach does not run there, such as
the examples other than `ethernet`, and neither does the compile-fail test, since Miri cannot run
the compiler. [Commands](#commands) gives the command of each run.

The safe reads of the pointees of [aims], through a reference to a value that holds the aim, give
access to bytes outside the value. Tree borrows allows this, and stacked borrows does not. So their
tests, [`aim_reads.rs`][aim-reads], run with tree borrows only, as `.github/tests.sh` lists them:
on x86-64, and apart on the 32-bit and big-endian targets. The other tests of aims read the same
pointees from pointers, under both models.

The model check runs under Miri too, with tree borrows on x86-64 only, on a selection of pairs
that reaches each of its `unsafe` paths. [`miri_sample.rs`][sample] names the paths of `try_pun`
and `try_pun_ref`, and `parts.rs`, `dst.rs`, `views.rs` and `shapes.rs` the others. There, it does
not check `pun`, `pun_mut`, the checked `_mut` functions or regions, which the tests of `puns`
already run under Miri, and it calls `pun_mut` only to fill a buffer.

## The record of verdicts

Without the `unstable` feature, `tests/seeds.rs` records in `tests/seeds.txt` the verdict of every
function on every ordered pair of 35 types, whole and in part (by `&mut` and from the end), and
which byte patterns each type accepts from bytes. The types include structs, tuples and enums of the
`Rust` representation, a `repr(u8)` enum and a `repr(C)` union that hold them, structs with a rule,
arrays, `Option<bool>`, `Option<NonZero<u16>>`, `Result<NonZero<u8>, ()>`, a struct with an eager
pointer and one with eager regions, and structs of the `Rust` representation that hold these two.
The layout of the enums of the `Rust` representation among them, `Option` and `Result` included, is
one that `puns` [assumes][layouts#what-puns-assumes]. A rule and an enum checked as a whole each
have a type there. CI makes the record without `-Zrandomize-layout` and under three seeds of it, and
each must equal the one in the repository. This checks, for these pairs, that no verdict depends on
an [unspecified layout][layouts] without the feature.

The layout that `puns` assumes of an enum of the `Rust` representation is checked in every build
that evaluates the layout of such an enum, the tests included. Without the feature,
`tests/assumed.rs` reads `Option`, `Result` and derived enums of the `Rust` representation through
it, and unit tests compare the values of niches with the ones that `rustc` gave in the
measurements.

With the feature, the tests that need it run too: enums of the `Rust` representation, `Option`
and `Result`, and the tests of the layout that `rustc` chose. CI also builds the example
`codegen`, which needs the feature, as does the benchmark, which is run by hand.

## Compile-fail tests

195 programs in [`puns/tests/`][compile-fail] must fail to compile, each with its expected message:
the 178 of `compile-fail` with and without the feature, the 9 of `compile-fail-kept` without it, and
the 8 of `compile-fail-unstable` with it. 4 others must compile. `discover_reads_padding`,
`discover_invalid_tag` and `structure_too_large` check the three [errors of const
evaluation][rust#errors-of-const-evaluation] that `puns` relies on.

## The derive against other paths and rewritten items

[What the derive trusts][trust] states what the code that the derive generates withstands, and
these tests check it:

- `::core` that names another crate, through `extern crate .. as core;`: in
  `puns/tests/core_shadowed.rs` and `puns-model/tests/core_shadowed.rs`, its `offset_of!` swaps two
  fields and its `PhantomData` is aligned to 16 bytes, and in the case `core_usize_max` of
  `compile-fail-kept`, its `usize::MAX` is 1.
- `#[pun(crate = ..)]` that names other items: the compile-fail cases `crate_path_*`, and
  `the_crate_path_does_not_choose_the_functions` in `puns/tests/soundness.rs`.
- `Self`, or a macro, in the type of a field: the compile-fail case `derive_field_type_shadowed`,
  and `self_in_a_field_is_the_type` and `a_macro_in_a_field_names_the_type_of_the_field` in
  `puns/tests/soundness.rs`.
- An attribute below the derive that changes the item after the derive read it: the compile-fail
  cases `derive_item_rewritten` and `derive_enum_tag_rewritten`, and the tests of
  `puns/tests/soundness.rs` that use the attribute of `puns-model/rewrite`.

## Commands

```sh
cargo test -p puns --features verdict              # Without the feature, the record included
cargo test -p puns --features unstable             # With the feature
cargo test -p puns-model                           # The sample of the model check
cargo test --profile model -p puns-model --features full   # The full model check
RUSTFLAGS="-Zrandomize-layout -Zlayout-seed=1" cargo +nightly-2026-11-17 test -p puns --features verdict --test seeds
MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-strict-provenance" cargo +nightly-2026-11-17 miri test -p puns --tests
MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly-2026-11-17 miri test -p puns --lib $(MIRI=1 STACKED=1 sh .github/tests.sh default) --target i686-unknown-linux-gnu
MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly-2026-11-17 miri test -p puns --lib $(MIRI=1 STACKED=1 sh .github/tests.sh default) --target s390x-unknown-linux-gnu
MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-strict-provenance" cargo +nightly-2026-11-17 miri test -p puns $(sh .github/tests.sh tree) --target i686-unknown-linux-gnu
MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-strict-provenance" cargo +nightly-2026-11-17 miri test -p puns-model
```

[Miri]: https://github.com/rust-lang/miri
[Miri]: https://github.com/rust-lang/miri
[aim-reads]: https://github.com/uael/puns/blob/v0.2.4/puns/tests/aim_reads.rs
[compile-fail]: https://github.com/uael/puns/tree/v0.2.4/puns/tests
[model]: https://github.com/uael/puns/blob/v0.2.4/puns-model/src/lib.rs
[sample]: https://github.com/uael/puns/blob/v0.2.4/puns-model/tests/miri_sample.rs

[aims]: aims.md
[layouts]: layouts.md
[layouts#what-puns-assumes]: layouts.md#what-puns-assumes
[rust#errors-of-const-evaluation]: rust.md#errors-of-const-evaluation
[rust#the-tag-of-an-enum-of-the-rust-representation]: rust.md#the-tag-of-an-enum-of-the-rust-representation
[soundness]: soundness.md
[trust]: trust.md
