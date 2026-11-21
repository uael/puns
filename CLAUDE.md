# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Development Commands

```sh
cargo build                          # Build all crates
cargo build --all-targets            # Also tests and examples: refusals are only reported by a build
cargo test --all                     # All tests: unit, integration, compile-fail, doc tests (README included)
cargo test --test api                # Run only one integration test file (also: bytes, parts, enums, ethernet, str, pointers)
cargo test --test api TESTNAME       # Run a single test
cargo test --test compile_fail       # Compile-fail tests (trybuild); TRYBUILD=overwrite to regenerate .stderr
cargo run --example ethernet         # The Ethernet example; it asserts its results and prints `ok`
cargo rustc --release --features unstable --example codegen -- --emit asm -C llvm-args=-x86-asm-syntax=intel  # Machine code, see DESIGN.md
cargo bench -p puns --features unstable --bench checks   # Time of the run-time checks, next to the same checks by hand (not in CI)
cargo bench -p puns --features unstable --bench checks -- ipv6  # Only the cases whose name has `ipv6`
MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-strict-provenance" cargo +nightly-2026-11-17 miri test -p puns --tests  # Miri, tree borrows
MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly-2026-11-17 miri test -p puns --tests --target s390x-unknown-linux-gnu  # Miri, stacked borrows, on a big-endian target
MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly-2026-11-17 miri test -p puns --tests --target i686-unknown-linux-gnu  # 32 bits
cargo test --profile model -p puns-model --features full  # The full model check (nightly in model.yml, also with `full,unstable`); `cargo test` runs a sample
cargo test -p puns --features verdict  # Without the `unstable` feature, the default: every test that builds without it, the record of verdicts included
cargo test -p puns --features unstable  # With it: the targets that need it too (`required-features`), the cases of `compile-fail-unstable`
PUNS_BLESS=1 cargo test -p puns --features verdict --test seeds  # Write the record of verdicts again (`puns/tests/seeds.txt`)
RUSTFLAGS="-Zrandomize-layout -Zlayout-seed=1" cargo +nightly-2026-11-17 test -p puns --features verdict --test seeds  # The same record, with another layout
MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly-2026-11-17 miri test -p puns-model  # A selection of the model check: the unsafe paths it reaches
cargo build -p puns --target thumbv7em-none-eabihf  # A target without std
cargo xtask fmt                      # Format code (cargo fmt on the pinned nightly)
cargo xtask fmt --check              # Check formatting
cargo xtask lint                     # Clippy (workspace, tests, examples, benches; -D warnings), without `unstable` then with it, + format check
cargo xtask lint --fix               # Auto-fix clippy + formatting
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all  # Build documentation
cargo deny check                     # License/advisory/ban auditing
```

The crates build on **stable** Rust (`1.100.0`, pinned in `rust-toolchain`). `rust-version` is
1.99, the first stable release with `size_of_val_raw` and `align_of_val_raw` in `const`, which the
layout of unsized types needs (`puns/DESIGN.md`, "What stable Rust gives us"); the CI job "Minimum
Rust version" builds on it, and a feature newer than 1.99 raises `rust-version` with its reason
written there. CI also builds for `thumbv7em-none-eabihf`, a target without `std`. A pinned nightly
(`nightly-2026-11-17`) is used by `cargo xtask fmt` (`rustfmt.toml` uses unstable options) and by
Miri, under which the trybuild test is ignored. Generate trybuild `.stderr` files with the pinned
stable.

Most refusals are panics of `const { .. }` blocks in generic functions, evaluated at
monomorphization: `cargo check` does not report them, `cargo build` does. The trybuild test has a
passing case (`tests/compile-pass/`) so that trybuild builds instead of checking.

Every check that CI runs on a push must take at most a minute, build included (CI builds the crates
of the workspace each time): check what matters, and time a check before adding one. The time that
counts is the one of the job on GitHub, which this machine does not tell: about 15 seconds of each
job go to the checkout and the toolchain before anything runs, its runners are slower and vary by a
third from one run to the next, so aim at 45 seconds and read the times of the jobs after a change
of the workflow (the first run after a change of a cache key has nothing cached). With the caches,
what comes before the work is 15 to 20 seconds, so the work of a job is at most about 20 seconds:
the build of the tests of `miri_sample` or of `parts` of the model is that alone. This is why `cargo
test --all` is twelve jobs there (the tests of `puns` in six, four without the `unstable` feature
and two with it; the sample of the model check in six, three each way), and why each Miri run of the
tests of `puns` is five jobs (without the feature, three parts of about the same time, two named and
every other file; with it, the files about it in two: split a job again rather than let it pass 45
seconds), with a cache of the standard library that Miri builds and of the sources it builds it from
(Miri builds it again when the modification time of a source changes, which a fresh install of the
toolchain does). Balance the parts by the time that passes for each test binary in the log of the
job: the time that Miri reports for a test is not that one. Under Miri, a test that loops is a
fraction of its size (`cfg!(miri)`): what Miri is there for is each unsafe path, not each value.
What is exhaustive (the full model check, behind the `full` feature of `puns-model`) runs in
`.github/workflows/model.yml`, every night and on demand, with a check of the compiler (an optimized
build refuses a wrong view behind pointers): run it when `rust-toolchain` changes. Under Miri, the
model check runs a selection of pairs that reaches every unsafe path
(`puns-model/tests/miri_sample.rs` says which pair is for which, and `tests/pointers.rs` for
pointers), not every pair, and of the examples only `ethernet` runs: the others reach the same
unsafe code of the library. A test file that is not for Miri starts with `#![cfg(not(miri))]` and
says why; CI gives Miri the other ones by name, since Miri takes a second to start each test binary,
even one that runs nothing. Each pair of types in a test of the model is an instance of the
analysis, about a tenth of a second of build: keep the tests that every push builds small.

What `unsafe` code assumes of the target (the alignment of a primitive, the width of `usize` and of
`isize`, the byte order) is asserted in a `const` where it is used, or derived from the target, and
the code is run under Miri on a target where the assumption does not hold: `i686-unknown-linux-gnu`
(32 bits, a `u64` aligned to 4) and `s390x-unknown-linux-gnu` (big-endian). A target that is
only built finds no undefined behavior, and a sentence of the design is not a check: `nearest`
0.7.1 was released with "memory is `u64` words, so 8-aligned",
which no test ever ran where it is false. A new `unsafe` block that reads a type at an address comes
with the test that stresses it (an over-aligned type, an integer wider than `isize`, the other byte
order).

Do not let the machine code of `examples/codegen.rs` regress: the instruction counts of its
functions are in `puns/DESIGN.md`; compare before and after a change of the library. A count is not a time:
`puns/benches/checks.rs` times what is checked at run time (IPv4, IPv6 behind extension headers,
a view enum, slices, CRCs, `sum16`) next to the same check written by hand, the best of 7
samples, on an idle machine; run it on both trees, back to back, before and after a change of the
evaluator or the validators. It takes minutes to build (release, LTO), and is not a check of CI.
With `PUNS_BENCH_RUNS=1000` and `PUNS_BENCH_SIDE=puns` (or `hand`) it runs a case that many times
instead, for `valgrind --tool=callgrind --toggle-collect='*checks*runs*'`: a count of instructions
per check, which a loaded machine does not change.

## Architecture

**The `unstable` feature.** Without it, nothing that `puns` does depends on what Rust lets `rustc`
lay out otherwise (the order of the fields of a struct or a tuple without `repr`, the tag of an enum
without `repr`, niches): `-Zrandomize-layout` must change no verdict and no check (the message of a
refusal may differ). With it, `puns` takes the layout that `rustc` chose. `puns/DESIGN.md`, "What
Rust keeps of a layout", is the design and the list of what Rust documents, with where. The default
is without it, and so is most of what the tests and CI run: every test of `puns` that builds without
the feature runs without it (`tests/seeds.rs` needs `verdict`), the cases of `compile-fail-kept`
included. With it run only what is about it: the targets of `required-features = ["unstable"]` and
the files with tests under `#[cfg(feature = "unstable")]` (`.github/tests.sh` lists both), and the
cases of `compile-fail-unstable`. A test that does not compile without the feature is one of these,
or relies on what Rust does not keep. The model check runs both ways too (`puns-model` has an
`unstable` feature, which turns on the one of `puns`); a refusal for what Rust does not keep is
counted there as imprecise, by design (`hook::Verdict::unspecified`). A change of the analysis that
changes a verdict of `puns/tests/seeds.txt` is written again with `PUNS_BLESS=1`, and read: the
record must be the same under every seed (CI runs three).

**puns** is type punning proven at compile time: the layout and the valid values of a type are a
`const` value (`Pun::LAYOUT`), compared by ordinary `const fn` code. A pun is **proven** (it
compiles to nothing), **checked** (only what could not be proven is checked at run time) or
**refused** (it does not compile). **`puns/DESIGN.md` is the design** (goal, what stable Rust
gives, architecture, soundness, what is not done yet): read it before changing
the library, and keep it, the crate docs and `README.md` in step with the code.

- **`puns/`** — the library (`#![no_std]`, no dependency but the derive). Self-relative
  pointers are a kind of its own (`layout::Pointer`, `deep.rs`), which a crate declares for its
  types with `SelfRelative` (`src/relative.rs`): `nearest` depends on `puns`, not the reverse.
  The tests declare pointers of their own through that API only (`tests/support/relative.rs`,
  shared by `#[path]` with the model check and the compile-fail cases), followed through their
  buffer, so that Miri runs them with stacked borrows too
- **`puns-derive/`** — `#[derive(Pun)]`, re-exported as `puns::Pun`
- **`puns-model/`** — model check of the analysis against oracles that do not use it (not
  published; uses the test-only `verdict` feature of `puns`), with `fakecore/`, a `core`
  that lies, for what the derive must not trust
- **`xtask/`** — dev automation (fmt, lint)

## Releasing

Releases are automated with [release-plz](https://release-plz.dev/). On every push to `main`, the GitHub Action opens (or updates) a release PR that bumps versions and updates `CHANGELOG.md`. Merging that PR publishes to crates.io and creates a GitHub release with a `v<version>` tag.

- Config: `release-plz.toml`
- Workflow: `.github/workflows/release-plz.yml`
- Only `puns` and `puns-derive` are published; `xtask`, `puns-model` and `fakecore` are excluded
- `puns` owns the changelog and git tags (`v{{ version }}`); `puns-derive` has no changelog or tags of its own
- Both crates are released under the same version (`version_group` in `release-plz.toml`): the higher of their next versions
- `release_always = false`: nothing is published by an ordinary push, only by merging a release PR (a branch whose name starts with `release-plz-`). The first release, 0.1.0, is the merge of the first release PR

### Commit Message Format

Use [Conventional Commits](https://www.conventionalcommits.org/) — release-plz uses them to determine version bumps and generate changelog entries.

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
- Clippy: `pedantic` + `nursery` at warn; `dbg_macro`, `allow_attributes`, `missing_safety_doc`, `undocumented_unsafe_blocks` denied; `#[expect(lint, reason = "..")]` where a lint is wrong for the code
- Every `unsafe` block has a `// SAFETY:` comment, every `unsafe fn` and trait a `# Safety` section
- Dual-licensed: Apache-2.0 OR MIT
