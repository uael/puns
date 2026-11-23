// The chapter is docs/soundness.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [`Error`]: Error
//! [`num`]: num
//! [`Pun`]: Pun
//! [`zeroed`]: zeroed
//! [limitations]: super::limitations
//! [macro@Pun#what-the-derive-trusts]: macro@Pun#what-the-derive-trusts
//! [never_made]: super::never_made
//! [num::u16bec]: num::u16bec
//! [num::u8in]: num::u8in
//! [portability]: super::portability
//! [properly aligned]: core::ptr#alignment
//! [unstable]: super::unstable
//! [unstable#what-rust-documents]: super::unstable#what-rust-documents
//! [unstable#what-this-relies-on]: super::unstable#what-this-relies-on
#![doc = include_str!("../../docs/soundness.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
