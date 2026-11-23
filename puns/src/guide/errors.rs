//! # Compile-time errors
//!
//! Most refusals are panics in `const` blocks inside the generic functions of `puns` (error E0080).
//! Because such a block depends on the type parameters, `rustc` only evaluates it when it generates
//! code for an instance of the function, after monomorphization. `cargo check` does not generate
//! code, so it does not report these errors, while `cargo build` does. For the same reason, a
//! generic function that calls into `puns` compiles fine until it is instantiated with types that
//! violate the conditions. Each function lists its conditions under "Compile-time errors". They
//! would be trait bounds if stable Rust had const traits.
//!
//! ## Reading an error
//!
//! A common mistake with `transmute` is reinterpreting a struct with padding as bytes. `puns`
//! refuses it, and the first lines of the error say why:
//!
//! ```compile_fail,E0080
//! #[derive(puns::Pun)]
//! #[repr(C)]
//! struct Header {
//!   kind: u8,
//!   length: u32,
//! }
//!
//! let bytes: &[u8; 8] = puns::pun_ref(&Header { kind: 1, length: 2 });
//! ```
//!
//! ```text
//! error[E0080]: evaluation panicked:
//!               cannot pun: a byte that the target requires to be initialized is padding in the source.
//!               This is at byte 1 of the target.
//! ...
//! note: the above error was encountered while instantiating `fn pun_ref::<Header, [u8; 8]>`
//!  --> src/main.rs:9:25
//!   |
//! 9 |   let bytes: &[u8; 8] = puns::pun_ref(&Header { kind: 1, length: 2 });
//!   |                         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
//! ```
//!
//! Byte 1 is the first of the three bytes of padding after `kind`. The last note gives the line of
//! your call. Each function in the table evaluates its constant itself, so the note names your call
//! rather than one inside `puns`. Between the first lines and that note, `rustc` prints about
//! thirty lines locating the failed constant inside `puns`, which you can skip.
//!
//! An optimized build may inline the function into yours before it evaluates the constant, and
//! `rustc` then prints no last note. The error still names the two types, as in
//! `Plan::<Header, [u8; 8]>`, but not the line of your code. A debug build, such as the one
//! `cargo build` makes, does name it.
//!
//! To fix this pun, declare the padding as a field, so that it is initialized:
//!
//! ```
//! #[derive(puns::Pun)]
//! #[repr(C)]
//! struct Header {
//!   kind: u8,
//!   reserved: [u8; 3],
//!   length: u32,
//! }
//!
//! let bytes: &[u8; 8] = puns::pun_ref(&Header { kind: 1, reserved: [0; 3], length: 2 });
//! assert_eq!(bytes[..4], [1, 0, 0, 0]);
//! ```
//!
//! ## Messages
//!
//! Every refusal prints a message that says why the pun is refused, and names the byte of the
//! target where the problem is, when there is one. Here are the most common messages, each with
//! code that compiles where the pun can be fixed.
//!
//! **The pun depends on a layout that Rust leaves to the compiler.** The fields of a struct of the
//! `Rust` representation may be in any order, so `puns` cannot tell which byte is the `bool`:
//!
#![cfg_attr(not(feature = "unstable"), doc = "```compile_fail,E0080")]
#![cfg_attr(feature = "unstable", doc = "```")]
//! #[derive(puns::Pun)]
//! struct Flagged {
//!   value: u8,
//!   flag: bool,
//! }
//!
//! let flagged: &Flagged = puns::from_bytes(&[1u8, 1]).unwrap();
//! ```
//!
//! ```text
//! cannot pun: the pun depends on a part of the source or of the target whose layout Rust leaves to the compiler (a struct, a tuple or an enum of the `Rust` representation, `Option` or `Result` of most types): another compiler, or `-Zrandomize-layout`, could arrange it otherwise. Only the same type at the same place proves it.
//! Add `#[repr(C)]` (or `#[repr(u8)]` to an enum), or enable the `unstable` feature of `puns`.
//! This is at byte 0 of the target.
//! ```
//!
//! With `#[repr(C)]`, the fields are in declaration order:
//!
//! ```
//! #[derive(puns::Pun)]
//! #[repr(C)]
//! struct Flagged {
//!   value: u8,
//!   flag: bool,
//! }
//!
//! let flagged: &Flagged = puns::from_bytes(&[7u8, 1]).unwrap();
//! assert!(flagged.flag);
//! ```
//!
//! **The sizes differ.**
//!
//! ```compile_fail,E0080
//! let n: u32 = puns::pun([1u8, 2, 3]);
//! ```
//!
//! ```text
//! cannot pun: the source is smaller than the target.
//! ```
//!
//! A source larger than the target gives "the source is larger than the target", and the message
//! suggests [`head`] and [`tail`], which take just a part of the source:
//!
//! ```
//! assert_eq!(puns::head::<[u8; 3], [u8; 2]>([1, 2, 3]), [1, 2]);
//! ```
//!
//! **The target needs a greater alignment than the source.** A `[u8; 4]` can be at any address,
//! but a `u32` cannot:
//!
//! ```compile_fail,E0080
//! let n: &u32 = puns::pun_ref(&[0u8; 4]);
//! ```
//!
//! ```text
//! cannot pun: the target has a greater alignment than the source.
//! Use `try_pun_ref` or `try_pun_mut` to check the address at run time.
//! ```
//!
//! [`pun`] takes the value, not a reference to it, so the address does not matter: it copies the
//! bytes into a `u32`. A [`num::u32le`] has an alignment of 1, so a reference to four bytes can be
//! reinterpreted as one. Otherwise, [`try_pun_ref`] checks the address at run time:
//!
//! ```
//! use puns::num::u32le;
//!
//! assert_eq!(puns::pun::<[u8; 4], u32>([1, 0, 0, 0]), u32::from_ne_bytes([1, 0, 0, 0]));
//! assert_eq!(puns::pun_ref::<[u8; 4], u32le>(&[1, 0, 0, 0]).get(), 1);
//! let n: Result<&u32, puns::Error> = puns::try_pun_ref(&[0u8; 4]);
//! assert!(n == Ok(&0) || n == Err(puns::Error::Alignment));
//! ```
//!
//! **Some values of the source are not valid for the target, and the function checks nothing.**
//!
//! ```compile_fail,E0080
//! let text: &str = puns::pun_ref(&[0xffu8; 4]);
//! ```
//!
//! ```text
//! cannot pun: some values of the source are not valid for the target, or do not prove one of its rules.
//! Use a `try_*` function to check them at run time.
//! This is at byte 0 of the target.
//! ```
//!
//! ```
//! assert_eq!(puns::try_pun_ref::<[u8; 4], str>(b"text"), Ok("text"));
//! assert_eq!(puns::try_pun_ref::<[u8; 4], str>(&[0xff; 4]), Err(puns::Error::Validity));
//! ```
//!
//! **A value that is not valid for the source could be written through a `&mut`.** Here, that value
//! would be bytes that are not UTF-8:
//!
//! ```compile_fail,E0080
//! let mut text = String::from("text");
//! let bytes: &mut [u8] = puns::pun_mut(text.as_mut_str());
//! ```
//!
//! ```text
//! cannot pun: some values of the target are not valid for the source (or are padding where the source is not), and could be written through the `&mut`.
//! ```
//!
//! Nothing is written through a shared reference, and every `str` is bytes:
//!
//! ```
//! let text = String::from("text");
//! let bytes: &[u8] = puns::pun_ref(text.as_str());
//! assert_eq!(bytes, b"text");
//! ```
//!
//! **No value of the source is valid for the target.** A byte that is always 2 is never a `bool`.
//! No function can make this pun, so the code that asks for it is a mistake:
//!
//! ```compile_fail,E0080
//! let never = puns::try_pun::<puns::num::u8c<2>, bool>(Default::default());
//! ```
//!
//! ```text
//! cannot pun: no value of the source is valid for the target.
//! This is at byte 0 of the target.
//! ```
//!
//! A byte that is padding in the source but must be initialized in the target is refused as well.
//! [Reading an error](#reading-an-error) shows its message, and
//! [Enums: what is refused](#enums-what-is-refused) shows it for an enum.
//!
//! ## Enums: what is refused
//!
//! A `Shape` is not a `[u8; 4]`, because the byte after the tag of a `Circle` is padding, and
//! nothing tells a `Circle` from a `Rect` at compile time:
//!
//! ```compile_fail,E0080
//! # use puns::Pun;
//! # #[derive(Pun, Clone, Copy)] #[repr(u8)] enum Shape { Circle { radius: u16 }, Rect(u8, bool) }
//! let _: [u8; 4] = puns::pun(Shape::Rect(3, true));
//! ```
//!
//! A `&mut Shape` is not a `&mut Drawing` either, since a `Drawing` written through it could be a
//! `Nothing`, or a `Rect` whose second field is not a `bool`:
//!
//! ```compile_fail,E0080
//! # use puns::Pun;
//! # #[derive(Pun, Clone, Copy)] #[repr(u8)] enum Shape { Circle { radius: u16 }, Rect(u8, bool) }
//! # #[derive(Pun, Clone, Copy)] #[repr(u8)]
//! # enum Drawing { Circle { radius: u16 }, Rect(u8, u8), Nothing }
//! let mut shape = Shape::Rect(3, true);
//! let _: &mut Drawing = puns::pun_mut(&mut shape);
//! ```

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
