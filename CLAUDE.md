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

A scalar valid for reasons that are not in its bytes, such as a self-relative offset of `nearest`,
is an [opaque scalar](puns/src/lib.rs) that its own crate declares and proves. So `nearest` depends
on `puns`, not the reverse.

The design is in the rustdoc: the crate page (the README), the chapters of the guide
(`puns::guide`), and the module docs of each part, private modules included
(`cargo doc --document-private-items`). `puns/CLAUDE.md` says where each part is written, the
invariants that cross modules and what is not done yet included. Read it, and the module docs of
what you change, before changing the library. Keep them, the guide and the README consistent with
the code.

## Toolchain

The crates build on **stable** Rust 1.100.0, pinned in `rust-toolchain`. Their `rust-version` is
1.99, for the reason that [Minimum Rust version](docs/target.md#minimum-rust-version) gives, and the
CI job "Minimum Rust version" builds on it. A change that needs a newer feature raises
`rust-version`, and gives the reason in that section. A pinned nightly (`nightly-2026-11-17`) is
used by `cargo xtask fmt` (`rustfmt.toml` uses unstable options) and by Miri.

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
[Compile-time errors](docs/errors.md). The commands of each crate are in its `CLAUDE.md`.

## CI

Every check that CI runs on a push must take at most a minute, build included, since CI builds the
crates of the workspace each time. Check what matters, and time a check before adding one. The time
that counts is the one of the job on GitHub, which a local run does not measure. About 15 seconds
of each job go to the checkout and the toolchain before anything runs, and the runners are slower
and vary by a third from one run to the next, so aim at 45 seconds, and read the times of the jobs
after a change of the workflow. The first run after a change of a cache key has nothing cached.
With the caches, a job takes 15 to 20 seconds before its work starts, and its work takes at most
about 20 seconds. Split a job again rather than let it pass 45 seconds.

This is why `cargo test --all` is fourteen jobs there: the tests of `puns` in six, four without the
`unstable` feature and two with it, and the sample of the model check in eight, four each way.

Under Miri, a test that loops runs a fraction of its iterations (`cfg!(miri)`), since Miri is there
for each unsafe path, not each value. A test file that is not for Miri starts with
`#![cfg(not(miri))]` and says why. CI gives Miri the other ones by name, since Miri takes a second
to start each test binary, even one that runs nothing. Balance the parts of a Miri run by the time
that passes for each test binary in the log of the job, since the time that Miri reports for a test
is not that one. The Miri jobs cache the standard library that Miri builds and the sources it builds
it from, since Miri builds it again when the modification time of a source changes, which a fresh
install of the toolchain does.

A target that is only built finds no undefined behavior, and a sentence of the design is not a
check: `nearest` 0.7.1 was released with "memory is `u64` words, so 8-aligned", which no test ever
ran where it is false. So the tests run under Miri on targets where the assumptions of the `unsafe`
code do not hold ([`puns/CLAUDE.md`](puns/CLAUDE.md#tests)).

## The documentation

The chapters of the guide are the files `docs/<chapter>.md`, which `puns/src/guide/<chapter>.rs`
includes, so they are read on GitHub and on docs.rs. [`xtask/src/links.rs`](xtask/src/links.rs)
states how their links and those of the README are written, and `cargo xtask links --check` checks
them in CI:

- Every link to docs.rs, crates.io or a file of the repository names the version of `puns` or its
  tag, so that it leads to the same version on GitHub, crates.io and docs.rs. A link to the `wip`
  branch, which keeps the prototypes for history, is the one exception. The release workflow pins
  the links on the branch of each release PR.
- A link between these files is relative, and the `.rs` file that includes a chapter defines its
  label as an intra-doc link.
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
