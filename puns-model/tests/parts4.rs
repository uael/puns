//! Checks the parts of every type of 4 bytes of the model as every type of 1, 2 and 3 bytes.
//!
//! This file needs the `full` feature, which `cargo test` does not turn on. It runs with
//! `cargo test --profile model -p puns-model --features full`, every night and on demand
//! (`.github/workflows/model.yml`).

use core::num::NonZero;

use puns::num::*;
use puns_model::types::*;

#[test]
fn parts4() {
  puns_model::part_pairs!(usize::MAX;
    [
      u32, i32, f32, char, NonZero<u32>, Option<NonZero<u32>>, u32le, u32be, u32lec<0x10_ffff>,
      u32bec<0xd800>, [u8; 4], [bool; 4], [u16be; 2], (u8, u16), PadMid, PadEnd, PadEndFlag,
      CEnum, Shape, Drawing, CU8, Tag16, ReadsPad, [u8in<0, 0x7f>; 4], (char,), [NonZero<u16>; 2], UnionBytes, [FirstU; 2], HoldsUnits, UnitsArray, UnitsFields,
    ];
    [
      u8, i8, bool, NonZero<u8>, Option<NonZero<u8>>, u8c<0>, u8c<7>, u8c<255>, u8in<0, 0x7f>,
      u8in<1, 1>, u8in<0x80, 0xff>, u8in<2, 5>, E01, E37, Eff, Eneg, Erust, [u8; 1], [bool; 1],
      (bool,), Flag, Byte, Ascii, SoftBit, SoftTrue, Third, core::mem::ManuallyDrop<bool>, Void1, [Void1; 1], HollowByte, ByteUnit,
    ]
  );
  puns_model::part_pairs!(usize::MAX;
    [
      u32, i32, f32, char, NonZero<u32>, Option<NonZero<u32>>, u32le, u32be, u32lec<0x10_ffff>,
      u32bec<0xd800>, [u8; 4], [bool; 4], [u16be; 2], (u8, u16), PadMid, PadEnd, PadEndFlag,
      CEnum, Shape, Drawing, CU8, Tag16, ReadsPad, [u8in<0, 0x7f>; 4], (char,), [NonZero<u16>; 2], UnionBytes, [FirstU; 2], HoldsUnits, UnitsArray, UnitsFields,
    ];
    [
      u16, i16, NonZero<u16>, Option<NonZero<u16>>, u16le, u16be, u16lec<0x1234>, u16bec<0x1234>,
      u16lec<1>, u16bec<0x100>, u16bec<0x3412>, [u8; 2], [bool; 2], (u8, bool), (bool, u8),
      [u8in<0, 0x7f>; 2], E16, ByteNz, NzByte, Aligned, AlignedFlag, PackedU16, RustPair,
      NotOneOne, Opt, OptFlag, Either, EitherC, Skewed, OptSoftBit, [E01; 2], (u8c<1>, u8), RuleOrUnit, HoldsRule, TagUnit, SoftUnit, SoftOrFlags, HalfNever, VoidBools, VoidFirstU, FirstU, MaybeHollow, NotHollow, FlagsArr, Option<[NonZero<u8>; 2]>, [Void1; 2], [ByteUnit; 2], UnitPair, OptOrUnits,
    ]
  );
  puns_model::part_pairs!(usize::MAX;
    [
      u32, i32, f32, char, NonZero<u32>, Option<NonZero<u32>>, u32le, u32be, u32lec<0x10_ffff>,
      u32bec<0xd800>, [u8; 4], [bool; 4], [u16be; 2], (u8, u16), PadMid, PadEnd, PadEndFlag,
      CEnum, Shape, Drawing, CU8, Tag16, ReadsPad, [u8in<0, 0x7f>; 4], (char,), [NonZero<u16>; 2], UnionBytes, [FirstU; 2], HoldsUnits, UnitsArray, UnitsFields,
    ];
    [
      [u8; 3], [bool; 3], (u8, u8, bool), u24le, u24be, u24lec<0x10>, u24bec<0x10>, PackedU16Bool,
      PackedBoolNz, Triple, Wide, WideBe, Lit, [E01; 3], (u8, [u8in<0x80, 0xff>; 2]), TagFlag, UnionIn, HoldsSoft, TagSoft, HoldsTuple, TupleNever, TagUnits,
    ]
  );
  puns_model::finish("parts of 4 bytes");
}
