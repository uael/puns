//! Executables and bytecode, declared with `#[derive(Pun)]` and read in place from bytes: ELF
//! (32 and 64 bits, of either byte order), PE/COFF, Mach-O, Java class files and WebAssembly
//! modules.
//!
//! As in `ethernet.rs`, the first half of this file holds only declarations of types and of two
//! constants, with no `unsafe`, no `impl` and no parsing code, and the second half reads files in
//! place, from bytes.
//!
//! The following table lists what is specific to these formats, and what in the declarations
//! expresses it:
//!
//! | Specific to these formats                      | Declared with                            |
//! |------------------------------------------------|------------------------------------------|
//! | the ELF class and byte order change all fields | type parameters, an enum of references   |
//! | ELF headers, at the offset and size it gives   | `regions(program_headers: [P] = .., ..)` |
//! | ELF entry sizes are the sizes of the types     | `assert(e_phentsize == size_of::<P>())`  |
//! | an alignment that is a power of two            | `assert(p_align & (p_align - 1) == 0)`   |
//! | PE headers, where the DOS header says          | `regions(stub = e_lfanew - 64, ..)`      |
//! | a PE magic at a distance gives the kind        | `assert(le(e_lfanew + 24, 2) == MAGIC)`  |
//! | PE data directories and sections               | `regions(sections: [..] = .., ..)`       |
//! | counted Mach-O commands fill `sizeofcmds`      | a `walk` of an offset and a count        |
//! | Mach-O sections, counted by their segment      | `len = nsects`                           |
//! | Java pool entries, of a size their tag gives   | a `walk`, a table of sizes in a constant |
//! | what follows the Java pool, from its end       | `be(at + 2, 2) < constant_pool_count`    |
//! | WebAssembly sections, of a size in LEB128      | `size = 1 + leb128_len(1) + leb128(1)`   |
//! | WebAssembly sections, in order                 | a walk, ranks in a constant              |
//! | WebAssembly value types                        | an enum without fields                   |
//!
//! The following are not covered:
//!
//! - The fields of the ELF header are fields of the file, not of a header struct inside it. An
//!   expression reads only the integer fields of its own struct, so it cannot name
//!   `header.e_phoff`. The program headers, then the section headers, must follow the header as
//!   consecutive regions. Other layouts are valid ELF, but these types reject them. Code that
//!   uses the types must follow the offsets to the segments and the sections. `main` reads only
//!   the regions.
//! - The checksum of a PE image and its import and export tables cannot be read from the bytes as
//!   they are. The tables hold addresses relative to where the sections are loaded, not to the
//!   file.
//! - The constant pool of a Java class file is walked, but its fields and methods are not
//!   checked. They are counted items whose attributes have sizes of their own and are counted
//!   too. Checking them takes a walk inside a walk, which this file does not write.
//! - The text of a `CONSTANT_Utf8` is "modified UTF-8". It is not a `str`, and the crate has no
//!   automaton for it, although one can be written for `matches`.
//!
//! `main` reads a file of each format from bytes and asserts its fields, regions and getters. It
//! reinterprets ELF files of the four classes and byte orders as an [`AnyElf`], and a PE file as an
//! [`AnyPe`]. It changes bytes so that a rule fails, asserts that each read then returns an
//! error, and prints `ok`.
#![forbid(unsafe_code)]

use core::marker::PhantomData;

use puns::{
  Error, Pun, from_bytes, from_head,
  num::{u8c, u16be, u16le, u16lec, u32be, u32bec, u32le, u32lec, u64be, u64le},
  try_pun_ref,
};

// -------------------------------------------------------------------------------------------------
// ELF

/// An ELF file, with the integer and header types that its type parameters give.
///
/// The type parameters are the following:
///
/// - `C` and `D` are the class and the data (byte order) of the identification, each `u8c<1>`
///   or `u8c<2>`.
/// - `H`, `W` and `A` are the integers of a half, a word and an address.
/// - `P` and `S` are the program header and the section header.
///
/// The class and the byte order change the size and the byte order of every field after them.
/// The class also changes the order of the fields of a program header. A field cannot choose its
/// type from another field. So each of the four combinations is an instance of this struct
/// ([`Elf64Le`] and the others), and an enum of references ([`AnyElf`]) chooses among them.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(e_version == 1))]
#[pun(assert(e_ehsize == if class == 2 { 64 } else { 52 }))]
#[pun(assert(e_phnum == 0 || e_phentsize == size_of::<P>()))]
#[pun(assert(e_shnum == 0 || e_shentsize == size_of::<S>()))]
#[pun(assert(e_shnum == 0 || e_shstrndx < e_shnum))]
// Program headers after the header, section headers after them.
#[pun(assert(e_phnum == 0 || e_phoff >= e_ehsize))]
#[pun(assert(e_shnum == 0 || e_shoff >= e_phoff + e_phnum * e_phentsize))]
#[pun(get(is_executable: bool = e_type == 2, is_shared: bool = e_type == 3))]
pub struct Elf<C, D, H: Copy, W, A, P, S> {
  pub magic: u32bec<0x7f45_4c46>,
  pub class: C,
  pub data: D,
  pub ident_version: u8c<1>,
  pub os_abi: u8,
  pub abi_version: u8,
  pub padding: [u8; 7],
  /// The file type: none, relocatable, executable, shared or core, or a type specific to an OS
  /// or a processor.
  #[pun(valid(0..=4, 0xfe00..))]
  pub e_type: H,
  pub e_machine: H,
  pub e_version: W,
  pub e_entry: A,
  pub e_phoff: A,
  pub e_shoff: A,
  pub e_flags: W,
  pub e_ehsize: H,
  pub e_phentsize: H,
  pub e_phnum: H,
  pub e_shentsize: H,
  pub e_shnum: H,
  pub e_shstrndx: H,
  /// A marker for `P` and `S`, which only the regions name.
  pub header_types: PhantomData<(P, S)>,
  #[pun(regions(
    before_program_headers = if e_phnum == 0 { 0 } else { e_phoff - e_ehsize },
    program_headers: [P] = e_phnum * e_phentsize,
    before_section_headers = if e_shnum == 0 {
      0
    } else {
      e_shoff
        - e_ehsize
        - (if e_phnum == 0 { 0 } else { e_phoff - e_ehsize })
        - e_phnum * e_phentsize
    },
    section_headers: [S] = e_shnum * e_shentsize,
    rest,
  ))]
  pub tail: [u8],
}

/// An ELF program header of 64 bits.
///
/// A loadable segment (`p_type` 1) takes no more bytes in the file than in memory. The alignment
/// is 0 or a power of two.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
#[pun(assert(p_filesz <= p_memsz || p_type != 1))]
#[pun(assert(p_align == 0 || p_align & (p_align - 1) == 0))]
pub struct Phdr64<W, X> {
  pub p_type: W,
  pub p_flags: W,
  pub p_offset: X,
  pub p_vaddr: X,
  pub p_paddr: X,
  pub p_filesz: X,
  pub p_memsz: X,
  pub p_align: X,
}

/// An ELF program header of 32 bits.
///
/// Its fields are in another order than those of [`Phdr64`]. Its rules are the same.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
#[pun(assert(p_filesz <= p_memsz || p_type != 1))]
#[pun(assert(p_align == 0 || p_align & (p_align - 1) == 0))]
pub struct Phdr32<W> {
  pub p_type: W,
  pub p_offset: W,
  pub p_vaddr: W,
  pub p_paddr: W,
  pub p_filesz: W,
  pub p_memsz: W,
  pub p_flags: W,
  pub p_align: W,
}

/// An ELF section header.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
pub struct Shdr<W, X> {
  pub sh_name: W,
  pub sh_type: W,
  pub sh_flags: X,
  pub sh_addr: X,
  pub sh_offset: X,
  pub sh_size: X,
  pub sh_link: W,
  pub sh_info: W,
  pub sh_addralign: X,
  pub sh_entsize: X,
}

/// An ELF file of 64 bits, little-endian.
pub type Elf64Le =
  Elf<u8c<2>, u8c<1>, u16le, u32le, u64le, Phdr64<u32le, u64le>, Shdr<u32le, u64le>>;
/// An ELF file of 64 bits, big-endian.
pub type Elf64Be =
  Elf<u8c<2>, u8c<2>, u16be, u32be, u64be, Phdr64<u32be, u64be>, Shdr<u32be, u64be>>;
/// An ELF file of 32 bits, little-endian.
pub type Elf32Le = Elf<u8c<1>, u8c<1>, u16le, u32le, u32le, Phdr32<u32le>, Shdr<u32le, u32le>>;
/// An ELF file of 32 bits, big-endian.
pub type Elf32Be = Elf<u8c<1>, u8c<2>, u16be, u32be, u32be, Phdr32<u32be>, Shdr<u32be, u32be>>;

/// An ELF file, of any class and byte order.
#[derive(Pun)]
pub enum AnyElf<'a> {
  Le64(&'a Elf64Le),
  Be64(&'a Elf64Be),
  Le32(&'a Elf32Le),
  Be32(&'a Elf32Be),
}

// -------------------------------------------------------------------------------------------------
// PE/COFF

/// A PE file: a DOS header, the DOS stub, then the headers `O`.
///
/// The field `e_lfanew` of the DOS header, at least 64 and a multiple of 8, says where the headers
/// are. The magic of the optional header is `MAGIC`, which says which optional header `O` holds.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(e_lfanew >= 64 && e_lfanew % 8 == 0))]
// The magic of the optional header, at a distance, says which `O` this is.
#[pun(assert(le(e_lfanew + 24, 2) == MAGIC))]
pub struct Pe<O: ?Sized, const MAGIC: u16> {
  pub e_magic: u16lec<0x5a4d>,
  pub dos: [u16le; 29],
  pub e_lfanew: u32le,
  pub headers_type: PhantomData<O>,
  #[pun(regions(stub = e_lfanew - 64, headers: O))]
  pub rest: [u8],
}

/// The PE headers: the PE signature, the COFF header, an optional header `H`, its data
/// directories, the section table, then the rest of the file.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(size_of_optional_header >= size_of::<H>()))]
#[pun(get(dll: bool = characteristics & 0x2000 != 0, executable: bool = characteristics & 2 != 0))]
pub struct Headers<H> {
  pub signature: u32lec<0x4550>,
  /// The machine: x86, x86-64, ARM64, ARM or ARM Thumb-2.
  #[pun(valid(0x14c, 0x8664, 0xaa64, 0x1c0, 0x1c4))]
  pub machine: u16le,
  #[pun(valid(1..=96))]
  pub number_of_sections: u16le,
  pub time_date_stamp: u32le,
  pub pointer_to_symbol_table: u32le,
  pub number_of_symbols: u32le,
  pub size_of_optional_header: u16le,
  pub characteristics: u16le,
  pub optional: H,
  #[pun(regions(
    data_directories: [DataDirectory] = size_of_optional_header - size_of::<H>(),
    sections: [SectionHeader] = number_of_sections * 40,
    rest,
  ))]
  pub rest: [u8],
}

/// The standard and Windows fields of the optional header of PE32+ (without its data
/// directories).
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
#[pun(assert(section_alignment >= file_alignment
  && file_alignment >= 512
  && file_alignment & (file_alignment - 1) == 0))]
#[pun(assert(size_of_image % section_alignment == 0))]
pub struct Optional64 {
  pub magic: u16lec<0x20b>,
  pub linker: [u8; 2],
  pub size_of_code: u32le,
  pub size_of_initialized_data: u32le,
  pub size_of_uninitialized_data: u32le,
  pub address_of_entry_point: u32le,
  pub base_of_code: u32le,
  pub image_base: u64le,
  pub section_alignment: u32le,
  pub file_alignment: u32le,
  pub versions: [u16le; 6],
  pub win32_version: u32lec<0>,
  pub size_of_image: u32le,
  pub size_of_headers: u32le,
  pub checksum: u32le,
  #[pun(valid(1..=3, 9..=14, 16))]
  pub subsystem: u16le,
  pub dll_characteristics: u16le,
  pub stack_reserve: u64le,
  pub stack_commit: u64le,
  pub heap_reserve: u64le,
  pub heap_commit: u64le,
  pub loader_flags: u32lec<0>,
  #[pun(valid(..=16))]
  pub number_of_rva_and_sizes: u32le,
}

/// The standard and Windows fields of the optional header of PE32, without its data
/// directories.
///
/// It differs from [`Optional64`] by its magic, a `base_of_data` field and sizes of 32 bits.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
pub struct Optional32 {
  pub magic: u16lec<0x10b>,
  pub linker: [u8; 2],
  pub sizes: [u32le; 3],
  pub address_of_entry_point: u32le,
  pub base_of_code: u32le,
  pub base_of_data: u32le,
  pub image_base: u32le,
  pub section_alignment: u32le,
  pub file_alignment: u32le,
  pub versions: [u16le; 6],
  pub win32_version: u32lec<0>,
  pub size_of_image: u32le,
  pub size_of_headers: u32le,
  pub checksum: u32le,
  pub subsystem: u16le,
  pub dll_characteristics: u16le,
  pub reserves: [u32le; 4],
  pub loader_flags: u32lec<0>,
  #[pun(valid(..=16))]
  pub number_of_rva_and_sizes: u32le,
}

/// A PE data directory: the address of a table once loaded, and its size.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
pub struct DataDirectory {
  pub virtual_address: u32le,
  pub size: u32le,
}

/// A PE section header.
///
/// Its raw data, if any, is at an offset in the file that is a multiple of 512.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
#[pun(assert(size_of_raw_data == 0 || pointer_to_raw_data % 512 == 0))]
pub struct SectionHeader {
  pub name: [u8; 8],
  pub virtual_size: u32le,
  pub virtual_address: u32le,
  pub size_of_raw_data: u32le,
  pub pointer_to_raw_data: u32le,
  pub pointer_to_relocations: u32le,
  pub pointer_to_line_numbers: u32le,
  pub number_of_relocations: u16le,
  pub number_of_line_numbers: u16le,
  pub characteristics: u32le,
}

/// A PE32+ file (64 bits).
pub type Pe64 = Pe<Headers<Optional64>, 0x20b>;
/// A PE32 file (32 bits).
pub type Pe32 = Pe<Headers<Optional32>, 0x10b>;

/// A PE file of either kind, PE32+ or PE32.
#[derive(Pun)]
pub enum AnyPe<'a> {
  Pe64(&'a Pe64),
  Pe32(&'a Pe32),
}

// -------------------------------------------------------------------------------------------------
// Mach-O

/// A Mach-O file of 64 bits, little-endian.
///
/// Its load commands fill `sizeofcmds` bytes, and their number is `ncmds`. The size
/// of each command is a multiple of 8, and at least 8.
#[derive(Pun)]
#[repr(C)]
// A walk whose state is the offset of a command (the low 32 bits) and the number of commands
// before it (the high bits).
#[pun(assert({
  let end = walk(
    32,
    |s| s >> 32 < ncmds,
    |s| {
      let at = s & 0xffff_ffff;
      let size = le(at + 4, 4);
      if size < 8 || size % 8 != 0 { 1 / 0 } else { (at + size) + (((s >> 32) + 1) << 32) }
    },
  );
  end & 0xffff_ffff == 32 + sizeofcmds
}))]
pub struct MachO64 {
  pub magic: u32lec<0xfeed_facf>,
  pub cputype: u32le,
  pub cpusubtype: u32le,
  #[pun(valid(1..=12))]
  pub filetype: u32le,
  pub ncmds: u32le,
  pub sizeofcmds: u32le,
  pub flags: u32le,
  pub reserved: u32le,
  #[pun(regions(commands = sizeofcmds, rest))]
  pub tail: [u8],
}

/// A Mach-O load command of any kind.
#[derive(Pun)]
#[repr(C)]
#[pun(size = cmdsize)]
pub struct LoadCommand {
  pub cmd: u32le,
  #[pun(valid(8..))]
  pub cmdsize: u32le,
  pub data: [u8],
}

/// A Mach-O `LC_SEGMENT_64` command and its sections.
///
/// Its size counts its sections, and it takes no more bytes in the file than in memory.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(cmdsize == 72 + nsects * 80))]
#[pun(assert(filesize <= vmsize))]
pub struct Segment64 {
  pub cmd: u32lec<0x19>,
  pub cmdsize: u32le,
  pub segname: [u8; 16],
  pub vmaddr: u64le,
  pub vmsize: u64le,
  pub fileoff: u64le,
  pub filesize: u64le,
  pub maxprot: u32le,
  pub initprot: u32le,
  pub nsects: u32le,
  pub flags: u32le,
  #[pun(len = nsects)]
  pub sections: [Section64],
}

/// A Mach-O section of a segment.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
pub struct Section64 {
  pub sectname: [u8; 16],
  pub segname: [u8; 16],
  pub addr: u64le,
  pub size: u64le,
  pub offset: u32le,
  /// The alignment, as the log2 of a power of two, at most 15.
  #[pun(valid(0..=15))]
  pub align: u32le,
  pub reloff: u32le,
  pub nreloc: u32le,
  pub flags: u32le,
  pub reserved: [u32le; 3],
}

// -------------------------------------------------------------------------------------------------
// Java class files

/// The size of a constant pool entry for each tag, 4 bits per tag.
///
/// A size of 0 marks a value that is not a tag. A size of 1 marks `CONSTANT_Utf8`, whose size is
/// in the entry. The sizes are a table in a constant because an `if` chain over the 17 tags
/// would be deeper than the stack of an expression.
pub const ENTRY_SIZES: u128 = 0x3_3553_4005_5553_3995_5010;

/// A Java class file: the constant pool, then the class.
#[derive(Pun)]
#[repr(C)]
// The constant pool: a walk whose state is the offset of an entry (the low 32 bits) and its
// index (the high bits); `CONSTANT_Long` and `CONSTANT_Double` take two indices. What follows
// the pool is reached from its end: the access flags, this class, the super class, the
// interfaces.
#[pun(assert({
  let end = walk(
    10 + (1 << 32),
    |s| s >> 32 < constant_pool_count,
    |s| {
      let at = s & 0xffff_ffff;
      let tag = be(at, 1);
      let size = (ENTRY_SIZES >> (tag * 4)) & 15;
      if size == 0 {
        1 / 0
      } else {
        s + (if size == 1 { 3 + be(at + 1, 2) } else { size })
          + (if tag == 5 || tag == 6 { 0x2_0000_0000 } else { 0x1_0000_0000 })
      }
    },
  );
  let at = end & 0xffff_ffff;
  end >> 32 == constant_pool_count
    && be(at + 2, 2) >= 1 && be(at + 2, 2) < constant_pool_count
    && be(at + 4, 2) < constant_pool_count
    && at + 8 + 2 * be(at + 6, 2) <= size_of_val(self)
    // An interface is abstract.
    && (be(at, 2) & 0x0200 == 0 || be(at, 2) & 0x0400 != 0)
}))]
pub struct Class {
  pub magic: u32bec<0xcafe_babe>,
  pub minor: u16be,
  /// The major version, from Java 1.1 (45) to Java 26 (70).
  #[pun(valid(45..=70))]
  pub major: u16be,
  #[pun(valid(1..))]
  pub constant_pool_count: u16be,
  pub rest: [u8],
}

/// A `CONSTANT_Utf8` entry of a Java constant pool.
///
/// Its text is "modified UTF-8", which is not a `str` (see the documentation of this module).
#[derive(Pun)]
#[repr(C)]
#[pun(size = length + 3)]
pub struct Utf8Entry {
  pub tag: u8c<1>,
  pub length: u16be,
  pub bytes: [u8],
}

// -------------------------------------------------------------------------------------------------
// WebAssembly

/// The rank of each section id in the order of a WebAssembly module, 4 bits per id, from id 0.
///
/// The order is type, import, function, table, memory, tag (13), global, export, start, element,
/// data count (12), code, data. Custom sections (0) may be anywhere.
pub const RANKS: u128 = 0x6b_dca9_8754_3210;

/// A WebAssembly module: a magic, a version, then sections.
///
/// The sections fill the module. Each has a known id, and they are in the order of [`RANKS`].
/// The state of the walk is the offset of a section, and the rank of the last section from bit
/// 40.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(walk(
  8,
  |s| s & 0xffff_ffff < size_of_val(self),
  |s| {
    let at = s & 0xffff_ffff;
    let id = be(at, 1);
    let rank = (RANKS >> (id * 4)) & 15;
    let next = at + 1 + leb128_len(at + 1) + leb128(at + 1);
    if id > 13 || leb128_len(at + 1) > 5 {
      1 / 0
    } else if id == 0 {
      next | (s >> 40 << 40)
    } else if rank > s >> 40 {
      next | (rank << 40)
    } else {
      1 / 0
    }
  },
) & 0xffff_ffff == size_of_val(self)))]
pub struct Module {
  pub magic: u32bec<0x0061_736d>,
  pub version: u32lec<1>,
  pub sections: [u8],
}

/// A WebAssembly section: an id, a size in LEB128, then that many bytes.
///
/// The size takes at most 5 bytes, the most that a `u32` takes in LEB128.
#[derive(Pun)]
#[repr(C)]
#[pun(size = 1 + leb128_len(1) + leb128(1))]
#[pun(assert(leb128_len(1) <= 5))]
pub struct Section {
  #[pun(valid(0..=13))]
  pub id: u8,
  #[pun(regions(size = leb128_len(1), payload))]
  pub rest: [u8],
}

/// A function type of the WebAssembly type section, whose count of parameters takes 1 byte of
/// LEB128.
#[derive(Pun)]
#[repr(C)]
pub struct FuncType {
  pub form: u8c<0x60>,
  #[pun(valid(..0x80))]
  pub params: u8,
  #[pun(len = params)]
  pub types: [ValueType],
}

/// A WebAssembly value type.
#[derive(Pun, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ValueType {
  I32 = 0x7f,
  I64 = 0x7e,
  F32 = 0x7d,
  F64 = 0x7c,
  V128 = 0x7b,
  FuncRef = 0x70,
  ExternRef = 0x6f,
}

// -------------------------------------------------------------------------------------------------
// Use

/// The bytes of an ELF executable for x86-64, of 64 bits, little-endian.
///
/// It holds a header, a program header (`PT_LOAD`), 8 bytes of code and two section headers.
const ELF64_LE: [u8; 256] = [
  0x7f, 0x45, 0x4c, 0x46, 0x02, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x02, 0x00, 0x3e, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x10, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x38, 0x00, 0x01, 0x00, 0x40, 0x00, 0x02, 0x00, 0x01, 0x00,
  0x01, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x78, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x10, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// The bytes of an ELF executable of 64 bits, big-endian.
///
/// It holds a header, a program header (`PT_LOAD`), 8 bytes of code and two section headers.
const ELF64_BE: [u8; 256] = [
  0x7f, 0x45, 0x4c, 0x46, 0x02, 0x02, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x02, 0x00, 0x3e, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x10, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x38, 0x00, 0x01, 0x00, 0x40, 0x00, 0x02, 0x00, 0x01,
  0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x78,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x10, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// The bytes of an ELF executable of 32 bits, little-endian.
///
/// It holds a header, a program header (`PT_LOAD`), 8 bytes of code and two section headers.
const ELF32_LE: [u8; 172] = [
  0x7f, 0x45, 0x4c, 0x46, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x02, 0x00, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x10, 0x40, 0x00, 0x34, 0x00, 0x00, 0x00,
  0x5c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x34, 0x00, 0x20, 0x00, 0x01, 0x00, 0x28, 0x00,
  0x02, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x54, 0x00, 0x00, 0x00, 0x00, 0x10, 0x40, 0x00,
  0x00, 0x10, 0x40, 0x00, 0x08, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00,
  0x00, 0x10, 0x00, 0x00, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// The bytes of an ELF executable of 32 bits, big-endian.
///
/// It holds a header, a program header (`PT_LOAD`), 8 bytes of code and two section headers.
const ELF32_BE: [u8; 172] = [
  0x7f, 0x45, 0x4c, 0x46, 0x01, 0x02, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x02, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x40, 0x10, 0x00, 0x00, 0x00, 0x00, 0x34,
  0x00, 0x00, 0x00, 0x5c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x34, 0x00, 0x20, 0x00, 0x01, 0x00, 0x28,
  0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x54, 0x00, 0x40, 0x10, 0x00,
  0x00, 0x40, 0x10, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x05,
  0x00, 0x00, 0x10, 0x00, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
];

/// The bytes of a PE32+ executable: a DOS header, the PE headers, two data directories and a
/// section header.
const PE64: [u8; 320] = [
  0x4d, 0x5a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x50, 0x45, 0x00, 0x00, 0x64, 0x86, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x80, 0x00, 0x22, 0x00, 0x0b, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x40, 0x01, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x30, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'.', b't', b'e', b'x', b't', 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// The bytes of a Mach-O executable: a segment `__TEXT` of one section, then an `LC_UUID`-like
/// command of 16 bytes.
const MACHO: [u8; 200] = [
  0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00,
  0x02, 0x00, 0x00, 0x00, 0xa8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x19, 0x00, 0x00, 0x00, 0x98, 0x00, 0x00, 0x00, b'_', b'_', b'T', b'E', b'X', b'T', 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
  0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00,
  0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'_', b'_', b't', b'e', b'x', b't', 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, b'_', b'_', b'T', b'E', b'X', b'T', 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1b, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00,
  0xab, 0xab, 0xab, 0xab, 0xab, 0xab, 0xab, 0xab,
];

/// The bytes of a Java class file: a constant pool of 5 entries in 6 slots, since a `long` takes
/// two, then the class.
const CLASS: [u8; 66] = [
  0xca, 0xfe, 0xba, 0xbe, 0x00, 0x00, 0x00, 0x34, 0x00, 0x07, 0x01, 0x00, 0x05, b'H', b'e', b'l',
  b'l', b'o', 0x07, 0x00, 0x01, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2a, 0x01, 0x00,
  0x10, b'j', b'a', b'v', b'a', b'/', b'l', b'a', b'n', b'g', b'/', b'O', b'b', b'j', b'e', b'c',
  b't', 0x07, 0x00, 0x05, 0x00, 0x21, 0x00, 0x02, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00,
];

/// The bytes of the start of a WebAssembly module: a type section, a custom section "name" and a
/// function section.
///
/// A code section of 200 bytes follows, of which only the id and the size are here.
const WASM: [u8; 31] = [
  0x00, b'a', b's', b'm', 0x01, 0x00, 0x00, 0x00, 1, 7, 1, 0x60, 2, 0x7f, 0x7f, 1, 0x7f, 0, 5, 4,
  b'n', b'a', b'm', b'e', 3, 2, 1, 0, 10, 0xc8, 0x01,
];

/// Reads the samples of this file through its types, and asserts the results.
pub fn main() {
  // ---- ELF: four kinds of files, one declaration.
  for bytes in [&ELF64_LE[..], &ELF64_BE, &ELF32_LE, &ELF32_BE] {
    let (kind, entry, load, code) = match AnyElf::from_bytes(bytes).unwrap() {
      AnyElf::Le64(elf) => {
        assert!(elf.is_executable() && !elf.is_shared());
        assert_eq!(elf.section_headers().unwrap()[1].sh_type.get(), 3);
        let load = elf.program_headers().unwrap()[0];
        (
          "64 LE",
          elf.e_entry.get(),
          (load.p_flags.get(), load.p_filesz.get()),
          elf.before_section_headers(),
        )
      }
      AnyElf::Be64(elf) => {
        let load = elf.program_headers().unwrap()[0];
        (
          "64 BE",
          elf.e_entry.get(),
          (load.p_flags.get(), load.p_filesz.get()),
          elf.before_section_headers(),
        )
      }
      AnyElf::Le32(elf) => {
        let load = elf.program_headers().unwrap()[0];
        let flags = (load.p_flags.get(), u64::from(load.p_filesz.get()));
        ("32 LE", u64::from(elf.e_entry.get()), flags, elf.before_section_headers())
      }
      AnyElf::Be32(elf) => {
        let load = elf.program_headers().unwrap()[0];
        let flags = (load.p_flags.get(), u64::from(load.p_filesz.get()));
        ("32 BE", u64::from(elf.e_entry.get()), flags, elf.before_section_headers())
      }
    };
    assert_eq!((entry, load, code), (0x40_1000, (5, 8), &[0x90; 8][..]));
    assert_eq!(
      kind,
      match bytes[4..6] {
        [2, 1] => "64 LE",
        [2, 2] => "64 BE",
        [1, 1] => "32 LE",
        _ => "32 BE",
      }
    );
  }
  // The magic, the class, the byte order, the version of the identification, the type, the
  // version, the size of the header, the size of a program header, the number of sections.
  for (at, value) in
    [(0, 0x7e), (4, 3), (5, 0), (6, 2), (16, 5), (20, 2), (52, 63), (54, 55), (60, 3)]
  {
    let mut bytes = ELF64_LE;
    bytes[at] = value;
    assert!(matches!(AnyElf::from_bytes(&bytes), Err(Error::Validity { .. })));
  }
  // A program header of an alignment that is not a power of two: a typed region, checked when
  // it is read.
  let mut bytes = ELF64_LE;
  bytes[64 + 48] = 3;
  let elf: &Elf64Le = try_pun_ref(&bytes[..]).unwrap();
  assert!(matches!(elf.program_headers(), Err(Error::Validity { .. })));
  // Section headers out of the file.
  let mut bytes = ELF64_LE;
  bytes[40] = 0xf0;
  assert!(matches!(AnyElf::from_bytes(&bytes), Err(Error::Validity { .. })));

  // ---- PE: headers where the DOS header says, an optional header of the kind its magic gives.
  let Ok(AnyPe::Pe64(pe)) = AnyPe::from_bytes(&PE64) else { panic!() };
  assert_eq!((pe.e_lfanew.get(), pe.stub().len()), (0x80, 64));
  let headers = pe.headers().unwrap();
  assert_eq!((headers.machine.get(), headers.executable(), headers.dll()), (0x8664, true, false));
  let optional = &headers.optional;
  assert_eq!(
    (optional.address_of_entry_point.get(), optional.image_base.get()),
    (0x1000, 0x1_4000_0000)
  );
  assert_eq!((optional.subsystem.get(), headers.data_directories().unwrap().len()), (3, 2));
  let sections = headers.sections().unwrap();
  assert_eq!(
    (&sections[0].name[..5], sections[0].pointer_to_raw_data.get()),
    (&b".text"[..], 0x400)
  );
  assert!(try_pun_ref::<_, Pe32>(&PE64[..]).is_err());
  // The DOS magic, `e_lfanew`, the PE signature, the machine, the number of sections, the magic
  // of the optional header.
  for (at, value) in [(0, b'N'), (0x3c, 0x81), (0x80, b'Q'), (0x84, 0x4d), (0x86, 0), (0x98, 0x0c)]
  {
    let mut bytes = PE64;
    bytes[at] = value;
    let rejected = match AnyPe::from_bytes(&bytes) {
      Err(_) => true,
      Ok(AnyPe::Pe64(pe)) => pe.headers().is_err(),
      Ok(AnyPe::Pe32(pe)) => pe.headers().is_err(),
    };
    assert!(rejected);
  }
  // A file alignment that is not a power of two: the headers are a typed region.
  let mut bytes = PE64;
  bytes[0x98 + 36..0x98 + 38].copy_from_slice(&[0x01, 0x03]);
  let Ok(AnyPe::Pe64(pe)) = AnyPe::from_bytes(&bytes) else { panic!() };
  assert!(matches!(pe.headers(), Err(Error::Validity { .. })));

  // ---- Mach-O: load commands that fill `sizeofcmds` bytes.
  let file: &MachO64 = try_pun_ref(&MACHO[..]).unwrap();
  assert_eq!((file.ncmds.get(), file.commands().len(), file.rest().len()), (2, 168, 0));
  let (command, rest) = from_head::<LoadCommand>(file.commands()).unwrap();
  assert_eq!(command.cmd.get(), 0x19);
  let segment: &Segment64 = try_pun_ref(&file.commands()[..152]).unwrap();
  assert_eq!(
    (&segment.segname[..6], segment.nsects.get(), segment.sections[0].align.get()),
    (&b"__TEXT"[..], 1, 4)
  );
  let (command, rest) = from_head::<LoadCommand>(rest).unwrap();
  assert_eq!((command.cmd.get(), command.data.len(), rest.len()), (0x1b, 8, 0));
  // A count of commands that is not theirs; a command size that is not a multiple of 8; a
  // `sizeofcmds` larger than the commands.
  for (at, value) in [(16, 3), (188, 20), (20, 0xf8)] {
    let mut bytes = MACHO;
    bytes[at] = value;
    assert!(matches!(try_pun_ref::<_, MachO64>(&bytes[..]), Err(Error::Validity { .. })));
  }
  // A segment whose size does not count its sections.
  let mut bytes = MACHO;
  bytes[32 + 4] = 160;
  assert!(matches!(try_pun_ref::<_, Segment64>(&bytes[32..32 + 152]), Err(Error::Validity { .. })));

  // ---- Java: a constant pool of entries of their own sizes.
  let class: &Class = try_pun_ref(&CLASS[..]).unwrap();
  assert_eq!((class.major.get(), class.constant_pool_count.get()), (52, 7));
  let (utf8, _) = from_head::<Utf8Entry>(&class.rest).unwrap();
  assert_eq!(&utf8.bytes, b"Hello");
  // A count that the entries do not reach; a tag that is not one; this class out of the pool; a
  // `CONSTANT_Long` counted as one entry; a major version above 70.
  for (at, value) in [(9, 8), (10, 2), (CLASS.len() - 11, 9), (9, 6), (7, 71)] {
    let mut bytes = CLASS;
    bytes[at] = value;
    assert!(matches!(try_pun_ref::<_, Class>(&bytes[..]), Err(Error::Validity { .. })));
  }

  // ---- WebAssembly: sections of a size in LEB128, in order.
  let mut module = WASM.to_vec();
  module.resize(31 + 200, 0);
  let wasm: &Module = try_pun_ref(&module[..]).unwrap();
  let (mut rest, mut sections) = (&wasm.sections[..], vec![]);
  while let Ok((section, next)) = from_head::<Section>(rest) {
    sections.push((section.id, section.payload().len()));
    rest = next;
  }
  assert_eq!((sections, rest.len()), (vec![(1, 7), (0, 5), (3, 2), (10, 200)], 0));
  let (types, _) = from_head::<Section>(&wasm.sections).unwrap();
  let (function, _) = from_head::<FuncType>(&types.payload()[1..]).unwrap();
  assert_eq!(function.types, [ValueType::I32, ValueType::I32]);
  assert!(matches!(from_head::<FuncType>(&[0x60, 1, 0x7a]), Err(Error::Validity { .. })));
  // An id that is not one; a size past the end; sections out of order.
  for (at, value) in [(8, 14), (29, 0xc9), (24, 1)] {
    let mut bytes = module.clone();
    bytes[at] = value;
    assert!(matches!(try_pun_ref::<_, Module>(&bytes[..]), Err(Error::Validity { .. })));
  }
  // A section of 16 KiB: 3 bytes of LEB128.
  let mut large = [&WASM[..8], &[0, 0x80, 0x80, 0x01]].concat();
  large.resize(12 + 16384, 0);
  let wasm: &Module = try_pun_ref(&large[..]).unwrap();
  assert_eq!(
    from_bytes::<Section>(&wasm.sections).map(|section| section.payload().len()),
    Ok(16384)
  );

  println!("ok");
}
