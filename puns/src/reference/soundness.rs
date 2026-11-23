// The chapter is docs/soundness.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [`Layout::is_inhabited`]: layout::Layout::is_inhabited
//! [`Layout::padding`]: layout::Layout::padding
//! [`Pun`]: Pun
//! [`zeroed`]: zeroed
//! [bytes]: super::rules#reading-the-bytes-of-the-value
//! [limits#not-yet]: super::limits#not-yet
//! [padding]: super::concepts#layouts-and-rules
//! [pair]: super::concepts#why-the-pair-decides
//! [requirements]: super::concepts#requirements
//! [rule]: super::rules
//! [rust]: super::rust
//! [rust#the-padding-of-a-new-value]: super::rust#the-padding-of-a-new-value
//! [rust#the-tag-of-a-reprc-enum]: super::rust#the-tag-of-a-reprc-enum
//! [testing]: super::testing
//! [trust]: super::trust
//! [uninhabited]: super::concepts#uninhabited-types
//! [valid]: super::concepts#what-is-a-valid-value
#![doc = include_str!("../../docs/soundness.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
