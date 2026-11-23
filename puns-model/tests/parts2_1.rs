//! Checks the pairs of `support/parts2.rs` whose source is the first of every two.
//!
//! This file needs the `full` feature, which `cargo test` does not turn on. It runs with
//! `cargo test --profile model -p puns-model --features full`, every night and on demand
//! (`.github/workflows/model.yml`).

#[macro_use]
#[path = "support/parts2.rs"]
mod parts2;

#[test]
fn parts2_1() {
  pairs!(x _);
}
