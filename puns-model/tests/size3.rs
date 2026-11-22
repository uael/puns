//! Checks every ordered pair of the types of 3 bytes of the model, as whole values and as parts.
//!
//! This file needs the `full` feature, which `cargo test` does not turn on. It runs with
//! `cargo test --profile model -p puns-model --features full`, every night and on demand
//! (`.github/workflows/model.yml`).

use puns::num::*;
use puns_model::types::*;

#[test]
#[cfg_attr(miri, ignore = "too slow under Miri, which runs `miri_sample`")]
fn size3() {
  puns_model::all_pairs!(usize::MAX;
    [u8; 3], [bool; 3], (u8, u8, bool), u24le, u24be, u24lec<0x10>, u24bec<0x10>, PackedU16Bool,
    PackedBoolNz, Triple, Wide, WideBe, Lit, [E01; 3], (u8, [u8in<0x80, 0xff>; 2]), TagFlag, UnionIn, HoldsSoft, TagSoft, HoldsTuple, TupleNever, TagUnits,
  );
  puns_model::finish("3 bytes");
}
