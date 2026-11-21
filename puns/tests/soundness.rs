//! Regression tests for soundness.
//!
//! Each test checks that safe code cannot use `puns` in one way to make an invalid value, or to
//! read memory that it must not read. The cases that must not compile are in `compile-fail/`.

use puns::{Error, Pun};

/// Checks that a `valid(..)` requirement does not prove a guarantee.
///
/// An enum whose variant holds a `u8` restricted to 0 and 1 is not an enum whose variant holds a
/// `bool`, even though their layouts look alike.
#[test]
fn a_requirement_does_not_prove_a_guarantee_in_an_enum() {
  #[derive(Pun, Clone, Copy)]
  #[repr(u8)]
  enum Soft {
    V(#[pun(valid(0..=1))] u8),
  }
  #[derive(Pun, Clone, Copy, Debug, PartialEq, Eq)]
  #[repr(u8)]
  enum Hard {
    V(bool),
  }
  let mut soft = Soft::V(0);
  let Soft::V(byte) = &mut soft;
  // Safe code can write anything to the field.
  *byte = 5;
  // This used to be `pun`, without any check, which made a `bool` of 5.
  assert_eq!(puns::try_pun::<Soft, Hard>(soft), Err(Error::Validity));
  assert_eq!(puns::try_pun::<Soft, Hard>(Soft::V(1)), Ok(Hard::V(true)));
}

/// A struct whose rule reads a byte of padding.
///
/// A rule that reads the bytes of the value (`le`, `be`, `crc32`, `sum16`) reads its padding too.
/// It can be checked only if the source has initialized bytes there.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
#[pun(assert(le(1, 1) == 0))]
struct ReadsPadding {
  a: u8,
  b: u16,
}

#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
struct Plain {
  a: u8,
  b: u16,
}

#[derive(Pun, Debug, Clone, Copy)]
#[repr(u8)]
enum HoldsReadsPadding {
  A(ReadsPadding),
}

#[test]
fn a_rule_reads_padding_only_if_it_is_initialized() {
  // From bytes, the rule is checked, padding included.
  let ok = [u16::from_ne_bytes([7, 0]), 9];
  let ok: &[u8; 4] = puns::pun_ref(&ok);
  assert_eq!(puns::try_pun_ref::<[u8; 4], ReadsPadding>(ok).map(|r| (r.a, r.b)), Ok((7, 9)));
  let bad = [u16::from_ne_bytes([1, 5]), 0];
  let bad: &[u8; 4] = puns::pun_ref(&bad);
  assert_eq!(puns::try_pun_ref::<[u8; 4], ReadsPadding>(bad).unwrap_err(), Error::Validity);
  // In the variant of an enum, the same.
  let mut bytes = [0u8; 6];
  bytes[2] = 7;
  bytes[4..].copy_from_slice(&9u16.to_ne_bytes());
  let HoldsReadsPadding::A(value) = puns::try_pun::<[u8; 6], HoldsReadsPadding>(bytes).unwrap();
  assert_eq!((value.a, value.b), (7, 9));
  bytes[3] = 5;
  assert_eq!(puns::try_pun::<[u8; 6], HoldsReadsPadding>(bytes).err(), Some(Error::Validity));
  // The same rule, at the same place, proves it: nothing is checked, nothing is read.
  let mut value = puns::pun::<HoldsReadsPadding, HoldsReadsPadding>(HoldsReadsPadding::A(value));
  let HoldsReadsPadding::A(inner) = &mut value;
  assert_eq!(puns::pun_ref::<ReadsPadding, ReadsPadding>(inner).b, 9);
  // A `&mut` is about what the type system enforces: rules are not invariants.
  let plain: &mut Plain = puns::pun_mut(inner);
  plain.a = 3;
  assert_eq!(inner.a, 3);
}

/// Checks that an unsized struct whose slice has elements of size zero is as large as its
/// alignment requires. That size may be more than the bytes that it is read from.
#[test]
fn an_unsized_target_fits_in_the_source_with_its_trailing_padding() {
  #[derive(Pun)]
  #[repr(C, align(4))]
  struct Aligned {
    a: u8,
    nothing: [()],
  }
  // One byte at an address that is a multiple of 4, at the end of its allocation.
  let words = [0u32; 2];
  let bytes: &[u8; 8] = puns::pun_ref(&words);
  let one: &[u8] = &bytes[4..5];
  // A `&Aligned` is 4 bytes, 3 of which would be out of `one`.
  assert_eq!(puns::try_pun_ref::<[u8], Aligned>(one).err(), Some(Error::Size));
  // Nor can any other number of bytes be one: its size without padding is 1.
  let four: &[u8] = &bytes[4..8];
  assert_eq!(puns::try_pun_ref::<[u8], Aligned>(four).err(), Some(Error::Size));
}

/// A path for `#[pun(crate = ..)]` whose functions differ from those of `puns`.
///
/// The path names the crate and nothing else. The generated code calls the functions of the
/// types that the traits name, whatever else the path leads to.
#[expect(
  dead_code,
  unused_imports,
  reason = "what the derive used to call through the path: it no longer does"
)]
mod fake {
  pub use puns::Scalar;
  pub mod layout {
    pub use puns::layout::{Endian, Enum, Field, Id, Layout, Op, Rule, Spec, Variant};
    /// Returns ranges that would make every byte a variant of the enum, if the generated code
    /// called it.
    pub const fn sorted_ranges<const N: usize>(_: [u128; N]) -> ([(u128, u128); N], usize) {
      ([(0, 0xff); N], 1)
    }
    pub const fn bits_of<T>(_: &T) -> u128 {
      0
    }
    /// Returns an offset that would place the last field of a `repr(C)` struct one byte too late,
    /// if the generated code called it.
    pub const fn tail_offset(end: usize, _: usize, _: usize) -> usize {
      end + 1
    }
  }
  pub use puns::{__site, At, Fields, Path, Pun};
}

#[derive(Pun, Debug)]
#[pun(crate = fake)]
#[repr(C)]
struct Late {
  x: u16,
  on: bool,
}

#[derive(Pun, Clone, Copy, Debug, PartialEq, Eq)]
#[pun(crate = fake)]
#[repr(u8)]
enum Toggle {
  Off,
  On,
}

#[test]
fn the_crate_path_does_not_choose_the_functions() {
  assert_eq!(puns::try_pun::<u8, Toggle>(1), Ok(Toggle::On));
  assert_eq!(puns::try_pun::<u8, Toggle>(7), Err(Error::Validity));
  let bytes = [0u16, u16::from_ne_bytes([7, 1])];
  assert_eq!(puns::try_pun_ref::<[u16; 2], Late>(&bytes).err(), Some(Error::Validity));
  let bytes = [0u16, u16::from_ne_bytes([1, 7])];
  assert!(puns::try_pun_ref::<[u16; 2], Late>(&bytes).unwrap().on);
}

/// Checks that `Self` in the type of a field is the type that derives `Pun`, wherever the
/// generated code writes the type of the field.
#[test]
fn self_in_a_field_is_the_type() {
  use core::marker::PhantomData;
  #[derive(Pun, Debug)]
  #[repr(C)]
  struct Node {
    on: bool,
    marker: PhantomData<Self>,
  }
  #[derive(Pun, Debug, Clone, Copy)]
  #[repr(u8)]
  enum Choice {
    A(bool, PhantomData<Self>),
  }
  assert!(puns::try_pun_ref::<[u8; 1], Node>(&[1]).unwrap().on);
  assert_eq!(puns::try_pun_ref::<[u8; 1], Node>(&[2]).err(), Some(Error::Validity));
  assert!(matches!(puns::try_pun::<[u8; 2], Choice>([0, 1]), Ok(Choice::A(true, _))));
  assert_eq!(puns::try_pun::<[u8; 2], Choice>([0, 2]).err(), Some(Error::Validity));
  assert_eq!(puns::pun::<Choice, [u8; 2]>(Choice::A(true, PhantomData)), [0, 1]);
}
