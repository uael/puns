//! A CRC has 1 to 64 bits.

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(crc::<0, 0, 0, false, 0>(0, 2) == 0))]
struct NoBit {
  a: u16,
}

fn main() {}
