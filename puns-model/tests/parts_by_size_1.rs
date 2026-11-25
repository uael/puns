//! Checks, natively, the parts of a sample of sources of 2 bytes and of more than 4, with targets
//! of every smaller size, against the oracles: a part of the sample of the parts that is not for
//! Miri, which runs the selection of `parts.rs`. The sample is in two files, `parts_by_size_1.rs`
//! and `parts_by_size_2.rs`, so that each builds in a job of CI.

#![cfg(not(miri))]

use core::num::NonZero;

use puns::num::*;
use puns_model::types::*;

/// Checks the parts of a sample of sources of 2 bytes and of more than 4.
#[test]
fn parts_by_size_1() {
  puns_model::part_pairs!(4;
    [u16, [bool; 2], (u8, bool), NotOneOne, Opt, OptFlag, Skewed, Aligned, PackedU16, OptSoftBit];
    [u8, bool, NonZero<u8>, u8in<0, 0x7f>, Third, SoftBit]
  );
  puns_model::part_pairs!(2; [CTagged, [Shape; 2], (u16, u8, bool, u32)]; [u32, Shape, bool]);
  puns_model::finish("parts, sample, 2 bytes and more than 4");
}
