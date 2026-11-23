//! Checks every ordered pair of the types of 4 bytes of the model, as whole values and as parts.
//!
//! The check is cut along its sources into the files `size4_1.rs` and `size4_2.rs`, since one
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
      u32, i32, f32, char, NonZero<u32>, Option<NonZero<u32>>, u32le, u32be, u32lec<0x10_ffff>,
      u32bec<0xd800>, [u8; 4], [bool; 4], [u16be; 2], (u8, u16), PadMid, PadEnd, PadEndFlag, CEnum,
      Shape, Drawing, CU8, Tag16, ReadsPad, [u8in<0, 0x7f>; 4], (char,), [NonZero<u16>; 2],
      Letter, Wid, PaddedU,
    );
    puns_model::finish(concat!("4 bytes, sources ", stringify!($($mark)+)));
  }};
}
