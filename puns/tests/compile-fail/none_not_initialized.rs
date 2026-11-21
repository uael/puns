//! `Layout::nullable` and `Layout::pointer` read the bytes of `None::<P>`. Here two of them are
//! not initialized: the field `low` and the padding byte. Reading them at run time would be
//! undefined behavior, so they are read in const evaluation, where it fails the build.
use core::num::NonZero;

use puns::layout::{Endian, Layout, Pointer};

/// An offset of four bytes whose `Option` has its size: `None` is a zero in `high` alone.
#[allow(dead_code)]
#[derive(Clone, Copy)]
struct Offset {
  low: u16,
  high: NonZero<u8>,
}

fn main() {
  let pointer = Pointer::one::<u32>("test", size_of::<Offset>(), Endian::NATIVE);
  let _ = Layout::nullable::<Offset, u32>(pointer);
  let _ = Layout::pointer::<Offset, u32>(pointer);
}
