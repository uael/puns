//! `validate_in` walks a value through the lists of its type (`Fields`, `Variants`, `Elem`),
//! which must be what its layout holds: here an implementation of `Pun` whose layout has a field,
//! a `bool`, that its list of fields does not have. Were the list believed, nothing would look at
//! the `bool`, and `validate_in` would accept a 2 there.
use puns::{
  Invalid, Pun, Visit,
  layout::{Field, Layout},
};

#[repr(C)]
struct Flag {
  on: bool,
}

// SAFETY: Not met, on purpose: `Fields` is not the fields of the layout, which must be refused.
unsafe impl Pun for Flag {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Rules = ();
  type Pointee = ();
  const PROBE: *const Self = core::ptr::null();
  // SAFETY: `Flag` is `Sized`, and `Layout::structure` requires nothing of a `Sized` type.
  const LAYOUT: Layout = unsafe {
    Layout::structure::<Self>(
      Self::PROBE,
      const { &Layout::sort_fields([Field::new(0, &<bool as Pun>::LAYOUT)]) },
      &[],
    )
  };
}

struct NoPointers;

impl Visit for NoPointers {
  type Error = Invalid;
  fn pointer<P: ?Sized + Pun>(&mut self, at: usize) -> Result<(), Invalid> {
    Err(Invalid::at(at))
  }
}

fn main() {
  let _ = puns::validate_in::<Flag, _>(&[2], 0, &mut NoPointers);
}
