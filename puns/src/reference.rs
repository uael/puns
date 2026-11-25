//! The chapters of the reference, the files of `docs/` in the repository, which the [contents]
//! of the crate page list in reading order.
//!
//! [contents]: crate

pub mod aims;
pub mod api;
pub mod errors;
pub mod how_puns_decides;
pub mod in_practice;
pub mod layouts;
pub mod limits;
pub mod performance;
pub mod rules;
pub mod rust_version_and_targets;
pub mod soundness;
pub mod testing;
pub mod trust;
pub mod what_puns_relies_on;

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
