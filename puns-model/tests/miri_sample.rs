//! Checks the sample of the model that runs on every push, natively and under Miri.
//!
//! The sample is a selection of pairs that reaches what Miri is there to see. Natively, every pair
//! of a few types of each size is checked as well, in `sample_by_size_1.rs` and
//! `sample_by_size_2.rs`, files of their own so that each builds in a job of CI.
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
//! | the tags of a variant (several for a niche, `RANGES`)  | `u8 -> MaybeFlag`                  |
//! | a union, field by field                                | `u8 -> ChoiceU`                    |
//! | a variant without values, rejected unread              | `ByteZero -> HalfNever`            |
//! | a field of a union without values, skipped unread      | `ByteZero -> VoidFirstU`           |
//! | a target with padding, read from initialized bytes     | `[u8; 4] -> PadMid`                |
//! | a direct tag widened to two bytes, `repr(Rust)`        | `[u8; 4] -> Wid`                   |
//!
//! A pair is not there for what the analysis proves of it. An enum proven variant by variant
//! (`Shape -> Drawing`) runs the same code as `bool -> u8`, which is no code. An enum checked from
//! an enum (`Drawing -> Shape`) runs the same code as one checked from `u32`. These are verdicts,
//! which the native run checks.
//!
//! `misaligned` checks that an address is rejected for its alignment (`try_pun_ref`, `from_tail`).
//! The parts (`head*`, `tail*`) are in `parts.rs`, slices and unsized tails in `dst.rs`, and view
//! enums in `views.rs`. The model does not run `pun`, `pun_mut`, `try_pun_mut`, `from_head_mut`,
//! `from_tail_mut` and the regions, which the tests of `puns` run under Miri.

use puns::{Error, hook};
use puns_model::types::*;

/// Checks the pairs of the table but the last two.
#[test]
fn sample() {
  // The selection of the table above: natively as well, so that it is checked on every push
  // with the full alphabets.
  puns_model::pairs!(check_pair, 3;
    (bool, u8), ([bool; 2], u16), (u8, bool), (u16, (u8, bool)), (u16, NotOneOne), (u8, SoftBit),
    ([u8; 2], u16), (u32, Shape), (u8, MaybeFlag), (u8, ChoiceU), (ByteZero, HalfNever),
    (ByteZero, VoidFirstU),
  );
  puns_model::finish("sample");
}

/// Checks the pair of the table with padding.
///
/// This and the next are tests of their own because, under Miri, each of the three tests is a job
/// of CI, told apart by name: a pair from four bytes takes about half the time of `sample` there,
/// and a job takes a minute at most.
#[test]
fn sample_padding() {
  puns_model::pairs!(check_pair, 3; ([u8; 4], PadMid));
  puns_model::finish("sample_padding");
}

/// Checks the pair of the table with a widened tag.
#[test]
fn sample_widened() {
  puns_model::pairs!(check_pair, 3; ([u8; 4], Wid));
  puns_model::finish("sample_widened");
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
