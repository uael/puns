//! A `core` that differs from the real one in a few items, for a crate that declares
//! `extern crate fakecore as core;`.
//!
//! The items are:
//!
//! - `mem::offset_of!`, which swaps the fields named `a` and `b`.
//! - `marker::PhantomData`, which is aligned to 16 bytes.
//! - `concat!`, `stringify!`, `module_path!`, `file!`, `line!` and `column!`, which give one
//!   value wherever they are, so that a name of a type made of them would name every type.
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

/// One string, whatever is concatenated.
#[macro_export]
macro_rules! concat {
  ($($t:tt)*) => {
    "one string"
  };
}

/// One string, whatever the tokens.
#[macro_export]
macro_rules! stringify {
  ($($t:tt)*) => {
    "one name"
  };
}

/// One module, wherever it is called.
#[macro_export]
macro_rules! module_path {
  () => {
    "one::module"
  };
}

/// One file, wherever it is called.
#[macro_export]
macro_rules! file {
  () => {
    "one file"
  };
}

/// One line, wherever it is called.
#[macro_export]
macro_rules! line {
  () => {
    1u32
  };
}

/// One column, wherever it is called.
#[macro_export]
macro_rules! column {
  () => {
    1u32
  };
}
