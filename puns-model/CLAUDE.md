# `puns-model`

The [model check](../docs/testing.md#the-model-check) of the analysis of `puns`. It is not
published, and uses the test-only `verdict` feature of `puns`. Its crate documentation,
[`src/lib.rs`](src/lib.rs), describes its types, its oracles and what runs where.

- `fakecore/` is the `core` whose items give wrong answers, against which the derive is built
  ([The derive against a fake `core`](../docs/testing.md#the-derive-against-a-fake-core)).
- `rewrite/` is an attribute that changes an item after the derive has read it, for the tests of
  `puns` about the item that `rustc` compiles: two compile-fail cases and `tests/soundness.rs`. It
  is a dev-dependency of `puns` by path, which `cargo publish` leaves out.

## Commands

```sh
cargo test -p puns-model                                  # A sample of the model check
cargo test --profile model -p puns-model --features full  # The full model check (every night in model.yml, also with `full,unstable`)
MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-strict-provenance" cargo +nightly-2026-11-17 miri test -p puns-model  # A selection of the model check: the unsafe paths it reaches
```

Run the full model check when `rust-toolchain` changes.

## Verdicts

`puns-model` has an `unstable` feature that turns on the one of `puns`, so the model check runs
both ways. A refusal for an unspecified layout is counted as imprecise, by design
(`hook::Verdict::unspecified`). A rule of the target that other rules of the source imply is
checked rather than proven ([Not yet](../docs/limits.md#not-yet)). This is sound, and the model
check does not count it as imprecise.

## Build cost

Each pair of types in a test of the model is an instance of the analysis, about a tenth of a second
of build, so keep the tests that every push builds small. Building the `miri_sample` tests, or the
`parts` tests, already takes about 20 seconds, the most that the work of a job of CI may take
([`CLAUDE.md`](../CLAUDE.md#ci)).

One `rustc` builds each test file, and must fit in the memory of a runner (16 GB). So the largest
checks are cut along their sources into several files (`tests/support/`), and the files are built in
groups, a job each (`.github/model-tests.sh`). A type added to the model adds pairs to these files:
measure the peak memory of their `rustc`.
