//! Tests enums with fields, whose layout a primitive representation, `repr(C)` or both specify.

use puns::{
  Error, Pun,
  num::{u8in, u16le},
};

#[derive(Pun, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
enum Shape {
  Circle { radius: u16 },
  Rect(u8, bool),
  Empty,
}

/// An enum whose bytes are all initialized in every variant.
#[derive(Pun, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
enum Bit {
  Zero(u8) = 0,
  One(u8) = 1,
}

#[derive(Pun, Debug, PartialEq, Clone, Copy)]
#[repr(C)]
enum Tagged {
  Byte(u8),
  Word(u32),
  Flag(bool),
}

#[derive(Pun, Debug, PartialEq, Clone, Copy)]
#[repr(C, u8)]
enum Either<A, B> {
  Left(A) = 0x10,
  Right(B) = 0x20,
}

#[derive(Pun, Debug, PartialEq, Clone, Copy)]
#[repr(i16)]
enum Signed {
  Minus(#[pun(valid(1..=9))] u8) = -1,
  Plus { digit: u8in<0, 9>, even: bool } = 1,
}

#[derive(Pun, Debug, PartialEq, Clone, Copy)]
#[repr(C)]
struct Tile {
  x: u16le,
  shape: Shape,
}

#[repr(C)]
#[derive(Pun, Debug, PartialEq, Clone, Copy)]
struct Raw(u8in<0, 1>, u8);

#[derive(Pun, Debug, PartialEq, Clone, Copy)]
#[repr(C)]
enum Wide {
  Long(u64),
  Short(u8, u16),
  Unit,
}

#[derive(Pun, Debug, PartialEq, Clone, Copy)]
#[repr(u8, align(4))]
enum Aligned<const N: usize> {
  Bytes([u8; N]) = 1,
  Flags([bool; N]) = 2,
}

/// An enum whose values include every value of `Shape`, and more.
#[derive(Pun, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
enum Drawing {
  Circle { radius: u16 },
  Rect(u8, u8),
  Empty,
  Line { to: (u8, u8) } = 9,
}

const FIVE: u8 = 5;

/// Discriminants that are a literal, none, a constant, an expression and a block with an
/// attribute. The derive evaluates them again where it declares the tag, and compares the tags with
/// those of the enum.
#[derive(Pun, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
enum Declared {
  Literal(bool) = 2,
  Next(u8),
  Constant(u8) = FIVE,
  Expression(u8) = FIVE * 2 + 1,
  Attributed(u8) = {
    #[expect(unused_parens, reason = "an attribute in a discriminant, which is the point")]
    let tag = (13);
    tag
  },
}

/// A trait whose constant is 1 for a `Copy` type, and 2 for `OfSelf`.
trait Num {
  const N: u8;
}

impl<T: Copy> Num for T {
  const N: u8 = 1;
}

impl Num for OfSelf {
  const N: u8 = 2;
}

/// A discriminant that names `Self`, which is this enum here, and would be the enum without fields
/// that holds the tags where the derive declares it, which is `Copy`.
#[derive(Pun, Debug, PartialEq)]
#[repr(u8)]
enum OfSelf {
  A(bool) = <Self as Num>::N,
  B(u8),
}

fn bytes<T: Copy, const N: usize>(value: T) -> [u8; N] {
  assert_eq!(size_of::<T>(), N);
  // SAFETY: `transmute_copy` reads `size_of::<[u8; N]>()` bytes of `value`, which is the size of
  // `T`, as asserted above. It reads them whatever the alignment. They must be a valid `[u8; N]`,
  // so initialized. The only value given is a `Bit`, a `u8` tag and a `u8`, whose 2 bytes are.
  unsafe { core::mem::transmute_copy(&value) }
}

#[test]
fn layouts_are_the_ones_of_the_compiler() {
  // What is made out of bytes is what the compiler makes.
  let circle = Shape::Circle { radius: 0x1234 };
  let mut raw = [0u8; 4];
  raw[2..].copy_from_slice(&0x1234u16.to_ne_bytes());
  assert_eq!(puns::try_pun::<[u8; 4], Shape>(raw), Ok(circle));

  let rect: [u8; 4] = [1, 7, 1, 0xaa];
  assert_eq!(puns::try_pun::<[u8; 4], Shape>(rect), Ok(Shape::Rect(7, true)));
  assert_eq!(puns::try_pun::<[u8; 4], Shape>([2, 9, 9, 9]), Ok(Shape::Empty));

  assert_eq!(puns::try_pun::<[u8; 2], Bit>([1, 42]), Ok(Bit::One(42)));
  assert_eq!(bytes::<_, 2>(Bit::One(42)), [1, 42]);

  let mut word = [0u8; 8];
  word[..4].copy_from_slice(&1u32.to_ne_bytes());
  word[4..].copy_from_slice(&0xdead_beefu32.to_ne_bytes());
  assert_eq!(puns::try_pun::<[u8; 8], Tagged>(word), Ok(Tagged::Word(0xdead_beef)));

  let right = puns::try_pun::<[u8; 3], Either<u8, u16le>>([0x20, 0x34, 0x12]);
  assert_eq!(right, Ok(Either::Right(u16le::new(0x1234))));
  let left = puns::try_pun::<[u8; 2], Either<u8, bool>>([0x10, 0xff]);
  assert_eq!(left, Ok(Either::Left(0xff)));

  let mut minus = [0u8; 4];
  minus[..2].copy_from_slice(&(-1i16).to_ne_bytes());
  minus[2] = 9;
  assert_eq!(puns::try_pun::<[u8; 4], Signed>(minus), Ok(Signed::Minus(9)));
}

#[test]
fn tags_are_the_discriminants() {
  // 2, then 3 after it, then 5, 11 and 13.
  assert_eq!(puns::try_pun::<[u8; 2], Declared>([2, 1]), Ok(Declared::Literal(true)));
  assert_eq!(puns::try_pun::<[u8; 2], Declared>([2, 7]), Err(Error::Validity));
  assert_eq!(puns::try_pun::<[u8; 2], Declared>([3, 7]), Ok(Declared::Next(7)));
  assert_eq!(puns::try_pun::<[u8; 2], Declared>([5, 7]), Ok(Declared::Constant(7)));
  assert_eq!(puns::try_pun::<[u8; 2], Declared>([11, 7]), Ok(Declared::Expression(7)));
  assert_eq!(puns::try_pun::<[u8; 2], Declared>([13, 7]), Ok(Declared::Attributed(7)));
  for tag in [0, 1, 4, 6, 10, 12] {
    assert_eq!(puns::try_pun::<[u8; 2], Declared>([tag, 0]), Err(Error::Validity));
  }
  // `A` is 2, checked as a `bool`, and `B` is 3. No variant is 1.
  assert_eq!(puns::try_pun_ref::<[u8; 2], OfSelf>(&[2, 1]), Ok(&OfSelf::A(true)));
  assert_eq!(puns::try_pun_ref::<[u8; 2], OfSelf>(&[2, 7]), Err(Error::Validity));
  assert_eq!(puns::try_pun_ref::<[u8; 2], OfSelf>(&[1, 0]), Err(Error::Validity));
  assert_eq!(puns::try_pun_ref::<[u8; 2], OfSelf>(&[3, 7]), Ok(&OfSelf::B(7)));
}

#[test]
fn offsets_and_alignments() {
  // A tag of the size of an `int`, then the fields of the variants where the largest alignment
  // of all of them puts them: that of a `u64`, which is 4 on some targets (i686), 8 on others.
  const SIZE: usize = size_of::<Wide>();
  let at = align_of::<u64>().max(4);
  assert_eq!((SIZE, align_of::<Wide>()), (at + 8, at));
  let mut bytes = [0u8; SIZE];
  bytes[at..at + 8].copy_from_slice(&0x0123_4567_89ab_cdefu64.to_ne_bytes());
  assert_eq!(puns::try_pun::<[u8; SIZE], Wide>(bytes), Ok(Wide::Long(0x0123_4567_89ab_cdef)));
  let mut bytes = [0u8; SIZE];
  bytes[..4].copy_from_slice(&1u32.to_ne_bytes());
  bytes[at] = 7;
  bytes[at + 2..at + 4].copy_from_slice(&0x1234u16.to_ne_bytes());
  assert_eq!(puns::try_pun::<[u8; SIZE], Wide>(bytes), Ok(Wide::Short(7, 0x1234)));
  bytes[..4].copy_from_slice(&2u32.to_ne_bytes());
  assert_eq!(puns::try_pun::<[u8; SIZE], Wide>(bytes), Ok(Wide::Unit));
  bytes[..4].copy_from_slice(&0x0100_0000u32.to_ne_bytes());
  assert_eq!(puns::try_pun::<[u8; SIZE], Wide>(bytes), Err(Error::Validity));
  // From words, where they are aligned as the enum is.
  let words = [Wide::Long(5); 2];
  let back = puns::try_pun_ref::<[Wide; 2], [Wide; 2]>(&words);
  assert_eq!(back, Ok(&words));

  // An alignment that the enum asks for: its size follows.
  assert_eq!((size_of::<Aligned<2>>(), align_of::<Aligned<2>>()), (4, 4));
  let flags = puns::try_pun::<u32, Aligned<2>>(u32::from_ne_bytes([2, 1, 0, 0xee]));
  assert_eq!(flags, Ok(Aligned::Flags([true, false])));
  let flags = puns::try_pun::<u32, Aligned<2>>(u32::from_ne_bytes([2, 1, 2, 0]));
  assert_eq!(flags, Err(Error::Validity));
  let bytes = puns::try_pun::<u32, Aligned<2>>(u32::from_ne_bytes([1, 1, 2, 0]));
  assert_eq!(bytes, Ok(Aligned::Bytes([1, 2])));
  assert_eq!(size_of::<Aligned<5>>(), 8);
  // Generic enums are themselves too.
  let same: Aligned<3> = puns::pun(Aligned::<3>::Flags([true; 3]));
  assert_eq!(same, Aligned::Flags([true; 3]));
}

#[test]
fn what_is_not_a_variant_is_refused() {
  // No such tag.
  assert_eq!(puns::try_pun::<[u8; 4], Shape>([3, 0, 0, 0]), Err(Error::Validity));
  // The tag of a variant whose field is not valid.
  assert_eq!(puns::try_pun::<[u8; 4], Shape>([1, 0, 2, 0]), Err(Error::Validity));
  // The same bytes are fine for another variant.
  assert!(puns::try_pun::<[u8; 4], Shape>([0, 0, 2, 0]).is_ok());
  assert_eq!(puns::try_pun::<[u8; 2], Bit>([2, 0]), Err(Error::Validity));

  let mut flag = [0u8; 8];
  flag[..4].copy_from_slice(&2u32.to_ne_bytes());
  flag[4] = 1;
  assert_eq!(puns::try_pun::<[u8; 8], Tagged>(flag), Ok(Tagged::Flag(true)));
  flag[4] = 2;
  assert_eq!(puns::try_pun::<[u8; 8], Tagged>(flag), Err(Error::Validity));
  flag[..4].copy_from_slice(&3u32.to_ne_bytes());
  assert_eq!(puns::try_pun::<[u8; 8], Tagged>(flag), Err(Error::Validity));

  assert!(puns::try_pun::<[u8; 2], Either<u8, bool>>([0x20, 2]).is_err());
  assert!(puns::try_pun::<[u8; 2], Either<u8, bool>>([0x00, 0]).is_err());

  // Restrictions of the fields of a variant, by an option or by their type.
  let mut signed = [0u8; 4];
  signed[..2].copy_from_slice(&(-1i16).to_ne_bytes());
  assert_eq!(puns::try_pun::<[u8; 4], Signed>(signed), Err(Error::Validity));
  signed[..2].copy_from_slice(&1i16.to_ne_bytes());
  let plus = Signed::Plus { digit: u8in::new(0).unwrap(), even: false };
  assert_eq!(puns::try_pun::<[u8; 4], Signed>(signed), Ok(plus));
  signed[2] = 10;
  assert_eq!(puns::try_pun::<[u8; 4], Signed>(signed), Err(Error::Validity));
  signed[..2].copy_from_slice(&0i16.to_ne_bytes());
  assert_eq!(puns::try_pun::<[u8; 4], Signed>(signed), Err(Error::Validity));
}

#[test]
fn an_enum_is_itself() {
  // The same enum at the same place needs no check, whatever its padding.
  let tiles = [
    Tile { x: u16le::new(1), shape: Shape::Empty },
    Tile { x: u16le::new(2), shape: Shape::Rect(3, false) },
  ];
  let same: [Tile; 2] = puns::pun(tiles);
  assert_eq!(same, tiles);
  let shape: &Shape = puns::pun_ref(&tiles[1].shape);
  assert_eq!(*shape, Shape::Rect(3, false));
  let mut tile = tiles[1];
  let inner: &mut Tile = puns::pun_mut(&mut tile);
  inner.shape = Shape::Circle { radius: 5 };
  assert_eq!(tile.shape, Shape::Circle { radius: 5 });
}

#[test]
fn what_is_known_of_an_enum() {
  // Its tag, and the bytes that every variant initializes.
  let raw: Raw = puns::pun(Bit::One(7));
  assert_eq!(raw, Raw(u8in::new(1).unwrap(), 7));
  let bytes: &[u8; 2] = puns::pun_ref(&Bit::Zero(9));
  assert_eq!(bytes, &[0, 9]);
  // The other way, the tag is checked, and nothing else.
  assert_eq!(puns::try_pun::<Raw, Bit>(Raw(u8in::new(0).unwrap(), 3)), Ok(Bit::Zero(3)));
}

#[test]
fn variant_by_variant() {
  // Every variant of `Shape` is the variant of `Drawing` that has its tag: nothing to check.
  let shapes = [Shape::Rect(3, true), Shape::Empty, Shape::Circle { radius: 7 }];
  let drawings: &[Drawing] = puns::pun_ref(&shapes[..]);
  assert_eq!(drawings, [Drawing::Rect(3, 1), Drawing::Empty, Drawing::Circle { radius: 7 }]);
  assert_eq!(puns::pun::<Shape, Drawing>(Shape::Empty), Drawing::Empty);
  // The other way, a variant may not be one, or may hold what the other does not.
  let back = puns::try_pun_ref::<[Drawing], [Shape]>(drawings);
  assert_eq!(back, Ok(&shapes[..]));
  assert_eq!(puns::try_pun::<Drawing, Shape>(Drawing::Rect(0, 2)), Err(Error::Validity));
  let line = Drawing::Line { to: (1, 1) };
  assert_eq!(puns::try_pun::<Drawing, Shape>(line), Err(Error::Validity));
  let drawings = [Drawing::Empty, line];
  assert_eq!(puns::try_pun_ref::<_, [Shape; 2]>(&drawings), Err(Error::Validity));
}

#[test]
fn enums_in_slices() {
  let bytes = [1u8, 0, 2, 0, 0, 0, 9, 0, 0, 0, 1, 5, 1, 0];
  let tiles = puns::try_pun_ref::<[u8], [Tile]>(&bytes[..12]);
  // `Tile` is aligned as its `u16` radius is: the bytes may not be.
  match tiles {
    Ok(tiles) => {
      assert_eq!(tiles.len(), 2);
      assert_eq!(tiles[0].shape, Shape::Empty);
      assert_eq!(tiles[1].x.get(), 9);
    }
    Err(error) => assert_eq!(error, Error::Alignment),
  }
  let bits = puns::try_pun_ref::<[u8], [Bit]>(&[1, 8, 0, 9, 1, 1]).unwrap();
  assert_eq!(bits, [Bit::One(8), Bit::Zero(9), Bit::One(1)]);
  assert_eq!(puns::try_pun_ref::<[u8], [Bit]>(&[1, 8, 2, 9]), Err(Error::Validity));
  // Half an element.
  assert_eq!(puns::try_pun_ref::<[u8], [Bit]>(&[1, 8, 0]), Err(Error::Size));
}

#[test]
fn zeroed() {
  assert_eq!(puns::zeroed::<Shape>(), Shape::Circle { radius: 0 });
  assert_eq!(puns::zeroed::<Tagged>(), Tagged::Byte(0));
  assert_eq!(puns::zeroed::<Bit>(), Bit::Zero(0));
}

// All of this is known at compile time.
const _: () = {
  assert!(matches!(puns::try_pun::<[u8; 4], Shape>([1, 7, 1, 0]), Ok(Shape::Rect(7, true))));
  assert!(puns::try_pun::<[u8; 4], Shape>([1, 7, 2, 0]).is_err());
  assert!(matches!(puns::pun::<Bit, Raw>(Bit::One(7)), Raw(_, 7)));
};
