//! Tests uninitialized bytes: `MaybeUninit<T>`, scalars that a source may hold uninitialized but
//! that a target requires to hold given values (`Spec::maybe_uninit`), and unions.

use core::mem::MaybeUninit;

use puns::{
  Error, Scalar,
  layout::{Endian, Spec},
};

/// A byte that a target requires to be 0, and that a source may hold uninitialized. A byte of a
/// tag that may be padding is such a byte.
#[derive(Clone, Copy)]
#[repr(transparent)]
struct Zero(MaybeUninit<u8>);

// SAFETY: `Zero` is `repr(transparent)` over `MaybeUninit<u8>`, so `SPEC`, of a `u8`, has its
// size. The items of the `# Safety` of `Scalar` hold:
// - `SPEC` is made with `Spec::maybe_uninit`, so a value of `Zero` may hold any byte,
//   uninitialized included.
// - `SPEC` accepts 0, which is a valid `MaybeUninit<u8>`. `Zero` has no other invariant.
// - `Zero` holds no `UnsafeCell`.
unsafe impl Scalar for Zero {
  const SPEC: Spec = Spec::ranges::<u8>(Endian::Little, &[(0, 0)]).maybe_uninit();
}

#[test]
fn maybe_uninit_takes_anything() {
  // Any value, padding included, is a `MaybeUninit` of its size: by value, by reference.
  let value: MaybeUninit<u32> = puns::pun(7u32);
  // SAFETY: `value` holds the bytes of `7u32`, which are an initialized `u32`, as `assume_init`
  // requires.
  assert_eq!(unsafe { value.assume_init() }, 7);
  let padded = (1u8, 2u16);
  let erased: &MaybeUninit<(u8, u16)> = puns::pun_ref(&padded);
  // SAFETY: `erased` points to `padded`, a valid `(u8, u16)`, as `assume_init_ref` requires.
  assert_eq!(unsafe { erased.assume_init_ref() }, &(1, 2));
  let _: &MaybeUninit<[u8; 4]> = puns::pun_ref(&padded);
  // Between two `MaybeUninit`s, by `&mut` too: anything written through one is the other.
  let mut words = MaybeUninit::<u32>::new(0x0102_0304);
  let bytes: &mut MaybeUninit<[u8; 4]> = puns::pun_mut(&mut words);
  bytes.write([0; 4]);
  // SAFETY: `words` was made of a `u32`, then its 4 bytes were written with zeros through
  // `bytes`. They are an initialized `u32`, as `assume_init` requires.
  assert_eq!(unsafe { words.assume_init() }, 0);
  // SAFETY: `zeroed` makes 2 bytes of zeros, an initialized `u16`, as `assume_init` requires.
  assert_eq!(unsafe { puns::zeroed::<MaybeUninit<u16>>().assume_init() }, 0);
}

#[test]
fn maybe_uninit_proves_nothing() {
  // Nothing proven of its bytes, a source of only what accepts uninitialized bytes (see the
  // compile-fail tests `maybe_uninit_*`).
  let _: MaybeUninit<u8> = puns::pun(MaybeUninit::<u8>::uninit());
  let _: [MaybeUninit<u8>; 2] = puns::pun(MaybeUninit::<u16>::uninit());
}

#[test]
fn maybe_uninit_scalar() {
  // As a target: required to be 0, checked from what is not known to be.
  assert_eq!(puns::try_pun::<u8, Zero>(0).map(|_| ()), Ok(()));
  assert_eq!(puns::try_pun::<u8, Zero>(1).map(|_| ()), Err(Error::Validity));
  assert!(puns::from_bytes::<Zero>(&[0]).is_ok());
  assert!(puns::from_bytes::<Zero>(&[3]).is_err());
  // Proven by what is known to be 0.
  let _: Zero = puns::pun(puns::num::u8c::<0>::new());
  // As a source: nothing that needs the byte, not even the same type (two types can have the
  // same `Spec` of this kind while one may be padding where the other may not).
  let _: MaybeUninit<u8> = puns::pun(Zero(MaybeUninit::uninit()));
}

/// A union of four bytes and a word.
#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
union Word {
  bytes: [u8; 4],
  word: u32,
}

/// A union of a `bool` and a number up to 3, which is a byte up to 3.
#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
union Choice {
  flag: bool,
  small: puns::num::u8in<0, 3>,
}

/// A union of a byte and of a pair of a byte and a `u16`. It requires its first byte to be
/// initialized, and nothing of its three other bytes.
#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
union Padded {
  byte: u8,
  pair: (u8, u16),
}

#[test]
fn unions() {
  // As a target, one of its fields: proven by one that every source is.
  let word: Word = puns::pun(0x0102_0304u32);
  // SAFETY: `word` holds the 4 bytes of a `u32`, at offset 0, which are a valid `u32`. A read of
  // the field `word` reads them as a `u32`.
  assert_eq!(unsafe { word.word }, 0x0102_0304);
  let _: &Word = puns::pun_ref(&7u32);
  let flag: Choice = puns::pun(true);
  // SAFETY: `flag` holds the byte of `true` at offset 0, which is a valid `bool`. A read of the
  // field `flag` reads it as a `bool`.
  assert!(unsafe { flag.flag });
  let _: Choice = puns::pun(puns::num::u8in::<1, 2>::new(1).unwrap());
  // Checked, field by field, when no field is proven: 2 is no `bool`, but is a `small`.
  assert!(puns::try_pun::<u8, Choice>(2).is_ok());
  assert_eq!(puns::try_pun::<u8, Choice>(4).map(|_| ()), Err(Error::Validity));
  // Its first field takes one byte: the rest may be padding in the source.
  let _: Padded = puns::pun((1u8, 2u16));
  // As a source, nothing: only what takes anything.
  let _: MaybeUninit<u32> = puns::pun(word);
  let _: Word = puns::pun(word);
}

/// Checks `packed` and `packed(N)` unions, whose fields may be at addresses that are not aligned
/// for their types. `packed` lowers the alignment of a union, and may lower its size, but its
/// fields stay at offset 0. The checks read bytes at any address, and a field is read by copy.
#[test]
fn packed_unions() {
  use core::num::NonZero;

  use puns::{
    Pun,
    num::{u8c, u16lec},
  };

  /// Size 4, alignment 1. Valid if `nz` is not 0, or if each byte is 1.
  #[derive(Pun, Clone, Copy)]
  #[repr(C, packed)]
  union Packed {
    nz: NonZero<u32>,
    ones: [u8c<1>; 4],
  }
  /// Proven by its field `flags` from four `bool`s, at any address.
  #[derive(Pun, Clone, Copy)]
  #[repr(C, packed)]
  union Flags {
    nz: NonZero<u32>,
    flags: [bool; 4],
  }
  /// `packed(2)` with a field more aligned than the packing: size 8, alignment 2.
  #[derive(Pun, Clone, Copy)]
  #[repr(C, packed(2))]
  union Wide {
    wide: NonZero<u64>,
    sevens: [u16lec<7>; 4],
  }
  /// `Wide` at offset 2, so at an address that is not aligned for a `u64`.
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  struct Framed {
    tag: u16,
    wide: Wide,
  }
  /// A packed union in a packed struct, at offset 1.
  #[derive(Pun, Clone, Copy)]
  #[repr(C, packed)]
  struct Record {
    tag: u8,
    packed: Packed,
  }
  /// `packed(2)`: size 6 and alignment 2, where they would be 8 and 4. Its last byte is padding.
  #[derive(Pun, Clone, Copy)]
  #[repr(C, packed(2))]
  union Odd {
    bytes: [u8; 5],
    word: u32,
  }
  /// Bytes at an address that is a multiple of 8.
  #[repr(C, align(8))]
  struct Aligned([u8; 24]);

  assert_eq!((size_of::<Packed>(), align_of::<Packed>()), (Packed::LAYOUT.size(), 1));
  assert_eq!((size_of::<Wide>(), align_of::<Wide>()), (Wide::LAYOUT.size(), 2));
  assert_eq!((size_of::<Odd>(), align_of::<Odd>()), (6, Odd::LAYOUT.align()));
  // Proven: from four `bool`s, by reference at any address; from three `u16`s, the first five
  // bytes of which are `bytes`.
  let flags = [true, false, true, false, true];
  let flags: &Flags = puns::pun_ref::<[bool; 4], Flags>(flags[1..].first_chunk().unwrap());
  // SAFETY: `flags` holds four `bool`s.
  assert_eq!(unsafe { flags.flags }, [false, true, false, true]);
  let odd: Odd = puns::pun([0x0101_u16; 3]);
  // SAFETY: `odd` holds six bytes, which are a `[u8; 5]` at offset 0.
  assert_eq!(unsafe { odd.bytes }, [1; 5]);

  let mut buffer = Aligned([0; 24]);
  let bytes = &mut buffer.0;
  // Checked, at every address modulo 8: the `u32` and the `u64` of the fields are read where they
  // are not aligned.
  for at in 0..8 {
    bytes.fill(0);
    bytes[at + 3] = 9;
    let packed = puns::from_bytes::<Packed>(&bytes[at..at + 4]).unwrap();
    // SAFETY: the check found `nz` valid. Copying a field reads it at any address.
    assert_eq!(unsafe { packed.nz }.get().to_ne_bytes(), [0, 0, 0, 9]);
    bytes[at + 3] = 0;
    assert_eq!(puns::from_bytes::<Packed>(&bytes[at..at + 4]).err(), Some(Error::Validity));
    bytes[at..at + 4].fill(1);
    assert!(puns::from_bytes::<Packed>(&bytes[at..at + 4]).is_ok());

    // `[0x07, 0x00]` four times: both fields are valid, and the `u64` is read at `at + 2`.
    bytes.fill(0);
    for i in 0..4 {
      bytes[at + 2 + 2 * i] = 7;
    }
    let framed = puns::from_bytes::<Framed>(&bytes[at..at + 10]);
    assert_eq!(framed.is_ok(), at % 2 == 0, "{at}");
    if let Ok(framed) = framed {
      let wide = framed.wide;
      // SAFETY: the check found `wide` valid, as `sevens` is, since 7 is not 0.
      assert_eq!(unsafe { wide.wide }.get().to_ne_bytes(), [7, 0, 7, 0, 7, 0, 7, 0]);
    }
    // Neither field: not a `u64` other than 0, nor a 7.
    bytes[at + 2..at + 10].fill(0);
    let wrong = if at % 2 == 0 { Error::Validity } else { Error::Alignment };
    assert_eq!(puns::from_bytes::<Framed>(&bytes[at..at + 10]).err(), Some(wrong));
    bytes[at + 9] = 0x80;
    assert_eq!(puns::from_bytes::<Framed>(&bytes[at..at + 10]).is_ok(), at % 2 == 0);

    // In a packed struct, at offset 1, and in a slice, at offsets 1 and 6.
    bytes.fill(0);
    bytes[at + 1..at + 5].fill(1);
    bytes[at + 6..at + 10].fill(1);
    let records = puns::from_bytes::<[Record]>(&bytes[at..at + 10]).unwrap();
    let packed = records[1].packed;
    // SAFETY: the check found `ones` valid. It is read by copy.
    assert_eq!(unsafe { packed.ones }.map(u8c::get), [1; 4]);
    // Neither field of the second.
    bytes[at + 6..at + 10].fill(0);
    assert_eq!(puns::from_bytes::<[Record]>(&bytes[at..at + 10]).err(), Some(Error::Validity));
    // `nz` is valid, though `ones` is not.
    bytes[at + 9] = 2;
    assert!(puns::from_bytes::<[Record]>(&bytes[at..at + 10]).is_ok());
  }
}

/// Checks that nothing is read of a variant of an enum, or of a field of a union, that has no
/// value.
///
/// The analysis does not look at such a variant, since no value is one. So it does not require
/// the bytes of its fields to be initialized in the source, and the check must not read them.
#[test]
fn a_variant_without_values_is_not_read() {
  use core::convert::Infallible;

  use puns::{Pun, num::u8in};

  #[derive(Pun, Clone, Copy)]
  #[repr(u8)]
  enum Tagged {
    Unit,
    Never(bool, Infallible),
  }
  /// A struct without values, since one of its fields has none.
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  struct Absent {
    byte: u8,
    flag: bool,
    none: Infallible,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  union Either {
    absent: Absent,
    small: u8in<0, 3>,
  }
  /// A byte, then a byte that is not written.
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  struct Source {
    byte: u8,
    rest: MaybeUninit<u8>,
  }

  let source = |byte| Source { byte, rest: MaybeUninit::uninit() };
  // The tag of `Never`: no value, whatever its `bool` would be, which is not read.
  assert!(puns::try_pun::<Source, Tagged>(source(0)).is_ok());
  assert_eq!(puns::try_pun::<Source, Tagged>(source(1)).err(), Some(Error::Validity));
  assert_eq!(puns::try_pun::<Source, Tagged>(source(2)).err(), Some(Error::Validity));
  // The field `absent` is first, and its `bool` is at the byte that is not written.
  assert!(puns::try_pun::<Source, Either>(source(3)).is_ok());
  assert_eq!(puns::try_pun::<Source, Either>(source(4)).err(), Some(Error::Validity));
}

/// Checks that a variant of a source that holds a union none of whose fields has values is related
/// as any other, since a value of it may exist.
#[test]
fn a_union_of_a_source_may_hold_a_value() {
  use core::convert::Infallible;

  use puns::Pun;

  /// A union none of whose fields has values.
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  union Hollow {
    a: Infallible,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(u8)]
  enum Source5 {
    A(Hollow) = 0,
    B(u8) = 1,
    C = 2,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(u8)]
  enum Plain5 {
    A(Infallible) = 0,
    B(u8) = 1,
    C = 2,
  }
  #[derive(Pun, Clone, Copy, Debug, PartialEq, Eq)]
  #[repr(u8)]
  enum Target5 {
    B(u8) = 1,
    C = 2,
  }

  // SAFETY: The language has not settled the validity of unions. The compiler and Miri accept any
  // bytes as a union today, so this value is accepted, though no field of it has a value. The test
  // shows that `puns` is sound under that rule too: a pun that relies on a union without values
  // being impossible would make an invalid `Target5` here.
  let hollow: Hollow = unsafe { MaybeUninit::<Hollow>::uninit().assume_init() };
  assert_eq!(puns::try_pun::<Source5, Target5>(Source5::A(hollow)), Err(Error::Validity));
  assert_eq!(puns::try_pun::<Source5, Target5>(Source5::B(7)), Ok(Target5::B(7)));
  assert_eq!(puns::try_pun::<Source5, Target5>(Source5::C), Ok(Target5::C));
  // A variant whose field truly has no value: no value is it, and the pun is proven.
  assert_eq!(puns::pun::<Plain5, Target5>(Plain5::B(7)), Target5::B(7));
  assert_eq!(puns::pun::<Plain5, Target5>(Plain5::C), Target5::C);
}

/// Checks that a variant that holds a union none of whose fields has values, which has no sample
/// from which the derive would read its tag, has a tag from its declaration: a discriminant without
/// a macro, or the tag of the variant before it plus one. One that is not known does not compile
/// (`enum_discriminant_of_a_hollow_variant` in `compile-fail/`).
#[test]
fn a_hollow_variant_has_a_tag_from_its_declaration() {
  use core::convert::Infallible;

  use puns::Pun;

  /// A union none of whose fields has values.
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  union Hollow {
    a: Infallible,
  }

  const TWELVE: u8 = 12;

  #[derive(Pun, Clone, Copy)]
  #[repr(u8)]
  enum Declared {
    Literal(Hollow) = 2,
    Next(u8),
    After(Hollow),
    Expression(u8) = 4 + 5,
    Constant(Hollow) = TWELVE,
  }

  // The tag of a value, as the enum holds it.
  fn tag(value: &Declared) -> u8 {
    // SAFETY: `Declared` is `repr(u8)`, so each value starts with its tag, a `u8`, which is
    // initialized. The pointer is that of a reference, so it is aligned and valid for reads.
    unsafe { *core::ptr::from_ref(value).cast::<u8>() }
  }

  // 3 and 9 are the variants that a pun makes. No bytes are a `Hollow` of a target, so 2, 4 and 12
  // are refused, as tags of no variant are.
  let pun = |tag| puns::try_pun::<[u8; 2], Declared>([tag, 0]);
  assert!(matches!(pun(3), Ok(Declared::Next(0))));
  assert!(matches!(pun(9), Ok(Declared::Expression(0))));
  for tag in [0, 2, 4, 10, 12] {
    assert_eq!(pun(tag).err(), Some(Error::Validity));
  }
  // SAFETY: The language has not settled the validity of unions. The compiler and Miri accept any
  // bytes as a union today, and a `Hollow` has none, so this value is accepted, as in
  // `a_union_of_a_source_may_hold_a_value`. It is only read for the tag of the enum that holds it.
  let hollow: Hollow = unsafe { MaybeUninit::<Hollow>::uninit().assume_init() };
  // The tags of the layout are those of the enum.
  let values = [
    Declared::Literal(hollow),
    Declared::Next(0),
    Declared::After(hollow),
    Declared::Expression(0),
    Declared::Constant(hollow),
  ];
  for (i, value) in values.iter().enumerate() {
    assert_eq!(<Declared as Pun>::LAYOUT.variant_tag(i), u128::from(tag(value)));
  }
  assert_eq!(tag(&values[4]), 12);
}
