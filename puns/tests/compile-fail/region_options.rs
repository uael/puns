//! The regions are those of the tail, so `regions` is an option of the last field, and only the
//! last region, which takes what remains, has no size.

#[derive(puns::Pun)]
#[repr(C)]
struct RegionsOfAFieldBefore {
  #[pun(regions(name = 1))]
  count: u8,
  items: [u8],
}

#[derive(puns::Pun)]
#[repr(C)]
struct UnsizedRegionFirst {
  count: u8,
  #[pun(regions(name, value = count))]
  items: [u8],
}

fn main() {}
