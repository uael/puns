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

/// Checks that a part that is proven without being read is not read by the check of another part,
/// and is checked where its bytes are initialized.
///
/// The same union, or the same enum, proves a part whatever its bytes are, and a union says
/// nothing of its bytes. The puns that would check such a part all the same must not compile: the
/// cases `try_union_proven_*`, `try_union_field_proven_in_array`, `try_enum_proven_*`,
/// `try_tuple_proven_past_marks`, `try_rule_proven_before_str` and `try_rule_reads_enum_padding`
/// in `compile-fail/`.
#[test]
fn a_part_that_is_proven_unread_is_not_read_by_a_check() {
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  union Flags {
    both: [bool; 2],
    first: bool,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  struct Source {
    flags: Flags,
    byte: u8,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  struct Target {
    flags: Flags,
    flag: bool,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(u8)]
  #[expect(dead_code, reason = "reinterpreted, not read")]
  enum Strict {
    A(Flags, bool),
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(u8)]
  #[expect(dead_code, reason = "made by a pun only")]
  enum Loose {
    A(Flags, u8),
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(u8)]
  #[expect(dead_code, reason = "reinterpreted, not read")]
  enum Shape {
    A(u8),
    B(u8),
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  struct Plain {
    shape: Shape,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  #[pun(assert(be(0, 2) != 0x0107))]
  struct Ruled {
    shape: Shape,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  #[pun(assert(be(0, 4) != 0))]
  struct Padded {
    a: u8,
    b: u16,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(u8)]
  #[expect(dead_code, reason = "reinterpreted, not read")]
  enum Holds {
    A(Padded),
  }

  // The second byte of the union is not written. The `bool` is checked, and the union is not read.
  let source = Source { flags: Flags { first: true }, byte: 1 };
  assert!(puns::try_pun::<Source, Target>(source).unwrap().flag);
  let source = Source { byte: 2, ..source };
  assert_eq!(puns::try_pun::<Source, Target>(source).err(), Some(Error::Validity));
  // The same union proves each element of an array, with nothing to check.
  let pair = [Flags { first: true }, Flags { both: [true, false] }];
  let _: &[Flags; 2] = puns::pun_ref(&pair);
  // From initialized bytes, each element is checked, its first field first.
  assert!(puns::try_pun::<[u8; 4], [Flags; 2]>([1, 0, 0, 1]).is_ok());
  assert_eq!(puns::try_pun::<[u8; 4], [Flags; 2]>([1, 0, 2, 1]).err(), Some(Error::Validity));
  // An enum that is proven variant by variant, and holds the union: nothing is checked.
  let Loose::A(_, byte) = puns::pun(Strict::A(Flags { first: true }, true));
  assert_eq!(byte, 1);
  // A rule that reads the bytes of its struct, over an enum whose bytes are initialized in each
  // variant, is checked.
  assert!(puns::try_pun::<Plain, Ruled>(Plain { shape: Shape::A(7) }).is_ok());
  let plain = Plain { shape: Shape::B(7) };
  assert_eq!(puns::try_pun::<Plain, Ruled>(plain).err(), Some(Error::Validity));
  // The same enum proves itself where a rule of what it holds reads padding: nothing is checked.
  let holds = [Holds::A(Padded { a: 1, b: 2 }); 2];
  let _: &[Holds; 2] = puns::pun_ref(&holds);
}

/// Checks that an enum or a union of the target over an array of the source is related as a
/// whole, and checked where its bytes are initialized.
///
/// The elements of an array of one byte repeat with a period of one byte. The first bytes of an
/// `Option<NonZero<u16>>` are its tag, which accepts every value. A period used to cut it, so that
/// it was checked rather than proven, and the bytes that its check reads were not recorded. The
/// puns that would read them must not compile: the cases `try_enum_under_a_period` and
/// `try_union_under_a_period` in `compile-fail/`, and `try_enum_under_a_period_tag_last` in
/// `compile-fail-unstable/`.
#[test]
fn a_period_does_not_cut_an_enum() {
  use core::num::NonZero;

  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  union Byte {
    any: u8,
    none: (),
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  struct Pair {
    x: Byte,
    y: Byte,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  union Either {
    opt: Option<NonZero<u16>>,
    pair: Pair,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(u8)]
  #[expect(dead_code, reason = "made by a pun only")]
  enum Target {
    V(Either),
    X,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  union Strict {
    wide: NonZero<u16>,
    pair: Pair,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(u8)]
  #[expect(dead_code, reason = "made by a pun only")]
  enum Tagged {
    V(Strict),
    X,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  struct Source {
    tag: u8,
    pad: u8,
    bytes: [u8; 2],
  }

  // Every two `bool`s are an `Option<NonZero<u16>>`: proven, as from a struct of two `bool`s.
  assert_eq!(puns::pun::<[bool; 2], Option<NonZero<u16>>>([false, false]), None);
  assert_eq!(puns::pun::<[bool; 2], Option<NonZero<u16>>>([true, true]), NonZero::new(0x0101));
  // From initialized bytes, the tag of the enum is checked, then its union, its first field first.
  let source = Source { tag: 0, pad: 0, bytes: [0, 0] };
  assert!(puns::try_pun::<Source, Target>(source).is_ok());
  assert!(puns::try_pun::<Source, Target>(Source { tag: 1, ..source }).is_ok());
  let tag = Source { tag: 2, ..source };
  assert_eq!(puns::try_pun::<Source, Target>(tag).err(), Some(Error::Validity));
  // A union that its second field proves. Its first field is checked first, and fails on zeros.
  assert!(puns::try_pun::<Source, Tagged>(source).is_ok());
  assert!(puns::try_pun::<Source, Tagged>(Source { bytes: [1, 0], ..source }).is_ok());
  assert_eq!(puns::try_pun::<Source, Tagged>(tag).err(), Some(Error::Validity));
}

/// Checks that a struct that ends with an unsized struct has values only if that struct does.
///
/// An unsized field used to count as having values, as a slice or a string does. Then a struct
/// that ends with a struct that holds `Infallible` was made from bytes.
#[test]
fn a_struct_that_ends_with_a_struct_without_values_has_none() {
  use core::convert::Infallible;

  #[derive(Pun)]
  #[repr(C)]
  struct Inner {
    v: Infallible,
    tail: [u8],
  }
  #[derive(Pun)]
  #[repr(C)]
  struct Outer {
    h: u8,
    inner: Inner,
  }
  #[derive(Pun)]
  #[repr(C)]
  struct Bytes {
    h: u8,
    tail: [u8],
  }
  #[derive(Pun)]
  #[repr(C)]
  struct Text {
    h: u8,
    text: str,
  }
  #[derive(Pun)]
  #[repr(C)]
  struct Nested {
    g: u8,
    bytes: Bytes,
  }

  assert!(!<Inner as Pun>::LAYOUT.is_inhabited());
  assert!(!<Outer as Pun>::LAYOUT.is_inhabited());
  // A slice or a string at the end has a value, the empty one.
  assert!(<Bytes as Pun>::LAYOUT.is_inhabited());
  assert!(<Text as Pun>::LAYOUT.is_inhabited());
  assert!(<Nested as Pun>::LAYOUT.is_inhabited());
  let nested = puns::from_bytes::<Nested>(&[1, 2, 3]).unwrap();
  assert_eq!((nested.g, nested.bytes.h, &nested.bytes.tail), (1, 2, &[3][..]));
  let text = puns::from_bytes::<Text>(b"\x01hi").unwrap();
  assert_eq!((text.h, &text.text), (1, "hi"));
}
