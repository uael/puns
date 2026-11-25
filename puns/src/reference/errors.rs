// The chapter is docs/errors.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [`aim_mut`]: aim_mut
//! [`from_bytes`]: from_bytes
//! [`from_head`]: from_head
//! [`from_tail`]: from_tail
//! [`head_mut`]: head_mut
//! [`head_ref`]: head_ref
//! [`num::u32le`]: num::u32le
//! [`pun`]: pun
//! [`try_pun_ref`]: try_pun_ref
//! [`zeroed`]: zeroed
//! [aims]: super::aims
//! [how_puns_decides#proven-checked-and-refused]: super::how_puns_decides#proven-checked-and-refused
//! [how_puns_decides#requirements]: super::how_puns_decides#requirements
//! [api#writes-leave-a-valid-source]: super::api#writes-leave-a-valid-source
//! [api#writing-through-mut]: super::api#writing-through-mut
//! [layouts#the-same-type]: super::layouts#the-same-type
//! [layouts#what-puns-assumes]: super::layouts#what-puns-assumes
//! [macro@Pun#compile-time-errors]: macro@Pun#compile-time-errors
//! [macro@Pun#crate]: macro@Pun#crate
//! [macro@Pun#enums-with-fields]: macro@Pun#enums-with-fields
//! [rules]: super::rules
//! [rules#fields-in-a-rule]: super::rules#fields-in-a-rule
//! [what_puns_relies_on]: super::what_puns_relies_on
//! [trust#the-generated-code]: super::trust#the-generated-code
//! [uninhabited]: super::how_puns_decides#uninhabited-types
//! [unspecified layout]: super::how_puns_decides#unspecified-layouts
//! [msrv]: super::rust_version_and_targets#minimum-rust-version
#![doc = include_str!("../../docs/errors.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
