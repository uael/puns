//! The layout of an enum, as `Enum::new` and `Layout::enumeration` take it, is the one of its
//! representation: a scalar tag in the enum, values of the tag, one for each variant, and variants
//! of the size of the enum whose fields are sized, apart, within it, and not over the tag.
use puns::{
  Pun,
  layout::{Endian, Enum, Field, Layout, Spec, Variant},
};

#[derive(Clone, Copy)]
#[repr(u8)]
enum Shape {
  Dot,
  Line(u8),
}

const TAG: Layout = Layout::scalar::<u8>(Spec::ranges::<u8>(Endian::NATIVE, &[(0, 1)]));
const BYTE: Layout = Layout::scalar::<u8>(Spec::any::<u8>());
const WORD: Layout = Layout::scalar::<u16>(Spec::any::<u16>());
const LONG: Layout = Layout::scalar::<u32>(Spec::any::<u32>());
const DOT: Layout = Layout::variant::<Shape>(&[]);
const LINE: Layout = Layout::variant::<Shape>(&[Field::new(1, &BYTE)]);
const SHADOWS: &[(usize, usize)] = &[(1, 1), (2, 1)];

/// The layout of `Shape`, with the tag `$tag` and the variants `$variants`.
macro_rules! shape {
  ($tag:expr, $variants:expr) => {
    Layout::enumeration::<Shape>(&Enum::new($tag, false, SHADOWS, 0, $variants))
  };
}

/// A variant of `Shape` whose tag is 1 and whose fields are `$fields`.
macro_rules! line {
  ($fields:expr) => {
    Variant::new(1, &Layout::variant::<Shape>($fields))
  };
}

const _: Enum = Enum::new(&TAG, true, SHADOWS, 0, &[]);
const _: Enum = Enum::new(&TAG, false, SHADOWS, 1, &[]);
const _: Enum = Enum::new(&TAG, false, &[(usize::MAX, 2)], 0, &[]);
const _: Layout = Layout::enumeration::<Shape>(&Enum::new(&TAG, false, &[(1, 1)], 0, &[]));
const _: Layout = shape!(&Layout::array(&BYTE, 1), &[]);
const _: Layout = shape!(&LONG, &[]);
const _: Layout = shape!(&TAG, &[Variant::new(0, &DOT), Variant::new(2, &LINE)]);
const _: Layout = shape!(&TAG, &[Variant::new(0, &DOT), Variant::new(0, &LINE)]);
const _: Layout = shape!(&TAG, &[Variant::new(0, &BYTE)]);
const _: Layout = shape!(&TAG, &[Variant::new(0, &Layout::variant::<u32>(&[]))]);
const _: Layout = shape!(&TAG, &[line!(&[Field::new(1, &<[u8] as Pun>::LAYOUT)])]);
const _: Layout = shape!(&TAG, &[line!(&[Field::new(1, &WORD)])]);
const _: Layout = shape!(&TAG, &[line!(&[Field::new(0, &BYTE)])]);

fn main() {}
