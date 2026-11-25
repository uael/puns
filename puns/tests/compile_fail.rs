//! Tests that each refused use of `puns` fails to compile with the expected message.
//!
//! Each case in `tests/compile-fail/` must fail to compile with the error of its `.stderr` file.
//! Each case in `tests/compile-pass/` must compile. Without the `unstable` feature, the cases in
//! `tests/compile-fail-kept/` must fail and the cases in `tests/compile-pass-kept/` must compile.
//! With the `unstable` feature, the cases in `tests/compile-fail-unstable/` must fail.
//!
//! The cases are in two tests, `from_a_to_s` (`tests/compile-fail/` from `a` to `s`) and
//! `the_others` (the others), which CI runs as two jobs: trybuild builds each program, and the
//! halves take about the same time. trybuild runs one at a time, under a lock.
//!
//! This file is not built under Miri. trybuild runs the compiler, which Miri cannot run.

#![cfg(not(miri))]

/// The cases of `tests/compile-fail/` whose names start with `a` to `s`.
#[test]
fn from_a_to_s() {
  cases("tests/compile-fail/[a-s]*.rs", false);
}

/// The other cases of `tests/compile-fail/`, and those of the folders next to it.
#[test]
fn the_others() {
  cases("tests/compile-fail/[!a-s]*.rs", true);
}

/// Checks that the cases that `pattern` names fail to compile, and, with `kept`, the cases of the
/// folders that the `unstable` feature chooses.
fn cases(pattern: &str, kept: bool) {
  let t = trybuild::TestCases::new();
  t.compile_fail(pattern);
  // A passing case makes trybuild build instead of check, so that the errors of the constants
  // evaluated at monomorphization (every `cannot pun`) are reported too.
  t.pass("tests/compile-pass/*.rs");
  if !kept {
    return;
  }
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
