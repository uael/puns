// The chapter is docs/concepts.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [`Error`]: Error
//! [`num`]: num
//! [`pun_ref`]: pun_ref
//! [`Pun`]: Pun
//! [`try_pun_ref`]: try_pun_ref
//! [core::mem::transmute]: core::mem::transmute
//! [crate]: crate
//! [num::u16bec]: num::u16bec
//! [num::u8in]: num::u8in
//! [opaque-scalars]: trait@Scalar#opaque-scalars
//! [properly aligned]: core::ptr#alignment
//! [rules]: super::rules
//! [rules#regions]: super::rules#regions
//! [soundness#what-deriving-pun-means]: super::soundness#what-deriving-pun-means
//! [layouts]: super::layouts
//! [limits#not-yet]: super::limits#not-yet
//! [`size_of_val`]: core::mem::size_of_val
#![doc = include_str!("../../docs/concepts.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
