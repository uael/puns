//! What Rust keeps in every layout that it allows, which compiles without the `unstable` feature.
use core::{mem::ManuallyDrop, num::NonZero};

use puns::Pun;

/// The `Rust` representation, of fields that are the same bytes in any order.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
struct Twins {
  a: bool,
  b: bool,
}

/// The `Rust` representation, of which Rust keeps that every byte is initialized.
#[derive(Pun, Clone, Copy)]
struct Mixed {
  a: u8,
  b: bool,
}

/// A `repr(C)` struct that holds one of the `Rust` representation, at a place that is kept.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Holds {
  mixed: Mixed,
  word: u16,
}

/// No `repr`: only itself proves itself.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
enum Ticket {
  Free,
  Paid(NonZero<u32>),
}

fn main() {
  // Bytes of any value, in any order; the same bytes, in any order.
  assert_eq!(puns::pun::<[u8; 2], (u8, u8)>([1, 1]), (1, 1));
  assert_eq!(puns::try_pun::<[u8; 2], Twins>([1, 0]).map(|t| t.a ^ t.b), Ok(true));
  assert!(puns::try_pun::<[u8; 2], Twins>([1, 2]).is_err());
  // A field may be anywhere, so it is proven everywhere it may be: two `bool`s are a `bool` and
  // a byte, in either order; two `NonZero`s, a `NonZero` and a word.
  let mixed: Mixed = puns::pun([true, false]);
  assert!(mixed.a <= 1);
  let _: (NonZero<u16>, u16) = puns::pun([NonZero::<u16>::MIN; 2]);
  // Initialized bytes, wherever they are.
  let bytes: [u8; 2] = puns::pun(Mixed { a: 7, b: true });
  assert!(bytes.contains(&7) && bytes.contains(&1));
  let _: &[u8] = puns::as_bytes(&(true, 1u8));
  // Fields that are all a multiple of every alignment: no padding in any order.
  let _: [u8; 4] = puns::pun((1u16, [2u8; 2]));
  // The same type at the same place.
  let holds = Holds { mixed: Mixed { a: 1, b: false }, word: 2 };
  let _: Holds = puns::pun(holds);
  let _: Mixed = puns::head(holds);
  assert_eq!(puns::pun::<Ticket, Ticket>(Ticket::Free), Ticket::Free);
  // A `ManuallyDrop` of a type, which `core` lays out as the type.
  let _: Mixed = puns::pun(ManuallyDrop::new(holds.mixed));
  let _: ManuallyDrop<ManuallyDrop<Mixed>> = puns::pun(holds.mixed);
  // Where `core` documents `Option` and `Result`.
  assert_eq!(puns::pun::<u32, Option<NonZero<u32>>>(7), NonZero::new(7));
  assert_eq!(puns::try_pun::<u8, Result<NonZero<u8>, ()>>(0), Ok(Err(())));
  // A `MaybeUninit` of anything: its size is kept where the one of what it holds is.
  let _: core::mem::MaybeUninit<(u8, u16)> = puns::pun(0u32);
  // Zeros, which every order of the fields of `Mixed` is.
  let _: Mixed = puns::zeroed();
}
