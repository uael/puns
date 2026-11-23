// The chapter is docs/errors.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [`from_head`]: from_head
//! [`from_tail`]: from_tail
//! [`num::u32le`]: num::u32le
//! [`pun_mut`]: pun_mut
//! [`pun`]: pun
//! [`try_pun_ref`]: try_pun_ref
//! [`validate_but_never`]: validate_but_never
//! [`zeroed`]: zeroed
//! [functions#writing-through-mut]: super::functions#writing-through-mut
//! [macro@Pun#compile-time-errors]: macro@Pun#compile-time-errors
//! [opaque scalar]: super::concepts#unspecified-layouts-opaque-scalars-and-uninhabited-types
//! [rules#fields]: super::rules#fields
//! [uninhabited]: super::concepts#unspecified-layouts-opaque-scalars-and-uninhabited-types
//! [unspecified layout]: super::concepts#unspecified-layouts-opaque-scalars-and-uninhabited-types
#![doc = include_str!("../../docs/errors.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
