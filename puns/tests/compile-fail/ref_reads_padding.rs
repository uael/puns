//! Reading padding: the byte after the `u8` of `Padded` is padding.
#[derive(puns::Pun)]
#[repr(C)]
struct Padded {
  byte: u8,
  word: u16,
}

fn main() {
  let _: &[u8; 4] = puns::pun_ref(&Padded { byte: 0, word: 0 });
}
