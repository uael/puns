//! The expression of a rule that `Layout::structure` takes is well formed: at most 256 operations,
//! a stack of at most 16 values, fields within the struct, variables that are set before they are
//! read, at most 8 of them, and walks nested at most 4 deep.
use puns::layout::{Field, Layout, Op, Rule, Spec};

#[repr(C)]
struct Byte(u8);

const BYTE: Layout = Layout::scalar::<u8>(Spec::any::<u8>());
const FIELDS: &[Field] = &[Field::new(0, &BYTE)];

/// The layout of `Byte` with the rule `assert(OPS)`.
macro_rules! asserting {
  ($ops:expr) => {
    // SAFETY: `Byte` is `Sized`, and `Layout::structure` requires nothing of a `Sized` type.
    unsafe { Layout::structure::<Byte>(core::ptr::null(), FIELDS, &[Rule::Assert($ops)]) }
  };
}

const LOAD: Op = Op::Load { offset: 1, size: 1, big: false, signed: false };
// A walk nested `n` deep, as `WALK_n`.
const WALK_1: &[Op] = &[Op::Lit(0), Op::Walk { var: 0, cond: &[Op::Lit(0)], next: &[Op::Lit(0)] }];
const WALK_2: &[Op] = &[Op::Lit(0), Op::Walk { var: 0, cond: WALK_1, next: WALK_1 }];
const WALK_3: &[Op] = &[Op::Lit(0), Op::Walk { var: 0, cond: WALK_2, next: WALK_2 }];
const WALK_4: &[Op] = &[Op::Lit(0), Op::Walk { var: 0, cond: WALK_3, next: WALK_3 }];
const WALK_5: &[Op] = &[Op::Lit(0), Op::Walk { var: 0, cond: WALK_4, next: WALK_4 }];

const _: Layout = asserting!(&[Op::Lit(1); 257]);
const _: Layout = asserting!(&[Op::Lit(1); 17]);
const _: Layout = asserting!(&[Op::Lit(1), Op::Add]);
const _: Layout = asserting!(&[LOAD]);
const _: Layout = asserting!(&[Op::Lit(1), Op::Set(8), Op::Lit(1)]);
const _: Layout = asserting!(&[Op::Get(0)]);
const _: Layout = asserting!(WALK_5);

fn main() {}
