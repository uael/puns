//! Runs the example `examples/executables.rs` as a test.
//!
//! This file is not built under Miri. The example has no unsafe code of its own. The unsafe code
//! that it reaches in the library (walks, regions, checksums, getters, splits) is also reached by
//! the tests of the library and by the example `ethernet`, which Miri runs.

#![cfg(not(miri))]

// As a module of a test, its items are not exported: the names of the fields of ELF headers are
// the ones of the specification.
#[expect(clippy::struct_field_names, reason = "the names of the ELF specification")]
#[path = "../examples/executables.rs"]
mod executables;

#[test]
fn executables() {
  executables::main();
}
