//! Checks the pairs of `support/size1.rs` whose source is the second of every three.
//!
//! This file needs the `full` feature, which `cargo test` does not turn on. It runs with
//! `cargo test --profile model -p puns-model --features full`, every night and on demand
//! (`.github/workflows/model.yml`).

#[macro_use]
#[path = "support/size1.rs"]
mod size1;

#[test]
#[cfg_attr(miri, ignore = "too slow under Miri, which runs `miri_sample`")]
fn size1_2() {
  pairs!(_ x _);
}
