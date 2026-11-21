//! What must not compile, and the message that tells why.
//!
//! Not built under Miri: trybuild runs the compiler, which Miri cannot.

#![cfg(not(miri))]

#[test]
fn compile_fail() {
  let t = trybuild::TestCases::new();
  t.compile_fail("tests/compile-fail/*.rs");
  // A passing case makes trybuild build instead of check, so that the errors of the constants
  // evaluated at monomorphization (every `cannot pun`) are reported too.
  t.pass("tests/compile-pass/*.rs");
  // Cases are built with the features of this test (trybuild reads them): what the `unstable`
  // feature changes has cases of each.
  if cfg!(feature = "unstable") {
    // The layout that `rustc` chose: refused for what it is.
    t.compile_fail("tests/compile-fail-unstable/*.rs");
  } else {
    // What Rust does not keep: refused, as nothing that is not the same in every layout that it
    // allows is relied on; and what it keeps, which compiles.
    t.compile_fail("tests/compile-fail-kept/*.rs");
    t.pass("tests/compile-pass-kept/*.rs");
  }
}
