//! Tests enums of the `Rust` representation whose layout `puns` assumes without the `unstable`
//! feature: `Option`, `Result`, and enums that derive `Pun`, through the same code.
//!
//! Each pun here compiles because the layout that `rustc` gives the enum is the one that `puns`
//! assumes, which is checked when the layout is built. What stays refused is in
//! `tests/compile-fail-kept/not_assumed.rs`.

use core::{convert::Infallible, mem::MaybeUninit, num::NonZero};

use puns::{
  Error, Pun, Scalar,
  layout::{Endian, Spec},
};

/// The layout that `puns` assumes of `Option<u16>`: a tag of 2 bytes, 0 for `None` and 1 for
/// `Some`, then the value, which `None` leaves uninitialized.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct RawOption {
  tag: u16,
  value: MaybeUninit<u16>,
}

/// The same, with a value that is always initialized.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
#[repr(C)]
struct Tagged {
  tag: u16,
  value: u16,
}

/// Two different `repr(C)` structs that hold an `Option<u16>` and a `RawOption` at offset 0.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct HoldsOption {
  part: Option<u16>,
  rest: u16,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct HoldsRaw {
  part: RawOption,
  rest: u16,
}

/// No variant has a niche: a tag of 1 byte, the alignment of `u8`, the least of the fields.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
enum Shape {
  Dot,
  Square(u16),
  Byte(u8),
}

/// A niche: `Free` is the value 0 of the `NonZero`.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
enum Ticket {
  Free,
  Paid(NonZero<u32>),
}

/// An enum without fields, of the `Rust` representation: a byte, 0, 1 or 2.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
enum Color {
  Red,
  Green,
  Blue,
}

/// A variant of two fields that every order puts at the same bytes.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
enum Pair {
  Both(u8, u8),
  Neither,
}

/// A `u32` stored little-endian, never zero, declared by hand, as the stored offsets of `nearest`
/// are: its bytes are a `NonZero<u32>`, which its `Spec` declares, so `rustc` gives its `Option`
/// the niche zero.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(transparent)]
struct U32le(NonZero<u32>);

impl U32le {
  fn new(value: u32) -> Option<Self> {
    NonZero::new(value.to_le()).map(Self)
  }

  const fn get(self) -> u32 {
    u32::from_le(self.0.get())
  }
}

// SAFETY: `U32le` is `repr(transparent)` over a `NonZero<u32>`, so it has its size, and its values
// are every 4 bytes but zeros, which are exactly the values that `SPEC` accepts, read
// little-endian. It holds no `UnsafeCell`. Its bytes are a `NonZero<u32>`, as `stored_as` declares.
unsafe impl Scalar for U32le {
  const SPEC: Spec =
    Spec::ranges::<U32le>(Endian::Little, &[(1, u32::MAX as u128)]).stored_as::<NonZero<u32>>();
}

/// Two `repr(C)` structs with the layout that `puns` assumes of an `Option<U32le>` at offset 0.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct HoldsOffset {
  offset: Option<U32le>,
  len: u32,
}

#[derive(Pun, Clone, Copy, Debug, PartialEq)]
#[repr(C)]
struct HoldsBytes {
  offset: [u8; 4],
  len: u32,
}

#[test]
fn option_of_a_type_without_niche_is_a_tag_then_the_value() {
  // Proven: every `Option<u16>` is a `RawOption`, in every layout that `puns` assumes.
  let raw: RawOption = puns::pun(Some(7u16));
  assert_eq!(raw.tag, 1);
  // SAFETY: `Some` initializes the value.
  assert_eq!(unsafe { raw.value.assume_init() }, 7);
  assert_eq!(puns::pun::<Option<u16>, RawOption>(None).tag, 0);
  // Checked: the tag is 0 or 1.
  assert_eq!(puns::try_pun::<Tagged, Option<u16>>(Tagged { tag: 1, value: 9 }), Ok(Some(9)));
  assert_eq!(puns::try_pun::<Tagged, Option<u16>>(Tagged { tag: 0, value: 9 }), Ok(None));
  assert_eq!(
    puns::try_pun::<Tagged, Option<u16>>(Tagged { tag: 2, value: 9 }),
    Err(Error::Validity)
  );
  // From bytes, in the byte order of the target.
  let mut bytes = [0u16; 2];
  bytes[0] = 1;
  bytes[1] = 0x1234;
  assert_eq!(puns::from_bytes::<Option<u16>>(puns::as_bytes(&bytes)), Ok(&Some(0x1234)));
}

#[test]
fn a_part_that_is_an_option_is_compared_by_its_layout() {
  // The same layout at the same place, though not the same type.
  let held: HoldsRaw = puns::pun(HoldsOption { part: Some(3), rest: 4 });
  assert_eq!((held.part.tag, held.rest), (1, 4));
  let raw: RawOption = puns::head(HoldsOption { part: None, rest: 4 });
  assert_eq!(raw.tag, 0);
}

#[test]
fn option_of_a_type_with_a_niche_takes_a_value_outside_it() {
  // `None` is 2, after the values of `bool`.
  assert_eq!(puns::try_pun::<u8, Option<bool>>(2), Ok(None));
  assert_eq!(puns::try_pun::<u8, Option<bool>>(1), Ok(Some(true)));
  assert_eq!(puns::try_pun::<u8, Option<bool>>(3), Err(Error::Validity));
  assert_eq!(puns::pun::<bool, Option<bool>>(false), Some(false));
  // `None` of `Option<Option<bool>>` is 255, before them.
  assert_eq!(puns::try_pun::<u8, Option<Option<bool>>>(255), Ok(None));
  assert_eq!(puns::try_pun::<u8, Option<Option<bool>>>(2), Ok(Some(None)));
  // `None` of `Option<char>` is `u32::MAX`.
  assert_eq!(puns::try_pun::<u32, Option<char>>(u32::MAX), Ok(None));
  assert_eq!(puns::try_pun::<u32, Option<char>>(0x41), Ok(Some('A')));
  assert_eq!(puns::try_pun::<u32, Option<char>>(0xD800), Err(Error::Validity));
}

#[test]
fn results_are_assumed_too() {
  // A tag of 1 byte, the alignment of `u8`, then each field where its alignment allows: `Ok` at 4,
  // `Err` at 1.
  let ok: [u32; 2] = puns::pun([0u8, 0, 0, 0, 7, 0, 0, 0]);
  let err: [u32; 2] = puns::pun([1u8, 5, 0, 0, 0, 0, 0, 0]);
  let value = u32::from_ne_bytes([7, 0, 0, 0]);
  assert_eq!(puns::from_bytes::<Result<u32, u8>>(puns::as_bytes(&ok)), Ok(&Ok(value)));
  assert_eq!(puns::from_bytes::<Result<u32, u8>>(puns::as_bytes(&err)), Ok(&Err(5)));
  // The niche of `char` holds `Err(())`.
  assert_eq!(puns::try_pun::<u32, Result<char, ()>>(u32::MAX), Ok(Err(())));
  // A variant without values is not laid out: `Result<u32, Infallible>` is its `u32`.
  assert_eq!(puns::pun::<u32, Result<u32, Infallible>>(5), Ok(5));
}

#[test]
fn derived_enums_are_assumed_too() {
  // Proven both ways: every `u32` is a `Ticket`, `Free` is 0.
  assert_eq!(puns::pun::<u32, Ticket>(0), Ticket::Free);
  assert_eq!(puns::pun::<Ticket, u32>(Ticket::Paid(NonZero::new(8).unwrap())), 8);
  // A tag, then the field. `Shape` is aligned to 2.
  let shape = |bytes: [u8; 4]| -> Result<Shape, Error> {
    let aligned: [u16; 2] = puns::pun(bytes);
    puns::from_bytes::<Shape>(puns::as_bytes(&aligned)).copied()
  };
  assert_eq!(shape([0, 0, 0, 0]), Ok(Shape::Dot));
  assert_eq!(shape([2, 9, 0, 0]), Ok(Shape::Byte(9)));
  assert_eq!(shape([3, 0, 0, 0]), Err(Error::Validity));
  // An enum without fields.
  assert_eq!(puns::from_bytes::<Color>(&[2]), Ok(&Color::Blue));
  assert_eq!(puns::from_bytes::<Color>(&[3]), Err(Error::Validity));
  assert_eq!(puns::pun::<Color, u8>(Color::Green), 1);
  // Which field holds which byte depends on the order that `rustc` chose, but not whether the
  // bytes are a `Pair`.
  assert!(matches!(puns::from_bytes::<Pair>(&[0, 4, 4]), Ok(&Pair::Both(4, 4))));
  assert_eq!(puns::from_bytes::<Pair>(&[1, 4, 4]), Ok(&Pair::Neither));
  assert_eq!(puns::from_bytes::<Pair>(&[2, 4, 4]), Err(Error::Validity));
}

#[test]
fn a_scalar_declared_by_hand_gives_its_niche() {
  // Proven: an `Option<U32le>` is 4 bytes, `None` the zeros, and so is a holder of one.
  assert_eq!(puns::pun::<Option<U32le>, [u8; 4]>(U32le::new(0x0102_0304)), [4, 3, 2, 1]);
  assert_eq!(puns::pun::<Option<U32le>, [u8; 4]>(None), [0; 4]);
  let held = HoldsOffset { offset: U32le::new(9), len: 3 };
  let bytes = HoldsBytes { offset: [9, 0, 0, 0], len: 3 };
  assert_eq!(puns::pun::<HoldsOffset, HoldsBytes>(held), bytes);
  let empty = HoldsBytes { offset: [0; 4], len: 3 };
  from_bytes(bytes, empty);
}

/// Reads `bytes` and `empty` as holders of an `Option<U32le>`: `Some(9)` and `None`.
///
/// Where the byte order of the target is little-endian, every 4 bytes are an `Option<U32le>`,
/// proven: the niche is the zeros, and the other values are those of `Some`. Where it is
/// big-endian, the analysis does not relate the values of the tag, in the byte order of the
/// target, to those of the field, little-endian, so it checks them.
fn from_bytes(bytes: HoldsBytes, empty: HoldsBytes) {
  #[cfg(target_endian = "little")]
  let (some, none): (HoldsOffset, HoldsOffset) = (puns::pun(bytes), puns::pun(empty));
  #[cfg(target_endian = "big")]
  let (some, none): (HoldsOffset, HoldsOffset) =
    (puns::try_pun(bytes).unwrap(), puns::try_pun(empty).unwrap());
  assert_eq!((some.offset.map(U32le::get), some.len), (Some(9), 3));
  assert_eq!((none.offset, none.len), (None, 3));
}
