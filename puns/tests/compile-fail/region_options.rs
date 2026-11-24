//! The regions are those of the tail, so `regions` is an option of the last field.

#[derive(puns::Pun)]
#[repr(C)]
struct RegionsOfAFieldBefore {
  #[pun(regions(name = 1))]
  count: u8,
  items: [u8],
}

fn main() {}
