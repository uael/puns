// The chapter is docs/msrv.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [limits#bounds-of-the-analysis]: super::limits#bounds-of-the-analysis
//! [limits#limits-of-rust]: super::limits#limits-of-rust
#![doc = include_str!("../../docs/msrv.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
