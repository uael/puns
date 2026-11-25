//! The chapters of the reference, the files of `docs/` in the repository, which the [contents]
//! of the crate page list in reading order.
//!
//! [contents]: crate

pub mod aims;
pub mod concepts;
pub mod errors;
pub mod formats;
pub mod functions;
pub mod layouts;
pub mod limits;
pub mod performance;
pub mod rules;
pub mod rust;
pub mod soundness;
pub mod target;
pub mod testing;
pub mod trust;

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
