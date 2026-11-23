// The chapter is docs/formats.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [`as_bytes`]: as_bytes
//! [`from_bytes`]: from_bytes
//! [`from_head`]: from_head
//! [`num`]: num
//! [`Tag`]: Tag
//! [concepts#layouts-and-rules]: super::concepts#layouts-and-rules
//! [concepts#requirements]: super::concepts#requirements
//! [crate#reading-a-packet]: crate#reading-a-packet
//! [macro@Pun]: macro@Pun
//! [macro@Pun#assert]: macro@Pun#assert
//! [macro@Pun#get]: macro@Pun#get
//! [macro@Pun#how-the-derive-reads-a-type]: macro@Pun#how-the-derive-reads-a-type
//! [macro@Pun#len]: macro@Pun#len
//! [macro@Pun#regions]: macro@Pun#regions
//! [macro@Pun#size]: macro@Pun#size
//! [macro@Pun#tag]: macro@Pun#tag
//! [macro@Pun#valid]: macro@Pun#valid
//! [macro@Pun#views]: macro@Pun#views
//! [rules]: super::rules
#![doc = include_str!("../../docs/formats.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
