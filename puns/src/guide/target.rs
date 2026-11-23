// The chapter is docs/target.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [`crc32`]: layout::crc32
//! [`layout::Crc`]: layout::Crc
//! [`sum16`]: layout::sum16
//! [limits#limits-of-rust]: super::limits#limits-of-rust
//! [testing#miri]: super::testing#miri
#![doc = include_str!("../../docs/target.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
