//! Checks the pairs of `support/size4.rs` whose source is the second of every two.
//!
//! This file needs the `full` feature, which `cargo test` does not turn on. It runs with
//! `cargo test --profile model -p puns-model --features full`, every night and on demand
//! (`.github/workflows/model.yml`).

#[macro_use]
#[path = "support/size4.rs"]
mod size4;

#[test]
fn size4_2() {
  pairs!(_ x);
}
