// The chapter is docs/functions.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [`as_bytes`]: as_bytes
//! [`from_bytes_mut`]: from_bytes_mut
//! [`from_bytes`]: from_bytes
//! [`from_head_mut`]: from_head_mut
//! [`from_head`]: from_head
//! [`from_tail_mut`]: from_tail_mut
//! [`from_tail`]: from_tail
//! [`head_mut`]: head_mut
//! [`head_ref`]: head_ref
//! [`head`]: head
//! [`pun_mut`]: pun_mut
//! [`pun_ref`]: pun_ref
//! [`pun`]: pun
//! [`tail_mut`]: tail_mut
//! [`tail_ref`]: tail_ref
//! [`tail`]: tail
//! [`try_pun_mut`]: try_pun_mut
//! [`try_pun_ref`]: try_pun_ref
//! [`try_pun`]: try_pun
//! [concepts#layouts-and-rules]: super::concepts#layouts-and-rules
//! [concepts#proven-checked-and-refused]: super::concepts#proven-checked-and-refused
//! [concepts#requirements]: super::concepts#requirements
//! [concepts#opaque-scalars]: super::concepts#opaque-scalars
//! [concepts#what-is-a-valid-value]: super::concepts#what-is-a-valid-value
//! [concepts#why-the-pair-decides]: super::concepts#why-the-pair-decides
//! [core::mem::transmute#transmutation-between-pointers-and-integers]: core::mem::transmute#transmutation-between-pointers-and-integers
//! [crate#going-further]: crate#going-further
//! [crate#reading-one-of-several-types]: crate#reading-one-of-several-types
//! [errors]: super::errors
//! [limits#limits-of-rust]: super::limits#limits-of-rust
//! [limits#not-yet]: super::limits#not-yet
#![doc = include_str!("../../docs/functions.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
