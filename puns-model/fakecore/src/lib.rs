//! A `core` that differs from the real one in two items, for a crate that declares
//! `extern crate fakecore as core;`.
//!
//! The two items are:
//!
//! - `mem::offset_of!`, which swaps the fields named `a` and `b`.
//! - `marker::PhantomData`, which is aligned to 16 bytes.
//!
//! Such a declaration is safe code, so the code that `#[derive(Pun)]` generates must not depend on
//! what `::core` names. The test `puns-model/tests/core_shadowed.rs` derives `Pun` in a crate that
//! declares it.
#![no_std]

/// The real `core`, which the macros of this crate name.
#[doc(hidden)]
pub use core as real;
pub use core::*;

/// The module `mem` of `core`, with another `offset_of!`.
pub mod mem {
  pub use core::mem::*;

  /// The offset of a field, except that the fields `a` and `b` of `Self` are swapped.
  #[macro_export]
  macro_rules! offset_of {
    (Self, a) => { $crate::real::mem::offset_of!(Self, b) };
    (Self, b) => { $crate::real::mem::offset_of!(Self, a) };
    ($($t:tt)*) => { $crate::real::mem::offset_of!($($t)*) };
  }

  pub use offset_of;
}

/// The module `marker` of `core`, with another `PhantomData`.
pub mod marker {
  pub use core::marker::*;

  /// A `PhantomData` aligned to 16 bytes, unlike the real one.
  #[repr(align(16))]
  pub struct PhantomData<T: ?Sized>(pub core::marker::PhantomData<T>);
}
