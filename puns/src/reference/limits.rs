// The chapter is docs/limits.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [`AIM_PARTS`]: AIM_PARTS
//! [`check_compact`]: check_compact
//! [`check`]: check
//! [`Error::Limit`]: Error::Limit
//! [`Error`]: Error
//! [`from_bytes`]: from_bytes
//! [`from_head`]: from_head
//! [`from_tail`]: from_tail
//! [`head_mut`]: head_mut
//! [`head`]: head
//! [`num`]: num
//! [`Pun`]: trait@Pun#safety
//! [`pun_mut`]: pun_mut
//! [`pun_ref`]: pun_ref
//! [`tail_mut`]: tail_mut
//! [`tail`]: tail
//! [aims]: super::aims
//! [how_puns_decides#why-the-pair-decides]: super::how_puns_decides#why-the-pair-decides
//! [errors]: super::errors
//! [errors#values-and-padding]: super::errors#values-and-padding
//! [in_practice#a-byte-order-that-the-value-gives]: super::in_practice#a-byte-order-that-the-value-gives
//! [api#writing-through-mut]: super::api#writing-through-mut
//! [api#writes-leave-a-valid-source]: super::api#writes-leave-a-valid-source
//! [kinds]: super::how_puns_decides#proven-checked-and-refused
//! [layouts#the-same-type]: super::layouts#the-same-type
//! [layouts#what-puns-assumes]: super::layouts#what-puns-assumes
//! [layouts#what-a-check-may-read]: super::layouts#what-a-check-may-read
//! [macro@Pun]: macro@Pun
//! [macro@Pun#enums-with-fields]: macro@Pun#enums-with-fields
//! [macro@Pun#get]: macro@Pun#get
//! [macro@Pun#what-the-view-functions-check]: macro@Pun#what-the-view-functions-check
//! [macro@Pun#views]: macro@Pun#views
//! [rules]: super::rules
//! [how_puns_decides#layouts-and-rules]: super::how_puns_decides#layouts-and-rules
//! [rules#fields-in-a-rule]: super::rules#fields-in-a-rule
//! [rules#functions]: super::rules#functions
//! [rules#limits-of-the-evaluator]: super::rules#limits-of-the-evaluator
//! [rules#walks]: super::rules#walks
//! [soundness#unions]: super::soundness#unions
#![doc = include_str!("../../docs/limits.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
