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
