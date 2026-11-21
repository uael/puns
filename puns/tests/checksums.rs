//! Tests the checksums that rules compute: a CRC of any width up to 64 bits given by its
//! parameters, Fletcher-16, Adler-32 and a sum of bytes.

use puns::{Pun, from_bytes, layout::Crc};

/// A Modbus RTU frame. It holds an address, a function and data, then the CRC-16/MODBUS of these
/// bytes, least significant byte first.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(
  crc::<16, 0x8005, 0xffff, true, 0>(0, size_of_val(self) - 2) == le(size_of_val(self) - 2, 2)
))]
struct Rtu {
  address: u8,
  function: u8,
  rest: [u8],
}

static MODBUS: Crc = Crc::new(16, 0x8005, 0xffff, true, 0);

#[test]
fn a_crc_by_its_parameters() {
  // Read 3 holding registers from 0x006b, of the device 0x11: its CRC is `0x8776`.
  let mut frame = vec![0x11, 0x03, 0x00, 0x6b, 0x00, 0x03];
  assert_eq!(MODBUS.check(&frame), 0x8776);
  frame.extend((MODBUS.check(&frame) as u16).to_le_bytes());
  assert!(from_bytes::<Rtu>(&frame).is_ok());
  frame[3] ^= 1;
  assert!(from_bytes::<Rtu>(&frame).is_err());
  assert!(from_bytes::<Rtu>(&[0x11, 0x03]).is_err());
  // The catalogue's checks, of every width.
  for ((width, poly, init, reflect, xorout), check) in [
    ((3, 0x3, 0, false, 0x7), 0x4),
    ((5, 0x05, 0x1f, true, 0x1f), 0x19),
    ((8, 0x07, 0, false, 0), 0xf4),
    ((15, 0x4599, 0, false, 0), 0x059e),
    ((16, 0x1021, 0, false, 0), 0x31c3),
    ((16, 0x1021, 0xffff, true, 0xffff), 0x906e),
    ((32, 0x1edc_6f41, 0xffff_ffff, true, 0xffff_ffff), 0xe306_9283),
    ((32, 0x04c1_1db7, 0xffff_ffff, false, 0xffff_ffff), 0xfc89_1918),
    ((64, 0x42f0_e1eb_a9ea_3693, u64::MAX, true, u64::MAX), 0x995d_c9bb_df19_39fa),
  ] {
    let crc = Crc::new(width, poly, init, reflect, xorout);
    assert_eq!(crc.check(b"123456789"), check, "{crc:?}");
  }
  assert_eq!(Crc::CRC32.check(b"123456789"), 0xcbf4_3926);
}

/// A header with two CRCs. Its CRC-32 is over the whole header with the CRC-32 taken as zeros, as
/// in GPT. Its CRC-32C is over the first 8 bytes, with the CRC-32 taken as zeros too.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(crc32(0, 12, 4, 8) == le(4, 4)))]
#[pun(assert(crc::<32, 0x1edc6f41, 0xffffffff, true, 0xffffffff>(0, 8, 4, 8) == le(8, 4)))]
struct Header {
  signature: [u8; 4],
  crc: puns::num::u32le,
  castagnoli: puns::num::u32le,
}

#[test]
fn a_crc_over_itself() {
  let mut header = *b"PUNS\0\0\0\0\0\0\0\0";
  let castagnoli = Crc::new(32, 0x1edc_6f41, 0xffff_ffff, true, 0xffff_ffff);
  let first = castagnoli.check(&header[..8]) as u32;
  header[8..].copy_from_slice(&first.to_le_bytes());
  let crc = puns::crc32(&header);
  header[4..8].copy_from_slice(&crc.to_le_bytes());
  assert!(from_bytes::<Header>(&header).is_ok());
  header[0] = b'Q';
  assert!(from_bytes::<Header>(&header).is_err());
}

/// Five bytes followed by their Fletcher-16, their Adler-32 and their sum modulo 256.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(fletcher16(0, 5) == be(5, 2)))]
#[pun(assert(adler32(0, 5) == be(7, 4)))]
#[pun(assert(sum(0, 5) % 256 == be(11, 1)))]
struct Sums {
  bytes: [u8; 5],
  fletcher: puns::num::u16be,
  adler: puns::num::u32be,
  sum: u8,
}

#[test]
fn sums() {
  // The checks of Fletcher-16 and of Adler-32 for "abcde": 0xc8f0, 0x05c801f0.
  let mut value = b"abcde".to_vec();
  value.extend([
    0xc8,
    0xf0,
    0x05,
    0xc8,
    0x01,
    0xf0,
    (b"abcde".iter().map(|&b| u32::from(b)).sum::<u32>() % 256) as u8,
  ]);
  assert!(from_bytes::<Sums>(&value).is_ok());
  value[6] ^= 1;
  assert!(from_bytes::<Sums>(&value).is_err());
}
