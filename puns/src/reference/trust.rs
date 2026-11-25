// The chapter is docs/trust.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [enums]: macro@Pun#enums-with-fields
//! [sample]: macro@Pun#enums-with-fields
//! [`AimOf`]: AimOf
//! [`aim_mut`]: aim_mut
//! [`aim`]: aim
//! [`Check`]: layout::Check
//! [`Checksum`]: layout::Checksum
//! [`Crc`]: layout::Crc
//! [`Enum`]: layout::Enum
//! [`Extent`]: layout::Extent
//! [`Field`]: layout::Field
//! [`InPlace`]: InPlace
//! [`Layout`]: layout::Layout
//! [`Op`]: layout::Op
//! [`Pun`]: trait@Pun
//! [`Rule`]: layout::Rule
//! [`Spec`]: layout::Spec
//! [`Start`]: layout::Start
//! [`Varint`]: layout::Varint
//! [`Variant`]: layout::Variant
//! [struct-aim]: layout::Aim
//! [independence]: super::layouts#layout-independence
//! [what_puns_relies_on#the-tag-of-an-enum-of-the-rust-representation]: super::what_puns_relies_on#the-tag-of-an-enum-of-the-rust-representation
//! [what_puns_relies_on#the-variant-read-from-the-tag]: super::what_puns_relies_on#the-variant-read-from-the-tag
//! [same]: super::layouts#the-same-type
//! [testing#the-derive-against-other-paths-and-rewritten-items]: super::testing#the-derive-against-other-paths-and-rewritten-items
#![doc = include_str!("../../docs/trust.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
