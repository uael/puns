// The chapter is docs/concepts.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [`Error`]: Error
//! [`num`]: num
//! [`pun_ref`]: pun_ref
//! [`Pun`]: Pun
//! [`try_pun_ref`]: try_pun_ref
//! [core::mem::transmute]: core::mem::transmute
//! [crate]: crate
//! [macro@Pun#views]: macro@Pun#views
//! [never_made]: super::never_made
//! [num::u16bec]: num::u16bec
//! [num::u8in]: num::u8in
//! [properly aligned]: core::ptr#alignment
//! [rules]: super::rules
//! [rules#extent]: super::rules#extent
//! [rules#regions]: super::rules#regions
//! [soundness#what-deriving-pun-means]: super::soundness#what-deriving-pun-means
//! [layouts]: super::layouts
#![doc = include_str!("../../docs/concepts.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
