//! Checks, natively, every pair of a few types of 1 and 2 bytes against the oracles: a part of the
//! sample of the model that is not for Miri, which runs the selection of `miri_sample.rs`. The
//! sample is in two files, `sample_by_size_1.rs` and `sample_by_size_2.rs`, so that each builds in
//! a job of CI: each pair is an instance of the analysis.

#![cfg(not(miri))]

use core::num::NonZero;

use puns_model::types::*;

/// Checks every pair of a few types of 1 and 2 bytes.
#[test]
fn sample_by_size_1() {
  puns_model::all_pairs!(8; u8, bool, NonZero<u8>, E37, SoftBit);
  puns_model::all_pairs!(6; u16, [bool; 2], (u8, bool), Aligned, Opt, NotOneOne);
  puns_model::finish("sample, 1 and 2 bytes");
}
