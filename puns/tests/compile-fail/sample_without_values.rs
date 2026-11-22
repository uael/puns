// The sample of a type, which the discovery of the layout of an enum is run on, is a value of it:
// a type without values has none.
use core::{convert::Infallible, mem::MaybeUninit};

const _: MaybeUninit<Infallible> = puns::layout::Layout::sample::<Infallible>();

fn main() {}
