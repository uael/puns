//! Storage formats (the SQLite database header, FAT boot sectors, GPT partition tables, and USB
//! descriptors), declared with `#[derive(Pun)]` and read in place from bytes.
//!
//! As in `ethernet.rs`, the first half of this file holds only type declarations, with no `unsafe`,
//! no `impl` and no parsing code, and the second half reads these formats in place, from bytes.
//!
//! The following table lists what is specific to these formats, and what in the declarations
//! expresses it:
//!
//! | Specific to these formats                    | Declared with                                |
//! |----------------------------------------------|----------------------------------------------|
//! | a magic of 16 bytes                          | `u128bec<0x5351_4c69_..>`                    |
//! | an SQLite page size is 2^n, or 1 for 65536   | `valid(1, 512, ..)`, `get(page_bytes: ..)`   |
//! | an SQLite file is the pages its header gives | `assert(be(28, 4) * .. == ..)`               |
//! | the kind of FAT, by a count of clusters      | `assert({ let total = ..; .. < 65525 })`     |
//! | a FAT jump that is one of two instructions   | `assert((be(0, 1) == 0xeb && ..) \|\| ..)`   |
//! | a GPT sector size that the bytes do not give | a const parameter, `GptDisk<512>`            |
//! | GPT entries are where the header places them | `le(S + 72, 8) * S`                          |
//! | the CRC-32 of the GPT partition entries      | `assert(crc32(start, ..) == le(S + 88, 4))`  |
//! | the CRC-32 of the GPT header, over itself    | `assert(crc32(S, .., S + 16, S + 20) == ..)` |
//! | USB descriptors of fixed lengths and types   | `u8c<18>`, `u8c<1>`                          |
//! | USB descriptors that fill a configuration    | `size = total_length`, `assert(walk(9, ..))` |
//! | the bit fields of a USB endpoint             | `get(input: bool = address & 0x80 != 0, ..)` |
//!
//! Some properties of these formats are declared in a particular way:
//!
//! - The sector size of a GPT disk is not in its bytes, so it is a const parameter, as in
//!   `GptDisk<512>`. The name of a GPT partition is UTF-16, for which the crate has no automaton,
//!   so it is a `[u16le; 36]`.
//! - Whether a FAT volume is FAT12, FAT16 or FAT32 depends on its count of clusters, computed from
//!   the fields of its BIOS parameter block. Both kinds of boot sector are declared, and an enum of
//!   references tells them apart. [`Fat32Boot`] reads the BIOS parameter block with `le(..)`,
//!   since it does not declare the block as fields.
//! - A USB configuration descriptor is followed by interface, endpoint and class descriptors of
//!   their own sizes. The code that uses the types reads them one by one. [`Configuration`]
//!   checks that they fill its total length, and counts its interfaces.
//!
//! The following are not covered:
//!
//! - The pages of an SQLite database (B-tree pages, made of cells at offsets that a page gives,
//!   and of varints) would be more declarations, of the forms of `executables.rs` and
//!   `encodings.rs`.
//! - The tables and the directories of a FAT volume would be more declarations of the same kind.
//!
//! `main` reads an SQLite header and database, a FAT16 and a FAT32 boot sector, a GPT disk and one
//! of its entries, and USB descriptors, and asserts their fields and getters. It changes or
//! truncates bytes so that a rule fails, asserts that each read then returns an error, and prints
//! `ok`.
#![forbid(unsafe_code)]

use puns::{
  Error, Pun, from_head,
  num::{u8c, u16be, u16le, u16lec, u32be, u32le, u64le, u128bec},
  try_pun_ref,
};

// -------------------------------------------------------------------------------------------------
// SQLite

/// The header of an SQLite database, its first 100 bytes.
#[derive(Pun)]
#[repr(C)]
// What a page leaves to the B-tree, once its reserved bytes are taken out, is at least 480 bytes.
#[pun(assert(reserved_space <= 32 || page_size + (page_size == 1) as u128 * 65535 - reserved_space >= 480))]
#[pun(get(page_bytes: u32 = page_size + (page_size == 1) as u128 * 65535))]
pub struct SqliteHeader {
  /// The magic string `SQLite format 3\0`.
  pub magic: u128bec<0x5351_4c69_7465_2066_6f72_6d61_7420_3300>,
  /// The page size in bytes, a power of two from 512 to 32768, or 1 for 65536.
  #[pun(valid(1, 512, 1024, 2048, 4096, 8192, 16384, 32768))]
  pub page_size: u16be,
  /// The file format write version, 1 for legacy and 2 for WAL.
  #[pun(valid(1..=2))]
  pub write_version: u8,
  #[pun(valid(1..=2))]
  pub read_version: u8,
  pub reserved_space: u8,
  pub max_payload_fraction: u8c<64>,
  pub min_payload_fraction: u8c<32>,
  pub leaf_payload_fraction: u8c<32>,
  pub change_counter: u32be,
  pub database_size: u32be,
  pub first_freelist_trunk: u32be,
  pub freelist_count: u32be,
  pub schema_cookie: u32be,
  #[pun(valid(1..=4))]
  pub schema_format: u32be,
  pub default_cache_size: u32be,
  pub largest_root_page: u32be,
  /// The text encoding, 1 for UTF-8, 2 for UTF-16LE and 3 for UTF-16BE.
  #[pun(valid(1..=3))]
  pub text_encoding: u32be,
  pub user_version: u32be,
  #[pun(valid(0..=1))]
  pub incremental_vacuum: u32be,
  pub application_id: u32be,
  pub reserved: [u8c<0>; 20],
  pub version_valid_for: u32be,
  pub sqlite_version: u32be,
}

/// An SQLite database file.
///
/// Its size is a multiple of the page size that its header gives, where 1 means 65536. The count
/// of pages in the header is valid when the "version valid for" field equals the change counter.
/// The file then holds exactly that many pages.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(size_of_val(self) % (page_size + (page_size == 1) as u128 * 65535) == 0))]
#[pun(assert(be(92, 4) != be(24, 4)
  || be(28, 4) * (page_size + (page_size == 1) as u128 * 65535) == size_of_val(self)))]
pub struct Sqlite {
  pub magic: u128bec<0x5351_4c69_7465_2066_6f72_6d61_7420_3300>,
  #[pun(valid(1, 512, 1024, 2048, 4096, 8192, 16384, 32768))]
  pub page_size: u16be,
  pub rest: [u8],
}

// -------------------------------------------------------------------------------------------------
// FAT

/// A FAT12 or FAT16 boot sector, of a volume of fewer than 65525 clusters.
#[derive(Pun)]
#[repr(C)]
// A jump: a short one and a NOP, or a near one. (An element of an array field cannot be named
// by an expression, `jump[0]`: it is read with `be`.)
#[pun(assert((be(0, 1) == 0xeb && be(2, 1) == 0x90) || be(0, 1) == 0xe9))]
#[pun(assert(fat_size16 != 0 && root_entries != 0))]
// The count of clusters: the sectors of data, divided by the sectors of a cluster.
#[pun(assert({
  let total = if total16 != 0 { total16 } else { total32 };
  let root = (root_entries * 32 + bytes_per_sector - 1) / bytes_per_sector;
  (total - reserved_sectors - fats * fat_size16 - root) / sectors_per_cluster < 65525
}))]
pub struct Fat16Boot {
  pub jump: [u8; 3],
  pub oem: [u8; 8],
  #[pun(valid(512, 1024, 2048, 4096))]
  pub bytes_per_sector: u16le,
  #[pun(valid(1, 2, 4, 8, 16, 32, 64, 128))]
  pub sectors_per_cluster: u8,
  #[pun(valid(1..))]
  pub reserved_sectors: u16le,
  #[pun(valid(1..))]
  pub fats: u8,
  pub root_entries: u16le,
  pub total16: u16le,
  #[pun(valid(0xf0, 0xf8..=0xff))]
  pub media: u8,
  pub fat_size16: u16le,
  pub sectors_per_track: u16le,
  pub heads: u16le,
  pub hidden: u32le,
  pub total32: u32le,
  pub drive: u8,
  pub reserved: u8,
  pub boot_signature: u8c<0x29>,
  pub volume_id: u32le,
  pub label: [u8; 11],
  pub fs_type: [u8; 8],
  pub code: [u8; 448],
  pub signature: u16lec<0xaa55>,
}

/// A FAT32 boot sector, of a volume of 65525 clusters or more.
///
/// Its BIOS parameter block is the one of [`Fat16Boot`], except that its 16-bit sizes are 0. The
/// rules read the block at its offsets.
#[derive(Pun)]
#[repr(C)]
#[pun(assert((be(0, 1) == 0xeb && be(2, 1) == 0x90) || be(0, 1) == 0xe9))]
// No root directory of a fixed size, a size of 32 bits, a FAT of a size of 32 bits.
#[pun(assert(le(17, 2) == 0 && le(19, 2) == 0 && le(22, 2) == 0 && fat_size32 != 0))]
#[pun(assert({
  let bytes_per_sector = le(11, 2);
  let sectors_per_cluster = be(13, 1);
  bytes_per_sector >= 512
    && sectors_per_cluster >= 1
    && (le(32, 4) - le(14, 2) - be(16, 1) * fat_size32) / sectors_per_cluster >= 65525
}))]
#[pun(assert(root_cluster >= 2))]
pub struct Fat32Boot {
  pub bpb: [u8; 36],
  pub fat_size32: u32le,
  pub ext_flags: u16le,
  pub version: u16lec<0>,
  pub root_cluster: u32le,
  pub fs_info: u16le,
  pub backup_boot: u16le,
  pub reserved: [u8; 12],
  pub drive: u8,
  pub reserved1: u8,
  pub boot_signature: u8c<0x29>,
  pub volume_id: u32le,
  pub label: [u8; 11],
  pub fs_type: [u8; 8],
  pub code: [u8; 420],
  pub signature: u16lec<0xaa55>,
}

/// A FAT boot sector, of the kind that its count of clusters gives.
#[derive(Pun)]
pub enum FatBoot<'a> {
  Fat32(&'a Fat32Boot),
  Fat16(&'a Fat16Boot),
}

// -------------------------------------------------------------------------------------------------
// GPT

/// A disk of GPT partitions, with sectors of `S` bytes.
///
/// A disk holds a protective MBR, the GPT header at LBA 1, and the partition entries where the
/// header places them. A `GptDisk` requires that the CRC-32 of the entries and the CRC-32 of the
/// header are the ones that the header gives.
#[derive(Pun)]
#[repr(transparent)]
// The protective MBR: its first partition is of type 0xee.
#[pun(assert(le(510, 2) == 0xaa55 && be(446 + 4, 1) == 0xee))]
// "EFI PART", revision 1.0, a header of 92 bytes or more, at LBA 1.
#[pun(assert(be(S, 8) == 0x4546_4920_5041_5254 && le(S + 8, 4) == 0x0001_0000))]
#[pun(assert(le(S + 12, 4) >= 92 && le(S + 12, 4) <= S && le(S + 24, 8) == 1))]
// The usable blocks, before the backup header.
#[pun(assert(le(S + 40, 8) <= le(S + 48, 8) + 1 && le(S + 48, 8) < le(S + 32, 8)))]
// Entries of 128 bytes times a power of two.
#[pun(assert({
  let size = le(S + 84, 4);
  size >= 128 && size & (size - 1) == 0
}))]
#[pun(assert({
  let start = le(S + 72, 8) * S;
  crc32(start, start + le(S + 80, 4) * le(S + 84, 4)) == le(S + 88, 4)
}))]
// The CRC of the header, its own field taken as zeros.
#[pun(assert(crc32(S, S + le(S + 12, 4), S + 16, S + 20) == le(S + 16, 4)))]
pub struct GptDisk<const S: usize>(pub [u8]);

/// A GPT partition entry, unused when its type is 0.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
#[pun(assert(first_lba <= last_lba || (first_lba == 0 && last_lba == 0)))]
pub struct GptEntry {
  pub type_guid: [u8; 16],
  pub unique_guid: [u8; 16],
  pub first_lba: u64le,
  pub last_lba: u64le,
  pub attributes: u64le,
  /// The name of the partition, in UTF-16LE.
  pub name: [u16le; 36],
}

// -------------------------------------------------------------------------------------------------
// USB

/// A USB device descriptor.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
pub struct DeviceDescriptor {
  pub length: u8c<18>,
  pub kind: u8c<1>,
  /// The USB version, from 1.0 to 3.2, in binary-coded decimal.
  #[pun(valid(0x0100, 0x0110, 0x0200, 0x0201, 0x0210, 0x0300, 0x0310, 0x0320))]
  pub usb: u16le,
  pub class: u8,
  pub subclass: u8,
  pub protocol: u8,
  /// The maximum packet size of endpoint 0, 8, 16, 32 or 64 bytes. For USB 3, it is 9, which
  /// means 512 bytes (2^9).
  #[pun(valid(8, 16, 32, 64, 9))]
  pub max_packet_size: u8,
  pub vendor: u16le,
  pub product: u16le,
  pub device: u16le,
  pub manufacturer: u8,
  pub product_string: u8,
  pub serial: u8,
  #[pun(valid(1..))]
  pub configurations: u8,
}

/// A USB configuration descriptor, followed by interface, endpoint and class descriptors that
/// fill its total length.
#[derive(Pun)]
#[repr(C)]
#[pun(size = total_length)]
// Bus powered (bit 7); bits 0 to 4 are 0.
#[pun(assert(attributes & 0x80 != 0 && attributes & 0x1f == 0))]
// The descriptors fill the total length, and `interfaces` of them are interfaces of alternate
// setting 0: a walk whose state is an offset (the low 16 bits) and a count.
#[pun(assert({
  let s = walk(
    9,
    |s| s & 0xffff < total_length,
    |s| {
      let at = s & 0xffff;
      if be(at, 1) < 2 {
        1 / 0
      } else {
        s + be(at, 1) + if be(at + 1, 1) == 4 && be(at + 3, 1) == 0 { 0x1_0000 } else { 0 }
      }
    },
  );
  s & 0xffff == total_length && s >> 16 == interfaces
}))]
pub struct Configuration {
  pub length: u8c<9>,
  pub kind: u8c<2>,
  pub total_length: u16le,
  #[pun(valid(1..))]
  pub interfaces: u8,
  pub value: u8,
  pub string: u8,
  pub attributes: u8,
  /// The maximum power drawn from the bus, in units of 2 mA.
  pub max_power: u8,
  pub descriptors: [u8],
}

/// A USB interface descriptor.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
pub struct InterfaceDescriptor {
  pub length: u8c<9>,
  pub kind: u8c<4>,
  pub number: u8,
  pub alternate: u8,
  pub endpoints: u8,
  pub class: u8,
  pub subclass: u8,
  pub protocol: u8,
  pub string: u8,
}

/// A USB endpoint descriptor, with a direction, an endpoint number from 1 to 15 and a transfer
/// type.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
#[pun(assert(address & 0x70 == 0 && address & 0x0f != 0))]
#[pun(get(input: bool = address & 0x80 != 0, transfer: u8 = attributes & 3))]
pub struct EndpointDescriptor {
  pub length: u8c<7>,
  pub kind: u8c<5>,
  pub address: u8,
  pub attributes: u8,
  pub max_packet_size: u16le,
  pub interval: u8,
}

// -------------------------------------------------------------------------------------------------
// Use

/// The bytes of the header of an SQLite database of two pages of 4096 bytes.
const SQLITE: [u8; 100] = [
  b'S', b'Q', b'L', b'i', b't', b'e', 0x20, b'f', b'o', b'r', b'm', b'a', b't', 0x20, 0x33, 0x00,
  0x10, 0x00, 0x01, 0x01, 0x00, 0x40, 0x20, 0x20, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x02,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03,
  0x00, 0x2e, 0x76, 0x89,
];

/// The first bytes of the boot sector of a FAT16 volume of 32 MiB. They hold its BIOS parameter
/// block, up to the type of the file system.
const FAT16: [u8; 62] = [
  0xeb, 0x3c, 0x90, b'M', b'S', b'W', b'I', b'N', b'4', b'.', b'1', 0x00, 0x02, 0x04, 0x01, 0x00,
  0x02, 0x00, 0x02, 0x00, 0x00, 0xf8, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x29, 0xcd, 0xab, 0x34, 0x12, 0x4e, 0x4f, 0x20, b'N', b'A',
  b'M', b'E', 0x20, 0x20, 0x20, 0x20, b'F', b'A', b'T', b'1', b'6', 0x20, 0x20, 0x20,
];

/// The first bytes of the boot sector of a FAT32 volume of 512 MiB.
const FAT32: [u8; 90] = [
  0xeb, 0x58, 0x90, b'M', b'S', b'W', b'I', b'N', b'4', b'.', b'1', 0x00, 0x02, 0x08, 0x20, 0x00,
  0x02, 0x00, 0x00, 0x00, 0x00, 0xf8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x10, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00,
  0x01, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x29, 0xcd, 0xab, 0x34, 0x12, 0x4e, 0x4f, 0x20, b'N', b'A', b'M', b'E', 0x20, 0x20,
  0x20, 0x20, b'F', b'A', b'T', b'3', b'2', 0x20, 0x20, 0x20,
];

/// The bytes of the GPT header of a disk of 40 sectors of 512 bytes. It gives four partition
/// entries from LBA 2, their CRC-32, and its own CRC-32.
const GPT_HEADER: [u8; 92] = [
  b'E', b'F', b'I', 0x20, b'P', b'A', b'R', b'T', 0x00, 0x00, 0x01, 0x00, 0x5c, 0x00, 0x00, 0x00,
  0xc4, b'C', b'u', b'y', 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x27, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x22, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x26, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
  0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x04, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0xa9, 0x5f, 0x19, 0x2b,
];

/// The bytes of the first partition entry of that disk, a Linux file system named `data` from
/// LBA 34 to 38.
const GPT_ENTRY: [u8; 128] = [
  0xaf, 0x3d, 0xc6, 0x0f, 0x83, 0x84, 0x72, 0x47, 0x8e, 0x79, 0x3d, 0x69, 0xd8, 0x47, 0x7d, 0xe4,
  0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10,
  0x22, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x26, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x64, 0x00, 0x61, 0x00, 0x74, 0x00, 0x61, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// The bytes of a USB device descriptor of USB 2.0, with a packet size of 64 bytes and the vendor
/// `0x1d6b`.
const DEVICE: [u8; 18] =
  [18, 1, 0x00, 0x02, 0, 0, 0, 64, 0x6b, 0x1d, 0x04, 0x01, 0x00, 0x01, 1, 2, 3, 1];

/// The bytes of a USB configuration with one interface of two alternate settings, each with one
/// endpoint.
const CONFIGURATION: [u8; 41] = [
  9, 2, 41, 0, 1, 1, 0, 0x80, 50, 9, 4, 0, 0, 1, 0xff, 0, 0, 0, 7, 5, 0x81, 2, 0, 2, 0, 9, 4, 0, 1,
  1, 0xff, 0, 0, 0, 7, 5, 0x81, 3, 8, 0, 1,
];

/// Reads the samples of this file through its types, and asserts the results.
pub fn main() {
  // ---- SQLite: a header of fixed values, and a file of the pages that it counts.
  let mut database = SQLITE.to_vec();
  database.resize(2 * 4096, 0);
  let header: &SqliteHeader = try_pun_ref(&database[..100]).unwrap();
  assert_eq!(
    (header.page_bytes(), header.database_size.get(), header.text_encoding.get()),
    (4096, 2, 1)
  );
  assert_eq!(header.sqlite_version.get(), 3_045_001);
  assert!(try_pun_ref::<_, Sqlite>(&database[..]).is_ok());
  // The magic; a page size that is not a power of two; the maximum payload fraction; an
  // incremental vacuum of 4; a reserved byte.
  for (at, value) in [(0, b's'), (17, 1), (21, 63), (67, 4), (75, 1)] {
    let mut bytes = SQLITE;
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, SqliteHeader>(&bytes[..]).err(), Some(Error::Validity));
  }
  // The header counts two pages, one is there; half a page.
  assert_eq!(try_pun_ref::<_, Sqlite>(&database[..4096]).err(), Some(Error::Validity));
  assert_eq!(try_pun_ref::<_, Sqlite>(&database[..6000]).err(), Some(Error::Validity));
  // A page size of 1 means 65536.
  let mut large = SQLITE;
  large[16..18].copy_from_slice(&[0, 1]);
  assert_eq!(try_pun_ref::<_, SqliteHeader>(&large[..]).unwrap().page_bytes(), 65536);

  // ---- FAT: the kind of a volume, by its count of clusters.
  let mut fat16 = FAT16.to_vec();
  fat16.resize(512, 0);
  fat16[510..].copy_from_slice(&[0x55, 0xaa]);
  let Ok(FatBoot::Fat16(boot)) = FatBoot::from_bytes(&fat16) else { panic!() };
  assert_eq!(
    (boot.root_entries.get(), boot.total32.get(), &boot.fs_type),
    (512, 65536, b"FAT16   ")
  );
  let mut fat32 = FAT32.to_vec();
  fat32.resize(512, 0);
  fat32[510..].copy_from_slice(&[0x55, 0xaa]);
  let Ok(FatBoot::Fat32(boot)) = FatBoot::from_bytes(&fat32) else { panic!() };
  assert_eq!(
    (boot.root_cluster.get(), boot.fat_size32.get(), &boot.fs_type),
    (2, 1024, b"FAT32   ")
  );
  // Too many clusters for FAT16: five times the sectors.
  let mut bytes = fat16.clone();
  bytes[34] = 5;
  assert_eq!(FatBoot::from_bytes(&bytes).err(), Some(Error::Validity));
  // The jump, the size of a sector, the media, the signature, the extended boot signature.
  for (at, value) in [(0, 0x00), (11, 0x01), (21, 0x00), (511, 0xab), (38, 0x28)] {
    let mut bytes = fat16.clone();
    bytes[at] = value;
    assert_eq!(FatBoot::from_bytes(&bytes).err(), Some(Error::Validity));
  }
  // FAT32: a root directory of a fixed size; the root cluster.
  for (at, value) in [(17, 1), (44, 1)] {
    let mut bytes = fat32.clone();
    bytes[at] = value;
    assert_eq!(FatBoot::from_bytes(&bytes).err(), Some(Error::Validity));
  }

  // ---- GPT: a protective MBR, a header, entries and their CRC.
  let mut disk = vec![0u8; 40 * 512];
  disk[446 + 4] = 0xee;
  disk[510..512].copy_from_slice(&[0x55, 0xaa]);
  disk[512..512 + 92].copy_from_slice(&GPT_HEADER);
  disk[1024..1024 + 128].copy_from_slice(&GPT_ENTRY);
  assert!(try_pun_ref::<_, GptDisk<512>>(&disk[..]).is_ok());
  let entry: &GptEntry = try_pun_ref(&disk[1024..1152]).unwrap();
  let name: String = entry.name.iter().map(|c| char::from(c.get() as u8)).take(4).collect();
  assert_eq!((entry.first_lba.get(), entry.last_lba.get(), name.as_str()), (34, 38, "data"));
  // A byte of an entry (their CRC); the signature; entries of 100 bytes; the protective type; the
  // first usable block after the last.
  for (at, value) in [(1024 + 3, 1), (512 + 1, 0), (512 + 84, 100), (450, 0x07), (512 + 40, 40)] {
    let mut bytes = disk.clone();
    bytes[at] = value;
    assert_eq!(try_pun_ref::<_, GptDisk<512>>(&bytes[..]).err(), Some(Error::Validity));
  }
  // The CRC of the header, or a byte that it covers.
  for at in [512 + 16, 512 + 40] {
    let mut bytes = disk.clone();
    bytes[at] ^= 1;
    assert_eq!(try_pun_ref::<_, GptDisk<512>>(&bytes[..]).err(), Some(Error::Validity));
  }
  // The sector size is a parameter: as sectors of 4096 bytes, this is not a GPT disk.
  assert_eq!(try_pun_ref::<_, GptDisk<4096>>(&disk[..]).err(), Some(Error::Validity));
  // An entry that ends before it starts.
  let mut bytes = GPT_ENTRY;
  bytes[40] = 30;
  assert_eq!(try_pun_ref::<_, GptEntry>(&bytes[..]).err(), Some(Error::Validity));

  // ---- USB: descriptors of fixed sizes, and a configuration that they fill.
  let device: &DeviceDescriptor = try_pun_ref(&DEVICE).unwrap();
  assert_eq!(
    (device.usb.get(), device.vendor.get(), device.product.get()),
    (0x0200, 0x1d6b, 0x0104)
  );
  let mut bytes = DEVICE;
  bytes[7] = 65;
  assert_eq!(try_pun_ref::<_, DeviceDescriptor>(&bytes).err(), Some(Error::Validity));
  let (configuration, rest) = from_head::<Configuration>(&CONFIGURATION).unwrap();
  assert_eq!((configuration.descriptors.len(), configuration.max_power, rest.len()), (32, 50, 0));
  let (mut descriptors, mut endpoints) = (&configuration.descriptors[..], vec![]);
  while !descriptors.is_empty() {
    let (interface, rest) = from_head::<InterfaceDescriptor>(descriptors).unwrap();
    let (endpoint, rest) = from_head::<EndpointDescriptor>(rest).unwrap();
    endpoints.push((
      interface.alternate,
      endpoint.input(),
      endpoint.transfer(),
      endpoint.max_packet_size.get(),
    ));
    descriptors = rest;
  }
  assert_eq!(endpoints, [(0, true, 2, 512), (1, true, 3, 8)]);
  // The descriptor counts two interfaces, one is there; not bus powered; a descriptor of a length
  // of 1; a total length that is not the one of the descriptors.
  for (at, value) in [(4, 2), (7, 0x40), (9, 1), (2, 40)] {
    let mut bytes = CONFIGURATION;
    bytes[at] = value;
    assert!(from_head::<Configuration>(&bytes).is_err());
  }
  // An endpoint 0, which is not described.
  let mut bytes = CONFIGURATION;
  bytes[20] = 0x80;
  assert_eq!(try_pun_ref::<_, EndpointDescriptor>(&bytes[18..25]).err(), Some(Error::Validity));

  println!("ok");
}
