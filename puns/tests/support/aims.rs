//! A self-relative pointer with an eager aim, derived as another crate (`nearest`) derives one: an
//! `i32` offset from its own address to its pointee, which is the value of a sized type, and, for
//! a list or a text, a `Thin` in the format of `nearest`.

use core::{
  marker::{PhantomData, PhantomPinned},
  num::NonZero,
  ptr,
};

use puns::{InPlace, Pun, Thin};

/// What a `Near<Self>` points at: the value of a sized type, or a list or a text behind its
/// length, as `nearest` maps them.
pub trait Aimed: Pun {
  /// The pointee, whose value is `Self`.
  type Pointee: ?Sized + puns::Pointee<Value = Self>;
}

impl<T: Pun> Aimed for T {
  type Pointee = T;
}

impl<T: Pun> Aimed for [T] {
  type Pointee = Thin<[T], ListLen>;
}

impl Aimed for str {
  type Pointee = Thin<str, i32>;
}

/// What is stored before the elements of a list: its capacity, then its length, which are equal
/// in the bytes that `puns` checks, since they hold no spare capacity.
#[derive(Pun)]
#[pun(assert(cap as u128 == len as u128))]
#[repr(C)]
pub struct ListLen {
  cap: i32,
  len: i32,
}

/// A pointer to a `T`, stored as the distance from its own address, an `i32` of the byte order of
/// the pointee that is never zero. A slice or a `str` is behind its length, an `i32`.
///
/// Its offset is an `InPlace`, so it has no constructor, and is neither `Clone` nor `Copy`: one
/// exists only where `puns` checked its pointee, which `get` reads. `core` documents
/// `Option<Near<T>>` as the `Near`, `None` as zero.
#[derive(Pun)]
#[pun(bound(T: Aimed))]
#[repr(transparent)]
pub struct Near<T: ?Sized> {
  /// The distance to the pointee, from the address of the `Near`.
  #[pun(pointer(target: <T as Aimed>::Pointee = off as u128))]
  off: InPlace<NonZero<i32>>,
  _t: PhantomData<T>,
  _pin: PhantomPinned,
}

impl<T: ?Sized + Aimed> Near<T> {
  /// Returns the pointee, read with the provenance of `image`.
  ///
  /// A reference to a `Near` borrows only its 4 bytes, and the stacked borrows model of Miri
  /// refuses a read of the pointee through a pointer made from it, though tree borrows accepts
  /// one. So the pointee is read with the provenance of the bytes that hold both.
  ///
  /// # Safety
  ///
  /// `image` holds the bytes in which `puns` checked this `Near`, and its pointee with it.
  pub unsafe fn get<'a>(&'a self, image: &'a [u8]) -> &'a T {
    #[expect(clippy::cast_ptr_alignment, reason = "the address of `self`, which is aligned")]
    let this = image.as_ptr().with_addr(ptr::from_ref(self).addr()).cast::<Self>();
    // SAFETY: A `Near` exists only where `puns` checked it with its pointee, in the bytes that it
    // checked, which `image` holds (the caller's contract), so `this` has the provenance of the
    // bytes of both, which are not written while `image` is borrowed.
    unsafe { Self::target(this) }
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
