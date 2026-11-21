//! Images and fonts (PNG files, BMP files, and the table directory of TrueType and OpenType
//! fonts), declared with `#[derive(Pun)]` and read in place from bytes.
//!
//! As in `ethernet.rs`, the first half of this file holds only type declarations, with no `unsafe`,
//! no `impl` and no parsing code, and the second half reads files in place, from bytes.
//!
//! The following table lists what is specific to these formats, and what in the declarations
//! expresses it:
//!
//! | Specific to these formats                       | Declared with                             |
//! |-------------------------------------------------|-------------------------------------------|
//! | a signature                                     | `u64bec<0x8950_4e47_0d0a_1a0a>`           |
//! | a PNG chunk has a CRC-32 of its type and data   | `assert(crc32(4, 8 + length) == ..)`      |
//! | a chunk type is letters, whose case is a flag   | `assert(..)`, `get(critical: bool = ..)`  |
//! | the type of a chunk says what its data is       | `tag(ChunkType = u32bec<0x4948_4452>)`    |
//! | the bit depths that each color type allows      | `assert((color_type == 0 && ..) \|\| ..)` |
//! | a PNG file has IHDR first, IEND last, good CRCs | `assert(walk(8, ..) == ..)`               |
//! | a BMP height is signed, negative for top-down   | `i32le`, `valid(1..=0x7fff_ffff, ..)`     |
//! | BMP rows, padded to 4 bytes, fit in the file    | `assert(.. (bits_per_pixel * ..) ..)`     |
//! | the BMP header, palette and pixels              | `regions(dib_rest = .., palette = ..)`    |
//! | font search fields derive from the count        | `assert(search_range == 16 * walk(..))`   |
//! | font records are sorted, tables in the file     | `assert(walk(12, ..) == ..)`              |
//!
//! The following are not covered:
//!
//! - The image data of PNG is compressed with zlib, and the image data of BMP may be compressed.
//!   Decompression makes a copy, not a view of the bytes. The CRCs of PNG chunks are checked. The
//!   Adler-32 at the end of the zlib stream is part of the compressed data, and is not checked.
//! - The checksum of each font table is a sum of 32-bit words over the table that a record points
//!   to, with one field of `head` taken as 0. It needs a walk over the records with a walk over
//!   each table inside it, which is not written here. The tables themselves are not declared
//!   either. They would be more declarations of the same kind.
//!
//! The height of a BMP image is an `i32le`, but a rule computes on unsigned integers. The rule
//! that needs the magnitude of the height reads its bits instead.
//!
//! `main` reads a PNG file and its chunks, a BMP image stored bottom-up and top-down, and two font
//! directories, and asserts their fields, regions and getters. It changes or truncates bytes so
//! that a rule fails, asserts that each read then returns an error, and prints `ok`.
#![forbid(unsafe_code)]

use core::marker::PhantomData;

use puns::{
  Error, Pun, Tag, from_head,
  num::{i32le, u8c, u16be, u16le, u16lec, u32be, u32bec, u32le, u64bec},
  try_pun_ref,
};

// -------------------------------------------------------------------------------------------------
// PNG

/// The type of a PNG chunk, when it is not known.
///
/// A chunk type is four ASCII letters, and the case of each letter is a flag. An upper-case first
/// letter marks a critical chunk, and a lower-case one an ancillary chunk.
#[derive(Pun, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
#[pun(assert({
  let lower = self.0 | 0x2020_2020;
  ((lower >> 24) & 0xff) >= 0x61 && ((lower >> 24) & 0xff) <= 0x7a
    && ((lower >> 16) & 0xff) >= 0x61 && ((lower >> 16) & 0xff) <= 0x7a
    && ((lower >> 8) & 0xff) >= 0x61 && ((lower >> 8) & 0xff) <= 0x7a
    && (lower & 0xff) >= 0x61 && (lower & 0xff) <= 0x7a
    && self.0 & 0x4040_4040 == 0x4040_4040
}))]
#[pun(get(critical: bool = self.0 & 0x2000_0000 == 0))]
pub struct ChunkType(pub u32be);

/// A PNG chunk whose data is a `P`.
///
/// A chunk holds a length, a type, the data, and the CRC-32 of the type and the data. `P` is
/// `[u8]` unless another type is given.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 12)]
#[pun(assert(length <= 0x7fff_ffff))]
#[pun(assert(crc32(4, 8 + length) == be(8 + length, 4)))]
pub struct Chunk<P: ?Sized + Tag<ChunkType> = [u8]> {
  pub length: u32be,
  pub kind: P::Tag,
  pub data_type: PhantomData<P>,
  #[pun(regions(data: P = length, crc))]
  pub rest: [u8],
}

/// The data of the IHDR chunk, which gives the dimensions and the kind of the pixels.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
#[pun(tag(ChunkType = u32bec<0x4948_4452>))]
// The bit depths that each color type allows: gray, palette, RGB, gray and alpha, RGBA.
#[pun(assert(
  (color_type == 0
    && (bit_depth == 1 || bit_depth == 2 || bit_depth == 4 || bit_depth == 8 || bit_depth == 16))
    || (color_type == 3 && bit_depth <= 8)
    || ((color_type == 2 || color_type == 4 || color_type == 6)
      && (bit_depth == 8 || bit_depth == 16))
))]
pub struct Ihdr {
  #[pun(valid(1..=0x7fff_ffff))]
  pub width: u32be,
  #[pun(valid(1..=0x7fff_ffff))]
  pub height: u32be,
  #[pun(valid(1, 2, 4, 8, 16))]
  pub bit_depth: u8,
  #[pun(valid(0, 2, 3, 4, 6))]
  pub color_type: u8,
  pub compression: u8c<0>,
  pub filter: u8c<0>,
  #[pun(valid(0..=1))]
  pub interlace: u8,
}

/// A PNG file.
///
/// A file holds the signature, the IHDR chunk, then chunks up to the IEND chunk. One walk checks
/// the CRC of every chunk.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(be(12, 4) == 0x4948_4452 && be(8, 4) == 13))]
#[pun(assert(be(size_of_val(self) - 8, 4) == 0x4945_4e44 && be(size_of_val(self) - 12, 4) == 0))]
#[pun(assert(walk(
  8,
  |at| at < size_of_val(self) && crc32(at + 4, at + 8 + be(at, 4)) == be(at + 8 + be(at, 4), 4),
  |at| at + 12 + be(at, 4),
) == size_of_val(self)))]
pub struct Png {
  pub signature: u64bec<0x8950_4e47_0d0a_1a0a>,
  #[pun(regions(header: Chunk<Ihdr> = 25, chunks))]
  pub data: [u8],
}

// -------------------------------------------------------------------------------------------------
// BMP

/// A BMP file with a `BITMAPINFOHEADER` (40 bytes) or a later header.
#[derive(Pun)]
#[repr(C)]
#[pun(size = file_size)]
#[pun(assert(pixel_offset >= 14 + dib_size))]
// Rows of pixels, each padded to 4 bytes, fit in the file (when they are not compressed). A rule
// computes on unsigned integers: it reads the bits of the height, two's complement, with `le`.
#[pun(assert(compression != 0 || {
  let bits = le(22, 4);
  let rows = if bits >> 31 != 0 { 0x1_0000_0000 - bits } else { bits };
  pixel_offset + (bits_per_pixel * width + 31) / 32 * 4 * rows <= file_size
}))]
// No more colors in the palette than the depth allows.
#[pun(assert(bits_per_pixel > 8 || colors_used <= 1 << bits_per_pixel))]
pub struct Bmp {
  pub magic: u16lec<0x4d42>,
  pub file_size: u32le,
  pub reserved: u32le,
  pub pixel_offset: u32le,
  #[pun(valid(40, 52, 56, 108, 124))]
  pub dib_size: u32le,
  #[pun(valid(1..=0x7fff_ffff))]
  pub width: u32le,
  /// The height of the image, negative when the rows are stored from the top. It is neither 0
  /// nor -2^31. `valid(..)` gives the valid values by their bits, read as unsigned.
  #[pun(valid(1..=0x7fff_ffff, 0x8000_0001..))]
  pub height: i32le,
  pub planes: u16lec<1>,
  #[pun(valid(1, 4, 8, 16, 24, 32))]
  pub bits_per_pixel: u16le,
  #[pun(valid(0..=6))]
  pub compression: u32le,
  pub image_size: u32le,
  pub x_pixels_per_meter: u32le,
  pub y_pixels_per_meter: u32le,
  pub colors_used: u32le,
  pub colors_important: u32le,
  #[pun(regions(dib_rest = dib_size - 40, palette = pixel_offset - 14 - dib_size, pixels))]
  pub data: [u8],
}

// -------------------------------------------------------------------------------------------------
// Fonts

/// The table directory of a TrueType or OpenType font.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(num_tables >= 1))]
// `search_range` is 16 times the greatest power of 2 not above the number of tables,
// `entry_selector` is its log2, and `range_shift` is what remains. Each of the first two is found
// by a walk.
#[pun(assert(search_range == 16 * walk(1, |p| p * 2 <= num_tables, |p| p * 2)))]
#[pun(assert(entry_selector == walk(0, |e| 2 << e <= num_tables, |e| e + 1)))]
#[pun(assert(range_shift == num_tables * 16 - search_range))]
// The records are sorted by tag.
#[pun(assert(walk(
  12,
  |at| at + 16 < 12 + num_tables * 16 && be(at, 4) < be(at + 16, 4),
  |at| at + 16,
) == 12 + (num_tables - 1) * 16))]
// Every table is in the file, at an offset that is a multiple of 4.
#[pun(assert(walk(
  12,
  |at| at < 12 + num_tables * 16
    && be(at + 8, 4) % 4 == 0
    && be(at + 8, 4) + be(at + 12, 4) <= size_of_val(self),
  |at| at + 16,
) == 12 + num_tables * 16))]
pub struct Font {
  /// The version of the font, `0x00010000` or `true` for TrueType, and `OTTO` for OpenType with
  /// CFF outlines.
  #[pun(valid(0x0001_0000, 0x7472_7565, 0x4f54_544f))]
  pub sfnt_version: u32be,
  pub num_tables: u16be,
  pub search_range: u16be,
  pub entry_selector: u16be,
  pub range_shift: u16be,
  #[pun(regions(records: [TableRecord] = num_tables * 16, tables))]
  pub data: [u8],
}

/// A record of the table directory of a font.
///
/// A record holds the tag of a table, its checksum, its offset and its length. The checksum is
/// not checked (see the documentation of this module).
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
pub struct TableRecord {
  pub tag: u32be,
  pub checksum: u32be,
  pub offset: u32be,
  pub length: u32be,
}

// -------------------------------------------------------------------------------------------------
// Use

/// The bytes of a PNG image of one pixel, with an IHDR chunk, a comment, an IDAT chunk and an
/// IEND chunk.
const PNG: [u8; 92] = [
  0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, b'I', b'H', b'D', b'R',
  0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
  0xde, 0x00, 0x00, 0x00, 0x0a, b't', b'E', b'X', b't', b'C', b'o', b'm', b'm', b'e', b'n', b't',
  0x00, b'h', b'i', 0xa2, 0xa2, 0x58, 0x66, 0x00, 0x00, 0x00, 0x0d, b'I', b'D', b'A', b'T', 0x78,
  0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0, 0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d,
  0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82,
];

/// The bytes of an IHDR chunk of RGB with 4 bits per sample, a depth that RGB does not allow.
/// Its CRC is correct.
const IHDR_4_BITS: [u8; 25] = [
  0x00, 0x00, 0x00, 0x0d, b'I', b'H', b'D', b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
  0x04, 0x02, 0x00, 0x00, 0x00, 0x55, 0x87, 0xbe, 0xdf,
];

/// The bytes of a BMP image of 3 by 2 pixels of 24 bits. Its rows of 9 bytes are padded to 12.
const BMP: [u8; 78] = [
  0x42, 0x4d, 0x4e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x36, 0x00, 0x00, 0x00, 0x28, 0x00,
  0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00, 0x18, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
  0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
];

/// The bytes of the table directory of a TrueType font, followed by its tables. It has five
/// tables of 8 bytes, sorted by tag.
const FONT: [u8; 132] = [
  0x00, 0x01, 0x00, 0x00, 0x00, 0x05, 0x00, 0x40, 0x00, 0x02, 0x00, 0x10, b'O', b'S', b'/', b'2',
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x5c, 0x00, 0x00, 0x00, 0x08, b'c', b'm', b'a', b'p',
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x64, 0x00, 0x00, 0x00, 0x08, b'g', b'l', b'y', b'f',
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x6c, 0x00, 0x00, 0x00, 0x08, b'h', b'e', b'a', b'd',
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x74, 0x00, 0x00, 0x00, 0x08, b'h', b'h', b'e', b'a',
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x7c, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00,
];

/// Reads the samples of this file through its types, and asserts the results.
pub fn main() {
  // ---- PNG: a file, its typed header, then chunks of any type.
  let png: &Png = try_pun_ref(&PNG[..]).unwrap();
  let header = png.header().unwrap();
  let ihdr = header.data().unwrap();
  assert_eq!((ihdr.width.get(), ihdr.height.get(), ihdr.bit_depth, ihdr.color_type), (1, 1, 8, 2));
  assert_eq!(header.kind.get(), 0x4948_4452);
  let (comment, rest) = from_head::<Chunk>(png.chunks()).unwrap();
  assert_eq!((comment.kind.critical(), comment.data().unwrap()), (false, &b"Comment\0hi"[..]));
  let (data, rest) = from_head::<Chunk>(rest).unwrap();
  assert_eq!((data.kind.critical(), data.data().unwrap().len()), (true, 13));
  let (end, rest) = from_head::<Chunk>(rest).unwrap();
  assert_eq!(
    (end.kind, end.crc(), rest.len()),
    (ChunkType(0x4945_4e44.into()), &[0xae, 0x42, 0x60, 0x82][..], 0)
  );
  // Any byte of any chunk: a length, a type, data, a CRC.
  for at in [8, 12, 20, 33, 40, 50, 60, 91] {
    let mut bytes = PNG;
    bytes[at] ^= 0x01;
    assert_eq!(try_pun_ref::<_, Png>(&bytes[..]).err(), Some(Error::Validity));
  }
  // No IEND at the end.
  assert_eq!(try_pun_ref::<_, Png>(&PNG[..80]).err(), Some(Error::Validity));
  // An IHDR chunk with a correct CRC and a bit depth that its color type does not allow. The file
  // is a valid `Png`, but reading its header returns an error.
  let bad = [&PNG[..8], &IHDR_4_BITS[..], &PNG[33..]].concat();
  let png: &Png = try_pun_ref(&bad[..]).unwrap();
  assert_eq!(png.header().unwrap().data().err(), Some(Error::Validity));
  // A chunk type that is not letters; IDAT first.
  assert_eq!(try_pun_ref::<_, ChunkType>(b"I1DR").err(), Some(Error::Validity));
  let idat_first = [&PNG[..8], &PNG[55..80], &PNG[8..55], &PNG[80..]].concat();
  assert!(try_pun_ref::<_, Png>(&idat_first[..]).is_err());

  // ---- BMP: a signed height, rows padded to 4 bytes.
  let (image, _) = from_head::<Bmp>(&BMP).unwrap();
  assert_eq!((image.height.get(), image.width.get(), image.bits_per_pixel.get()), (2, 3, 24));
  assert_eq!((image.dib_rest().len(), image.palette().len(), image.pixels().len()), (0, 0, 24));
  // Rows from the top: a height of -2.
  let mut top_down = BMP;
  top_down[22..26].copy_from_slice(&(-2i32).to_le_bytes());
  let image: &Bmp = try_pun_ref(&top_down[..]).unwrap();
  assert_eq!(image.height.get(), -2);
  // Three rows do not fit; 23 bits per pixel; a height of 0; a pixel offset in the header; a
  // size that is not the one of the file.
  for (at, value) in [(22, 3), (28, 23), (22, 0), (10, 40), (2, 0x4d)] {
    let mut bytes = BMP;
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, Bmp>(&bytes[..]).err(), Some(Error::Validity));
  }

  // ---- Fonts: fields derived from a count, records sorted, tables in the file.
  let font: &Font = try_pun_ref(&FONT[..]).unwrap();
  let records = font.records().unwrap();
  let tags: Vec<[u8; 4]> = records.iter().map(|record| record.tag.get().to_be_bytes()).collect();
  assert_eq!(tags, [*b"OS/2", *b"cmap", *b"glyf", *b"head", *b"hhea"]);
  assert_eq!((records[1].offset.get(), records[1].length.get(), font.tables().len()), (100, 8, 40));
  // One table, and its search fields.
  let one = [
    &[0, 1, 0, 0, 0, 1, 0, 16, 0, 0, 0, 0][..],
    b"OS/2",
    &[0; 4],
    &[0, 0, 0, 28],
    &[0, 0, 0, 8],
    &[0; 8],
  ]
  .concat();
  assert_eq!(try_pun_ref::<_, Font>(&one[..]).unwrap().records().unwrap().len(), 1);
  // A search range, an entry selector, a range shift; records out of order; a table beyond the
  // file; a table at an offset that is not a multiple of 4.
  for (at, value) in [(7, 0x41), (9, 3), (11, 1), (12, b'p'), (23, 0xff), (23, 0x5e)] {
    let mut bytes = FONT;
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, Font>(&bytes[..]).err(), Some(Error::Validity));
  }

  println!("ok");
}
