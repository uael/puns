# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/uael/puns/compare/v0.2.0-beta.3...v0.2.0) - 2026-11-22

### Other

- share repeated code, and fix docs that drifted from it ([#36](https://github.com/uael/puns/pull/36))
- how puns is written ([#34](https://github.com/uael/puns/pull/34))

## [0.2.0-beta.3](https://github.com/uael/puns/compare/v0.2.0-beta.2...v0.2.0-beta.3) - 2026-11-21

### Added

- an `Option` of a scalar that a pun never makes, and `validate_but_never` at any address ([#33](https://github.com/uael/puns/pull/33))
- [**breaking**] replace self-relative pointers with scalars that a pun never makes ([#30](https://github.com/uael/puns/pull/30))

### Other

- the tests of the text of the documentation do not run under Miri ([#31](https://github.com/uael/puns/pull/31))

## [0.2.0-beta.2](https://github.com/uael/puns/compare/v0.2.0-beta.1...v0.2.0-beta.2) - 2026-11-21

### Other

- pin the links of the README to the version of the crate ([#28](https://github.com/uael/puns/pull/28))

## [0.2.0-beta.1](https://github.com/uael/puns/compare/v0.1.5...v0.2.0-beta.1) - 2026-11-21

### Added

- [**breaking**] without `unstable`, no pun depends on what Rust lets rustc lay out otherwise ([#23](https://github.com/uael/puns/pull/23))

### Fixed

- read the bytes of `None` in const evaluation only ([#24](https://github.com/uael/puns/pull/24))

### Other

- rewrite the documentation in the style of core and zerocopy ([#25](https://github.com/uael/puns/pull/25))

## [0.1.5](https://github.com/uael/puns/compare/v0.1.4...v0.1.5) - 2026-11-18

### Fixed

- rust-version 1.99, the first stable release that has what puns needs ([#21](https://github.com/uael/puns/pull/21))

## [0.1.4](https://github.com/uael/puns/compare/v0.1.3...v0.1.4) - 2026-11-18

### Other

- rules computed in the width that their values fit ([#19](https://github.com/uael/puns/pull/19))

## [0.1.3](https://github.com/uael/puns/compare/v0.1.2...v0.1.3) - 2026-11-18

### Other

- a view enum decides each variant once, from its base ([#17](https://github.com/uael/puns/pull/17))

## [0.1.2](https://github.com/uael/puns/compare/v0.1.1...v0.1.2) - 2026-11-18

### Fixed

- automata compared by their states; every public item documented, with an example ([#14](https://github.com/uael/puns/pull/14))

## [0.1.1](https://github.com/uael/puns/compare/v0.1.0...v0.1.1) - 2026-11-18

### Added

- `pointers_in` and `copy_in`, the pointers and the bytes of a valid value ([#12](https://github.com/uael/puns/pull/12))

## [0.1.0](https://github.com/uael/puns/releases/tag/v0.1.0) - 2026-11-18

### Added

- rules compiled to their arithmetic, and proven by the same rules in another struct ([#11](https://github.com/uael/puns/pull/11))
- `validate_in`, signed `NonZero`, a build for 16 bits: what `nearest` needs of `puns` ([#10](https://github.com/uael/puns/pull/10))
- enums without `repr`, `Option` and `Result`, unions, uninitialized bytes ([#9](https://github.com/uael/puns/pull/9))
- [**breaking**] self-relative pointers are declared by the crate that has them ([#5](https://github.com/uael/puns/pull/5))
- puns 0.1: the library, its checks and its release system ([#1](https://github.com/uael/puns/pull/1))

### Fixed

- the doc tests of the README build from the published sources ([#7](https://github.com/uael/puns/pull/7))

### Other

- a whole compile error, complex layouts and measured times in the README ([#8](https://github.com/uael/puns/pull/8))
- a README that starts from what a reader wants to do ([#6](https://github.com/uael/puns/pull/6))
- a failure returns at once, checksums and slices by blocks ([#4](https://github.com/uael/puns/pull/4))
- what a release ships: a status that is one, and the licenses ([#3](https://github.com/uael/puns/pull/3))
- README and CLAUDE.md
