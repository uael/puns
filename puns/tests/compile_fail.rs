//! Tests that each refused use of `puns` fails to compile with the expected message.
//!
//! Each case in `tests/compile-fail/` must fail to compile with the error of its `.stderr` file.
//! Each case in `tests/compile-pass/` must compile. Without the `unstable` feature, the cases in
//! `tests/compile-fail-kept/` must fail and the cases in `tests/compile-pass-kept/` must compile.
//! With the `unstable` feature, the cases in `tests/compile-fail-unstable/` must fail.
//!
//! This file is not built under Miri. trybuild runs the compiler, which Miri cannot run.

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
