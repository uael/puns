//! A `str` from a struct whose last byte is padding. Bytes of any value are not all UTF-8, so the
//! string is checked at run time, and the check reads every byte, the padding too.
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Padded {
  wide: u16,
  narrow: u8,
}

fn main() {
  let _ = puns::try_pun_ref::<Padded, str>(&Padded { wide: 0x6161, narrow: b'a' });
}
