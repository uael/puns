// The chapter is docs/msrv.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [limitations#limits]: super::limitations#limits
#![doc = include_str!("../../docs/msrv.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
