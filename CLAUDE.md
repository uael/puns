# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this
repository.

## Build & Development Commands

```sh
cargo build                          # Build all crates
cargo build --all-targets            # Also tests and examples: refusals are only reported by a build
cargo test --all                     # All tests: unit, integration, compile-fail, doc tests (README included)
cargo test --test api                # Run only one integration test file (also: bytes, parts, enums, ethernet, str, aims)
cargo test --test api TESTNAME       # Run a single test
cargo test --test compile_fail       # Compile-fail tests (trybuild); TRYBUILD=overwrite to regenerate .stderr
cargo run --example ethernet         # The Ethernet example; it asserts its results and prints `ok`
cargo rustc --release --features unstable --example codegen -- --emit asm -C llvm-args=-x86-asm-syntax=intel  # Machine code, see examples/codegen.rs
cargo bench -p puns --features unstable --bench checks   # Time of the run-time checks, next to the same checks by hand (not in CI)
cargo bench -p puns --features unstable --bench checks -- ipv6  # Only the cases whose name has `ipv6`
MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-strict-provenance" cargo +nightly-2026-11-17 miri test -p puns --tests  # Miri, tree borrows
MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly-2026-11-17 miri test -p puns --tests --target s390x-unknown-linux-gnu  # Miri, stacked borrows, on a big-endian target
MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly-2026-11-17 miri test -p puns --tests --target i686-unknown-linux-gnu  # 32 bits
cargo test --profile model -p puns-model --features full  # The full model check (every night in model.yml, also with `full,unstable`); `cargo test` runs a sample
cargo test -p puns --features verdict  # Without the `unstable` feature, the default: every test that builds without it, the record of verdicts included
cargo test -p puns --features unstable  # With it: the targets that need it too (`required-features`), the cases of `compile-fail-unstable`
PUNS_BLESS=1 cargo test -p puns --features verdict --test seeds  # Write the record of verdicts again (`puns/tests/seeds.txt`)
RUSTFLAGS="-Zrandomize-layout -Zlayout-seed=1" cargo +nightly-2026-11-17 test -p puns --features verdict --test seeds  # The same record, with another layout
MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-strict-provenance" cargo +nightly-2026-11-17 miri test -p puns-model  # A selection of the model check: the unsafe paths it reaches
cargo build -p puns --target thumbv7em-none-eabihf  # A target without std
cargo xtask fmt                      # Format code (cargo fmt on the pinned nightly)
cargo xtask fmt --check              # Check formatting
cargo xtask lint                     # Clippy (workspace, tests, examples, benches; -D warnings), without `unstable` then with it, + format check
cargo xtask lint --fix               # Auto-fix clippy + formatting
cargo xtask build-time               # Build times of the analysis, for docs/performance.md (minutes)
cargo xtask checks-chart             # Write the table and the chart of check times of docs/performance.md from the record
cargo xtask checks-chart --check     # Fail if the table or the chart is not what the record gives
cargo xtask links                    # Pin the links of README.md and docs/*.md to the version of puns
cargo xtask links --check            # Fail if a link is not pinned or its target is missing (after cargo doc)
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all  # Build documentation
cargo deny check                     # License/advisory/ban auditing
```

The crates build on **stable** Rust 1.100.0, pinned in `rust-toolchain`. `rust-version` is 1.99, the
first stable release with `size_of_val_raw` and `align_of_val_raw` in `const`, which the layout of
unsized types needs. The CI job "Minimum Rust version" builds on 1.99. A change that needs a newer
feature raises `rust-version`, and gives the reason in "Minimum Rust version" in the crate docs. CI
also builds for two targets without `std`: `thumbv7em-none-eabihf`, and, on the pinned nightly,
`msp430-none-elf`, whose `usize` has 16 bits. A pinned nightly (`nightly-2026-11-17`) is used by
`cargo xtask fmt` (`rustfmt.toml` uses unstable options) and by Miri, under which the trybuild test
is ignored. Generate trybuild `.stderr` files with the pinned stable.

Most refusals are panics of `const { .. }` blocks in generic functions, evaluated at
monomorphization: `cargo check` does not report them, `cargo build` does. The trybuild test has a
passing case (`tests/compile-pass/`) so that trybuild builds instead of checking.

Every check that CI runs on a push must take at most a minute, build included, since CI builds the
crates of the workspace each time. Check what matters, and time a check before adding one. The time
that counts is the one of the job on GitHub, which cannot be measured on this machine. About 15
seconds of each job go to the checkout and the toolchain before anything runs, and the runners are
slower and vary by a third from one run to the next, so aim at 45 seconds and read the times of the
jobs after a change of the workflow. The first run after a change of a cache key has nothing cached.
With the caches, a job takes 15 to 20 seconds before its work starts, so the work is at most about
20 seconds. Building the model's `miri_sample` tests, or its `parts` tests, already takes that long.

This is why `cargo test --all` is fourteen jobs there: the tests of `puns` in six, four without the
`unstable` feature and two with it, and the sample of the model check in eight, four each way. It is
also why each Miri run of the tests of `puns` is five jobs. Without the feature, there are three
parts of about the same time: two name their files, and the third takes all the remaining files.
With the feature, the files that test it are split in two. Split a job again rather than let it pass
45 seconds. These jobs have a cache of the standard library that Miri builds and of the sources it
builds it from, and Miri builds it again when the modification time of a source changes, which a
fresh install of the toolchain does. Balance the parts by the time that passes for each test binary
in the log of the job, since the time that Miri reports for a test is not that one.

Under Miri, a test that loops runs a fraction of its iterations (`cfg!(miri)`), since Miri is there
for each unsafe path, not each value. The full model check is exhaustive. It is behind the `full`
feature of `puns-model`, and runs in `.github/workflows/model.yml` every night and on demand. Run
it when `rust-toolchain` changes. One `rustc` builds each test file, and must fit in the memory of a
runner (16 GB): the largest checks are cut along their sources into several files
(`puns-model/tests/support/`, "What runs where" in the docs of `puns-model`), and the files are
built in groups, a job each (`.github/model-tests.sh`). A type added to the model adds pairs to
these files: measure the peak memory of their `rustc`. Under Miri, the model check runs a selection
of pairs that reaches every unsafe path, not every pair. `puns-model/tests/miri_sample.rs` names the
unsafe path that each pair reaches. Of the examples, only `ethernet` runs under Miri, since the
others reach the same unsafe code of the library. A test file that is not for Miri starts with
`#![cfg(not(miri))]` and says why. CI gives Miri the other ones by name, since Miri takes a second
to start each test binary, even one that runs nothing. Each pair of types in a test of the model is
an instance of the analysis, about a tenth of a second of build, so keep the tests that every push
builds small.

What `unsafe` code assumes of the target (the alignment of a primitive, the width of `usize` and of
`isize`, the byte order) is asserted in a `const` where it is used, or derived from the target, and
the code is run under Miri on a target where the assumption does not hold: `i686-unknown-linux-gnu`
(32 bits, a `u64` aligned to 4) and `s390x-unknown-linux-gnu` (big-endian). A target that is only
built finds no undefined behavior, and a sentence of the design is not a check: `nearest` 0.7.1 was
released with "memory is `u64` words, so 8-aligned", which no test ever ran where it is false. A new
`unsafe` block that reads a type at an address comes with the test that stresses it (an over-aligned
type, an integer wider than `isize`, the other byte order).

Do not let the machine code of `examples/codegen.rs` regress. The instruction counts of its
functions are in its module documentation, so compare them before and after a change of the library.
A count is not a time, though. `puns/benches/checks.rs` times what is checked at run time (IPv4,
IPv6 behind extension headers, a view enum, slices, CRCs, `sum16`) next to the same check written by
hand, the best of 7 samples, on an idle machine. Run it on the tree before and the tree after a
change of the evaluator or the validators, one right after the other. It takes minutes to build
(release, LTO), and is not a check of CI. With `PUNS_BENCH_RUNS=1000` and `PUNS_BENCH_SIDE=puns` (or
`hand`), it runs a case that many times instead, for
`valgrind --tool=callgrind --toggle-collect='*checks*runs*'`, which gives a count of instructions
per check that a loaded machine does not change. The times published in `docs/performance.md` name
the machine, the revision and the load, and the output of their runs is in `docs/checks-<date>.txt`,
so new times come with a new record.

## Architecture

**The `unstable` feature.** Without it, nothing that `puns` does depends on a layout that Rust
leaves to the compiler (the order of the fields of a struct or a tuple of the `Rust` representation,
the tag of an enum of that representation, niches), so `-Zrandomize-layout` must change no verdict
and no check, though the message of a refusal may differ. The one exception is the layout of an enum
of the `Rust` representation that `puns` assumes (`puns/src/assume.rs`): it is computed from the
layouts of the fields, which `-Zrandomize-layout` does not change, and compared with the layout that
`rustc` chose, and the build fails if they differ. With the feature, `puns` takes the layout that
`rustc` chose. "The `unstable` feature" in the crate docs (`puns/src/lib.rs`) states the design,
lists what Rust documents and where, and what `puns` assumes. The default is without it, and so is
most of what the tests and CI run: every test of `puns` that builds without the feature runs without
it (`tests/seeds.rs` needs `verdict`), the cases of `compile-fail-kept` included. Only what is about
the feature runs with it: the targets of `required-features = ["unstable"]`, the files with tests
under `#[cfg(feature = "unstable")]` (`.github/tests.sh` lists both), and the cases of
`compile-fail-unstable`. A test that does not compile without the feature is one of these, or relies
on a layout that Rust leaves to the compiler. The model check runs both ways too, since `puns-model`
has an `unstable` feature that turns on the one of `puns`, and a refusal for a layout that Rust
leaves to the compiler is counted there as imprecise, by design (`hook::Verdict::unspecified`). A
change of the analysis that changes a verdict of `puns/tests/seeds.txt` is written again with
`PUNS_BLESS=1` and read, since the record must be the same under every seed (CI runs three).

**puns** is type punning proven at compile time: the layout and the valid values of a type are a
`const` value (`Pun::LAYOUT`), compared by ordinary `const fn` code. A pun is **proven** (it makes
no check at run time), **checked** (only what could not be proven is checked at run time) or
**refused** (it does not compile). **The design is in the rustdoc**: the crate docs (goal,
soundness, the `unstable` feature, minimum Rust version, portability, limitations) and the module
docs of each part, private modules included (`cargo doc --document-private-items`).
**`puns/CLAUDE.md` summarizes it**, with a link to where each part is written, and holds what is not
done yet: read it, and the module docs of what you change, before changing the library. Keep them,
the crate docs and `README.md` consistent with the code.

- **`puns/`**: the library (`#![no_std]`, no dependency but the derive). A self-relative pointer,
  such as one of `nearest`, derives `Pun` with `#[pun(pointer(..))]` on a field of type `InPlace<T>`,
  an eager aim (`layout::Aim`) whose pointee `puns` checks, so `nearest` depends on `puns`, not the
  reverse. The tests declare such a pointer through that API only (`tests/support/aims.rs`, shared
  by `#[path]` with the compile-fail cases).
- **`puns-derive/`**: `#[derive(Pun)]`, re-exported as `puns::Pun`.
- **`puns-model/`**: the model check of the analysis against oracles that do not use it (not
  published, and using the test-only `verdict` feature of `puns`), with `fakecore/`, a `core` whose
  items give wrong answers, for what the derive must not trust, and `rewrite/`, an attribute that
  changes an item after the derive has read it, for a compile-fail case of `puns` (a
  dev-dependency by path, which `cargo publish` leaves out).
- **`xtask/`**: dev automation (fmt, lint, build times, the chart of check times, the links of the
  README).

## Releasing

Releases are automated with [release-plz](https://release-plz.dev/). On every push to `main`, the
GitHub Action opens (or updates) a release PR that bumps versions and updates `CHANGELOG.md`.
Merging that PR publishes to crates.io and creates a GitHub release with a `v<version>` tag.

- Config: `release-plz.toml`
- Workflow: `.github/workflows/release-plz.yml`
- Only `puns` and `puns-derive` are published; `xtask`, `puns-model`, `fakecore` and `rewrite`
  are excluded
- `puns` owns the changelog and git tags (`v{{ version }}`); `puns-derive` has no changelog or tags
  of its own
- Both crates are released under the same version (`version_group` in `release-plz.toml`): the
  higher of their next versions
- `release_always = false`: nothing is published by an ordinary push, only by merging a release PR
  (a branch whose name starts with `release-plz-`). The first release, 0.1.0, was the merge of the
  first release PR
- Every link of `README.md` to docs.rs, crates.io or a file of the repository names the version of
  `puns` or its tag, so that the README leads to the same version on GitHub, crates.io and docs.rs
  (`xtask/src/links.rs`). A link to the `wip` branch, which keeps the prototypes for history, is
  the one exception. The workflow pins them on the branch of each release PR. A new link is
  written pinned (`cargo xtask links`), and CI checks them (`cargo xtask links --check`). The files
  of `docs/` are read on GitHub only, and link to the repository relatively

### Commit Message Format

Use [Conventional Commits](https://www.conventionalcommits.org/), which release-plz uses to
determine version bumps and generate changelog entries.

```
<type>[optional scope]: <description>

[optional body]
```

| Type | Bump | Example |
|------|------|---------|
| `fix:` | patch | `fix: reject packed structs whose tail is aligned` |
| `feat:` | minor | `feat: patterns other than str from the derive` |
| `feat!:` / `BREAKING CHANGE:` | major | `feat!: rename unpack to from_head` |
| `docs:`, `ci:`, `chore:`, `refactor:`, `test:`, `style:` | none | `ci: pin the Miri nightly` |

## Code Style

- 2-space indentation, max 100 columns
- Imports: reordered, grouped by `StdExternalCrate`, granularity `Crate`
- Clippy: `pedantic` + `nursery` at warn; `dbg_macro`, `allow_attributes`, `missing_safety_doc`,
  `undocumented_unsafe_blocks` denied; `#[expect(lint, reason = "..")]` where a lint is wrong for
  the code
- Every `unsafe` block has a `// SAFETY:` comment, every `unsafe fn` and trait a `# Safety` section
- Dual-licensed: Apache-2.0 OR MIT
