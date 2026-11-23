//! # Choosing a function
//!
//! A function of `puns` is chosen by what it reads, a typed value or bytes, and by how much of it:
//! the whole, the front or the back ([the table of functions](crate#going-further)). The choice also
//! decides whether a check at run time is possible. [`pun`], [`pun_ref`], [`pun_mut`], [`as_bytes`]
//! and the functions of the front and the back check nothing, and refuse a pair that needs a check.
//! The `try_` and `from_` functions check what the analysis could not prove ([Proven, checked and
//! refused](super::concepts#proven-checked-and-refused)). Each function lists the conditions of its
//! pairs under "Compile-time errors", and these are checked after monomorphization ([Compile-time
//! errors](super::errors)).
//!
//! ## Reinterpreting a value
//!
//! For a value that already has a type, use [`pun_ref`] or [`pun_mut`] to get a reference, or
//! [`pun`] to get a copy. [`pun_ref`], [`pun_mut`], [`try_pun_ref`] and [`try_pun_mut`] return a
//! reference to the same bytes as their argument. [`pun`], [`try_pun`], [`head`] and [`tail`] take
//! and return values, and require both types to be `Copy`. Where [`pun`] or [`pun_ref`] is refused
//! because some values of the source are not valid targets, [`try_pun`] or [`try_pun_ref`] checks
//! them at run time:
//!
//! ```
//! // Reinterpret an `f32` as its bits. A `u32` is a `char` only if it is a Unicode scalar value.
//! assert_eq!(puns::pun::<f32, u32>(1.0), 0x3f80_0000);
//! assert_eq!(puns::try_pun::<u32, char>(0xe9), Ok('é'));
//! assert_eq!(puns::try_pun::<u32, char>(0xd800), Err(puns::Error::Validity));
//! ```
//!
//! ## Reading bytes
//!
//! Bytes from outside the program, such as a file or a packet, are read with the `from_` functions,
//! which check them. Bytes that already have a type that proves the target are read with
//! [`pun_ref`] instead, which checks nothing.
//!
//! [`from_bytes`], [`from_bytes_mut`] and [`as_bytes`] are `try_pun_ref::<[u8], D>`,
//! `try_pun_mut::<[u8], D>` and `pun_ref::<S, [u8]>` with a single type parameter. Prefer them to
//! the `try_` functions for bytes: `try_pun_ref(&[1, 0])` infers its source as an `[i32; 2]`,
//! because an integer literal without a suffix is an `i32` unless something says otherwise.
//! [`from_head`] and [`from_head_mut`] take as many bytes as the `size` or `len` rule of the target
//! says, or else as many elements as fit, and return the bytes that follow:
//!
//! ```
//! use puns::Pun;
//!
//! /// A string of UTF-8, after its length in one byte.
//! #[derive(Pun)]
//! #[repr(C)]
//! #[pun(size = 1 + length)]
//! struct Text {
//!   length: u8,
//!   text: str,
//! }
//!
//! // Read strings from the front of a stream, until the bytes are not one.
//! let mut stream: &[u8] = b"\x05hello\x00\x05world\xff";
//! let mut words = vec![];
//! while let Ok((word, rest)) = puns::from_head::<Text>(stream) {
//!   words.push(&word.text);
//!   stream = rest;
//! }
//! assert_eq!((words, stream), (vec!["hello", "", "world"], &[0xff][..]));
//! ```
//!
//! [`from_tail`] and [`from_tail_mut`] take a value from the end of bytes, and return the bytes
//! before it. They require a sized target, because the extent of an unsized target cannot be found
//! from its end:
//!
//! ```
//! use puns::num::u32le;
//!
//! // Read a checksum from the back of a file.
//! let file = [b'd', b'a', b't', b'a', 0x78, 0x56, 0x34, 0x12];
//! let (data, checksum) = puns::from_tail::<u32le>(&file).unwrap();
//! assert_eq!((data, checksum.get()), (&b"data"[..], 0x1234_5678));
//! ```
//!
//! The `from_` functions check the size and the address, then the values of the target that the
//! bytes do not prove, rules included. [`from_bytes`] and [`from_bytes_mut`] check the address,
//! then the size. [`from_head`] and [`from_head_mut`] check that the bytes hold the sized part of
//! the target, then the address, then the extent that its rules give. [`from_tail`] and
//! [`from_tail_mut`] check the size first, since it decides where the last bytes start, then the
//! address. A view has a `from_bytes` and a `from_head` of its own ([Reading one of several
//! types](crate#reading-one-of-several-types)).
//!
//! ## Taking the front or the back
//!
//! [`head`], [`head_ref`] and [`head_mut`] take the front of a typed value, and [`tail`],
//! [`tail_ref`] and [`tail_mut`] its back. For a sized target, the front or the back is the first
//! or last `size_of::<D>()` bytes. If the target of [`head_ref`] or [`head_mut`] is a slice, it
//! holds as many elements as fit. The functions of the back require a sized source, because where
//! the last bytes of an unsized value start is known only at run time.
//!
//! ```
//! use puns::num::{u16be, u32be, u32le};
//!
//! // The low half of a big-endian number is its last two bytes.
//! assert_eq!(puns::tail::<u32be, u16be>(0x1234_5678.into()).get(), 0x5678);
//! assert_eq!(puns::head::<u32le, u8>(0x1234_5678.into()), 0x78);
//! ```
//!
//! These functions check nothing. Where the size of the front is read from the bytes, or the front
//! needs a check, read the bytes with [`from_head`] instead of [`head_ref`].
//!
//! ## Writing through `&mut`
//!
//! Any `D` may be written through a `&mut D`. A function that returns one must therefore hold the
//! other way as well: what is written must leave a valid source. The page of each function lists
//! what this requires of the pair.
//!
//! [`pun_mut`] and [`try_pun_mut`] let a write break a rule of the source, as a write to a field
//! may, because a rule is a [requirement](super::concepts#requirements). To check the value again,
//! pass its bytes, from [`as_bytes`], to [`from_bytes`]. [`head_mut`] and [`tail_mut`] are
//! stricter: they refuse a source in which a struct that has a rule overlaps the part, unless the
//! target has the same struct with the same rules there. A more precise analysis is [not done
//! yet](super::limitations#not-done-yet).
//!
//! Where [`pun_mut`] is refused because some values of the source are not valid targets,
//! [`try_pun_mut`] checks them. Where it is refused because a `D` written through it could be an
//! invalid source, as when `D` is an enum with more variants than the source, take a `&mut` to only
//! the part that is written, with [`head_mut`] or [`tail_mut`]:
//!
//! ```
//! use puns::Pun;
//!
//! /// A pixel. Its first three bytes are its color.
//! #[derive(Pun, Debug, PartialEq)]
//! #[repr(C)]
//! struct Rgba {
//!   red: u8,
//!   green: u8,
//!   blue: u8,
//!   alpha: u8,
//! }
//!
//! // Turn RGBA into BGRA in place, through a `&mut` to the color alone.
//! let mut pixel = Rgba { red: 255, green: 128, blue: 0, alpha: 200 };
//! puns::head_mut::<_, [u8; 3]>(&mut pixel).reverse();
//! assert_eq!(pixel, Rgba { red: 0, green: 128, blue: 255, alpha: 200 });
//! ```
//!
//! No function makes a value that holds an [opaque
//! scalar](super::concepts#unspecified-layouts-opaque-scalars-and-uninhabited-types). A function
//! may read such a value, as initialized bytes.
//!
//! ## In a constant
//!
//! Every function of `puns` is a `const fn`, and so is every function that the derive generates:
//! bytes in a constant can be checked while the program compiles. Only the implementations of
//! standard traits, such as `From` and `Display`, are not, since stable Rust cannot call a trait
//! method in `const`. `unwrap` is not `const` either: match on the result instead.
//!
//! A check of an address does not compile in a constant, because const evaluation cannot read an
//! address. `core` calls reading one as an integer undefined behavior there, and says that using
//! the integer stops the evaluation
//! ([`transmute`](core::mem::transmute#transmutation-between-pointers-and-integers)). A checked
//! function therefore compiles in a constant only where it checks no address, as for a target with
//! an alignment of 1:
//!
//! ```
//! use puns::{
//!   Pun,
//!   num::{u16le, u32bec},
//! };
//!
//! #[derive(Pun)]
//! #[repr(C)]
//! #[pun(assert(hp <= level * 10))]
//! struct Save {
//!   magic: u32bec<0x5341_5645>,
//!   version: u8,
//!   #[pun(valid(1..=99))]
//!   level: u8,
//!   hp: u16le,
//!   bag: [u8],
//! }
//!
//! // A new game starts from a save in the binary, which the build checks.
//! const NEW_GAME: &Save = match puns::from_bytes(b"SAVE\x02\x01\x0a\x00") {
//!   Ok(save) => save,
//!   Err(_) => panic!("the built-in save is invalid"),
//! };
//! assert_eq!((NEW_GAME.level, NEW_GAME.hp.get()), (1, 10));
//! ```
//!
//! With 11 hp at level 1, the rule does not hold, and the build fails:
//!
//! ```compile_fail,E0080
//! # use puns::{Pun, num::{u16le, u32bec}};
//! # #[derive(Pun)]
//! # #[repr(C)]
//! # #[pun(assert(hp <= level * 10))]
//! # struct Save {
//! #   magic: u32bec<0x5341_5645>,
//! #   version: u8,
//! #   #[pun(valid(1..=99))]
//! #   level: u8,
//! #   hp: u16le,
//! #   bag: [u8],
//! # }
//! const NEW_GAME: &Save = match puns::from_bytes(b"SAVE\x02\x01\x0b\x00") {
//!   Ok(save) => save,
//!   Err(_) => panic!("the built-in save is invalid"),
//! };
//! # let _ = NEW_GAME;
//! ```
//!
//! ```text
//! error[E0080]: evaluation panicked: the built-in save is invalid
//! ```

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
