//! # Minimum Rust version
//!
//! The design depends on what stable Rust can do in `const`. This table lists what such a design
//! can use, and the first stable release that provides it. `puns` is built without the first three
//! rows, which no stable release provides.
//!
//! | Feature                                                    | Stable                               |
//! |------------------------------------------------------------|--------------------------------------|
//! | [`TransmuteFrom`], [`ptr::metadata`]                       | no                                   |
//! | [const traits], [`generic_const_exprs`]                    | no                                   |
//! | reflection, [`offset_of!` on enums and slices][offset_of]  | no                                   |
//! | [`offset_of!`][offset_of] on sized fields                  | [1.77][1.77], also in unsized structs |
//! | inline `const { .. }` in generic code                      | [1.79][1.79]                         |
//! | `&mut` in a `const fn`, with loops and raw pointers        | [1.83][1.83]                         |
//! | [`size_of_val_raw`] and [`align_of_val_raw`] in `const`    | 1.99                                 |
//!
//! The last row sets the minimum version to 1.99. `puns` reads the layout of an unsized type (a
//! struct that ends with a slice or a `str`) in `const`, from a raw pointer with an arbitrary
//! length. `size_of_val` cannot do this, because it needs a reference, and a reference needs a
//! valid value. On 1.98, `puns` does not build, because `layout_for_ptr` and its use in `const` are
//! unstable there. CI builds the crate and its examples on 1.99.
//!
//! Const evaluation is bounded, since the deny-by-default lint [`long_running_const_eval`] stops it
//! after 2,000,000 basic blocks ([`LINT_TERMINATOR_LIMIT`] in `rustc`). The analysis therefore
//! cannot take a step for each byte of a type, nor a step or an instance for each element of an
//! array. Its cost grows with the number of fields, variants, ranges of values and states of
//! automata of the types, and where it would grow with their size, a search stops at a limit
//! ([Limits](super::limitations#limits)).
//!
//! [`TransmuteFrom`]: https://doc.rust-lang.org/core/mem/trait.TransmuteFrom.html
//! [`ptr::metadata`]: https://doc.rust-lang.org/core/ptr/fn.metadata.html
//! [const traits]: https://doc.rust-lang.org/nightly/unstable-book/language-features/const-trait-impl.html
//! [`generic_const_exprs`]: https://doc.rust-lang.org/nightly/unstable-book/language-features/generic-const-exprs.html
//! [offset_of]: https://doc.rust-lang.org/core/mem/macro.offset_of.html
//! [1.77]: https://blog.rust-lang.org/2024/03/21/Rust-1.77.0/
//! [1.79]: https://blog.rust-lang.org/2024/06/13/Rust-1.79.0/
//! [1.83]: https://blog.rust-lang.org/2024/11/28/Rust-1.83.0/
//! [`size_of_val_raw`]: https://doc.rust-lang.org/core/mem/fn.size_of_val_raw.html
//! [`align_of_val_raw`]: https://doc.rust-lang.org/core/mem/fn.align_of_val_raw.html
//! [`long_running_const_eval`]: https://doc.rust-lang.org/rustc/lints/listing/deny-by-default.html#long-running-const-eval
//! [`LINT_TERMINATOR_LIMIT`]: https://github.com/rust-lang/rust/blob/main/compiler/rustc_const_eval/src/const_eval/machine.rs

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
