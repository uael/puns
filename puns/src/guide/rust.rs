// The chapter is docs/rust.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [derive#enums-with-fields]: macro@Pun#enums-with-fields
//! [functions#in-a-constant]: super::functions#in-a-constant
//! [limits#bounds-of-the-analysis]: super::limits#bounds-of-the-analysis
//! [opaque scalar]: super::concepts#opaque-scalars
//! [soundness#unions]: super::soundness#unions
//! [testing#compile-fail-tests]: super::testing#compile-fail-tests
//! [testing#miri]: super::testing#miri
//! [testing#the-model-check]: super::testing#the-model-check
//! [testing#the-record-of-verdicts]: super::testing#the-record-of-verdicts
//! [trust#attribute-macros-below-the-derive]: super::trust#attribute-macros-below-the-derive
#![doc = include_str!("../../docs/rust.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
