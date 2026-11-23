# `puns`

The library: `#![no_std]`, no dependency but the derive.

## Where the design is written

- Each chapter of the reference is a file of [`docs/`](../docs), included by
  `src/reference/<chapter>.rs`, which only documents and imports the crate under `#[cfg(doc)]` for
  its links.
- The derive page, on `pub use puns_derive::Pun` in [`src/lib.rs`](src/lib.rs), states each
  attribute, [The language of rules](../docs/rules.md) what a rule means and reads, and
  [What the derive trusts](../docs/trust.md) what the derive trusts.
- `Scalar`, in [`src/lib.rs`](src/lib.rs), states the contract of a scalar declared by hand, an
  opaque scalar (`Spec::never`) included, and what `puns` guarantees its crate in return.
- [Unspecified layouts](../docs/layouts.md) states the design of the `unstable` feature, and
  [What Rust documents](../docs/rust.md#what-rust-documents) lists what Rust documents and where.
- The invariants that cross modules are in [Soundness](../docs/soundness.md),
  [Layout independence](../docs/layouts.md#layout-independence),
  [Requirements](../docs/concepts.md#requirements), `Scalar`, the "Safety" of `Pun`, and
  [What the derive trusts](../docs/trust.md).
- What is not done yet is in [Not yet](../docs/limits.md#not-yet).

`cargo doc --no-deps -p puns --document-private-items` renders the private modules, but `view` is
`#[doc(hidden)]`, so read its documentation in the source.

- [`analysis`](src/analysis.rs) compares two layouts at compile time: whether a pun fits, which
  values are valid, what must be checked, and whether a `&mut` is allowed, a section each.
- [`validate`](src/validate.rs) checks at run time what the marks of the analysis name. It recurses
  over the types, not the values, and each operation of a rule is a constant of a function of its
  own, applied to `layout::Machine`, so no interpreter chooses operations at run time.
- [`narrow`](src/narrow.rs) computes the range of every value of a rule, so that each operation
  runs in a register where it fits, with the same results as on `u128`.
- [`view`](src/view.rs) decides each variant of a view enum once, from its base: the first later
  variant to which `pun_ref` of the variant is proven, and from which `try_pun_ref` to it compiles.
- [`discover`](src/discover.rs) finds the layouts of enums of the `Rust` representation by
  experiments, and names the facts of the compiler that they rely on.
  [`std_enums`](src/std_enums.rs) applies it to `Option` and `Result`.
- [`ptr`](src/ptr.rs) splits and builds wide pointers in `const`, verified for each type.

## Commands

The commands that run the tests both ways, the record of verdicts under a seed, and Miri on each
target are in [Commands](../docs/testing.md#commands). These are the others:

```sh
cargo test --test api                # Run only one test file of tests/ (also: bytes, parts, enums, ethernet, str, never)
cargo test --test api TESTNAME       # Run a single test
cargo test --test compile_fail       # Compile-fail tests (trybuild); TRYBUILD=overwrite to regenerate .stderr
cargo run --example ethernet         # The Ethernet example; it asserts its results and prints `ok`
PUNS_BLESS=1 cargo test -p puns --features verdict --test seeds  # Write the record of verdicts again (`puns/tests/seeds.txt`)
cargo build -p puns --target thumbv7em-none-eabihf  # A target without std (CI also builds msp430-none-elf, see docs/target.md)
cargo rustc --release --features unstable --example codegen -- --emit asm -C llvm-args=-x86-asm-syntax=intel  # Machine code, see examples/codegen.rs
cargo bench -p puns --features unstable --bench checks   # Time of the run-time checks, next to the same checks by hand (not in CI)
cargo bench -p puns --features unstable --bench checks -- ipv6  # Only the cases whose name has `ipv6`
```

## Tests

Most of what the tests and CI run is without the `unstable` feature, the default: every test
that builds without it (`tests/seeds.rs` needs `verdict`), the cases of `compile-fail-kept`
included. Only what is about the feature runs with it: the targets of
`required-features = ["unstable"]`, the files with tests under `#[cfg(feature = "unstable")]`
(`.github/tests.sh` lists both), and the cases of `compile-fail-unstable`. A test that does not
compile without the feature is one of these, or relies on an unspecified layout. A change of the
analysis that changes a verdict of `tests/seeds.txt` is written again with `PUNS_BLESS=1` and read,
since the record must be the same under every seed (CI runs three).

The trybuild test has a passing case (`tests/compile-pass/`), so that trybuild builds instead of
checking. Miri ignores it. Generate its `.stderr` files with the pinned stable.

Each Miri run of the tests is five jobs in CI. Without the feature, there are three parts of about
the same time: two name their files, and the third takes all the remaining files. With the feature,
the files that test it are split in two.

What `unsafe` code assumes of the target is run under Miri on a target where the assumption does
not hold ([Miri](../docs/testing.md#miri)). A new `unsafe` block that reads a type at an address
comes with the test that stresses it (an over-aligned type, an integer wider than `isize`, the
other byte order).

`Layout::PIN` fixes the types whose functions make a layout, so a new type whose functions the
derive calls goes there. The cases `crate_path_*` of `tests/compile-fail` test that no path of
`#[pun(crate = ..)]` replaces one, and `fakecore` in `puns-model` that `::core` does not.

Every opaque scalar of the repository is declared through the public API only (`Scalar`,
`Spec::never`), as another crate would. Most share `tests/support/never.rs`, which the compile-fail
cases include by `#[path]`.

## Measurements

Do not let the machine code of `examples/codegen.rs` regress: compare it with the table of its
module documentation before and after a change of the library.

Run `benches/checks.rs` on the tree before and the tree after a change of the evaluator or the
validators, one right after the other. The machine should be idle, since its load changes the
times. The benchmark takes minutes to build (release, LTO), and is not a check of CI. With
`PUNS_BENCH_RUNS=1000` and `PUNS_BENCH_SIDE=puns` (or `hand`), it runs a case that many times
instead, for `valgrind --tool=callgrind --toggle-collect='*checks*runs*'`, which gives a count of
instructions per check that a loaded machine does not change.
