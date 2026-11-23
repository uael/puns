//! # The guide
//!
//! The chapters of the guide are listed in reading order in the [contents](crate#contents) of
//! the crate page.

pub mod concepts;
pub mod errors;
pub mod functions;
pub mod limitations;
pub mod msrv;
pub mod never_made;
pub mod portability;
pub mod soundness;
pub mod unstable;

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
