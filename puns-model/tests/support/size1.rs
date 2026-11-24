//! Checks every ordered pair of the types of 1 byte of the model, as whole values and as parts.
//!
//! The check is cut along its sources into the files `size1_1.rs` to `size1_3.rs`, since one
//! `rustc` that builds all of it needs more memory than a runner of CI has. Each file keeps the
//! sources of its marks (`puns_model::sources!`), against every target, and `tests/sources.rs`
//! checks that the files keep each source once between them.
//!
//! These files need the `full` feature, which `cargo test` does not turn on. They run with
//! `cargo test --profile model -p puns-model --features full`, every night and on demand
//! (`.github/workflows/model.yml`).

/// Checks the pairs of this file whose source the marks keep.
macro_rules! pairs {
  ($($mark:tt)+) => {{
    use core::num::NonZero;

    use puns::num::*;
    use puns_model::types::*;

    puns_model::all_pairs!(usize::MAX, sources [$($mark)+];
      u8, i8, bool, NonZero<u8>, Option<NonZero<u8>>, u8c<0>, u8c<7>, u8c<255>, u8in<0, 0x7f>,
      u8in<1, 1>, u8in<0x80, 0xff>, u8in<2, 5>, E01, E37, Eff, Eneg, Erust, [u8; 1], [bool; 1],
      (bool,), Flag, Byte, Ascii, SoftBit, SoftTrue, Third, core::mem::ManuallyDrop<bool>, Zero,
      core::mem::MaybeUninit<u8>, MaybeFlag, NzOpt, ChoiceU, Option<bool>,
      Option<Option<bool>>, ThirdSoft, Fifth, LastNotFf1, ThirdU,
    );
    puns_model::finish(concat!("1 bytes, sources ", stringify!($($mark)+)));
  }};
}
