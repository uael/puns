//! Checks every ordered pair of the types of 8 bytes of the model, as whole values and as parts.
//!
//! This file needs the `full` feature, which `cargo test` does not turn on. It runs with
//! `cargo test --profile model -p puns-model --features full`, every night and on demand
//! (`.github/workflows/model.yml`).

use puns_model::types::*;

#[test]
fn size8() {
  puns_model::all_pairs!(usize::MAX;
    u64, [u8; 8], [bool; 8], [u32; 2], (u32, u32), CTagged, CTaggedB, [Shape; 2], [Drawing; 2],
    (u16, u8, bool, u32), OneOfTwo,
  );
  puns_model::finish("8 bytes");
}
