//! A self-relative pointer with an eager aim, derived as another crate derives one: an `i32` offset
//! from its own address to its pointee, a `Thin<T, i32>`, which is the value of a sized type, and,
//! for a list or a text, the `i32` length then the elements.

use core::{
  marker::{PhantomData, PhantomPinned},
  num::NonZero,
  ptr,
};

use puns::{InPlace, Pun, Thin};

/// A pointer to a `T`, stored as the distance from its own address, an `i32` of the byte order of
/// the target that is never zero. A slice or a `str` is behind its length, an `i32`.
///
/// Its offset is an `InPlace`, so it has no constructor, and is neither `Clone` nor `Copy`: one
/// exists only where `puns` checked its pointee, which `target` reads. `core` documents
/// `Option<Near<T>>` as the `Near`, `None` as zero.
#[derive(Pun)]
#[pun(bound(T: Pun))]
#[repr(transparent)]
pub struct Near<T: ?Sized> {
  /// The distance to the pointee, from the address of the `Near`, which reads it (`target`), as
  /// visible as the field.
  #[pun(pointer(target: Thin<T, i32>))]
  pub off: InPlace<NonZero<i32>>,
  _t: PhantomData<T>,
  _pin: PhantomPinned,
}

impl<T: ?Sized + Pun> Near<T> {
  /// Returns the pointee, read with the provenance of `image`.
  ///
  /// A reference to a `Near` borrows only its 4 bytes, and the stacked borrows model of Miri
  /// refuses a read of the pointee through a pointer made from it, which `target` makes, though
  /// tree borrows accepts one. So the pointee is read with the provenance of the bytes that hold
  /// both.
  ///
  /// # Safety
  ///
  /// `image` holds this `Near` and its pointee.
  pub unsafe fn get<'a>(&'a self, image: &'a [u8]) -> &'a T {
    #[expect(clippy::cast_ptr_alignment, reason = "the address of `self`, which is aligned")]
    let this = image.as_ptr().with_addr(ptr::from_ref(self).addr()).cast::<Self>();
    // SAFETY: A `Near` exists only where `puns` checked it with its pointee, and `image` holds
    // both (the caller's contract), so `this` has the provenance of the bytes of both, which are
    // not written while `image` is borrowed.
    unsafe { puns::aim(this, Self::AIMS.target) }
  }
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
