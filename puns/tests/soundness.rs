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
    pub use puns::layout::{
      Checksum, Crc, Endian, Enum, Field, Id, Layout, Op, Rule, Spec, Variant, Varint,
    };
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

/// Checks that a macro in the type of a field names one type, the type of the field in the item,
/// wherever the generated code uses it.
///
/// `this!()` is `Self`, which the derive cannot see in the tokens of the item: it is the struct in
/// the item, and other types in the items that the derive generates. The type of `on` in `Tail`
/// used to be `bool` in the layout and `u8` in the list of fields, which the check of `from_bytes`
/// reads, so a `bool` of 2 was valid. Where a macro names another type in the generated code than
/// in the item, or an attribute changes the item after the derive read it, the derive does not
/// compile: the cases `derive_field_type_shadowed` and `derive_item_rewritten` in `compile-fail/`.
#[test]
fn a_macro_in_a_field_names_the_type_of_the_field() {
  macro_rules! this {
    () => {
      Self
    };
  }
  macro_rules! byte {
    () => {
      u8
    };
  }
  trait Pick {
    type Out;
  }
  impl<T> Pick for T {
    type Out = u8;
  }
  #[derive(Pun)]
  #[repr(C)]
  struct Tail {
    on: <this!() as Pick>::Out,
    rest: [byte!()],
  }
  // Not the impl above, since `Tail` is unsized.
  impl Pick for Tail {
    type Out = bool;
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C, packed)]
  struct Packed<T> {
    on: bool,
    more: T,
  }
  #[derive(Pun, Clone, Copy, Debug)]
  #[repr(u8)]
  enum Choice {
    A(byte!()),
    B { on: bool },
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  union Either {
    on: bool,
    byte: byte!(),
  }
  assert!(puns::from_bytes::<Tail>(&[1, 7]).unwrap().on);
  assert_eq!(puns::from_bytes::<Tail>(&[2, 7]).err(), Some(Error::Validity));
  assert!(puns::try_pun::<[u8; 3], Packed<[byte!(); 2]>>([1, 2, 3]).is_ok_and(|packed| packed.on));
  assert!(puns::try_pun::<[u8; 3], Packed<[byte!(); 2]>>([2, 2, 3]).is_err());
  assert!(matches!(puns::try_pun::<[u8; 2], Choice>([0, 9]), Ok(Choice::A(9))));
  assert!(matches!(puns::try_pun::<[u8; 2], Choice>([1, 1]), Ok(Choice::B { on: true })));
  assert_eq!(puns::try_pun::<[u8; 2], Choice>([1, 2]).err(), Some(Error::Validity));
  assert_eq!(puns::pun::<Choice, [u8; 2]>(Choice::A(9)), [0, 9]);
  assert_eq!(puns::pun::<Choice, [u8; 2]>(Choice::B { on: true }), [1, 1]);
  // SAFETY: `byte` is a `u8`, valid for every byte.
  assert_eq!(puns::try_pun::<u8, Either>(2).map(|either| unsafe { either.byte }), Ok(2));
}

/// Checks that the offsets of the fields of a struct are those of the struct as compiled, measured
/// on it, where an attribute below `#[derive(Pun)]` changed the struct after the derive read it.
///
/// The derive used to compute the offsets of a `repr(C)` struct from the order of the fields and
/// the `repr` that it read, so a `bool` was read where the struct as compiled has a `u8`, and a
/// `bool` of 2 was valid.
#[test]
fn offsets_are_those_of_the_struct_as_compiled() {
  use core::mem::offset_of;
  /// Returns the bytes of a value whose byte `at` is `byte` and whose other bytes are 0.
  fn bytes<const N: usize>(at: usize, byte: u8) -> [u8; N] {
    let mut bytes = [0; N];
    bytes[at] = byte;
    bytes
  }
  // The fields reversed: `{ b: u8, a: bool }`.
  #[derive(Pun, Clone, Copy)]
  #[rewrite::reverse]
  #[repr(C)]
  struct Reversed {
    a: bool,
    b: u8,
  }
  // `repr(C)` removed: the compiler chooses the order.
  #[derive(Pun, Clone, Copy)]
  #[rewrite::repr()]
  #[repr(C)]
  struct Bare {
    a: u8,
    b: u32,
    c: bool,
  }
  // `packed` added: `c` at 3 rather than 4.
  #[derive(Pun, Clone, Copy)]
  #[rewrite::repr(C, packed)]
  #[repr(C)]
  struct Packed {
    a: u8,
    b: u16,
    c: bool,
  }
  // `packed` removed: `c` at 4 rather than 3.
  #[derive(Pun, Clone, Copy)]
  #[rewrite::repr(C)]
  #[repr(C, packed)]
  struct Unpacked {
    a: u8,
    b: u16,
    c: bool,
  }
  // Generic, the fields reversed: `{ b: T, a: bool }`.
  #[derive(Pun, Clone, Copy)]
  #[rewrite::reverse]
  #[repr(C)]
  struct Pair<T> {
    a: bool,
    b: T,
  }
  // Unsized, `packed` added: `c` at 3 rather than 4, the tail at 4 rather than 5.
  #[derive(Pun)]
  #[rewrite::repr(C, packed)]
  #[repr(C)]
  struct Tail {
    a: u8,
    b: u16,
    c: bool,
    rest: [u8],
  }
  // Larger than the memory in which the derive measures: the offsets of `repr(C)`.
  #[derive(Pun)]
  #[repr(C)]
  struct Large {
    head: [u8; 5000],
    on: bool,
  }

  let at = offset_of!(Reversed, a);
  assert_eq!(at, 1);
  assert!(puns::try_pun::<[u8; 2], Reversed>(bytes(at, 1)).is_ok_and(|value| value.a));
  assert!(puns::try_pun::<[u8; 2], Reversed>(bytes(at, 2)).is_err());
  let at = offset_of!(Bare, c);
  assert!(puns::try_pun::<[u8; 8], Bare>(bytes(at, 1)).is_ok_and(|value| value.c));
  assert!(puns::try_pun::<[u8; 8], Bare>(bytes(at, 2)).is_err());
  let at = offset_of!(Packed, c);
  assert_eq!(at, 3);
  assert!(puns::try_pun::<[u8; 4], Packed>(bytes(at, 1)).is_ok_and(|value| value.c));
  assert!(puns::try_pun::<[u8; 4], Packed>(bytes(at, 2)).is_err());
  let at = offset_of!(Unpacked, c);
  assert_eq!(at, 4);
  assert!(puns::try_pun::<[u8; 6], Unpacked>(bytes(at, 1)).is_ok_and(|value| value.c));
  assert!(puns::try_pun::<[u8; 6], Unpacked>(bytes(at, 2)).is_err());
  let at = offset_of!(Pair<u16>, a);
  assert_eq!(at, 2);
  assert!(puns::try_pun::<[u8; 4], Pair<u16>>(bytes(at, 1)).is_ok_and(|value| value.a));
  assert!(puns::try_pun::<[u8; 4], Pair<u16>>(bytes(at, 2)).is_err());
  let tail = puns::from_bytes::<Tail>(&[0, 0, 0, 1, 7]).unwrap();
  assert!(tail.c && tail.rest == [7]);
  assert!(puns::from_bytes::<Tail>(&[0, 0, 0, 2, 7]).is_err());
  let mut large = [0; 5001];
  large[5000] = 1;
  assert!(puns::from_bytes::<Large>(&large).unwrap().on);
  large[5000] = 2;
  assert!(puns::from_bytes::<Large>(&large).is_err());
}

/// Checks that the offsets of the fields of a variant of an enum with a primitive representation
/// are those of the enum as compiled, measured on a sample of the variant, where an attribute
/// below `#[derive(Pun)]` reversed them after the derive read the enum.
///
/// A tag that such an attribute makes wider than the one that the derive read is refused: the case
/// `derive_enum_tag_rewritten` in `compile-fail/`.
#[test]
fn variant_offsets_are_those_of_the_enum_as_compiled() {
  // The fields of `A` reversed: `A { b: u8, a: bool }`, `a` at 2.
  #[derive(Pun, Clone, Copy)]
  #[rewrite::reverse]
  #[repr(u8)]
  enum Reversed {
    A { a: bool, b: u8 },
    B,
  }
  assert!(matches!(
    puns::try_pun::<[u8; 3], Reversed>([0, 2, 1]),
    Ok(Reversed::A { a: true, b: 2 })
  ));
  assert!(puns::try_pun::<[u8; 3], Reversed>([0, 1, 2]).is_err());
  assert!(matches!(puns::try_pun::<[u8; 3], Reversed>([1, 9, 9]), Ok(Reversed::B)));
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
  enum Strict {
    A(Flags, bool),
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(u8)]
  enum Loose {
    A(Flags, u8),
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(u8)]
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

/// Checks that a rule that loads a field is checked where the field is initialized, and reads the
/// field, not the padding next to it.
///
/// A field of any value is proven by any initialized byte, and its rule loads it. Where the source
/// may leave the field uninitialized, in a union that a later field proves, the puns that would
/// check the rule must not compile: `try_union_proven_rule_loads` and
/// `try_union_proven_rule_loads_in_array` in `compile-fail/`.
#[test]
fn a_rule_reads_the_fields_that_it_loads() {
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  union Byte {
    any: u8,
    none: (),
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  #[pun(assert(x < 3))]
  struct Small {
    x: u8,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  struct Plain {
    x: u8,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  union Rule {
    small: Small,
    byte: Byte,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  union Unruled {
    plain: Plain,
    byte: Byte,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(u8)]
  enum Target {
    V(Rule),
    X,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(u8)]
  enum Free {
    V(Unruled),
    X,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  struct Source {
    tag: u8,
    byte: Byte,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  #[pun(assert(x < 3))]
  struct Padded {
    x: u8,
    y: u16,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  struct Gapped {
    x: u8,
    gap: Byte,
    y: u16,
  }

  // From initialized bytes: the rule of the first field holds for 1, and the second field takes 7.
  assert!(matches!(puns::try_pun::<[u8; 2], Target>([0, 1]), Ok(Target::V(_))));
  assert!(matches!(puns::try_pun::<[u8; 2], Target>([0, 7]), Ok(Target::V(_))));
  assert_eq!(puns::try_pun::<[u8; 2], Target>([2, 7]).err(), Some(Error::Validity));
  // Without a rule, nothing reads the byte, which is not written.
  let source = Source { tag: 0, byte: Byte { none: () } };
  assert!(matches!(puns::try_pun::<Source, Free>(source), Ok(Free::V(_))));
  // The rule loads `x`, and does not read the padding after it, which is not written.
  let gapped = Gapped { x: 1, gap: Byte { none: () }, y: 2 };
  assert_eq!(puns::try_pun::<Gapped, Padded>(gapped).map(|padded| padded.y), Ok(2));
  let gapped = Gapped { x: 5, ..gapped };
  assert_eq!(puns::try_pun::<Gapped, Padded>(gapped).err(), Some(Error::Validity));
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

/// Checks that a part whose layout Rust leaves to the compiler, which the source does not prove, is
/// checked only from initialized bytes.
///
/// A union that a later field proves drops what an earlier field does not prove. Where the union
/// is checked all the same, in an enum that is checked as a whole, its check tries the earlier
/// field where `rustc` put its fields. From bytes that may be uninitialized, this must not
/// compile: the cases `try_tuple_unproven_in_union` and `try_struct_unproven_in_union` in
/// `compile-fail/`. From initialized bytes, it compiles, and the answer does not depend on where
/// `rustc` put the fields, since the later field accepts every value.
#[test]
fn a_part_left_to_the_compiler_is_checked_only_from_initialized_bytes() {
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
    tuple: (bool, u8),
    pair: Pair,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(u8)]
  enum Target {
    V(Either),
    X,
  }

  // Whatever the bytes of the tuple, `pair` accepts them.
  for bytes in [[0, 0, 0], [0, 1, 7], [0, 2, 2], [0, 7, 1]] {
    assert!(matches!(puns::try_pun::<[u8; 3], Target>(bytes), Ok(Target::V(_))));
  }
  assert!(matches!(puns::try_pun::<[u8; 3], Target>([1, 2, 2]), Ok(Target::X)));
  assert_eq!(puns::try_pun::<[u8; 3], Target>([2, 0, 0]).err(), Some(Error::Validity));
}

/// Checks that a union proven by a field through a requirement is checked by its next field where
/// a value breaks the requirement.
///
/// Safe code may write any `u8` to a field restricted by `valid(..)`, or read by a rule. The same
/// requirement of the source proves that of the target, but its check fails then, and the check of
/// the union tries the next field. Where that field would read a byte that may be uninitialized in
/// the source, the pun must not compile: `try_union_proven_by_a_restriction` and
/// `try_union_proven_by_a_rule` in `compile-fail/`.
#[test]
fn a_union_proven_through_a_requirement_is_checked_past_it() {
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  struct Soft {
    #[pun(valid(2..=3))]
    x: u8,
    rest: u8,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  #[pun(assert(x > 1))]
  struct Ruled {
    x: u8,
    rest: u8,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  union EitherSoft {
    soft: Soft,
    flags: [bool; 2],
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  union EitherRuled {
    ruled: Ruled,
    flags: [bool; 2],
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(u8)]
  enum TargetSoft {
    V(EitherSoft),
    X,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(u8)]
  enum TargetRuled {
    V(EitherRuled),
    X,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  struct SourceSoft {
    tag: u8,
    soft: Soft,
  }
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  struct SourceRuled {
    tag: u8,
    ruled: Ruled,
  }

  // The requirement holds: the first field.
  let soft = SourceSoft { tag: 0, soft: Soft { x: 2, rest: 7 } };
  assert!(matches!(puns::try_pun::<SourceSoft, TargetSoft>(soft), Ok(TargetSoft::V(_))));
  let ruled = SourceRuled { tag: 0, ruled: Ruled { x: 2, rest: 7 } };
  assert!(matches!(puns::try_pun::<SourceRuled, TargetRuled>(ruled), Ok(TargetRuled::V(_))));
  // Safe code broke it: the second field, two `bool`s.
  let soft = SourceSoft { tag: 0, soft: Soft { x: 1, rest: 0 } };
  assert!(matches!(puns::try_pun::<SourceSoft, TargetSoft>(soft), Ok(TargetSoft::V(_))));
  let ruled = SourceRuled { tag: 0, ruled: Ruled { x: 1, rest: 0 } };
  assert!(matches!(puns::try_pun::<SourceRuled, TargetRuled>(ruled), Ok(TargetRuled::V(_))));
  // Neither field.
  let soft = SourceSoft { tag: 0, soft: Soft { x: 1, rest: 7 } };
  assert_eq!(puns::try_pun::<SourceSoft, TargetSoft>(soft).err(), Some(Error::Validity));
  let ruled = SourceRuled { tag: 0, ruled: Ruled { x: 1, rest: 7 } };
  assert_eq!(puns::try_pun::<SourceRuled, TargetRuled>(ruled).err(), Some(Error::Validity));
}
