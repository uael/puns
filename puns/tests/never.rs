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
  // At any address: no reference is made.
  let mut odd = [0u8; 9];
  odd[1..].copy_from_slice(puns::as_bytes(&node));
  assert_eq!(puns::validate_but_never::<Node>(&odd[1..]), Ok(()));
  odd[1..5].copy_from_slice(puns::as_bytes(&30u32));
  assert_eq!(puns::validate_but_never::<never::Ruled>(&odd[1..]), Err(puns::Error::Validity));
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

#[test]
fn a_generic_one_is_read_as_its_bytes() {
  let offset = never::Ref::<Node>::new(3);
  assert_eq!(puns::as_bytes(&offset), &3u32.to_ne_bytes());
}
