// The chapter is docs/trust.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [enums]: macro@Pun#enums-with-fields
//! [least]: macro@Pun#enums-with-fields
//! [`Crc`]: layout::Crc
//! [`Enum`]: layout::Enum
//! [`Field`]: layout::Field
//! [`Layout`]: layout::Layout
//! [`Op`]: layout::Op
//! [`Rule`]: layout::Rule
//! [`Spec`]: layout::Spec
//! [`Variant`]: layout::Variant
//! [independence]: super::layouts#layout-independence
//! [opaque scalar]: super::concepts#unspecified-layouts-opaque-scalars-and-uninhabited-types
//! [rust]: super::rust
//! [same]: super::layouts#the-same-type
//! [testing]: super::testing
#![doc = include_str!("../../docs/trust.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
