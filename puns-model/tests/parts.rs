//! Checks the parts of values (`head`, `tail`, `head_mut`, `tail_mut` and the others) against the
//! oracles.
//!
//! This file checks a selection that reaches the unsafe paths, natively and under Miri. Natively, a
//! sample of each size of source, with targets of every smaller size, is checked as well, in
//! `parts_by_size_1.rs` and `parts_by_size_2.rs`, files of their own so that each builds in a job
//! of CI. The full check, behind the
//! `full` feature, runs every pair of the model natively, in `parts2_*.rs`, `parts3.rs`,
//! `parts4_*.rs` and `parts8.rs`.

use puns_model::types::*;

/// Checks the parts of a selection of pairs.
#[test]
fn parts_sample() {
  // What Miri is there for: each unsafe path of the parts once, at its offset (see
  // `miri_sample.rs`). `head`, `head_ref`, `head_mut` and `tail_mut` of a scalar: `u16 -> u8`;
  // `tail_ref` at an aligned offset: `u32 -> u16`; an enum of which the head is proven:
  // `Opt -> bool`; a `&mut` refused for a rule: `NotOneOne -> u8`.
  puns_model::pairs!(check_part, 2; (u16, u8), (u32, u16), (Opt, bool), (NotOneOne, u8));
  puns_model::finish("parts, sample");
}
