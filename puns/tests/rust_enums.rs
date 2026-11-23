//! Tests enums whose layout Rust leaves to the compiler, and types without values.
//!
//! `puns` finds the layout that `rustc` chose for an enum of the `Rust` representation by
//! experiments in const evaluation. The tests also cover `Option` and `Result`, and enums read from
//! types that are not enums.
//!
//! This file needs the `unstable` feature (`required-features` in `puns/Cargo.toml`). Without it,
//! `puns` does not depend on the layout that `rustc` chose.

// Validation recurses over the variants of an enum: `Many` has 300.
#![recursion_limit = "1024"]

use core::{convert::Infallible, num::NonZero};

use puns::{Error, Pun};

/// An enum with a variant that holds a type without values. No value is that variant.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
enum Never {
  A(u8),
  B(Infallible),
}

#[derive(Pun, Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
enum Unit {
  A(u8),
  B(()),
}

/// An enum without variants, which has no values.
#[derive(Pun, Clone, Copy)]
#[expect(clippy::empty_enums, reason = "a type without values, which is the point")]
enum Void {}

#[test]
fn uninhabited() {
  assert_eq!(puns::try_pun::<[u8; 2], Never>([0, 5]), Ok(Never::A(5)));
  // The tag of `B`, whose field has no value.
  assert_eq!(puns::try_pun::<[u8; 2], Never>([1, 5]), Err(Error::Validity));
  // Variant by variant: `A` is proven, `B` is never a `Never`.
  assert_eq!(puns::try_pun::<Unit, Never>(Unit::A(3)), Ok(Never::A(3)));
  assert_eq!(puns::try_pun::<Unit, Never>(Unit::B(())), Err(Error::Validity));
  // A `Never` is always a `Unit`: its `B` is never there.
  assert_eq!(puns::pun::<Never, Unit>(Never::A(3)), Unit::A(3));
  // A source without values proves anything, as nothing is ever made of one.
  let _ = puns::pun::<(u8, Void), (u8, Void)>;
}

// --- Enums of the `Rust` representation ----------------------------------------------------------

/// An enum with a direct tag of one byte. The first field of `A` follows the tag.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
enum Direct {
  A(u8),
  B(u16),
}

/// An enum with a direct tag that `rustc` widened to two bytes. No field starts before offset 2.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
enum Widened {
  A(u16),
  B(u32),
}

/// An enum whose tag is a niche, the values of the byte of the `bool` that are not a `bool`.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
enum MaybeFlag {
  Nothing,
  Flag(bool),
}

/// An enum with a niche in a `char`, for two variants.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
enum Letter {
  Char(char),
  End,
  Error,
}

/// An enum with a niche in the field of one variant, and another variant with a field beside it.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
enum Mixed {
  Both(u8, bool),
  Byte(u8),
  Nothing,
}

/// An enum with one variant, which has the layout of a struct.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
enum Single {
  Only { byte: u8, flag: bool },
}

/// A generic enum, as `Option` is.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
enum Maybe<T> {
  No,
  Yes(T),
}

/// An enum with a niche in the widened tag of another enum.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
enum Outer {
  Inner(Widened),
  Other,
}

/// An enum with a variant that has no values but has a field with a size. `rustc` gives that
/// variant a tag all the same.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
enum OneOfTwo {
  A(u8),
  B(u32, Infallible),
}

/// Returns the `N` low bytes of `value`, in native byte order.
fn bytes<const N: usize>(value: u128) -> [u8; N] {
  let mut out = [0; N];
  for (i, byte) in out.iter_mut().enumerate() {
    let digit = if cfg!(target_endian = "little") { i } else { N - 1 - i };
    *byte = (value >> (8 * digit)) as u8;
  }
  out
}

#[test]
fn direct() {
  // The tag, then the field of the variant; the rest is padding, which bytes may hold.
  assert_eq!(puns::try_pun::<[u8; 4], Direct>([0, 7, 0xaa, 0xbb]), Ok(Direct::A(7)));
  let [lo, hi] = bytes::<2>(0x0102);
  assert_eq!(puns::try_pun::<[u8; 4], Direct>([1, 0xaa, lo, hi]), Ok(Direct::B(0x0102)));
  assert_eq!(puns::try_pun::<[u8; 4], Direct>([2, 0, 0, 0]), Err(Error::Validity));
  // Proven between enums of the same layout.
  assert_eq!(puns::pun::<Direct, Direct>(Direct::B(3)), Direct::B(3));
  // The widened tag is two bytes, both read.
  let tag = bytes::<2>(1);
  let [a, b, c, d] = bytes::<4>(9);
  let value = [tag[0], tag[1], 0, 0, a, b, c, d];
  assert_eq!(puns::try_pun::<[u8; 8], Widened>(value), Ok(Widened::B(9)));
  let tag = bytes::<2>(0x0101);
  let value = [tag[0], tag[1], 0, 0, a, b, c, d];
  assert!(puns::try_pun::<[u8; 8], Widened>(value).is_err());
  assert_eq!(puns::pun::<Widened, Widened>(Widened::A(5)), Widened::A(5));
}

#[test]
fn niche() {
  // The values of the `bool`, then the one of `Nothing`.
  assert_eq!(puns::pun::<MaybeFlag, u8>(MaybeFlag::Flag(true)), 1);
  assert_eq!(puns::try_pun::<u8, MaybeFlag>(0), Ok(MaybeFlag::Flag(false)));
  let nothing = puns::pun::<MaybeFlag, u8>(MaybeFlag::Nothing);
  assert!(nothing > 1);
  assert_eq!(puns::try_pun::<u8, MaybeFlag>(nothing), Ok(MaybeFlag::Nothing));
  for byte in 2..=255u8 {
    if byte != nothing {
      assert_eq!(puns::try_pun::<u8, MaybeFlag>(byte), Err(Error::Validity));
    }
  }
  // A `char` and two more values.
  let end: u32 = puns::pun(Letter::End);
  assert_eq!(puns::try_pun::<u32, Letter>(end), Ok(Letter::End));
  assert_eq!(puns::try_pun::<u32, Letter>(0x41), Ok(Letter::Char('A')));
  assert_eq!(puns::try_pun::<u32, Letter>(0xd800), Err(Error::Validity));
  assert_eq!(puns::pun::<Letter, u32>(Letter::Char('A')), 0x41);
  // Proven: each variant of the source is the variant of the target of the same tags.
  assert_eq!(puns::pun::<Mixed, Mixed>(Mixed::Byte(4)), Mixed::Byte(4));
}

#[test]
fn single_and_generic() {
  // Its fields where `rustc` put them, which is not where they are declared.
  let only = Single::Only { byte: 3, flag: true };
  let pair: [u8; 2] = puns::pun(only);
  assert!(pair == [3, 1] || pair == [1, 3]);
  assert_eq!(puns::try_pun::<[u8; 2], Single>(pair), Ok(only));
  assert!(puns::try_pun::<[u8; 2], Single>([2, 2]).is_err());
  // A niche in `NonZero`, a direct tag beside a `u8`.
  assert_eq!(puns::pun::<Maybe<NonZero<u32>>, u32>(Maybe::No), 0);
  let five = NonZero::new(5).unwrap();
  assert_eq!(puns::try_pun::<u32, Maybe<NonZero<u32>>>(5), Ok(Maybe::Yes(five)));
  assert_eq!(puns::try_pun::<[u8; 2], Maybe<u8>>([1, 9]), Ok(Maybe::Yes(9)));
  // A niche in the widened tag of another enum.
  assert_eq!(puns::pun::<Outer, Outer>(Outer::Other), Outer::Other);
  // One variant that has values, and a tag for the other.
  assert_eq!(puns::try_pun::<[u8; 8], OneOfTwo>([0, 3, 0, 0, 0, 0, 0, 0]), Ok(OneOfTwo::A(3)));
  assert!(puns::try_pun::<[u8; 8], OneOfTwo>([1, 3, 0, 0, 0, 0, 0, 0]).is_err());
}

/// An enum with more than 256 variants, whose tag is two bytes.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
enum Many {
  First(u16),
  V0,
  V1,
  V2,
  V3,
  V4,
  V5,
  V6,
  V7,
  V8,
  V9,
  V10,
  V11,
  V12,
  V13,
  V14,
  V15,
  V16,
  V17,
  V18,
  V19,
  V20,
  V21,
  V22,
  V23,
  V24,
  V25,
  V26,
  V27,
  V28,
  V29,
  V30,
  V31,
  V32,
  V33,
  V34,
  V35,
  V36,
  V37,
  V38,
  V39,
  V40,
  V41,
  V42,
  V43,
  V44,
  V45,
  V46,
  V47,
  V48,
  V49,
  V50,
  V51,
  V52,
  V53,
  V54,
  V55,
  V56,
  V57,
  V58,
  V59,
  V60,
  V61,
  V62,
  V63,
  V64,
  V65,
  V66,
  V67,
  V68,
  V69,
  V70,
  V71,
  V72,
  V73,
  V74,
  V75,
  V76,
  V77,
  V78,
  V79,
  V80,
  V81,
  V82,
  V83,
  V84,
  V85,
  V86,
  V87,
  V88,
  V89,
  V90,
  V91,
  V92,
  V93,
  V94,
  V95,
  V96,
  V97,
  V98,
  V99,
  V100,
  V101,
  V102,
  V103,
  V104,
  V105,
  V106,
  V107,
  V108,
  V109,
  V110,
  V111,
  V112,
  V113,
  V114,
  V115,
  V116,
  V117,
  V118,
  V119,
  V120,
  V121,
  V122,
  V123,
  V124,
  V125,
  V126,
  V127,
  V128,
  V129,
  V130,
  V131,
  V132,
  V133,
  V134,
  V135,
  V136,
  V137,
  V138,
  V139,
  V140,
  V141,
  V142,
  V143,
  V144,
  V145,
  V146,
  V147,
  V148,
  V149,
  V150,
  V151,
  V152,
  V153,
  V154,
  V155,
  V156,
  V157,
  V158,
  V159,
  V160,
  V161,
  V162,
  V163,
  V164,
  V165,
  V166,
  V167,
  V168,
  V169,
  V170,
  V171,
  V172,
  V173,
  V174,
  V175,
  V176,
  V177,
  V178,
  V179,
  V180,
  V181,
  V182,
  V183,
  V184,
  V185,
  V186,
  V187,
  V188,
  V189,
  V190,
  V191,
  V192,
  V193,
  V194,
  V195,
  V196,
  V197,
  V198,
  V199,
  V200,
  V201,
  V202,
  V203,
  V204,
  V205,
  V206,
  V207,
  V208,
  V209,
  V210,
  V211,
  V212,
  V213,
  V214,
  V215,
  V216,
  V217,
  V218,
  V219,
  V220,
  V221,
  V222,
  V223,
  V224,
  V225,
  V226,
  V227,
  V228,
  V229,
  V230,
  V231,
  V232,
  V233,
  V234,
  V235,
  V236,
  V237,
  V238,
  V239,
  V240,
  V241,
  V242,
  V243,
  V244,
  V245,
  V246,
  V247,
  V248,
  V249,
  V250,
  V251,
  V252,
  V253,
  V254,
  V255,
  V256,
  V257,
  V258,
  V259,
  V260,
  V261,
  V262,
  V263,
  V264,
  V265,
  V266,
  V267,
  V268,
  V269,
  V270,
  V271,
  V272,
  V273,
  V274,
  V275,
  V276,
  V277,
  V278,
  V279,
  V280,
  V281,
  V282,
  V283,
  V284,
  V285,
  V286,
  V287,
  V288,
  V289,
  V290,
  V291,
  V292,
  V293,
  V294,
  V295,
  V296,
  V297,
  V298,
}

/// A generic enum of two variants with a field each, as `Result` is.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
enum Either<T, E> {
  Left(T),
  Right(E),
}

/// An enum with a field that requires some values of its type (`valid(..)`). The requirement is
/// not a niche, and it is checked.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
enum Restricted {
  Small(#[pun(valid(0..=9))] u8),
  Other(u8),
}

#[test]
fn many_variants() {
  // The tag is the first two bytes, which every value has initialized: reinterpreted as such
  // (proven), without the check of 300 variants, which is the one of any enum and costs seconds to
  // build.
  assert_eq!(puns::head::<Many, [u8; 2]>(Many::V297), bytes::<2>(298));
  assert_eq!(puns::head::<Many, [u8; 2]>(Many::First(7)), bytes::<2>(0));
  assert_eq!(puns::pun::<Many, Many>(Many::V3), Many::V3);
}

#[test]
fn generic_and_restricted() {
  // `Left` alone has values: a struct of a `u32`.
  assert_eq!(puns::pun::<Either<u32, Infallible>, u32>(Either::Left(7)), 7);
  assert_eq!(puns::try_pun::<u32, Either<u32, Infallible>>(7), Ok(Either::Left(7)));
  // A niche in a niche.
  assert_eq!(puns::try_pun::<u8, Maybe<MaybeFlag>>(0), Ok(Maybe::Yes(MaybeFlag::Flag(false))));
  let no: u8 = puns::pun(Maybe::<MaybeFlag>::No);
  assert_eq!(puns::try_pun::<u8, Maybe<MaybeFlag>>(no), Ok(Maybe::No));
  // `valid(..)` is checked, and is not a niche: the tag is before the byte.
  assert_eq!(puns::try_pun::<[u8; 2], Restricted>([0, 9]), Ok(Restricted::Small(9)));
  assert!(puns::try_pun::<[u8; 2], Restricted>([0, 10]).is_err());
  assert_eq!(puns::try_pun::<[u8; 2], Restricted>([1, 10]), Ok(Restricted::Other(10)));
}

// --- Enums made of what is not an enum -----------------------------------------------------------

/// An enum whose layout Rust specifies. It is a tag of one byte, then the byte of `Some`.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
enum Opt {
  None,
  Some(u8),
}

/// A struct of two bytes, the first of which is always 1.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Tagged {
  tag: puns::num::u8c<1>,
  value: u8,
}

#[test]
fn from_other_than_enums() {
  // Every byte is a `Maybe<NonZero<u8>>`: proven, nothing checked.
  assert_eq!(puns::pun::<u8, Maybe<NonZero<u8>>>(0), Maybe::No);
  assert_eq!(puns::pun::<u32, Maybe<NonZero<u32>>>(9), Maybe::Yes(NonZero::new(9).unwrap()));
  // A `bool` is a `MaybeFlag`, which only some bytes are.
  assert_eq!(puns::pun::<bool, MaybeFlag>(true), MaybeFlag::Flag(true));
  assert_eq!(puns::try_pun::<u8, MaybeFlag>(1), Ok(MaybeFlag::Flag(true)));
  // A tag that is always the one of `Some`, and what `Some` holds.
  let some: Opt = puns::pun(Tagged { tag: puns::num::u8c::new(), value: 7 });
  assert_eq!(some, Opt::Some(7));
  // A tag of `None` or of `Some`, and a byte that `None` does not look at.
  assert_eq!(puns::pun::<(bool, u8), Opt>((false, 7)), Opt::None);
  assert_eq!(puns::pun::<(bool, u8), Opt>((true, 7)), Opt::Some(7));
  // Not every byte is a tag: checked.
  assert_eq!(puns::try_pun::<[u8; 2], Opt>([2, 0]), Err(Error::Validity));
}

// --- `Option` and `Result` -----------------------------------------------------------------------

#[test]
fn option_and_result() {
  // A niche in a `NonZero`: every `u32` is an `Option<NonZero<u32>>`, both ways proven.
  assert_eq!(puns::pun::<Option<NonZero<u32>>, u32>(None), 0);
  assert_eq!(puns::pun::<u32, Option<NonZero<u32>>>(5), NonZero::new(5));
  let none: &[Option<NonZero<u16>>] = puns::pun_ref(&[0u16, 3][..]);
  assert_eq!(none, &[None, NonZero::new(3)]);
  // A niche in a `bool`: `None` is 2, checked from a byte, proven from a `bool`.
  assert_eq!(puns::pun::<Option<bool>, u8>(None), 2);
  assert_eq!(puns::try_pun::<u8, Option<bool>>(2), Ok(None));
  assert_eq!(puns::try_pun::<u8, Option<bool>>(1), Ok(Some(true)));
  assert_eq!(puns::try_pun::<u8, Option<bool>>(3), Err(Error::Validity));
  assert_eq!(puns::pun::<bool, Option<bool>>(false), Some(false));
  // Two niches in one byte: `Some(None)` is 2, `None` where `rustc` put it (255 today, below the
  // valid values rather than after them), which is found, not assumed.
  let none: u8 = puns::pun(None::<Option<bool>>);
  assert!(none > 2);
  assert_eq!(puns::try_pun::<u8, Option<Option<bool>>>(none), Ok(None));
  assert_eq!(puns::try_pun::<u8, Option<Option<bool>>>(2), Ok(Some(None)));
  assert_eq!(puns::try_pun::<u8, Option<Option<bool>>>(none ^ 1), Err(Error::Validity));
  // A niche in a `char`.
  let none: u32 = puns::pun(None::<char>);
  assert_eq!(puns::try_pun::<u32, Option<char>>(none), Ok(None));
  assert_eq!(puns::try_pun::<u32, Option<char>>(0x41), Ok(Some('A')));
  // A direct tag of two bytes (widened to the alignment of the `u16`): no niche in a `u16`.
  assert_eq!(puns::try_pun::<[u8; 4], Option<u16>>([0, 0, 9, 9]), Ok(None));
  assert!(puns::try_pun::<[u8; 4], Option<u16>>([0, 9, 9, 9]).is_err());
  assert!(puns::try_pun::<[u8; 4], Option<u16>>([2, 0, 0, 0]).is_err());
  // `Ok` alone has values: a struct of its field.
  assert_eq!(puns::pun::<Result<u32, Infallible>, u32>(Ok(7)), 7);
  assert_eq!(puns::pun::<u32, Result<u32, Infallible>>(7), Ok(7));
  // Both have values: a tag, then the field.
  let err = puns::pun::<Result<u8, bool>, [u8; 2]>(Err(true));
  assert_eq!(puns::try_pun::<[u8; 2], Result<u8, bool>>(err), Ok(Err(true)));
  // Proven between them, variant by variant.
  assert_eq!(puns::pun::<Result<NonZero<u8>, bool>, Result<u8, u8>>(Err(true)), Err(1));
}

/// Checks that what is known of a niche is known of its own bytes only.
///
/// The niche of an `Option` of an array is in the first element. In `Some`, that element is not
/// the tag of `None`, which says nothing of the next elements. The puns that would take them to be
/// proven as the first is must not compile: the cases `niche_in_array*` in
/// `compile-fail-unstable/`.
#[test]
fn a_niche_in_an_array_is_in_its_first_element() {
  type Four = Option<[NonZero<u8>; 4]>;
  let four = |bytes: [u8; 4]| bytes.map(|byte| NonZero::new(byte).unwrap());
  // Proven: every byte is a value of an array of one element, and each `NonZero` is one.
  assert_eq!(puns::pun::<[u8; 1], Option<[NonZero<u8>; 1]>>([0]), None);
  assert_eq!(puns::pun::<[u8; 1], Option<[NonZero<u8>; 1]>>([7]), Some([NonZero::new(7).unwrap()]));
  assert_eq!(puns::pun::<[NonZero<u8>; 4], Four>(four([1, 2, 3, 4])), Some(four([1, 2, 3, 4])));
  // Checked: each element of `Some`, and nothing but the tag of `None`.
  assert_eq!(puns::try_pun::<[u8; 4], Four>([1, 2, 3, 4]), Ok(Some(four([1, 2, 3, 4]))));
  assert_eq!(puns::try_pun::<[u8; 4], Four>([1, 0, 0, 0]), Err(Error::Validity));
  assert_eq!(puns::try_pun::<[u8; 4], Four>([1, 2, 3, 0]), Err(Error::Validity));
  assert_eq!(puns::try_pun::<[u8; 4], Four>([0, 0, 0, 0]), Ok(None));
  assert_eq!(puns::try_pun::<[u8; 4], Four>([0, 9, 9, 9]), Ok(None));
  let bytes = [[1u8, 2], [3, 0]];
  assert!(puns::try_pun_ref::<[[u8; 2]; 2], Option<[[NonZero<u8>; 2]; 2]>>(&bytes).is_err());
  let bytes = [[1u8, 2], [3, 4]];
  let nested = puns::try_pun_ref::<[[u8; 2]; 2], Option<[[NonZero<u8>; 2]; 2]>>(&bytes).unwrap();
  assert_eq!(nested.map(|rows| rows.map(|row| row.map(NonZero::get))), Some([[1, 2], [3, 4]]));
}

/// A struct whose field requires one value and whose rule no value meets.
///
/// Safe code can make values of it all the same, because `valid(..)` and rules are requirements,
/// not guarantees of the type system.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(x == 1 && x == 2))]
struct Impossible {
  #[pun(valid(3))]
  x: u8,
}

#[derive(Pun, Clone, Copy)]
enum HoldsImpossible {
  A(Impossible),
  B(u16),
}

/// Checks that a layout has no values only where the type system says so, as for a field of a
/// type without values.
///
/// A requirement never makes a layout have no values. A variant taken to have no values would be
/// left out of what a source can be.
#[test]
fn no_value_is_what_the_type_system_says() {
  assert!(<Impossible as puns::Pun>::LAYOUT.is_inhabited());
  let layout = <HoldsImpossible as puns::Pun>::LAYOUT;
  assert!(layout.is_inhabited());
  // Both variants have tags: `A` is one that a value can be.
  assert_ne!(layout.variant_ranges(0), Some(&[][..]));
  assert_ne!(layout.variant_ranges(1), Some(&[][..]));
  // And a type without values is said to have none, as is what holds one.
  assert!(!<Infallible as puns::Pun>::LAYOUT.is_inhabited());
  assert!(!<(u8, Infallible) as puns::Pun>::LAYOUT.is_inhabited());
  assert!(<[Infallible; 0] as puns::Pun>::LAYOUT.is_inhabited());
  assert!(<Never as puns::Pun>::LAYOUT.is_inhabited());
  assert_eq!(<Never as puns::Pun>::LAYOUT.variant_ranges(1), None);
}

/// A scalar of 32 bytes, wider than the 16 of a `u128`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(transparent)]
struct Wide([u8; 32]);

// SAFETY: `Wide` is `repr(transparent)` over `[u8; 32]`, so `SPEC`, made for `Self`, has its size.
// The items of the `# Safety` of `Scalar` hold:
// - `SPEC` is made with `Spec::any`, and every byte of a `[u8; 32]` is initialized.
// - Every initialized value of its bytes is a valid `[u8; 32]`. `Wide` has no other invariant.
// - `Wide` holds no `UnsafeCell`.
unsafe impl puns::Scalar for Wide {
  const SPEC: puns::layout::Spec = puns::layout::Spec::any::<Self>();
}

#[test]
fn wide_scalars_in_option() {
  // The experiments write a sample of the scalar, of which a `u128` gives the first 16 bytes. A
  // debug build fails here if the others overflow.
  assert_eq!(size_of::<Option<Wide>>(), 33);
  // SAFETY: `Option<Wide>` and `[u8; 33]` have the same size, asserted above. `Some` of a `Wide`
  // has an alignment of 1 and no padding: its 33 bytes are its tag and its 32 bytes, all
  // initialized, which is a valid `[u8; 33]`.
  let bytes: [u8; 33] = unsafe { core::mem::transmute(Some(Wide([7; 32]))) };
  assert_eq!(puns::try_pun::<[u8; 33], Option<Wide>>(bytes), Ok(Some(Wide([7; 32]))));
}

/// An opaque offset ([`Spec::opaque`](puns::layout::Spec::opaque)), which a pun never makes.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(transparent)]
struct Off(NonZero<u32>);

// SAFETY: `Off` is `repr(transparent)` over a `NonZero<u32>`, so `SPEC`, made for `Self`, has its
// size and its values, every `u32` but zero, each a valid `Off` in Rust's sense. Where it aims is
// not in its bytes: `SPEC` is opaque. It holds no `UnsafeCell`.
unsafe impl puns::Scalar for Off {
  const SPEC: puns::layout::Spec =
    puns::layout::Spec::ranges::<Self>(puns::layout::Endian::NATIVE, &[(1, u32::MAX as u128)])
      .opaque()
      .not_a_number();
}

/// An enum of a specified representation, whose sample is `End`, which holds no `Off`.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
enum Link {
  End,
  Next(Off),
}

/// `puns` finds the layout of the enum, and of an `Option` of it, from a sample of each variant,
/// one of which holds the least `Off`.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
enum Linked {
  Value(u32),
  Link(Link),
}

#[test]
fn opaque_scalars_in_variants() {
  // A check never accepts the variant that holds an `Off`, in the enum or in an `Option` of it.
  let next = Linked::Link(Link::Next(Off(NonZero::new(8).unwrap())));
  assert_eq!(puns::try_pun_ref::<Linked, Linked>(&next), Err(Error::Validity));
  assert_eq!(puns::try_pun_ref::<_, Option<Linked>>(&Some(next)), Err(Error::Validity));
  let end = Linked::Link(Link::End);
  assert_eq!(puns::try_pun_ref::<Linked, Linked>(&end), Ok(&end));
  let value = Linked::Value(1);
  assert_eq!(puns::try_pun_ref::<Linked, Linked>(&value), Ok(&value));
  assert_eq!(puns::try_pun_ref::<_, Option<Linked>>(&Some(value)), Ok(&Some(value)));
  assert_eq!(puns::try_pun_ref::<_, Option<Linked>>(&None::<Linked>), Ok(&None));
  // A `Deferred` takes every variant, by the values of the `Off`.
  let deferred = puns::try_pun_ref::<Linked, puns::Deferred<Linked>>(&next).unwrap();
  assert!(puns::try_pun_ref::<_, puns::Deferred<Linked>>(deferred).is_ok());
  assert!(puns::try_pun::<_, puns::Deferred<Option<Linked>>>(Some(next)).is_ok());
}
