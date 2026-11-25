//! Checks, natively, the parts of a sample of sources of 3 and 4 bytes, with targets of every
//! smaller size, against the oracles: a part of the sample of the parts that is not for Miri, which
//! runs the selection of `parts.rs`. The sample is in two files, `parts_by_size_1.rs` and
//! `parts_by_size_2.rs`, so that each builds in a job of CI.

#![cfg(not(miri))]

use puns_model::types::*;

/// Checks the parts of a sample of sources of 3 and 4 bytes.
#[test]
fn parts_by_size_2() {
  puns_model::part_pairs!(4;
    [[u8; 3], Wide, Triple, PackedBoolNz, PackedU16Bool];
    [u8, bool, [u8; 2], u16, (u8, bool), Opt, NotOneOne]
  );
  puns_model::part_pairs!(2;
    [u32, PadMid, PadEnd, Shape, Drawing, ReadsPad, Tag16, CU8];
    [u8, bool, u16, [u8; 2], [u8; 3], Opt, Triple]
  );
  puns_model::finish("parts, sample, 3 and 4 bytes");
}
