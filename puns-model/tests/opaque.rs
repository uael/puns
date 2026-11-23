//! Checks the opaque scalars (`Spec::opaque`) against their oracles: read by their values, never
//! made, not even from the same type, and excluded from the variants that a check accepts, but in
//! a `Deferred<T>`, which takes them by their values.
//!
//! The full run adds them to every pair of their size (`support/size1.rs`, `support/size2.rs`).

use core::num::NonZero;

use puns::Deferred;
use puns_model::types::*;

#[test]
fn opaque() {
  puns_model::pairs!(check_pair, usize::MAX;
    (u8, Opaque), (Opaque, u8), (bool, Opaque), (Opaque, Opaque),
    ([u8; 2], HoldsOpaque), (HoldsOpaque, [u8; 2]), (HoldsOpaque, (bool, u8)),
    (HoldsOpaque, HoldsOpaque), (u16, MaybeOpaque), (MaybeOpaque, MaybeOpaque),
    (MaybeOpaque, [u8; 2]), (ByteZero, MaybeOpaque), (u8, Option<OpaqueNz>),
    (Option<OpaqueNz>, u8), (OpaqueNz, Option<OpaqueNz>), (Option<OpaqueNz>, OpaqueNz),
    (NonZero<u8>, Option<OpaqueNz>), (Option<OpaqueNz>, Option<OpaqueNz>),
    ([u8; 2], HoldsOptOpaque), (HoldsOptOpaque, [u8; 2]), (HoldsOptOpaque, HoldsOptOpaque),
    (u8, Deferred<Opaque>), (u8, Deferred<OpaqueNz>), (Opaque, Deferred<OpaqueNz>),
    (u8, Deferred<Option<OpaqueNz>>), ([u8; 2], Deferred<HoldsOpaque>),
    (HoldsOpaque, Deferred<HoldsOpaque>), ([u8; 2], Deferred<HoldsOptOpaque>),
    (u16, Deferred<MaybeOpaque>), (ByteZero, Deferred<MaybeOpaque>),
  );
  puns_model::pairs!(check_part, usize::MAX;
    (HoldsOpaque, bool), (HoldsOpaque, Opaque), ([u8; 2], Opaque), (MaybeOpaque, u8),
    (HoldsOptOpaque, u8), (HoldsOptOpaque, Option<OpaqueNz>),
  );
  puns_model::finish("opaque");
}
