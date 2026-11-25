//! A region without a size before the last is of the size of its type, so its type stores its
//! length, which bytes do not, and the check of the struct refuses it.

#[derive(puns::Pun)]
#[repr(C)]
struct UnsizedRegionFirst {
  count: u8,
  #[pun(regions(name, value = count))]
  items: [u8],
}

fn main() {
  let _ = puns::from_bytes::<UnsizedRegionFirst>(&[1, 2]);
}
