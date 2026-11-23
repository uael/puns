//! Opaque scalars (`Spec::opaque`): read by their values, never a target, but in a `Deferred<T>`,
//! which `Validate` proves.

#[path = "support/opaque.rs"]
mod opaque;

use core::num::NonZero;

use opaque::{Context, Link, Node, Nullable, Off, Packed, Ruled};
use puns::{Deferred, Error, Pun};

#[test]
fn a_source_proves_its_values() {
  let node = Node { tag: 7, next: Off(NonZero::new(12).unwrap()) };
  let bytes: [u8; 8] = puns::pun(node);
  assert_eq!(&bytes[..4], &7u32.to_ne_bytes());
  assert_eq!(&bytes[4..], &12u32.to_ne_bytes());
  assert_eq!(puns::as_bytes(&node), &bytes);
  // Its values are not zero, which proves a `NonZero`.
  let next: NonZero<u32> = puns::pun(node.next);
  assert_eq!(next.get(), 12);
}

#[test]
fn a_variant_that_holds_one_is_never_made() {
  let value = [2u32, 5];
  assert_eq!(puns::from_bytes::<Link>(puns::as_bytes(&value)), Ok(&Link::Value(5)));
  let next = [1u32, 5];
  assert!(puns::from_bytes::<Link>(puns::as_bytes(&next)).is_err());
  let end = [0u32, 9];
  assert_eq!(puns::from_bytes::<Link>(puns::as_bytes(&end)), Ok(&Link::End));
}

#[test]
fn a_deferred_target_checks_everything_but_the_places() {
  let node = [7u32, 12];
  let deferred = puns::from_bytes::<Deferred<Node>>(puns::as_bytes(&node)).unwrap();
  assert_eq!(deferred.validated(), Ok(&Node { tag: 7, next: Off(NonZero::new(12).unwrap()) }));
  // Its values: zero is no offset, refused at its place.
  let zero = [7u32, 0];
  assert_eq!(
    puns::from_bytes::<Deferred<Node>>(puns::as_bytes(&zero)).err(),
    Some(Error::ValidityAt(4))
  );
  // The size and the alignment, as for any target.
  let bytes = puns::as_bytes(&node);
  assert_eq!(puns::from_bytes::<Deferred<Node>>(&bytes[..7]).err(), Some(Error::Size));
  let mut buffer = Aligned([0; 16]);
  buffer.0[1..9].copy_from_slice(bytes);
  assert_eq!(puns::from_bytes::<Deferred<Node>>(&buffer.0[1..9]).err(), Some(Error::Alignment));
  // A rule: where the struct is.
  assert_eq!(
    puns::from_bytes::<Deferred<Ruled>>(puns::as_bytes(&[30u32, 4])).err(),
    Some(Error::ValidityAt(0))
  );
  assert!(puns::from_bytes::<Deferred<Ruled>>(puns::as_bytes(&[3u32, 4])).is_ok());
  // Every variant of the enum, the one with the offset included, and not a tag of none.
  assert!(puns::from_bytes::<Deferred<Link>>(puns::as_bytes(&[1u32, 5])).is_ok());
  assert!(puns::from_bytes::<Deferred<Link>>(puns::as_bytes(&[2u32, 5])).is_ok());
  assert_eq!(
    puns::from_bytes::<Deferred<Link>>(puns::as_bytes(&[1u32, 0])).err(),
    Some(Error::ValidityAt(4))
  );
  assert_eq!(
    puns::from_bytes::<Deferred<Link>>(puns::as_bytes(&[3u32, 5])).err(),
    Some(Error::ValidityAt(0))
  );
}

#[test]
fn validate_proves_the_places() {
  let node = [7u32, 12];
  let deferred = puns::from_bytes::<Deferred<Node>>(puns::as_bytes(&node)).unwrap();
  // Within 16 bytes, but not within 12.
  assert!(deferred.validated_with(&mut Context { len: 16 }).is_ok());
  assert_eq!(deferred.validated_with(&mut Context { len: 12 }).err(), Some(12));
  // An offset that `Validate` refuses, which `puns` takes.
  let odd = [7u32, 6];
  let deferred = puns::from_bytes::<Deferred<Node>>(puns::as_bytes(&odd)).unwrap();
  assert_eq!(deferred.validated().err(), Some(6));
}

#[test]
fn every_function_takes_a_deferred_target() {
  let words = [7u32, 12, 9];
  let bytes = puns::as_bytes(&words);
  let (head, rest) = puns::from_head::<Deferred<Node>>(bytes).unwrap();
  assert_eq!((head.validated().map(|node| node.tag), rest.len()), (Ok(7), 4));
  let tail_words = [9u32, 7, 12];
  let (front, tail) = puns::from_tail::<Deferred<Node>>(puns::as_bytes(&tail_words)).unwrap();
  assert_eq!((front.len(), tail.validated().map(|node| node.tag)), (4, Ok(7)));
  // From the end: the offset is from the start of the bytes.
  assert_eq!(
    puns::from_tail::<Deferred<Node>>(puns::as_bytes(&[9u32, 7, 0])).err(),
    Some(Error::ValidityAt(8))
  );
  let pair = puns::try_pun_ref::<[u32; 2], Deferred<Node>>(&[7, 12]).unwrap();
  assert_eq!(pair.validated().map(|node| node.tag), Ok(7));
  // By value: the `Deferred` is a copy, proven where it is.
  let copy = puns::try_pun::<[u32; 2], Deferred<Node>>([7, 12]).unwrap();
  assert_eq!(copy.validated().map(|node| node.tag), Ok(7));
  assert_eq!(puns::try_pun::<[u32; 2], Deferred<Node>>([7, 0]).err(), Some(Error::ValidityAt(4)));
  // A slice: the element that fails, in a block of elements or after them.
  let mut nodes = [7u32; 40];
  nodes[33] = 0;
  assert_eq!(
    puns::from_bytes::<Deferred<[Node]>>(puns::as_bytes(&nodes)).err(),
    Some(Error::ValidityAt(33 * 4))
  );
}

#[test]
fn the_position_is_the_part_that_fails() {
  /// A field restricted by `valid(..)`.
  #[derive(Pun)]
  #[repr(C)]
  struct Kind {
    a: u8,
    #[pun(valid(1..=3))]
    kind: u8,
  }
  /// A string after a byte.
  #[derive(Pun)]
  #[repr(C)]
  struct Text {
    len: u8,
    text: str,
  }
  /// A union, of which a check accepts 0, 1, 4 and 5.
  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  union Flag {
    yes: bool,
    small: puns::num::u8in<4, 5>,
  }
  #[derive(Pun)]
  #[repr(C)]
  struct Flags {
    first: u8,
    flag: Flag,
  }

  // A block of 64 `bool`s is checked without a branch, then again, element by element, after it
  // fails.
  let mut bools = [1u8; 200];
  bools[150] = 7;
  assert_eq!(puns::from_bytes::<Deferred<[bool]>>(&bools).err(), Some(Error::ValidityAt(150)));
  bools[150] = 0;
  bools[199] = 9;
  assert_eq!(puns::from_bytes::<Deferred<[bool]>>(&bools).err(), Some(Error::ValidityAt(199)));
  // A restricted field: where the field is.
  assert_eq!(puns::from_bytes::<Deferred<Kind>>(&[0, 9]).err(), Some(Error::ValidityAt(1)));
  // A string: where it starts.
  assert_eq!(
    puns::from_bytes::<Deferred<Text>>(&[2, b'a', 0xff]).err(),
    Some(Error::ValidityAt(1))
  );
  // A union none of whose fields fits: where the union is.
  assert!(puns::from_bytes::<Deferred<Flags>>(&[9, 4]).is_ok());
  assert_eq!(puns::from_bytes::<Deferred<Flags>>(&[9, 3]).err(), Some(Error::ValidityAt(1)));
  // Without `Deferred`, no position.
  assert_eq!(puns::from_bytes::<Kind>(&[0, 9]).err(), Some(Error::Validity));
}

#[test]
fn an_option_of_one_is_one() {
  // `None` is zero, `Some` the bytes of the offset: every `u32`.
  let some = Nullable { tag: 1, next: Some(Off(NonZero::new(8).unwrap())), packed: None };
  let none = Nullable { tag: 2, next: None, packed: Some(Packed(NonZero::new(9).unwrap())) };
  assert_eq!(puns::pun::<Nullable, [u32; 3]>(some), [1, 8, 0]);
  assert_eq!(puns::pun::<Nullable, [u32; 3]>(none), [2, 0, 9]);
  let deferred = puns::from_bytes::<Deferred<Nullable>>(puns::as_bytes(&[3u32, 0, 5])).unwrap();
  assert_eq!(puns::as_bytes(deferred), puns::as_bytes(&[3u32, 0, 5]));
  assert!(puns::from_bytes::<Deferred<Nullable>>(puns::as_bytes(&[3u32, 5, 0])).is_ok());
  assert_eq!(
    puns::from_bytes::<Deferred<Nullable>>(&puns::as_bytes(&[3u32, 5, 0])[..11]).err(),
    Some(Error::Size)
  );
}

#[test]
fn a_rule_reads_one() {
  #[derive(Pun)]
  #[repr(C)]
  #[pun(assert(next >= 8))]
  struct Far {
    next: Off,
  }
  assert!(puns::from_bytes::<Deferred<Far>>(puns::as_bytes(&8u32)).is_ok());
  assert_eq!(
    puns::from_bytes::<Deferred<Far>>(puns::as_bytes(&4u32)).err(),
    Some(Error::ValidityAt(0))
  );
}

/// A variant that holds one in a tuple is never valid, but the enum is checked as a whole, and the
/// check of the variant reads the fields of the tuple where `rustc` put them. From initialized
/// bytes, it compiles, and only the other variant is ever made, whatever the order of the fields.
/// From bytes that may be uninitialized, it must not compile: `try_tuple_opaque_in_variant` in
/// `compile-fail/`.
#[test]
fn a_variant_that_holds_one_in_a_tuple_is_checked_from_initialized_bytes() {
  #[derive(Pun, Clone, Copy, Debug)]
  #[repr(u8)]
  enum Before {
    A((bool, Off)),
    B,
  }
  #[derive(Pun, Clone, Copy, Debug)]
  #[repr(u8)]
  enum After {
    A((Off, bool)),
    B,
  }

  let mut bytes = [0u8; 12];
  for flag in [0, 1, 2] {
    bytes[4..].fill(flag);
    assert_eq!(puns::try_pun::<[u8; 12], Before>(bytes).err(), Some(Error::Validity));
    assert_eq!(puns::try_pun::<[u8; 12], After>(bytes).err(), Some(Error::Validity));
  }
  bytes[0] = 1;
  assert!(matches!(puns::try_pun::<[u8; 12], Before>(bytes), Ok(Before::B)));
  assert!(matches!(puns::try_pun::<[u8; 12], After>(bytes), Ok(After::B)));
}

/// A variant that holds one has a sample, of the least values of its fields, from which the derive
/// reads its tag, whatever its discriminant is written with.
#[test]
fn a_variant_that_holds_one_has_a_sample() {
  const TWELVE: u8 = 12;

  macro_rules! two {
    () => {
      2
    };
  }

  #[derive(Pun, Clone, Copy, Debug, PartialEq)]
  #[repr(u8)]
  enum Declared {
    Literal(Off) = two!(),
    Next(u8),
    After(Off),
    Expression(u8) = 4 + 5,
    Constant(Off) = TWELVE,
  }

  // The tag of a value, as the enum holds it.
  fn tag(value: &Declared) -> u8 {
    // SAFETY: `Declared` is `repr(u8)`, so each value starts with its tag, a `u8`, which is
    // initialized. The pointer is that of a reference, so it is aligned and valid for reads.
    unsafe { *core::ptr::from_ref(value).cast::<u8>() }
  }

  // 3 and 9 are the variants that a pun makes. 2, 4 and 12 hold an offset.
  let pun = |tag| puns::try_pun::<[u8; 8], Declared>([tag, 0, 0, 0, 7, 0, 0, 0]);
  assert_eq!(pun(3), Ok(Declared::Next(0)));
  assert_eq!(pun(9), Ok(Declared::Expression(0)));
  for tag in [0, 2, 4, 10, 12] {
    assert_eq!(pun(tag), Err(Error::Validity));
  }
  // The tags of the layout are those of the enum.
  let offset = Off(NonZero::new(5).unwrap());
  let values = [
    Declared::Literal(offset),
    Declared::Next(0),
    Declared::After(offset),
    Declared::Expression(0),
    Declared::Constant(offset),
  ];
  for (i, value) in values.iter().enumerate() {
    assert_eq!(<Declared as Pun>::LAYOUT.variant_tag(i), u128::from(tag(value)));
  }
  assert_eq!(tag(&values[4]), 12);
  // A `Deferred` of it takes every variant.
  assert!(puns::try_pun::<[u8; 8], Deferred<Declared>>([2, 0, 0, 0, 7, 0, 0, 0]).is_ok());
}

/// An enum of the `Rust` representation that holds one in a variant: its layout is found on samples
/// (its puns, which need the `unstable` feature, are in `rust_enums.rs`).
#[test]
fn an_enum_of_the_rust_representation_holds_one() {
  #[derive(Pun, Clone, Copy)]
  enum Hop {
    Next(Off),
    End,
  }
  assert!(<Hop as Pun>::LAYOUT.holds_opaque());
  assert!(!<(u8, bool) as Pun>::LAYOUT.holds_opaque());
}

#[test]
fn a_generic_one_is_read_as_its_bytes() {
  let offset = opaque::Ref::<Node>::new(3);
  assert_eq!(puns::as_bytes(&offset), &3u32.to_ne_bytes());
}

/// Bytes aligned to 8, so that one byte past their start is not aligned to 4.
#[repr(C, align(8))]
struct Aligned([u8; 16]);
