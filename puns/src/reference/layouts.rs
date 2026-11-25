// The chapter is docs/layouts.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [`as_bytes`]: as_bytes
//! [`layout::UNSTABLE`]: layout::UNSTABLE
//! [`num`]: num
//! [`Scalar`]: Scalar
//! [`Spec::maybe_uninit`]: layout::Spec::maybe_uninit
//! [`transmute`]: core::mem::transmute
//! [`Spec::stored_as`]: layout::Spec::stored_as
//! [errors#unspecified-layouts]: super::errors#unspecified-layouts
//! [kinds]: super::how_puns_decides#proven-checked-and-refused
//! [rule]: super::how_puns_decides#layouts-and-rules
//! [what_puns_relies_on#how-rustc-places-fields]: super::what_puns_relies_on#how-rustc-places-fields
//! [what_puns_relies_on#what-rust-documents]: super::what_puns_relies_on#what-rust-documents
//! [trust-identity]: super::trust#the-identity-of-a-type-and-the-compiler
//! [unspecified layout]: super::how_puns_decides#unspecified-layouts
#![doc = include_str!("../../docs/layouts.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
