//! A packed union is refused as any union is: as a target, a value that may not be one of its
//! fields; as a source, a byte that may be padding.
#[derive(puns::Pun, Clone, Copy)]
#[repr(C, packed)]
union Packed {
  nz: core::num::NonZero<u32>,
  bytes: [bool; 4],
}

/// Size 6, alignment 2: its last byte is padding.
#[derive(puns::Pun, Clone, Copy)]
#[repr(C, packed(2))]
union Odd {
  bytes: [u8; 5],
  word: u32,
}

fn main() {
  // Neither field: 0 is not a `NonZero`, and 2 is not a `bool`.
  let _: Packed = puns::pun([2u8, 0, 0, 0]);
  let _: [u8; 6] = puns::pun(Odd { word: 1 });
}
