//! Tests that a rule is proven by the same rules in another struct, and not by another rule or
//! extent.
//!
//! The pairs are checked as whole values and as parts. The sources are a struct, an enum with a
//! `repr`, an enum without one (whose layout `puns` discovers), an array, a union, and a struct
//! with a byte that may be uninitialized. At the place of the target, each holds a struct with the
//! same rules as the target, or a struct with another rule or another extent.
//!
//! This file is a sample, for every push. The full check (`size1.rs`, `size2.rs`, `parts2.rs`) has
//! every pair of these types.
//!
//! This file is not built under Miri. It checks verdicts, and `miri_sample.rs` reaches their
//! unsafe paths.
#![cfg(not(miri))]
// Not for Miri: verdicts, which `miri_sample` reaches the unsafe paths of.

use puns_model::types::*;

/// Checks the pairs of these types, as whole values and as parts, against the oracles.
#[test]
fn rules() {
  puns_model::pairs!(check_pair, usize::MAX;
    (ThirdSoft, Third), (Third, ThirdSoft), (Third, Fifth), (Fifth, Third), (u8, Third),
    (NotOneOneNz, NotOneOne), (NotOneOne, NotOneOneNz), (LastNotFf2, NotOneOne),
    (ThirdSoftIn, ThirdIn), (ThirdIn, ThirdSoftIn), (RustThirdSoft, RustThird),
    (RustThird, RustThirdSoft), ([ThirdSoft; 2], [Third; 2]), ([Third; 2], [ThirdSoft; 2]),
    (ThirdSoftUninit, ThirdUninit), (ThirdUninit, ThirdSoftUninit), (ThirdSoft, ThirdU),
    (ThirdU, Third),
  );
  puns_model::pairs!(check_part, usize::MAX;
    (LastNotFf2, LastNotFf1), (LastNotFf2, Third), ([ThirdSoft; 2], Third),
    ([Third; 2], ThirdSoft), ([ThirdSoft; 2], [Third; 2]), (ThirdSoftUninit, Third),
    (ThirdUninit, ThirdSoft), (ThirdSoftIn, ThirdIn), (NotOneOneNz, NotOneOne),
    (NotOneOne, NotOneOneNz), (Third, Fifth), ([Third; 2], Fifth),
    (Third, u8), ([Third; 2], u8), ([ThirdSoft; 2], [u8; 2]),
  );
  puns_model::finish("rules proven by the same rules");
}
