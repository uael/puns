//! The two types are the same except for what is two pointers down: a `u8` reinterpreted as a
//! `bool`. Only the proof that follows the pointers, through the types, sees it.
//!
//! It declares its own pointer, and needs nothing else: `model.yml` builds it as an example, in
//! an optimized build, which must refuse it too.
use core::marker::PhantomData;

use puns::{
  Pun, SelfRelative,
  layout::{Endian, Pointer},
};

/// A pointer to a `T`, or none: the distance from its own address, an `i32`, or zero.
#[repr(transparent)]
pub struct Opt<T>(i32, PhantomData<T>);

impl<T> Opt<T> {
  pub const NONE: Self = Self(0, PhantomData);
}

// SAFETY: The type is its `i32`, and the only values of this program are null: none is followed.
unsafe impl<T: Pun> SelfRelative for Opt<T> {
  type Target = T;
  const POINTER: Pointer = Pointer::one::<T>("refused", 4, Endian::NATIVE).or_null();
}

puns::self_relative!(impl[T: Pun] Opt<T>);

mod a {
  use super::Opt;

  #[derive(puns::Pun)]
  #[repr(C)]
  pub struct List {
    pub head: u32,
    pub tail: Opt<List>,
    pub leaf: Opt<Leaf>,
  }

  #[derive(puns::Pun)]
  #[repr(C)]
  pub struct Leaf {
    pub flag: u8,
  }
}

mod b {
  use super::Opt;

  #[derive(puns::Pun)]
  #[repr(C)]
  pub struct List {
    pub head: u32,
    pub tail: Opt<List>,
    pub leaf: Opt<Leaf>,
  }

  #[derive(puns::Pun)]
  #[repr(C)]
  pub struct Leaf {
    pub flag: bool,
  }
}

fn main() {
  let list = a::List { head: 1, tail: Opt::NONE, leaf: Opt::NONE };
  let _: &b::List = puns::view_ref(&list);
}
