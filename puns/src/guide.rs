// The guide's page is docs/README.md, read on GitHub as well. Rustdoc keeps the first definition
// of a link, so this one, before it, makes its link to the crate page intra-doc.
//! [crate]: crate
#![doc = include_str!("../docs/README.md")]

pub mod concepts;
pub mod errors;
pub mod formats;
pub mod functions;
pub mod layouts;
pub mod limits;
pub mod msrv;
pub mod never_made;
pub mod portability;
pub mod rules;
pub mod soundness;

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
