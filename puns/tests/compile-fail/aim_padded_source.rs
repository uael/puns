//! The check of an eager aim reads its target in the bytes of the source, which must all be
//! initialized.
#[allow(dead_code)]
#[path = "../support/aims.rs"]
mod aims;

#[derive(puns::Pun)]
#[repr(C)]
struct Padded {
  byte: u8,
  word: u32,
}

fn main() {
  let padded = Padded { byte: 0, word: 4 };
  let _ = puns::try_pun_ref::<Padded, [aims::Near<u32>; 2]>(&padded);
}
