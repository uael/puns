//! Checks, natively, every pair of a few types of 3 and 4 bytes, and of each kind of type that is new against the oracles: a part of the sample
//! of the model that is not for Miri, which runs the selection of `miri_sample.rs`. The sample is
//! in two files, `sample_by_size_1.rs` and `sample_by_size_2.rs`, so that each builds in a job of
//! CI: each pair is an instance of the analysis.

#![cfg(not(miri))]

use puns_model::types::*;

/// Checks every pair of a few types of 3 and 4 bytes, and of each kind of type that is new.
#[test]
fn sample_by_size_2() {
  puns_model::all_pairs!(4; [u8; 3], Wide, Triple);
  puns_model::all_pairs!(4; u32, char, PadMid, Shape, Drawing);
  // What is new of a kind, a pair or two each, both ways where both mean something (every pair of
  // these types is in the full run): bytes that may be uninitialized, `MaybeUninit`, a variant
  // without values, enums of the `Rust` representation (niches, a niche in a niche, two niches in
  // one byte, a direct tag, a widened one, from what is not an enum and from an enum), unions.
  puns_model::pairs!(check_pair, 8;
    (Zero, u8), (u8, Zero), (Zero, bool), (u8, core::mem::MaybeUninit<u8>),
    (core::mem::MaybeUninit<u8>, u8), (ByteZero, u16), ([u8; 2], Never), (Never, Opt),
    (u8, MaybeFlag), (bool, MaybeFlag), (MaybeFlag, u8), (u8, NzOpt), (u8, Option<Option<bool>>),
    ([u8; 2], Mixed), (Mixed, Flags2), (Opt, NestPair), ((u8, bool), Pair), (u32, Letter),
    (Letter, u32), ([u8; 4], Wid), (Wid, Wid), (u8, ChoiceU), (ChoiceU, u8), ([u8; 2], FlagsU),
  );
  puns_model::finish("sample, 3 and 4 bytes, kinds");
}
