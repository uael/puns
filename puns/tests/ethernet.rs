//! Runs the example `examples/ethernet.rs` as a test.
//!
//! This is the only example that Miri runs. The other examples reach the same unsafe code of the
//! library.

#[path = "../examples/ethernet.rs"]
mod ethernet;

#[test]
fn ethernet() {
  ethernet::main();
}
