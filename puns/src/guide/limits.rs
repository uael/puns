// The chapter is docs/limits.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [`from_head`]: from_head
//! [`from_tail`]: from_tail
//! [`head_mut`]: head_mut
//! [`Layout::pattern`]: layout::Layout::pattern
//! [`pun_mut`]: pun_mut
//! [`Spec::maybe_uninit`]: layout::Spec::maybe_uninit
//! [`tail_mut`]: tail_mut
//! [errors]: super::errors
//! [errors#values-and-padding]: super::errors#values-and-padding
//! [layouts#puns-without-the-feature]: super::layouts#puns-without-the-feature
//! [layouts#the-same-type]: super::layouts#the-same-type
//! [layouts#what-a-check-may-read]: super::layouts#what-a-check-may-read
//! [macro@Pun]: macro@Pun
//! [macro@Pun#enums-with-fields]: macro@Pun#enums-with-fields
//! [macro@Pun#how-the-derive-reads-a-type]: macro@Pun#how-the-derive-reads-a-type
//! [rules]: super::rules
//! [rules#fields]: super::rules#fields
//! [rules#functions]: super::rules#functions
//! [rules#limits-of-the-evaluator]: super::rules#limits-of-the-evaluator
#![doc = include_str!("../../docs/limits.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
