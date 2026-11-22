//! Checks every ordered pair of the types of 1 byte of the model, as whole values and as parts.
//!
//! This file needs the `full` feature, which `cargo test` does not turn on. It runs with
//! `cargo test --profile model -p puns-model --features full`, every night and on demand
//! (`.github/workflows/model.yml`).

use core::num::NonZero;

use puns::num::*;
use puns_model::types::*;

#[test]
#[cfg_attr(miri, ignore = "too slow under Miri, which runs `miri_sample`")]
fn size1() {
  puns_model::all_pairs!(usize::MAX;
    u8, i8, bool, NonZero<u8>, Option<NonZero<u8>>, u8c<0>, u8c<7>, u8c<255>, u8in<0, 0x7f>,
    u8in<1, 1>, u8in<0x80, 0xff>, u8in<2, 5>, E01, E37, Eff, Eneg, Erust, [u8; 1], [bool; 1],
    (bool,), Flag, Byte, Ascii, SoftBit, SoftTrue, Third, core::mem::ManuallyDrop<bool>, Zero,
    core::mem::MaybeUninit<u8>, MaybeFlag, NzOpt, ChoiceU, Option<bool>,
    Option<Option<bool>>, ThirdSoft, Fifth, LastNotFf1, ThirdU, Opaque,
    OpaqueNz, Option<OpaqueNz>, Void1, [Void1; 1], HollowByte, ByteUnit,
  );
  puns_model::finish("1 bytes");
}
