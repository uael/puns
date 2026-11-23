//! # Using the functions
//!
//! The [reference](crate#reference) puts the functions in a table: a row for what they read, a
//! typed value or bytes, and a column for how much of it. This chapter says what each one checks
//! and in which order, with examples.
//!
//! - The functions in the first row of the table are proven, except for [`try_pun`],
//!   [`try_pun_ref`] and [`try_pun_mut`], which are checked.
//! - The functions that take a value, [`pun`], [`try_pun`], [`head`] and [`tail`], require both
//!   types to be `Copy`.
//! - The front or the back is the first or last `size_of::<D>()` bytes. If the target of
//!   [`head_ref`] or [`head_mut`] is a slice, it holds as many elements as fit. [`tail`],
//!   [`tail_ref`] and [`tail_mut`] require a sized source.
//! - The functions in the second row are checked, and each makes its checks in the order below.
//!   [`from_bytes`] and [`from_bytes_mut`] check the address, then the size. [`from_head`] and
//!   [`from_head_mut`] check that the bytes hold the sized part of the target, then the address,
//!   then the extent that its rules give. [`from_tail`] and [`from_tail_mut`] check the size first,
//!   since it decides where the last bytes start, then the address. All of them then check whatever
//!   the bytes do not prove about the target.
//! - [`from_bytes`], [`from_bytes_mut`] and [`as_bytes`] behave like `try_pun_ref::<[u8], D>`,
//!   `try_pun_mut::<[u8], D>` and `pun_ref::<S, [u8]>`. Because they fix one side to bytes, they
//!   take a single type parameter, which also keeps an array of unsuffixed integer literals from
//!   being inferred as an `[i32; N]`.
//! - [`from_head`] and [`from_head_mut`] take as many bytes as the `size` or `len` rule of the
//!   target says, or else as many elements as fit. [`from_tail`] and [`from_tail_mut`] require a
//!   sized target, because the extent of an unsized target cannot be found from its end.
//! - A view enum is an enum whose variants each hold a reference. [The derive](macro@Pun) gives it
//!   `from_bytes` and `from_head`, which return the first variant that the bytes match.
//!
//! None of these functions makes a value that holds a scalar that a pun never makes, such as the
//! offset of a self-relative pointer ([Scalars that a pun never
//! makes](super::never_made)), but they read such a value as plain data.
//!
//! ## Examples
//!
//! These examples apply the functions of the table to a few kinds of values:
//!
//! ```
//! use puns::{
//!   Pun,
//!   num::{u8in, u16be, u32be, u32le},
//! };
//!
//! // Reinterpret an `f32` as its bits. A `u32` is a `char` only if it is a Unicode scalar value.
//! assert_eq!(puns::pun::<f32, u32>(1.0), 0x3f80_0000);
//! assert_eq!(puns::try_pun::<u32, char>(0xe9), Ok('é'));
//! assert_eq!(puns::try_pun::<u32, char>(0xd800), Err(puns::Error::Validity));
//!
//! // The low half of a big-endian number is its last two bytes.
//! assert_eq!(puns::tail::<u32be, u16be>(0x1234_5678.into()).get(), 0x5678);
//! assert_eq!(puns::head::<u32le, u8>(0x1234_5678.into()), 0x78);
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
//! // Read strings from the front of a stream. `from_head` returns each one and the bytes after it.
//! let mut stream: &[u8] = b"\x05hello\x00\x05world\xff";
//! let mut words = vec![];
//! while let Ok((word, rest)) = puns::from_head::<Text>(stream) {
//!   words.push(&word.text);
//!   stream = rest;
//! }
//! assert_eq!((words, stream), (vec!["hello", "", "world"], &[0xff][..]));
//!
//! // Read a checksum from the back of a file.
//! let file = [b'd', b'a', b't', b'a', 0x78, 0x56, 0x34, 0x12];
//! let (data, checksum) = puns::from_tail::<u32le>(&file).unwrap();
//! assert_eq!((data, checksum.get()), (&b"data"[..], 0x1234_5678));
//!
//! // Reinterpreting bytes as ASCII is checked. Reinterpreting ASCII as a `str` is proven.
//! let ascii: &[u8in<0, 0x7f>] = puns::from_bytes(b"hello").unwrap();
//! let text: &str = puns::pun_ref(ascii);
//! assert_eq!(text, "hello");
//! ```
//!
//! ## In a constant
//!
//! Since every function is a `const fn`, bytes in a constant are checked while the program
//! compiles:
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
//! // A new game starts from a save in the binary. With 11 hp at level 1, the build fails.
//! const NEW_GAME: &Save = match puns::from_bytes(b"SAVE\x02\x01\x0a\x00") {
//!   Ok(save) => save,
//!   Err(_) => panic!("the built-in save is invalid"),
//! };
//! assert_eq!((NEW_GAME.level, NEW_GAME.hp.get()), (1, 10));
//! ```

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
