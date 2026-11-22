//! The values of a scalar, as `Spec::with_ranges` takes them: a scalar of 1 to 16 bytes, at least
//! one value, ranges that are values of the scalar, sorted and apart; and `maybe_uninit` only of a
//! scalar that restricts its values.
use puns::layout::{Endian, Spec};

const _: Spec = Spec::any::<()>().with_ranges(&[(0, 0)]);
const _: Spec = Spec::ranges::<u8>(Endian::NATIVE, &[]);
const _: Spec = Spec::ranges::<u8>(Endian::NATIVE, &[(0, 256)]);
const _: Spec = Spec::ranges::<u8>(Endian::NATIVE, &[(5, 9), (0, 2)]);
const _: Spec = Spec::any::<u8>().maybe_uninit();

fn main() {}
