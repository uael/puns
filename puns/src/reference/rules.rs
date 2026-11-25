// The chapter is docs/rules.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [`from_head`]: from_head
//! [`layout::Crc`]: layout::Crc
//! [`layout::Dfa`]: layout::Dfa
//! [`layout::Op`]: layout::Op
//! [`try_pun`]: try_pun
//! [aims]: super::aims
//! [how_puns_decides#layouts-and-rules]: super::how_puns_decides#layouts-and-rules
//! [how_puns_decides#requirements]: super::how_puns_decides#requirements
//! [how_puns_decides#why-the-pair-decides]: super::how_puns_decides#why-the-pair-decides
//! [fn@crc32]: fn@layout::crc32
//! [fn@sum16]: fn@layout::sum16
//! [soundness#what-deriving-pun-means]: super::soundness#what-deriving-pun-means
//! [in_practice]: super::in_practice
//! [limits#not-yet]: super::limits#not-yet
//! [limits#limits-of-rust]: super::limits#limits-of-rust
//! [macro@Pun#assert]: macro@Pun#assert
//! [macro@Pun#len]: macro@Pun#len
//! [macro@Pun#pointer]: macro@Pun#pointer
//! [macro@Pun#regions]: macro@Pun#regions
//! [macro@Pun#size]: macro@Pun#size
#![doc = include_str!("../../docs/rules.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
