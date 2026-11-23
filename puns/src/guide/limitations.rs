// The chapter is docs/limitations.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [`from_head`]: from_head
//! [`from_tail`]: from_tail
//! [`head_mut`]: head_mut
//! [`pun_mut`]: pun_mut
//! [`Scalar`]: Scalar
//! [`tail_mut`]: tail_mut
//! [errors#values-and-padding]: super::errors#values-and-padding
//! [macro@Pun#what-the-derive-trusts]: macro@Pun#what-the-derive-trusts
//! [soundness#guarantees-and-requirements]: super::soundness#guarantees-and-requirements
//! [soundness#unions]: super::soundness#unions
//! [soundness#what-deriving-pun-means]: super::soundness#what-deriving-pun-means
#![doc = include_str!("../../docs/limitations.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
