//! A self-relative pointer with an eager aim, derived as another crate derives one: an `i32` offset
//! from its own address to its pointee, a `Thin<T, i32>`, which is the value of a sized type, and,
//! for a list or a text, the `i32` length then the elements.

use core::{
  marker::{PhantomData, PhantomPinned},
  num::NonZero,
};

use puns::{InPlace, Pun, Thin};

/// A pointer to a `T`, stored as the distance from its own address, an `i32` of the byte order of
/// the target that is never zero. A slice or a `str` is behind its length, an `i32`.
///
/// Its offset is an `InPlace`, so it has no constructor, and is neither `Clone` nor `Copy`: one
/// exists only where `puns` checked its pointee, which `pointee` reads. `core` documents
/// `Option<Near<T>>` as the `Near`, `None` as zero.
#[derive(Pun)]
#[pun(bound(T: Pun))]
#[repr(transparent)]
pub struct Near<T: ?Sized> {
  /// The distance to the pointee, from the address of the `Near`, which reads it (`pointee`), as
  /// visible as the field.
  #[pun(pointer(pointee: Thin<T, i32>))]
  pub off: InPlace<NonZero<i32>>,
  _t: PhantomData<T>,
  _pin: PhantomPinned,
}

/// A pair of a name, numbers and the next pair.
#[derive(Pun)]
#[repr(C)]
pub struct Pair {
  pub name: Near<str>,
  pub values: Near<[u32]>,
  pub next: Option<Near<Pair>>,
}

/// A node whose children are a list.
#[derive(Pun)]
#[repr(C)]
pub struct Tree {
  pub id: u32,
  pub kids: Near<[Tree]>,
}

/// An expression whose operands are pointers.
#[derive(Pun)]
#[repr(u8)]
pub enum Expr {
  Lit(i64),
  Add(Near<Expr>, Near<Expr>),
}
