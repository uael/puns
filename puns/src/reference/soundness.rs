// The chapter is docs/soundness.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [`Layout::is_inhabited`]: layout::Layout::is_inhabited
//! [`Layout::padding`]: layout::Layout::padding
//! [`Pun`]: Pun
//! [`zeroed`]: zeroed
//! [bytes]: super::rules#reading-the-bytes-of-the-value
//! [limits#not-yet]: super::limits#not-yet
//! [padding]: super::how_puns_decides#layouts-and-rules
//! [pair]: super::how_puns_decides#why-the-pair-decides
//! [requirements]: super::how_puns_decides#requirements
//! [rule]: super::rules
//! [what_puns_relies_on]: super::what_puns_relies_on
//! [what_puns_relies_on#the-padding-of-a-new-value]: super::what_puns_relies_on#the-padding-of-a-new-value
//! [what_puns_relies_on#the-tag-of-a-reprc-enum]: super::what_puns_relies_on#the-tag-of-a-reprc-enum
//! [testing]: super::testing
//! [trust]: super::trust
//! [uninhabited]: super::how_puns_decides#uninhabited-types
//! [valid]: super::how_puns_decides#what-is-a-valid-value
#![doc = include_str!("../../docs/soundness.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
