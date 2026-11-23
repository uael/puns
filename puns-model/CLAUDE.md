# `puns-model`

The [model check](../docs/testing.md#the-model-check) of the analysis of `puns`. It is not
published, and uses the test-only `verdict` feature of `puns`. Its crate documentation,
[`src/lib.rs`](src/lib.rs), describes its types, its oracles and what runs where.

- `fakecore/` is the `core` whose items give wrong answers, against which the derive is built
  ([The derive against other paths and rewritten
  items](../docs/testing.md#the-derive-against-other-paths-and-rewritten-items)).
- `rewrite/` is an attribute that changes an item after the derive has read it, for the tests of
  `puns` about the item that `rustc` compiles: two compile-fail cases and `tests/soundness.rs`. It
  is a dev-dependency of `puns` by path, which `cargo publish` leaves out.

## Commands

The commands of the sample, of the full model check and of its selection under Miri are in
[Commands](../docs/testing.md#commands). The full model check runs every night in `model.yml`,
also with `full,unstable`. Run it when `rust-toolchain` changes.

## Verdicts

`puns-model` has an `unstable` feature that turns on the one of `puns`, so the model check runs
both ways. A refusal for an unspecified layout is counted as imprecise, by design
(`hook::Verdict::unspecified`). A rule of the target that other rules of the source imply is
checked rather than proven ([Not yet](../docs/limits.md#not-yet)). This is sound, and the model
check does not count it as imprecise.

## Build cost

Each pair of types in a test of the model is an instance of the analysis, which every push builds,
so keep these tests small. The jobs of CI that build the `miri_sample` tests or the `parts` tests
are already among the longest ([`CLAUDE.md`](../CLAUDE.md#ci) gives their times).

One `rustc` builds each test file, and must fit in the memory of a runner (16 GB). So the largest
checks are cut along their sources into several files (`tests/support/`), and the files are built in
groups, a job each (`.github/model-tests.sh`). A type added to the model adds pairs to these files:
measure the peak memory of their `rustc`.
