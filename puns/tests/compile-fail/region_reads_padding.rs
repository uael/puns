//! A region is bytes of its struct, which are not read from a struct that has padding.
#[derive(puns::Pun)]
#[repr(C)]
struct Record {
  count: u8,
  id: u16,
  #[pun(regions(names = count, rest))]
  data: [u8],
}

fn main() {
  if let Ok(record) = puns::from_bytes::<Record>(&[0; 8]) {
    let _ = record.names();
  }
}
