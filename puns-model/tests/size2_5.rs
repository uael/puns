//! Checks the pairs of `support/size2.rs` whose source is the fifth of every five.
//!
//! This file needs the `full` feature, which `cargo test` does not turn on. It runs with
//! `cargo test --profile model -p puns-model --features full`, every night and on demand
//! (`.github/workflows/model.yml`).

#[macro_use]
#[path = "support/size2.rs"]
mod size2;

#[test]
fn size2_5() {
  pairs!(_ _ _ _ x);
}
