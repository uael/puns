// The chapter is docs/rust_version_and_targets.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [`crc32`]: layout::crc32
//! [`layout::Crc`]: layout::Crc
//! [`sum16`]: layout::sum16
//! [limits#limits-of-rust]: super::limits#limits-of-rust
//! [testing#miri]: super::testing#miri
//! [trust#sizes-and-offsets]: super::trust#sizes-and-offsets
#![doc = include_str!("../../docs/rust_version_and_targets.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
