//! Checks every ordered pair of the types of 2 bytes of the model, as whole values and as parts.
//!
//! This file needs the `full` feature, which `cargo test` does not turn on. It runs with
//! `cargo test --profile model -p puns-model --features full`, every night and on demand
//! (`.github/workflows/model.yml`).

use core::num::NonZero;

use puns::num::*;
use puns_model::types::*;

#[test]
fn size2() {
  puns_model::all_pairs!(usize::MAX;
    u16, i16, NonZero<u16>, Option<NonZero<u16>>, u16le, u16be, u16lec<0x1234>, u16bec<0x1234>,
    u16lec<1>, u16bec<0x100>, u16bec<0x3412>, [u8; 2], [bool; 2], (u8, bool), (bool, u8),
    [u8in<0, 0x7f>; 2], E16, ByteNz, NzByte, Aligned, AlignedFlag, PackedU16, RustPair, NotOneOne,
    Opt, OptFlag, Either, EitherC, Skewed, OptSoftBit, [E01; 2], (u8c<1>, u8), ByteZero, Never, Pair, Mixed, Single, Flags2, FlagsU, NestPair,
    core::mem::MaybeUninit<u16>, LastNotFf2, NotOneOneNz, ThirdIn, ThirdSoftIn, RustThird,
    RustThirdSoft, ThirdUninit, ThirdSoftUninit, [Third; 2], [ThirdSoft; 2], HoldsOpaque,
    MaybeOpaque, HoldsOptOpaque, RuleOrUnit, HoldsRule, TagUnit, SoftUnit, SoftOrFlags, HalfNever, VoidBools, VoidFirstU, FirstU, MaybeHollow, NotHollow, FlagsArr, Option<[NonZero<u8>; 2]>, [Void1; 2], [ByteUnit; 2], UnitPair, OptOrUnits,
  );
  puns_model::finish("2 bytes");
}
