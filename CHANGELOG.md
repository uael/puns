# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0](https://github.com/uael/puns/compare/v0.2.4...v0.3.0) - 2026-11-26

### Added

- [**breaking**] Error::Validity says where the check fails ([#79](https://github.com/uael/puns/pull/79))
- the pointees of eager aims form a tree in the order of a copy, unless an aim is shared ([#78](https://github.com/uael/puns/pull/78))
- the check of eager aims follows pointees at any depth, with a loop per type ([#73](https://github.com/uael/puns/pull/73))
- [**breaking**] aims, values at a place of the checked bytes that a struct points at ([#72](https://github.com/uael/puns/pull/72))
- assume, and check when compiling, the layout that rustc gives enums of the Rust representation ([#71](https://github.com/uael/puns/pull/71))
- tuples of up to 12 elements, and packed unions ([#70](https://github.com/uael/puns/pull/70))

### Fixed

- the only aim of a struct that ends with its tail takes nothing from the budget of bytes ([#83](https://github.com/uael/puns/pull/83))
- a pointee at the cursor takes nothing from the budget of bytes ([#81](https://github.com/uael/puns/pull/81))
- the parts of the check stop no data that nearest writes ([#77](https://github.com/uael/puns/pull/77))
- the identity of a derived type is a literal of its crate as rustc tells crates apart ([#74](https://github.com/uael/puns/pull/74))
- a generic type that derives `Pun` has an identity made of its parameters ([#69](https://github.com/uael/puns/pull/69))
- a `usize::MAX` named through `::core` no longer chooses a verdict ([#68](https://github.com/uael/puns/pull/68))
- measure the offsets of a repr(C) struct of any size ([#66](https://github.com/uael/puns/pull/66))

### Other

- remove check, the worklist, the read for stacked borrows and lazy pointers ([#75](https://github.com/uael/puns/pull/75))
- pin Rust 1.99.1, the minimum version, and test the derive on 1.100 ([#76](https://github.com/uael/puns/pull/76))

## [0.2.4](https://github.com/uael/puns/compare/v0.2.3...v0.2.4) - 2026-11-23

### Fixed

- no path of `#[pun(crate = ..)]` chooses what a layout is made of ([#63](https://github.com/uael/puns/pull/63))

## [0.2.3](https://github.com/uael/puns/compare/v0.2.2...v0.2.3) - 2026-11-23

### Fixed

- the derive measures the offsets of fields on the item as compiled ([#61](https://github.com/uael/puns/pull/61))
- the derive names each field type once and checks it against the item as compiled ([#58](https://github.com/uael/puns/pull/58))
- the tags of an enum with fields are those of the enum ([#57](https://github.com/uael/puns/pull/57))
- a union proven by a field through a requirement is checked past that field ([#56](https://github.com/uael/puns/pull/56))
- a rule that loads a field reads it, in a union that a later field proves ([#55](https://github.com/uael/puns/pull/55))
- the check of a string reads no padding past where the analysis stopped ([#54](https://github.com/uael/puns/pull/54))
- a part left to the compiler that is not proven is checked only from initialized bytes ([#53](https://github.com/uael/puns/pull/53))
- a period of an array does not cut an enum of the target ([#51](https://github.com/uael/puns/pull/51))

## [0.2.2](https://github.com/uael/puns/compare/v0.2.1...v0.2.2) - 2026-11-22

### Fixed

- a union of a source may hold a value even if none of its fields has one ([#49](https://github.com/uael/puns/pull/49))
- a slice of elements without values is valid only if it is empty ([#48](https://github.com/uael/puns/pull/48))
- a struct that ends with an unsized struct without values has no value ([#47](https://github.com/uael/puns/pull/47))
- the empty string of a target string type is proven only if its automaton accepts it ([#46](https://github.com/uael/puns/pull/46))

### Other

- a plain quote for the note of the README, and a CI badge of main ([#50](https://github.com/uael/puns/pull/50))
- compile-fail cases for two refused enums and for the checks of the discovery ([#45](https://github.com/uael/puns/pull/45))
- a call of a field, an `Option` whose `None` takes the tag of a variant; drop one case ([#44](https://github.com/uael/puns/pull/44))
- compile-fail cases for the functions and the refusals that had none ([#43](https://github.com/uael/puns/pull/43))
- a `bool` of an array proven over padding by the niche of the first ([#41](https://github.com/uael/puns/pull/41))

## [0.2.1](https://github.com/uael/puns/compare/v0.2.0...v0.2.1) - 2026-11-22

### Fixed

- do not prove an array from the niche in its first element ([#37](https://github.com/uael/puns/pull/37))
- read nothing of a variant that has no value ([#38](https://github.com/uael/puns/pull/38))
- refuse a check that would read uninitialized bytes of a part that is proven ([#39](https://github.com/uael/puns/pull/39))

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
