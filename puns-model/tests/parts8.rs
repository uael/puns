//! Checks the parts of every type of 8 bytes of the model as every type of 4 bytes, and as some
//! smaller types.
//!
//! This file needs the `full` feature, which `cargo test` does not turn on. It runs with
//! `cargo test --profile model -p puns-model --features full`, every night and on demand
//! (`.github/workflows/model.yml`).

use core::num::NonZero;

use puns::num::*;
use puns_model::types::*;

#[test]
fn parts8() {
  puns_model::part_pairs!(usize::MAX;
    [
      u64, [u8; 8], [bool; 8], [u32; 2], (u32, u32), CTagged, CTaggedB, [Shape; 2], [Drawing; 2],
      (u16, u8, bool, u32),
    ];
    [
      u32, i32, f32, char, NonZero<u32>, Option<NonZero<u32>>, u32le, u32be, u32lec<0x10_ffff>,
      u32bec<0xd800>, [u8; 4], [bool; 4], [u16be; 2], (u8, u16), PadMid, PadEnd, PadEndFlag,
      CEnum, Shape, Drawing, CU8, Tag16, ReadsPad, [u8in<0, 0x7f>; 4], (char,), [NonZero<u16>; 2],
      u8, bool, u16, NonZero<u16>, Opt, [u8; 3], Triple,
    ]
  );
  puns_model::finish("parts of 8 bytes");
}
