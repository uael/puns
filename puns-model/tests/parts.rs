//! Checks the parts of values (`head`, `tail`, `head_mut`, `tail_mut` and the others) against the
//! oracles.
//!
//! Natively, this file checks a sample of each size of source, with targets of every smaller size.
//! Under Miri, it checks a selection that reaches the unsafe paths. The full check, behind the
//! `full` feature, runs every pair of the model natively, in `parts2.rs`, `parts3.rs`, `parts4.rs`
//! and `parts8.rs`.

#[cfg(not(miri))]
use core::num::NonZero;

use puns::num::*;
use puns_model::types::*;

/// Checks the parts of a selection of pairs, then natively of a sample of each size of source.
#[test]
fn parts_sample() {
  // What Miri is there for: each unsafe path of the parts once, at its offset (see
  // `miri_sample.rs`). `head`, `head_ref`, `head_mut` and `tail_mut` of a scalar: `u16 -> u8`;
  // `tail_ref` at an aligned offset: `u32 -> u16`; an enum of which the head is proven:
  // `Opt -> bool`; a `&mut` refused for a rule: `NotOneOne -> u8`.
  puns_model::pairs!(check_part, 2; (u16, u8), (u32, u16), (Opt, bool), (NotOneOne, u8));
  // Natively, a sample of each size of source as well, not compiled under Miri.
  #[cfg(not(miri))]
  {
    puns_model::part_pairs!(4;
      [u16, [bool; 2], (u8, bool), NotOneOne, Opt, OptFlag, Skewed, Aligned, PackedU16, OptSoftBit];
      [u8, bool, NonZero<u8>, u8in<0, 0x7f>, Third, SoftBit]
    );
    puns_model::part_pairs!(4;
      [[u8; 3], Wide, Triple, PackedBoolNz, PackedU16Bool];
      [u8, bool, [u8; 2], u16, (u8, bool), Opt, NotOneOne]
    );
    puns_model::part_pairs!(2;
      [u32, PadMid, PadEnd, Shape, Drawing, ReadsPad, Tag16, CU8];
      [u8, bool, u16, [u8; 2], [u8; 3], Opt, Triple]
    );
    puns_model::part_pairs!(2; [CTagged, [Shape; 2], (u16, u8, bool, u32)]; [u32, Shape, bool]);
  }
  puns_model::finish("parts, sample");
}
