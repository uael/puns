//! An `Option` of an opaque scalar that leaves out more than one value of its size: its `None` is
//! one of them, which `rustc` chooses and no documentation promises.
use puns::{
  Deferred, Scalar,
  layout::{Endian, Spec},
};

#[derive(Clone, Copy)]
#[repr(transparent)]
struct Flag(bool);

// SAFETY: `Flag` is `repr(transparent)` over a `bool`: its values are 0 and 1, each a valid
// `Flag`, and it holds no `UnsafeCell`.
unsafe impl Scalar for Flag {
  const SPEC: Spec = Spec::ranges::<Flag>(Endian::NATIVE, &[(0, 1)]).opaque();
}

fn main() {
  let _ = puns::from_bytes::<Deferred<Option<Flag>>>(&[2]);
}
