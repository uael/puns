// The chapter is docs/layouts.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [`layout::UNSTABLE`]: layout::UNSTABLE
//! [`num`]: num
//! [`Scalar`]: Scalar
//! [`transmute`]: core::mem::transmute
//! [opaque scalar]: super::concepts#unspecified-layouts-opaque-scalars-and-uninhabited-types
//! [part]: super::concepts#layouts-and-rules
//! [rule]: super::concepts#layouts-and-rules
//! [unspecified layout]: super::concepts#unspecified-layouts-opaque-scalars-and-uninhabited-types
#![doc = include_str!("../../docs/layouts.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
