//! Checks the sample of the model that runs on every push, natively and under Miri.
//!
//! The sample is a selection of pairs that reaches what Miri is there to see. Natively, this file
//! also checks every pair of a few types of each size.
//!
//! Under Miri, the point is undefined behavior, not the verdicts. The full native run checks every
//! verdict, behind the `full` feature (`size*.rs`, `parts*.rs`). Miri runs the unsafe paths of the
//! library that the model reaches, once or a few times for each class of layout. Here these paths
//! are those of `try_pun` and `try_pun_ref`, with values that the oracle makes (`make`). Each pair
//! is there for one path, over the few values that `alphabet` returns under Miri:
//!
//! | Path                                                   | Pair                               |
//! |--------------------------------------------------------|------------------------------------|
//! | nothing to check (a proven pun, through `try_*`)       | `bool -> u8`, `[bool; 2] -> u16`   |
//! | a scalar checked, accepted or rejected for validity    | `u8 -> bool`, `u16 -> (u8, bool)`  |
//! | a rule checked at run time                             | `u16 -> NotOneOne`                 |
//! | a requirement (`valid(..)`) checked                    | `u8 -> SoftBit`                    |
//! | the address checked (aligned here, see `misaligned`)   | `[u8; 2] -> u16`                   |
//! | an enum with fields checked, the tag then the variant  | `u32 -> Shape`                     |
//! | a target with padding, read from initialized bytes     | `[u8; 4] -> PadMid`                |
//! | the tags of a variant (several for a niche, `RANGES`)  | `u8 -> MaybeFlag`                  |
//! | a direct tag widened to two bytes, `repr(Rust)`        | `[u8; 4] -> Wid`                   |
//! | a union, field by field                                | `u8 -> ChoiceU`                    |
//! | a variant that holds a never scalar, rejected unread   | `ByteZero -> MaybeOpaque`          |
//! | a variant without values, rejected unread              | `ByteZero -> HalfNever`            |
//! | a field of a union without values, skipped unread      | `ByteZero -> VoidFirstU`           |
//!
//! A pair is not there for what the analysis proves of it. An enum proven variant by variant
//! (`Shape -> Drawing`) runs the same code as `bool -> u8`, which is no code. An enum checked from
//! an enum (`Drawing -> Shape`) runs the same code as one checked from `u32`. These are verdicts,
//! which the native run checks.
//!
//! `misaligned` checks that an address is rejected for its alignment (`try_pun_ref`, `from_tail`).
//! The parts (`head*`, `tail*`) are in `parts.rs`, slices and unsized tails in `dst.rs`, and view
//! enums in `views.rs`. The model does not check `pun`, `pun_mut`, `try_pun_mut`,
//! `from_head_mut`, `from_tail_mut` and the regions, which the tests of `puns` run under Miri. It
//! calls `pun_mut` only to fill a buffer (`src/views.rs`, `tests/dst.rs`).

#[cfg(not(miri))]
use core::num::NonZero;

use puns::{Error, hook};
use puns_model::types::*;

/// Checks the first pairs of the table, then natively every pair of a few types of each size.
#[test]
fn sample() {
  // The selection of the table above: natively as well, so that it is checked on every push
  // with the full alphabets.
  puns_model::pairs!(check_pair, 3;
    (bool, u8), ([bool; 2], u16), (u8, bool), (u16, (u8, bool)), (u16, NotOneOne), (u8, SoftBit),
    ([u8; 2], u16), (u32, Shape),
  );
  // Natively, every pair of a few types of each size as well: about a hundred pairs, which is
  // what builds in seconds (each pair is an instance of the analysis). Not compiled under Miri.
  #[cfg(not(miri))]
  {
    puns_model::all_pairs!(8; u8, bool, NonZero<u8>, E37, SoftBit);
    puns_model::all_pairs!(6; u16, [bool; 2], (u8, bool), Aligned, Opt, NotOneOne);
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
  }
  puns_model::finish("sample");
}

/// Checks the other pairs of the table.
///
/// This is a test of its own because, under Miri, each of the two tests is a job of CI, told
/// apart by name. A job takes a minute at most.
#[test]
fn sample_layouts() {
  puns_model::pairs!(check_pair, 3;
    ([u8; 4], PadMid), (u8, MaybeFlag), ([u8; 4], Wid), (u8, ChoiceU), (ByteZero, MaybeOpaque),
    (ByteZero, HalfNever), (ByteZero, VoidFirstU),
  );
  puns_model::finish("sample_layouts");
}

/// Checks that an address that is not aligned for the target is rejected, wherever the allocator
/// put the bytes.
#[test]
fn misaligned() {
  let words = [0x0102_0304u32, 0x0506_0708];
  let bytes: &[u8; 8] = puns::pun_ref(&words);
  let got = hook::try_pun_ref::<[u8], u32>(&bytes[1..5]).expect("compiles");
  assert_eq!(got, Err(Error::Alignment));
  let got = hook::from_tail::<u32>(&bytes[..7]).expect("compiles").map(|(_, w)| *w);
  assert_eq!(got, Err(Error::Alignment));
  let got = hook::from_tail::<u32>(&bytes[..]).expect("compiles").map(|(rest, w)| (rest.len(), *w));
  assert_eq!(got, Ok((4, 0x0506_0708)));
}
