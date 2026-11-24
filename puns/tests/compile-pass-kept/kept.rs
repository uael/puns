//! What Rust keeps in every layout that it allows, which compiles without the `unstable` feature.
use core::{marker::PhantomData, mem::ManuallyDrop, num::NonZero};

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

/// A generic type of the `Rust` representation, whose parameter is bounded by `Pun`.
#[derive(Pun, Clone, Copy)]
struct Bounded<T: Pun> {
  a: T,
  b: bool,
}

/// The same, bounded in the `where` clause.
#[derive(Pun, Clone, Copy)]
struct Where<T>
where
  T: Pun,
{
  a: PhantomData<T>,
  b: u8,
  c: bool,
}

/// The same, bounded by `#[pun(bound(..))]`.
#[derive(Pun, Clone, Copy)]
#[pun(bound(T: Pun))]
struct Told<T> {
  a: PhantomData<T>,
  b: u8,
  c: bool,
}

/// The same, bounded by the derive, for its field of type `T`.
#[derive(Pun, Clone, Copy)]
struct Found<T> {
  a: T,
  b: bool,
}

/// A lifetime, which `rustc` does not lay out.
#[derive(Pun, Clone, Copy)]
struct Borrowed<'a, T: Pun> {
  a: T,
  b: bool,
  c: PhantomData<&'a ()>,
}

/// Const parameters of each kind that stable Rust allows.
#[derive(Pun, Clone, Copy)]
struct Valued<const N: i64, const C: char, const B: bool> {
  a: u8,
  b: bool,
}

/// An enum of the `Rust` representation, whose tag Rust leaves to the compiler.
#[derive(Pun, Clone, Copy)]
enum Either<T: Pun> {
  Left(T),
  Right(bool),
}

/// An enum without fields of the `Rust` representation, whose bytes Rust leaves to the compiler.
#[derive(Pun, Clone, Copy)]
enum Sign<const N: u8> {
  Plus,
  Minus,
}

/// Gives a `Borrowed` of any lifetime the lifetime `'static`, which is the same type to `rustc`.
fn forever<'a>(borrowed: Borrowed<'a, u8>) -> Borrowed<'static, u8> {
  puns::pun(borrowed)
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
  // An instance of a generic type, whose parameters are bounded by `Pun`.
  let bounded = Bounded { a: 1u8, b: true };
  let _: &Bounded<u8> = puns::pun_ref(&bounded);
  let _: Bounded<u8> = puns::pun(bounded);
  let _: Bounded<Mixed> = puns::pun(Bounded { a: holds.mixed, b: true });
  let _: Where<u16> = puns::pun(Where::<u16> { a: PhantomData, b: 1, c: true });
  let _: Told<u16> = puns::pun(Told::<u16> { a: PhantomData, b: 1, c: true });
  let _: Found<u8> = puns::pun(Found { a: 1u8, b: true });
  let _: Borrowed<'static, u8> = forever(Borrowed { a: 1u8, b: true, c: PhantomData });
  let _: Valued<-1, 'x', true> = puns::pun(Valued::<-1, 'x', true> { a: 1, b: true });
  let _: Either<u8> = puns::pun(Either::<u8>::Right(true));
  let _: Sign<7> = puns::pun(Sign::<7>::Minus);
  // Where `core` documents `Option` and `Result`.
  assert_eq!(puns::pun::<u32, Option<NonZero<u32>>>(7), NonZero::new(7));
  assert_eq!(puns::try_pun::<u8, Result<NonZero<u8>, ()>>(0), Ok(Err(())));
  // A `MaybeUninit` of anything: its size is kept where the one of what it holds is.
  let _: core::mem::MaybeUninit<(u8, u16)> = puns::pun(0u32);
  // Zeros, which every order of the fields of `Mixed` is.
  let _: Mixed = puns::zeroed();
}
