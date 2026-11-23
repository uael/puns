// The chapter is docs/layouts.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [`as_bytes`]: as_bytes
//! [`layout::UNSTABLE`]: layout::UNSTABLE
//! [`num`]: num
//! [`Scalar`]: Scalar
//! [`Spec::maybe_uninit`]: layout::Spec::maybe_uninit
//! [`transmute`]: core::mem::transmute
//! [`validate_but_never`]: validate_but_never
//! [errors#unspecified-layouts]: super::errors#unspecified-layouts
//! [kinds]: super::concepts#proven-checked-and-refused
//! [opaque-scalars]: trait@Scalar#opaque-scalars
//! [opaque scalar]: super::concepts#unspecified-layouts-opaque-scalars-and-uninhabited-types
//! [part]: super::concepts#layouts-and-rules
//! [rule]: super::concepts#layouts-and-rules
//! [rust#what-rust-documents]: super::rust#what-rust-documents
//! [trust-identity]: super::trust#the-identity-of-a-type-and-the-compiler
//! [unspecified layout]: super::concepts#unspecified-layouts-opaque-scalars-and-uninhabited-types
#![doc = include_str!("../../docs/layouts.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
