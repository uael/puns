// The chapter is docs/trust.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [enums]: macro@Pun#enums-with-fields
//! [least]: macro@Pun#enums-with-fields
//! [`Checksum`]: layout::Checksum
//! [`Crc`]: layout::Crc
//! [`Enum`]: layout::Enum
//! [`Field`]: layout::Field
//! [`Layout`]: layout::Layout
//! [`Op`]: layout::Op
//! [`Rule`]: layout::Rule
//! [`Spec`]: layout::Spec
//! [`Varint`]: layout::Varint
//! [`Variant`]: layout::Variant
//! [independence]: super::layouts#layout-independence
//! [opaque scalar]: super::concepts#opaque-scalars
//! [rust#the-tag-of-an-enum-of-the-rust-representation]: super::rust#the-tag-of-an-enum-of-the-rust-representation
//! [rust#the-variant-read-from-the-tag]: super::rust#the-variant-read-from-the-tag
//! [same]: super::layouts#the-same-type
//! [testing#the-derive-against-a-fake-core]: super::testing#the-derive-against-a-fake-core
#![doc = include_str!("../../docs/trust.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
