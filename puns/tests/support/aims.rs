//! A self-relative pointer with an eager aim, derived as another crate (`nearest`) derives one: an
//! `i32` offset from its own address to a `Thin<T, i32>`.

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
/// exists only where `puns` checked its target, which `get` reads. `core` documents
/// `Option<Near<T>>` as the `Near`, `None` as zero.
#[derive(Pun)]
#[pun(bound(T: Pun))]
#[repr(transparent)]
pub struct Near<T: ?Sized> {
  /// The distance to the target, from the address of the `Near`.
  #[pun(aim(target: Thin<T, i32> = off as u128))]
  off: InPlace<NonZero<i32>>,
  _t: PhantomData<T>,
  _pin: PhantomPinned,
}

impl<T: ?Sized + Pun> Near<T> {
  /// Returns the target, read with the provenance of `image`.
  ///
  /// A reference to a `Near` borrows only its 4 bytes, and the stacked borrows model of Miri
  /// refuses a read of the target through a pointer made from it, though tree borrows accepts
  /// one. So the target is read with the provenance of the bytes that hold both.
  ///
  /// # Safety
  ///
  /// `image` holds the bytes in which `puns` checked this `Near`, and its target with it.
  pub unsafe fn get<'a>(&'a self, image: &'a [u8]) -> &'a T {
    #[expect(clippy::cast_ptr_alignment, reason = "the address of `self`, which is aligned")]
    let this = image.as_ptr().with_addr(ptr::from_ref(self).addr()).cast::<Self>();
    // SAFETY: A `Near` exists only where `puns` checked it with its target, in the bytes that it
    // checked, which `image` holds (the caller's contract), so `this` has the provenance of the
    // bytes of both, which are not written while `image` is borrowed.
    unsafe { Self::target(this) }.value()
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
