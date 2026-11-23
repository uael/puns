//! # Scalars that a pun never makes
//!
//! Some scalars are valid for reasons that are not in their bytes. The offset of a self-relative
//! pointer, like the ones in the regions of [`nearest`](https://docs.rs/nearest), is valid only at
//! its place in a buffer, and only if what it aims at is there. A handle or an index into a table
//! is another such scalar. A crate declares such a scalar as a [`Scalar`] whose `SPEC` is
//! [`Spec::never`](layout::Spec::never): it accepts no value, and its bytes are known to be
//! initialized.
//!
//! A pun reads such a scalar as initialized bytes, and never makes one, not even from a value of
//! the same type. Nothing proves a part of the target that holds one, so a pun or a check that
//! would make one does not compile, nor does a `&mut` through which one could be written. An enum
//! with a variant that holds one is the exception: a check never accepts that variant, so a pun to
//! the enum compiles if the source has no value of that variant, and a check refuses its tag at run
//! time. The crate that declares the scalar makes the values that hold it, and proves them. To
//! read such a value from bytes, it checks the bytes with [`validate_but_never`], which checks
//! every other part of the value, rules included. It then proves its scalars as only it can, and
//! makes the reference.
//!
//! An `Option` of such a scalar is one too, if it has the size of the scalar and every byte of
//! `None` is initialized. Neither `None` nor `Some` is checked. Otherwise, the `Option` does not
//! compile. `core` documents the size where the scalar is a `NonZero` integer or a
//! `repr(transparent)` struct around one ([`Option` representation][option representation]), and
//! `None` is then zero. For another scalar, such as a `repr(packed)` struct around a `NonZero`, the
//! size depends on the niche that `rustc` gives it, which no documentation promises. That decides
//! only whether the `Option` compiles, never what a pun checks or accepts, since nothing of it is
//! checked.
//!
//! [option representation]: https://doc.rust-lang.org/core/option/index.html#representation

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
