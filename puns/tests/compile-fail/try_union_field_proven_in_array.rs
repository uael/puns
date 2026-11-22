//! A union that its second field proves in the second element of an array, since that field takes
//! a byte of padding. In the first element no field is proven, so the union is checked there, and
//! the elements share their check. It tries the first field first, which would read the byte of
//! padding of the second element.
use puns::Pun;

/// A `bool`, then a byte of padding.
#[derive(Pun, Clone, Copy)]
#[repr(C, align(2))]
struct Padded {
  flag: bool,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Flags {
  both: [bool; 2],
  first: Padded,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Source {
  bytes: [u8; 2],
  padded: Padded,
}

fn main() {
  let source = Source { bytes: [1, 1], padded: Padded { flag: true } };
  let _ = puns::try_pun_ref::<Source, [Flags; 2]>(&source);
}
