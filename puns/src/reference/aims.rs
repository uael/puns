// The chapter is docs/aims.md, read on GitHub as well. Rustdoc keeps the first
// definition of a link, so these, before it, make its links to items and chapters intra-doc.
//! [`AimOf`]: AimOf
//! [`aim_mut`]: aim_mut
//! [`aim`]: aim
//! [`check_compact`]: check_compact
//! [`check`]: check
//! [`Failure`]: Failure
//! [`from_bytes_mut`]: from_bytes_mut
//! [`from_bytes`]: from_bytes
//! [`from_head`]: from_head
//! [`from_tail`]: from_tail
//! [`InPlace`]: InPlace
//! [`layout::Aim`]: layout::Aim
//! [`layout::Check`]: layout::Check
//! [`layout::Extent`]: layout::Extent
//! [`layout::Start`]: layout::Start
//! [`Pointee`]: Pointee
//! [`pun_mut`]: pun_mut
//! [`pun`]: pun
//! [`Thin`]: Thin
//! [trait-pun]: trait@Pun
//! [`try_pun_mut`]: try_pun_mut
//! [`try_pun_ref`]: try_pun_ref
//! [`try_pun`]: try_pun
//! [`Worklist`]: Worklist
//! [`zeroed`]: zeroed
//! [concepts#aims]: super::concepts#aims
//! [layouts#the-same-type]: super::layouts#the-same-type
//! [limits#bounds-of-the-checks]: super::limits#bounds-of-the-checks
//! [macro@Pun#aims]: macro@Pun#aims
//! [rust#the-provenance-of-a-reference-to-a-value-with-an-eager-aim]: super::rust#the-provenance-of-a-reference-to-a-value-with-an-eager-aim
#![doc = include_str!("../../docs/aims.md")]

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
