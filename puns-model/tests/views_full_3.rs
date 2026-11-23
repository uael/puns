//! Checks the pairs of `support/views_full.rs` whose source is the third of every three.
//!
//! This file needs the `full` feature, which `cargo test` does not turn on. It runs with
//! `cargo test --profile model -p puns-model --features full`, every night and on demand
//! (`.github/workflows/model.yml`).

#[macro_use]
#[path = "support/views_full.rs"]
mod views_full;

#[test]
fn views_full_3() {
  pairs!(_ _ x);
}
