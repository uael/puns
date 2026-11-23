//! # The guide
//!
//! The [crate documentation](crate) shows what `puns` does, and its [reference](crate#reference)
//! lists the functions. The chapters of this guide explain the rest. Read them in order, or go to
//! the one you need:
//!
//! 1. [Using the functions](functions): what each function checks and in which order, with
//!    examples, in code and in constants.
//! 2. [Compile-time errors](errors): why a pun does not compile, how to read the error, and how to
//!    fix the most common ones.
//! 3. [Soundness](soundness): what a valid value is and what the analysis relies on, padding,
//!    unions, types without values, and what deriving `Pun` means.
//! 4. [Scalars that a pun never makes](never_made): scalars whose validity is not in their bytes,
//!    such as the offset of a self-relative pointer.
//! 5. [The `unstable` feature](unstable): the parts of a layout that Rust leaves to the compiler,
//!    and what `puns` does with them, with the feature and without it.
//! 6. [Minimum Rust version](msrv): what `puns` needs of stable Rust, and why that is 1.99.
//! 7. [Portability](portability): what `puns` assumes of the target, and how it is checked.
//! 8. [Limitations](limitations): the limits of the analysis, and what is not done yet.
//!
//! The options of the derive, rules included, are documented on [`Pun`](macro@Pun), and the numbers
//! of a fixed byte order in [`num`].

pub mod errors;
pub mod functions;
pub mod limitations;
pub mod msrv;
pub mod never_made;
pub mod portability;
pub mod soundness;
pub mod unstable;

// The items of the crate, which the links of this guide name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
