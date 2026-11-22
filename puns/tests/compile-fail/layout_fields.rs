//! The fields of a struct, as `Layout::structure` takes them, are sorted, apart, within the struct,
//! and only the last is unsized; `len` is about an array or a slice at the end; a field is
//! restricted only if it is a scalar whose bytes are initialized; an array has a size that a
//! `usize` holds.
use puns::{
  Pun,
  layout::{Field, Layout, Op, Rule, Spec},
};

#[repr(C)]
struct Pair(u8, u8);

const BYTE: Layout = Layout::scalar::<u8>(Spec::any::<u8>());
const WORD: Layout = Layout::scalar::<u16>(Spec::any::<u16>());
const BYTES: &Layout = &<[u8] as Pun>::LAYOUT;

// SAFETY (of each call of `Layout::structure`): `Pair` is `Sized`, and `Layout::structure` requires
// nothing of a `Sized` type.
const _: Layout = unsafe {
  Layout::structure::<Pair>(core::ptr::null(), &[Field::new(0, BYTES), Field::new(1, &BYTE)], &[])
};
const _: Layout = unsafe {
  Layout::structure::<Pair>(core::ptr::null(), &[Field::new(0, &WORD), Field::new(1, BYTES)], &[])
};
const _: Layout = unsafe {
  Layout::structure::<Pair>(core::ptr::null(), &[Field::new(0, &WORD), Field::new(1, &BYTE)], &[])
};
const _: Layout =
  unsafe { Layout::structure::<Pair>(core::ptr::null(), &[Field::new(1, &WORD)], &[]) };
const _: Layout = unsafe {
  Layout::structure::<Pair>(
    core::ptr::null(),
    &[Field::new(0, &BYTE), Field::new(1, &BYTE)],
    &[Rule::Len(&[Op::Lit(1)])],
  )
};
const _: Field = Field::restricted(0, &Layout::array(&BYTE, 2));
const _: Layout = Layout::scalar::<u8>(Spec::uninit::<u8>()).restricted(&[(0, 1)]);
const _: Layout = Layout::array(&WORD, usize::MAX);

fn main() {}
