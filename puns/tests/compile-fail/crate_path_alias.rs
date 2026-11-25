//! `#[pun(crate = ..)]` cannot lead to an alias of `At` that names another place than the field
//! (here, one leaf further, which validation would skip: the `bool` would not be checked).
mod fake {
  pub use puns::{Pun, Fields, Path, __site};
  pub type At<P, F> =
    puns::At<puns::At<P, puns::TupleField<(bool, bool), 1>>, F>;
  pub mod layout {
    pub use puns::layout::{Checksum, Crc, Enum, Field, Id, Layout, Offsets, Op, Rule, Spec};
    pub use puns::layout::{Variant, Varint};
  }
}

#[derive(puns::Pun)]
#[pun(crate = fake)]
#[repr(C)]
struct Flag {
  a: u8,
  on: bool,
}

fn main() {
  let _ = puns::try_pun_ref::<[u8; 2], Flag>(&[0, 7]);
}
