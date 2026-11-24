//! A scalar that a pun never makes (`Spec::never`): read as initialized bytes, never a target.

#[path = "support/never.rs"]
mod never;

use core::num::NonZero;

use never::{Link, Node, Nullable, Off, Packed};

#[test]
fn a_source_proves_initialized_bytes() {
  let node = Node { tag: 7, next: Off(NonZero::new(12).unwrap()) };
  let bytes: [u8; 8] = puns::pun(node);
  assert_eq!(&bytes[..4], &7u32.to_ne_bytes());
  assert_eq!(&bytes[4..], &12u32.to_ne_bytes());
  assert_eq!(puns::as_bytes(&node), &bytes);
}

#[test]
fn a_variant_that_holds_one_is_never_made() {
  let mut bytes = [0u8; 8];
  bytes[..4].copy_from_slice(&2u32.to_ne_bytes());
  bytes[4..].copy_from_slice(&5u32.to_ne_bytes());
  let aligned: [u32; 2] = puns::pun(bytes);
  assert_eq!(puns::from_bytes::<Link>(puns::as_bytes(&aligned)), Ok(&Link::Value(5)));
  let next = [1u32, 5];
  assert!(puns::from_bytes::<Link>(puns::as_bytes(&next)).is_err());
  let end = [0u32, 9];
  assert_eq!(puns::from_bytes::<Link>(puns::as_bytes(&end)), Ok(&Link::End));
}

#[test]
fn validate_but_never_checks_the_rest() {
  let node = [7u32, 12];
  assert_eq!(puns::validate_but_never::<Node>(puns::as_bytes(&node)), Ok(()));
  // The offset is not checked, not even that it is not zero.
  assert_eq!(puns::validate_but_never::<Node>(puns::as_bytes(&[7u32, 0])), Ok(()));
  assert_eq!(puns::validate_but_never::<Node>(&puns::as_bytes(&node)[..7]), Err(puns::Error::Size));
  // At any address: no reference is made. One byte past an address aligned to 8, so misaligned
  // for the `u32` of `Node`.
  let mut buffer = Aligned([0; 16]);
  let odd = &mut buffer.0[1..9];
  odd.copy_from_slice(puns::as_bytes(&node));
  assert_ne!(odd.as_ptr().addr() % align_of::<u32>(), 0);
  assert_eq!(puns::validate_but_never::<Node>(odd), Ok(()));
  odd[..4].copy_from_slice(puns::as_bytes(&30u32));
  assert_eq!(puns::validate_but_never::<never::Ruled>(odd), Err(puns::Error::Validity));
  // Every variant of the enum, the one with the offset included; not a tag of none.
  assert_eq!(puns::validate_but_never::<Link>(puns::as_bytes(&[1u32, 5])), Ok(()));
  assert_eq!(puns::validate_but_never::<Link>(puns::as_bytes(&[2u32, 5])), Ok(()));
  assert_eq!(
    puns::validate_but_never::<Link>(puns::as_bytes(&[3u32, 5])),
    Err(puns::Error::Validity)
  );
  // A rule holds or not.
  assert_eq!(puns::validate_but_never::<never::Ruled>(puns::as_bytes(&[3u32, 5])), Ok(()));
  assert_eq!(
    puns::validate_but_never::<never::Ruled>(puns::as_bytes(&[30u32, 5])),
    Err(puns::Error::Validity)
  );
}

#[test]
fn an_option_of_one_is_one() {
  // `None` is zero, `Some` the bytes of the offset: both are initialized bytes, proven.
  let some = Nullable { tag: 1, next: Some(Off(NonZero::new(8).unwrap())), packed: None };
  let none = Nullable { tag: 2, next: None, packed: Some(Packed(NonZero::new(9).unwrap())) };
  assert_eq!(puns::pun::<Nullable, [u32; 3]>(some), [1, 8, 0]);
  assert_eq!(puns::pun::<Nullable, [u32; 3]>(none), [2, 0, 9]);
  // Neither is checked: nothing in the bytes says whether an offset is valid.
  assert_eq!(puns::validate_but_never::<Nullable>(puns::as_bytes(&[3u32, 0, 5])), Ok(()));
  assert_eq!(puns::validate_but_never::<Nullable>(puns::as_bytes(&[3u32, 5, 0])), Ok(()));
  assert_eq!(
    puns::validate_but_never::<Nullable>(&puns::as_bytes(&[3u32, 5, 0])[..11]),
    Err(puns::Error::Size)
  );
}

/// A variant that holds one in a tuple is never valid, but the enum is checked as a whole, and the
/// check of the variant reads the fields of the tuple where `rustc` put them. From initialized
/// bytes, it compiles, and only the other variant is ever made, whatever the order of the fields.
/// From bytes that may be uninitialized, it must not compile: `try_tuple_never_in_variant` in
/// `compile-fail/`.
#[test]
fn a_variant_that_holds_one_in_a_tuple_is_checked_from_initialized_bytes() {
  #[derive(puns::Pun, Clone, Copy, Debug)]
  #[repr(u8)]
  enum Before {
    A((bool, Off)),
    B,
  }
  #[derive(puns::Pun, Clone, Copy, Debug)]
  #[repr(u8)]
  enum After {
    A((Off, bool)),
    B,
  }

  let mut bytes = [0u8; 12];
  for flag in [0, 1, 2] {
    bytes[4..].fill(flag);
    assert_eq!(puns::try_pun::<[u8; 12], Before>(bytes).err(), Some(puns::Error::Validity));
    assert_eq!(puns::try_pun::<[u8; 12], After>(bytes).err(), Some(puns::Error::Validity));
  }
  bytes[0] = 1;
  assert!(matches!(puns::try_pun::<[u8; 12], Before>(bytes), Ok(Before::B)));
  assert!(matches!(puns::try_pun::<[u8; 12], After>(bytes), Ok(After::B)));
}

/// A variant that holds one has no sample, from which the derive would read its tag. Its tag is
/// known from a discriminant without a macro, or from the variant before it. One that is not known
/// does not compile: `enum_discriminant_of_a_never_variant` in `compile-fail/`.
#[test]
fn a_variant_that_holds_one_has_a_tag_from_its_declaration() {
  const TWELVE: u8 = 12;

  #[derive(puns::Pun, Clone, Copy, Debug, PartialEq)]
  #[repr(u8)]
  enum Declared {
    Literal(Off) = 2,
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
    assert_eq!(pun(tag), Err(puns::Error::Validity));
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
    assert_eq!(<Declared as puns::Pun>::LAYOUT.variant_tag(i), u128::from(tag(value)));
  }
  assert_eq!(tag(&values[4]), 12);
}

#[test]
fn a_generic_one_is_read_as_its_bytes() {
  let offset = never::Ref::<Node>::new(3);
  assert_eq!(puns::as_bytes(&offset), &3u32.to_ne_bytes());
}

/// An offset to a `T`, in a generic type that derives `Pun`, whose identity is made of the one of
/// `T`.
#[derive(puns::Pun)]
#[repr(C)]
struct Boxed<T: puns::Pun> {
  offset: never::Ref<T>,
}

/// A type that holds an offset to itself, through `Boxed`.
#[derive(puns::Pun)]
#[repr(C)]
struct Chain {
  tag: u32,
  next: Boxed<Chain>,
}

/// The identity of `Boxed<Chain>` is made of the identity of `Chain`, not of its layout, which
/// holds the layout of `Boxed<Chain>`: this compiles.
#[test]
fn a_type_that_holds_itself_through_a_generic_type_has_a_layout() {
  let chain = Chain { tag: 7, next: Boxed { offset: never::Ref::new(8) } };
  let bytes = puns::as_bytes(&chain);
  assert_eq!(&bytes[..4], &7u32.to_ne_bytes());
  assert_eq!(&bytes[4..], &8u32.to_ne_bytes());
}

/// Bytes aligned to 8, so that one byte past their start is not aligned to 4.
#[repr(C, align(8))]
struct Aligned([u8; 16]);
