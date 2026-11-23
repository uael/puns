// The chapter is docs/errors.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [`from_head`]: from_head
//! [`from_tail`]: from_tail
//! [`head_mut`]: head_mut
//! [`head_ref`]: head_ref
//! [`num::u32le`]: num::u32le
//! [`pun`]: pun
//! [`tail_mut`]: tail_mut
//! [`tail_ref`]: tail_ref
//! [`try_pun_ref`]: try_pun_ref
//! [`validate_but_never`]: validate_but_never
//! [`zeroed`]: zeroed
//! [concepts#proven-checked-and-refused]: super::concepts#proven-checked-and-refused
//! [concepts#requirements]: super::concepts#requirements
//! [functions#writes-leave-a-valid-source]: super::functions#writes-leave-a-valid-source
//! [functions#writing-through-mut]: super::functions#writing-through-mut
//! [macro@Pun#compile-time-errors]: macro@Pun#compile-time-errors
//! [opaque scalar]: super::concepts#opaque-scalars
//! [rules#fields-in-a-rule]: super::rules#fields-in-a-rule
//! [uninhabited]: super::concepts#uninhabited-types
//! [unspecified layout]: super::concepts#unspecified-layouts
//! [msrv]: super::target#minimum-rust-version
#![doc = include_str!("../../docs/errors.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
