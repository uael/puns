// The chapter is docs/rules.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [`from_head`]: from_head
//! [`layout::Crc`]: layout::Crc
//! [`layout::Dfa`]: layout::Dfa
//! [`layout::Op`]: layout::Op
//! [`size_of_val`]: size_of_val
//! [`try_pun`]: try_pun
//! [concepts#layouts-and-rules]: super::concepts#layouts-and-rules
//! [concepts#requirements]: super::concepts#requirements
//! [concepts#unspecified-layouts-opaque-scalars-and-uninhabited-types]: super::concepts#unspecified-layouts-opaque-scalars-and-uninhabited-types
//! [concepts#why-the-pair-decides]: super::concepts#why-the-pair-decides
//! [fn@crc32]: fn@crc32
//! [fn@sum16]: fn@sum16
//! [formats]: super::formats
//! [macro@Pun#assert]: macro@Pun#assert
//! [macro@Pun#len]: macro@Pun#len
//! [macro@Pun#regions]: macro@Pun#regions
//! [macro@Pun#size]: macro@Pun#size
#![doc = include_str!("../../docs/rules.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
