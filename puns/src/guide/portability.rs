// The chapter is docs/portability.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [`crc32`]: crc32
//! [`layout::Crc`]: layout::Crc
//! [`sum16`]: sum16
#![doc = include_str!("../../docs/portability.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
