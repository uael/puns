//! Checks every ordered pair of the types of 2 bytes of the model, as whole values and as parts.
//!
//! The check is cut along its sources into the files `size2_1.rs` to `size2_5.rs`, since one
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
      u16, i16, NonZero<u16>, Option<NonZero<u16>>, u16le, u16be, u16lec<0x1234>, u16bec<0x1234>,
      u16lec<1>, u16bec<0x100>, u16bec<0x3412>, [u8; 2], [bool; 2], (u8, bool), (bool, u8),
      [u8in<0, 0x7f>; 2], E16, ByteNz, NzByte, Aligned, AlignedFlag, PackedU16, RustPair, NotOneOne,
      Opt, OptFlag, Either, EitherC, Skewed, OptSoftBit, [E01; 2], (u8c<1>, u8), ByteZero, Never,
      Pair, Mixed, Single, Flags2, FlagsU, NestPair,
      core::mem::MaybeUninit<u16>, LastNotFf2, NotOneOneNz, ThirdIn, ThirdSoftIn, RustThird,
      RustThirdSoft, ThirdUninit, ThirdSoftUninit, [Third; 2], [ThirdSoft; 2],
    );
    puns_model::finish(concat!("2 bytes, sources ", stringify!($($mark)+)));
  }};
}
