# CLAUDE.md

This file guides Claude Code (claude.ai/code), and any person, working in this repository, with what
holds for the whole workspace. The `CLAUDE.md` of each crate holds what is specific to it.

## The workspace

- [`puns/`](puns/CLAUDE.md): the library.
- [`puns-derive/`](puns-derive/CLAUDE.md): its derive.
- [`puns-model/`](puns-model/CLAUDE.md): the model check of its analysis (with `fakecore/` and
  `rewrite/`).
- `xtask/`: dev automation: formatting, lints, build times, the chart of check times, and the links
  of `README.md` and of `docs/`.

A self-relative pointer, such as one of `nearest`, derives `Pun` with `#[pun(pointer(..))]` on a
field of type `InPlace<T>`, an eager [aim](docs/aims.md) (`layout::Aim`) whose pointee `puns`
checks. So `nearest` depends on `puns`, not the reverse.

The design is in the rustdoc: the crate page (the README), the chapters of the reference
(`puns::reference`), and the module docs of each part, private modules included (`cargo doc
--document-private-items`). `puns/CLAUDE.md` says where each part is written, the invariants that
cross modules and what is not done yet included. Read it, and the module docs of what you change,
before changing the library. Keep them, the reference and the README consistent with the code.

## Toolchain

The crates build on **stable** Rust 1.100.0, pinned in `rust-toolchain`. Their `rust-version` is
1.99, for the reason that [Minimum Rust
version](docs/rust_version_and_targets.md#minimum-rust-version) gives, and the CI job "Minimum Rust
version" builds on it. A change that needs a newer feature raises `rust-version`, and gives the
reason in that section. A pinned nightly (`nightly-2026-11-17`) is used by `cargo xtask fmt`
(`rustfmt.toml` uses unstable options) and by Miri.

## Commands

```sh
cargo build                          # Build all crates
cargo build --all-targets            # Also tests and examples: refusals are only reported by a build
cargo test --all                     # All tests: unit, integration, compile-fail, doc tests (README included)
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all  # Build documentation
cargo xtask fmt                      # Format code (cargo fmt on the pinned nightly)
cargo xtask fmt --check              # Check formatting
cargo xtask lint                     # Clippy (workspace, tests, examples, benches; -D warnings), without `unstable` then with it, + format check
cargo xtask lint --fix               # Auto-fix clippy + formatting
cargo xtask build-time               # Build times of the analysis, for docs/performance.md (minutes)
cargo xtask checks-chart             # Write the table and the chart of check times of docs/performance.md from the record
cargo xtask checks-chart --check     # Fail if the table or the chart is not what the record gives (not in CI)
cargo xtask links                    # Pin the links of README.md and docs/*.md to the version of puns
cargo xtask links --check            # Fail if a link is not pinned or its target is missing (after cargo doc)
cargo deny check                     # License/advisory/ban auditing
```

Why a build reports refusals that `cargo check` does not is in
[Compile-time errors](docs/errors.md). The commands that run the tests, the model check and Miri
are in [Commands](docs/testing.md#commands), and the other commands of each crate in its
`CLAUDE.md`.

## CI

Every check that CI runs on a push must take at most a minute, build included, since CI builds the
crates of the workspace each time. Check what matters, and time a check before adding one. The time
that counts is the one of the job on GitHub, which a local run does not measure. A job spends time
before its work starts, even with the caches, and the time of a job varies from one run to the
next. So aim at 45 seconds, and read the times of the jobs after a change of the workflow. The
first run after a change of a cache key has nothing cached. Split a job again rather than let it
pass 45 seconds.

So `cargo test --all` is twenty jobs there: the tests of `puns` in eight, five without the
`unstable` feature and three with it (the cases of `compile-fail` in two halves each way), and the
sample of the model check in twelve, six each way (the native samples of pairs and of parts are in
two files each, `sample_by_size_*` and `parts_by_size_*`).

Under Miri, a test that loops runs a fraction of its iterations (`cfg!(miri)`), since Miri is there
for each unsafe path, not each value. A test file that is not for Miri starts with
`#![cfg(not(miri))]` and says why. One that Miri runs with tree borrows only, since it reads
through a reference what stacked borrows gives no access to (the safe reads of pointees outside
their values), is in `tree_borrows_only` of `.github/tests.sh`, and the Miri jobs of 32 bits and
big-endian run it with tree borrows apart (`tests.sh tree`). CI gives Miri the other ones by name,
since Miri takes a second to start each test binary, even one that runs nothing. Balance the parts
of a Miri run by the time that passes for each test binary in the log of the job, since the time
that Miri reports for a test is not that one. The Miri jobs cache the standard library that Miri
builds and the sources it builds it from, since Miri builds it again when the modification time of
a source changes, which a fresh install of the toolchain does.

A target that is only built finds no undefined behavior, and a sentence of the design is not a
check: `nearest` 0.7.1 was released with "memory is `u64` words, so 8-aligned", which no test ever
ran where it is false. So the tests run under Miri on targets where the assumptions of the `unsafe`
code do not hold ([`puns/CLAUDE.md`](puns/CLAUDE.md#tests)).

## The documentation

The chapters of the reference are the files `docs/<chapter>.md`, which
`puns/src/reference/<chapter>.rs` includes, so they are read on GitHub and on docs.rs.
[`xtask/src/links.rs`](xtask/src/links.rs) states how their links and those of the README are
written. In CI, `cargo xtask links --check` checks that each of these links names the version where
it must, and that its target exists if it is a file or a heading of the repository, or a page or a
section of the documentation of `puns`. It also checks that no label has two definitions, case
aside, and that the `.rs` file and the Markdown file give a label the same item. It checks no link
to another site, and no link of a `CLAUDE.md` file or of a comment. `cargo doc` with `-D warnings`
checks the intra-doc links. The links are written so:

- Every link to `puns` on docs.rs or crates.io, or to a file of the repository, names the version of
  `puns` or its tag, so that it leads to the same version on GitHub, crates.io and docs.rs. A link
  to the `wip` branch, which keeps the prototypes for history, is the one exception. The release
  workflow pins the links on the branch of each release PR.
- A link of a chapter to another chapter is relative, so that GitHub stays in the revision being
  read, and the `.rs` file that includes the chapter defines its label as an intra-doc link. The
  README links to a chapter by its URL at the tag, since crates.io resolves a relative link in
  `puns/`, where `docs` is a symbolic link that GitHub does not follow. `puns/src/lib.rs` defines
  the label of each such link as an intra-doc link.
- Each block of code names its language, and has no hidden lines (`# `), since GitHub shows them.

The times published in `docs/performance.md`, the chapter "Measured performance", name the machine,
the revision and the load. The output of the runs of the check times is in `docs/checks-<date>.txt`,
so new check times come with a new record.

## Releasing

Releases are automated with [release-plz](https://release-plz.dev/). On every push to `main`, the
GitHub Action opens (or updates) a release PR that bumps versions and updates `CHANGELOG.md`.
Merging that PR publishes to crates.io and creates a GitHub release with a `v<version>` tag.

- Config: `release-plz.toml`. Workflow: `.github/workflows/release-plz.yml`.
- Only `puns` and `puns-derive` are published; `xtask`, `puns-model`, `fakecore` and `rewrite` are
  excluded.
- `puns` owns the changelog and git tags (`v{{ version }}`); `puns-derive` has no changelog or tags
  of its own.
- Both crates are released under the same version (`version_group` in `release-plz.toml`): the
  higher of their next versions.
- `release_always = false`: nothing is published by an ordinary push, only by merging a release PR
  (a branch whose name starts with `release-plz-`). The first release, 0.1.0, was the merge of the
  first release PR.

### Commit Message Format

Use [Conventional Commits](https://www.conventionalcommits.org/), which release-plz uses to
determine version bumps and generate changelog entries.

```text
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
- Clippy: `pedantic` + `nursery` at warn; `dbg_macro`, `allow_attributes`,
  `allow_attributes_without_reason`, `missing_safety_doc`, `undocumented_unsafe_blocks` denied;
  `#[expect(lint, reason = "..")]` where a lint is wrong for the code
- Every `unsafe` block has a `// SAFETY:` comment, every `unsafe fn` and trait a `# Safety` section
