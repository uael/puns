//! Sound and video containers (RIFF and WAVE files, and the boxes of ISO-BMFF, or MP4), declared
//! with `#[derive(Pun)]` and read in place from bytes.
//!
//! As in `ethernet.rs`, the first half of this file holds only type declarations, with no `unsafe`,
//! no `impl` and no parsing code, and the second half reads files in place, from bytes.
//!
//! The following table lists what is specific to these formats, and what in the declarations
//! expresses it:
//!
//! | Specific to these formats                      | Declared with                            |
//! |------------------------------------------------|------------------------------------------|
//! | four-character codes say what is held          | `tag(FourCc = u32bec<0x666d_7420>)`      |
//! | a RIFF pad byte, not counted in the length     | `size = 8 + length + (length & 1)`       |
//! | the fields of a WAVE format agree              | `assert(byte_rate == .. * block_align)`  |
//! | WAVE chunks fill the file, `fmt ` first        | `assert(walk(12, ..) == ..)`             |
//! | WAVE data is whole frames                      | `assert({ let data = walk(..); .. })`    |
//! | an MP4 size of 32 bits, 64 bits, or to the end | a struct per form, an enum of references |
//! | the version and flags of a full box            | `u8c<0>`, `u24be`                        |
//! | brands, up to the end of a box                 | `[FourCc]`                               |
//!
//! The size of an MP4 box takes 4 or 8 bytes, as its first 4 bytes say. When these bytes are 0,
//! the box extends to the end of what holds it. That end is not in the box, so a `size` rule
//! cannot give it. Each form of box is a struct with a payload of its type, and an enum of
//! references tells the forms apart.
//!
//! The following are not covered:
//!
//! - The code that uses the types reads boxes inside boxes (`moov`, `trak`, ...) one level at a
//!   time, since a box does not say what kind of box its children are.
//! - The samples are compressed, or interleaved by tables that point anywhere in the file (`stco`,
//!   `stsz`). Decoding them makes a copy, not a view of the bytes.
//!
//! `main` reads a WAVE file chunk by chunk and an MP4 file box by box, and asserts their fields,
//! regions and getters. It changes bytes so that a rule fails or the size is wrong, asserts that
//! each read then returns an error, and prints `ok`.
#![forbid(unsafe_code)]

use core::marker::PhantomData;

use puns::{
  Error, Pun, Tag, from_head,
  num::{u8c, u16le, u24be, u32be, u32bec, u32le, u64be},
};

/// A four-character code whose meaning is not known.
#[derive(Pun, Debug, Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct FourCc(pub u32be);

// -------------------------------------------------------------------------------------------------
// RIFF

/// A RIFF chunk whose data is a `P`.
///
/// The data is followed by a pad byte if its size is odd. `P` is `[u8]` unless another type is
/// given.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 8 + length + (length & 1))]
pub struct Chunk<P: ?Sized + Tag<FourCc> = [u8]> {
  pub id: P::Tag,
  pub length: u32le,
  pub data_type: PhantomData<P>,
  #[pun(regions(data: P = length, pad))]
  pub rest: [u8],
}

/// The data of the `fmt ` chunk of PCM (1), IEEE float (3) or extensible (`0xfffe`) audio.
#[derive(Pun)]
#[repr(C)]
#[pun(tag(FourCc = u32bec<0x666d_7420>))]
#[pun(assert(block_align == channels * ((bits_per_sample + 7) / 8)))]
#[pun(assert(byte_rate == sample_rate * block_align))]
pub struct Format {
  #[pun(valid(1, 3, 0xfffe))]
  pub encoding: u16le,
  #[pun(valid(1..))]
  pub channels: u16le,
  #[pun(valid(1..))]
  pub sample_rate: u32le,
  pub byte_rate: u32le,
  pub block_align: u16le,
  #[pun(valid(8, 16, 24, 32, 64))]
  pub bits_per_sample: u16le,
  pub extension: [u8],
}

/// A WAVE file.
///
/// Its chunks fill the size that the RIFF header gives, and the `fmt ` chunk is first. The `data`
/// chunk, wherever it is, holds whole frames of the block alignment of the format.
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 8)]
#[pun(assert(be(12, 4) == 0x666d_7420))]
#[pun(assert(walk(
  12,
  |at| at < size_of_val(self),
  |at| at + 8 + le(at + 4, 4) + (le(at + 4, 4) & 1),
) == size_of_val(self)))]
// The block alignment is at a fixed place, since "fmt " is first.
#[pun(assert({
  let data = walk(
    12,
    |at| be(at, 4) != 0x6461_7461,
    |at| at + 8 + le(at + 4, 4) + (le(at + 4, 4) & 1),
  );
  le(data + 4, 4) % le(32, 2) == 0
}))]
pub struct Wave {
  pub riff: u32bec<0x5249_4646>,
  pub length: u32le,
  /// The form of the RIFF file, `WAVE`.
  pub form: u32bec<0x5741_5645>,
  pub chunks: [u8],
}

// -------------------------------------------------------------------------------------------------
// MP4

/// An MP4 box with a 32-bit size of at least 8, and a payload `P`.
///
/// The size of a box is in its first 4 bytes. When they hold 1, the size is in 8 bytes after the
/// type. When they hold 0, the box extends to the end of the file. [`Box32`], [`Box64`] and
/// [`BoxToEnd`] are these three forms, and [`AnyBox`] tells them apart.
#[derive(Pun)]
#[repr(C)]
#[pun(size = size)]
pub struct Box32<P: ?Sized + Tag<FourCc> = [u8]> {
  #[pun(valid(8..))]
  pub size: u32be,
  pub kind: P::Tag,
  pub payload: P,
}

/// An MP4 box with a 64-bit size.
#[derive(Pun)]
#[repr(C)]
#[pun(size = large_size)]
pub struct Box64<P: ?Sized + Tag<FourCc> = [u8]> {
  pub size: u32bec<1>,
  pub kind: P::Tag,
  #[pun(valid(16..))]
  pub large_size: u64be,
  pub payload: P,
}

/// An MP4 box that extends to the end of the file.
#[derive(Pun)]
#[repr(C)]
pub struct BoxToEnd<P: ?Sized + Tag<FourCc> = [u8]> {
  pub size: u32bec<0>,
  pub kind: P::Tag,
  pub payload: P,
}

/// An MP4 box of any of the three forms.
#[derive(Pun)]
pub enum AnyBox<'a> {
  Box32(&'a Box32),
  Box64(&'a Box64),
  ToEnd(&'a BoxToEnd),
}

/// The payload of `ftyp`, which holds the major brand, a minor version and compatible brands.
#[derive(Pun)]
#[repr(C)]
#[pun(tag(FourCc = u32bec<0x6674_7970>))]
pub struct FileType {
  pub major_brand: FourCc,
  pub minor_version: u32be,
  pub compatible: [FourCc],
}

/// The payload of `mvhd`, version 0.
///
/// It is a full box, with a version and flags, followed by times of 32 bits.
#[derive(Pun)]
#[repr(C)]
#[pun(tag(FourCc = u32bec<0x6d76_6864>))]
#[pun(assert(timescale != 0))]
#[pun(get(seconds: u32 = duration / timescale))]
pub struct MovieHeader {
  pub version: u8c<0>,
  pub flags: u24be,
  pub creation: u32be,
  pub modification: u32be,
  pub timescale: u32be,
  pub duration: u32be,
  /// The rate, the volume, reserved bytes, the matrix, predefined bytes and the next track ID.
  pub rest: [u8; 80],
}

// -------------------------------------------------------------------------------------------------
// Use

/// The bytes of a WAVE file of 16-bit stereo PCM at 8000 Hz, with a `LIST` chunk and two frames
/// of data.
const WAVE: [u8; 64] = [
  b'R', b'I', b'F', b'F', 0x38, 0x00, 0x00, 0x00, b'W', b'A', b'V', b'E', b'f', b'm', b't', b' ',
  0x10, 0x00, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00, 0x40, 0x1f, 0x00, 0x00, 0x00, 0x7d, 0x00, 0x00,
  0x04, 0x00, 0x10, 0x00, b'L', b'I', b'S', b'T', 0x04, 0x00, 0x00, 0x00, b'a', b'b', b'c', b'd',
  b'd', b'a', b't', b'a', 0x08, 0x00, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00,
];

/// The bytes of an MP4 file, with an `ftyp` box, an `mdat` box of a 64-bit size, and a `free` box
/// up to the end.
const MP4: [u8; 57] = [
  0x00, 0x00, 0x00, 0x18, b'f', b't', b'y', b'p', b'i', b's', b'o', b'm', 0x00, 0x00, 0x02, 0x00,
  b'i', b's', b'o', b'm', b'i', b's', b'o', b'2', 0x00, 0x00, 0x00, 0x01, b'm', b'd', b'a', b't',
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x14, 0xaa, 0xaa, 0xaa, 0xaa, 0x00, 0x00, 0x00, 0x00,
  b'f', b'r', b'e', b'e', 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// Reads the samples of this file through its types, and asserts the results.
pub fn main() {
  // ---- WAVE: chunks, a typed format, data of whole frames.
  let (file, rest) = from_head::<Wave>(&WAVE).unwrap();
  assert_eq!((file.length.get(), rest.len()), (56, 0));
  let (format, rest) = from_head::<Chunk<Format>>(&file.chunks).unwrap();
  let format = format.data().unwrap();
  assert_eq!(
    (format.encoding.get(), format.channels.get(), format.sample_rate.get()),
    (1, 2, 8000)
  );
  assert_eq!((format.block_align.get(), format.bits_per_sample.get()), (4, 16));
  let (list, rest) = from_head::<Chunk>(rest).unwrap();
  assert_eq!(
    (list.id, list.data().unwrap(), list.pad().len()),
    (FourCc(0x4c49_5354.into()), &b"abcd"[..], 0)
  );
  let (data, rest) = from_head::<Chunk>(rest).unwrap();
  assert_eq!((data.data().unwrap().len(), rest.len()), (8, 0));
  // A chunk of an odd size is padded.
  let mut odd = WAVE;
  odd[40] = 3;
  let (file, _) = from_head::<Wave>(&odd).unwrap();
  let (_, rest) = from_head::<Chunk>(&file.chunks).unwrap();
  let (list, _) = from_head::<Chunk>(rest).unwrap();
  assert_eq!((list.data().unwrap(), list.pad()), (&b"abc"[..], &b"d"[..]));
  // A byte rate that disagrees: "fmt " is a typed region, checked when read.
  let mut bytes = WAVE;
  bytes[28] = 1;
  let (file, _) = from_head::<Wave>(&bytes).unwrap();
  assert!(matches!(
    from_head::<Chunk<Format>>(&file.chunks).unwrap().0.data(),
    Err(Error::Validity { .. })
  ));
  // Data of 7 bytes (and padded): not whole frames.
  let mut bytes = WAVE.to_vec();
  bytes[52] = 7;
  assert!(matches!(from_head::<Wave>(&bytes), Err(Error::Validity { .. })));
  // A chunk that extends beyond the file; "fmt " not first; not RIFF; a RIFF size beyond the bytes.
  for (at, value, error) in [
    (40, 9, Error::Validity { at: 0 }),
    (12, b'g', Error::Validity { at: 0 }),
    (0, b'r', Error::Validity { at: 0 }),
    (4, 0x39, Error::Size),
  ] {
    let mut bytes = WAVE;
    bytes[at] = value;
    assert_eq!(from_head::<Wave>(&bytes).err(), Some(error));
  }

  // ---- MP4: boxes of three forms, of typed payloads.
  let (first, rest) = from_head::<Box32<FileType>>(&MP4).unwrap();
  let brands: Vec<[u8; 4]> =
    first.payload.compatible.iter().map(|brand| brand.0.get().to_be_bytes()).collect();
  assert_eq!(
    (first.payload.major_brand.0.get().to_be_bytes(), brands),
    (*b"isom", vec![*b"isom", *b"iso2"])
  );
  let mut boxes = vec![];
  let mut rest = rest;
  while let Ok((any, next)) = AnyBox::from_head(rest) {
    boxes.push(match any {
      AnyBox::Box32(any) => (any.kind, any.payload.len()),
      AnyBox::Box64(any) => (any.kind, any.payload.len()),
      AnyBox::ToEnd(any) => (any.kind, any.payload.len()),
    });
    rest = next;
  }
  assert_eq!(
    (boxes, rest.len()),
    (vec![(FourCc(0x6d64_6174.into()), 4), (FourCc(0x6672_6565.into()), 5)], 0)
  );
  // A size below 8; a 64 bits size below 16; not "ftyp".
  assert!(AnyBox::from_head(&[0, 0, 0, 7, b'f', b'r', b'e', b'e']).is_err());
  let short = [&[0, 0, 0, 1][..], b"mdat", &15u64.to_be_bytes(), &[0; 8]].concat();
  assert!(AnyBox::from_head(&short).is_err());
  assert!(matches!(from_head::<Box32<FileType>>(&MP4[24..]), Err(Error::Validity { .. })));
  // A movie header, version 0: one minute at 1000 units a second.
  let mut header = [
    &[0, 0, 0, 108][..],
    b"mvhd",
    &[0; 12],
    &1000u32.to_be_bytes(),
    &60_000u32.to_be_bytes(),
    &[0; 80],
  ]
  .concat();
  let (movie, _) = from_head::<Box32<MovieHeader>>(&header).unwrap();
  assert_eq!((movie.payload.timescale.get(), movie.payload.seconds()), (1000, 60));
  // Version 1 (its times are of 64 bits: another struct); a timescale of 0.
  header[8] = 1;
  assert!(matches!(from_head::<Box32<MovieHeader>>(&header), Err(Error::Validity { .. })));
  header[8] = 0;
  header[23] = 0;
  header[22] = 0;
  assert!(matches!(from_head::<Box32<MovieHeader>>(&header), Err(Error::Validity { .. })));

  println!("ok");
}
