// The chapter is docs/never_made.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [`Scalar`]: Scalar
//! [`validate_but_never`]: validate_but_never
//! [layout::Spec::never]: layout::Spec::never
#![doc = include_str!("../../docs/never_made.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
