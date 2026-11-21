//! Runs the example `examples/media.rs` as a test.
//!
//! This file is not built under Miri. The example has no unsafe code of its own. The unsafe code
//! that it reaches in the library (walks, regions, checksums, getters, splits) is also reached by
//! the tests of the library and by the example `ethernet`, which Miri runs.

#![cfg(not(miri))]

#[path = "../examples/media.rs"]
mod media;

#[test]
fn media() {
  media::main();
}
