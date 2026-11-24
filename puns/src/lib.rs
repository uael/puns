#![doc = include_str!("../README.md")]
//!
//! # Reference
//!
//! A *pun* reinterprets the bytes of a value of one type, the *source* `S`, as a value of another
//! type, the *target* `D`. At compile time, `puns` puts each pun in one of three kinds:
//!
//! - **Proven**: every value of the source is a [valid] value of the target. The pun compiles to no
//!   check, though it may still compute an address or a length.
//! - **Checked**: only some values of the source are valid values of the target. A `try_` or
//!   `from_` function checks at run time what could not be proven.
//! - **Refused**: the pun can never be valid, or some part of it can be neither proven nor checked.
//!   It does not compile.
//!
//! `puns` works with plain data: structs and enums of any representation, `repr(C)` unions, tuples,
//! arrays, slices, structs that end with a slice, `str`, `MaybeUninit`, and types without values.
//! For an enum of the `Rust` representation, `puns` finds the layout that `rustc` chose, and checks
//! it against the layout that it assumes ([What `puns` assumes](#what-puns-assumes)).
//! `#[derive(Pun)]` also describes what the bytes of a value say about it: its size, the length of
//! its trailing slice, and conditions such as a checksum.
//!
//! |               | Whole                                  | Front                     | Back                      |
//! |---------------|----------------------------------------|---------------------------|---------------------------|
//! | a typed value | [`pun`], [`pun_ref`], [`pun_mut`], [`as_bytes`] (and [`try_pun`], [`try_pun_ref`], [`try_pun_mut`]) | [`head`], [`head_ref`], [`head_mut`] | [`tail`], [`tail_ref`], [`tail_mut`] |
//! | bytes         | [`from_bytes`], [`from_bytes_mut`] | [`from_head`], [`from_head_mut`] | [`from_tail`], [`from_tail_mut`] |
//!
//! Each row says what is read, a typed value or bytes, and each column says how much of it: the
//! whole, the front, or the back.
//!
//! Every function of `puns` is a `const fn`, and so is every function that [the derive](macro@Pun)
//! generates. Only the implementations of standard traits, such as `From` and `Display`, are not,
//! since stable Rust cannot call a trait method in `const`.
//!
//! - The functions in the first row are proven, except for [`try_pun`], [`try_pun_ref`] and
//!   [`try_pun_mut`], which are checked.
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
//! [`num`] has numbers of a fixed byte order, and integers restricted to some values. [`macro@Pun`]
//! documents the options of the derive. [`layout`] has the types that describe a layout, and the
//! checksums. The tests and the examples of the repository show more uses. Each example of a format
//! says which parts of the format it does not cover, and why.
//!
//! ## Using the functions
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
//!
//! # Soundness
//!
//! A pun only ever produces a `D` from bytes that are a valid `D`, and only produces a reference to
//! a `D` at an address that is [properly aligned] for `D`. Here, a *valid* `D` is a value that is
//! [valid] for `D` in the sense of the Reference and that also meets the invariants `puns` knows
//! about: a `str` is UTF-8, a type from [`num`] holds one of its values, and a type that derives
//! `Pun` meets what the derive was told (the values of fields marked `#[pun(valid(..))]`, and its
//! rules). The analysis either proves this at compile time, or the function checks it at run time
//! and returns an [`Error`] if the check fails. A pun with a part that can be neither proven nor
//! checked, such as a byte that may be padding, does not compile.
//!
//! What the derive is told is a requirement, which the type does not enforce ([Guarantees and
//! requirements](#guarantees-and-requirements)). Where the source has the same requirement at the
//! same place, `puns` takes it as met and does not check it again. If the source does not meet it,
//! because safe code wrote its fields or [`zeroed`] made it, the target does not meet it either.
//! Unsafe code may rely on what a type guarantees, but not on what it requires of a value that it
//! did not check itself.
//!
//! The analysis reads the layout of each type from its implementation of [`Pun`]: its size, its
//! alignment, the offset of each field, the values each scalar may hold, and its rules.
//! `#[derive(Pun)]` takes sizes and alignments from the compiler. It measures offsets on the type as
//! compiled, except in a variant of an enum of which it has no value. A layout never depends on a
//! path that the deriving crate could redefine (see [What the derive
//! trusts](macro@Pun#what-the-derive-trusts)). For the types of `core` that it supports, `puns`
//! implements `Pun` itself.
//!
//! `puns` relies on the layouts that the [Reference] specifies: `repr(C)`, `repr(transparent)`, the
//! primitive representations of enums, `packed` and `align`. It also relies on the layouts of
//! `Option` and `Result` that `core` documents. [What Rust documents](#what-rust-documents) lists
//! each of these guarantees with its source. Where Rust leaves a layout to the compiler, `puns`
//! reads the one that `rustc` chose for the build. It measures field offsets on a value, and finds
//! the tag of an enum of the `Rust` representation by experimenting on a value of each variant. An
//! experiment that fails stops the build, so no wrong layout is used. Beyond that, `puns` relies on
//! three behaviors of `rustc` that no documentation states:
//!
//! - A pointer to an unsized type has the representation of a pointer to a slice. This is checked
//!   at compile time for each unsized type, against a pointer made with an `as` cast.
//! - Const evaluation reports an error when it reads an uninitialized byte, when it reads the
//!   variant of a value whose tag is not the tag of a variant, and when it computes the size of a
//!   slice that is larger than an object can be. The experiments that find the layout of an enum of
//!   the `Rust` representation rely on the first two, and only run at compile time. The derive
//!   relies on the first to refuse an enum without fields that has padding, and on the third for an
//!   unsized struct whose tail is so large that 8 elements overflow an `isize`. Compile-fail tests
//!   check each of the three errors.
//! - Const evaluation reads the variant of a value whose tag is the tag of a variant from the tag
//!   alone, without reading or checking its other bytes, though `mem::discriminant` takes a
//!   reference to the whole value and the Reference requires a reference to point to a valid value.
//!   The experiments rely on it when they move the tag of one variant into a value of another. The
//!   model check compares the layouts that they find with oracles that do not use them.
//!
//! To check the alignment of an address at run time, `puns` reinterprets the address as an integer,
//! through a union, since no stable `const fn` gives it on Rust 1.99. At run time, `core` does not
//! call this undefined behavior, but it advises against it because its semantics are not settled.
//! No documentation promises that the integer is the address either. In const evaluation, `core`
//! calls it undefined behavior and says that using the integer stops the evaluation, so a check of
//! an address does not compile in a constant. The integer is never made into a pointer again, and
//! the tests run it under Miri with `-Zmiri-strict-provenance`.
//!
//! Without the `unstable` feature, whether a pun compiles, what it checks and which values a check
//! accepts do not depend on a layout that Rust leaves to the compiler: the order of the fields of a
//! struct or tuple of the `Rust` representation, the tag of an enum of that representation, or
//! niches. The exception is the layout of most enums of the `Rust` representation, `Option` and
//! `Result` included, which `puns` assumes and checks when compiling: a compiler that lays one out
//! otherwise makes it fail to compile, rather than change what a pun does ([What `puns`
//! assumes](#what-puns-assumes)). With the feature, `puns` also uses the layout that `rustc` chose.
//! [The `unstable` feature](#the-unstable-feature) states the rule and [what it relies
//! on](#what-this-relies-on).
//!
//! The tests run under [Miri], on the targets that [Portability](#portability) names. The analysis
//! itself is checked against oracles that do not use it, both with and without the `unstable`
//! feature.
//!
//! ## Guarantees and requirements
//!
//! The analysis relies only on what a type *guarantees*, no matter what safe code does: the valid
//! values of `bool`, `char`, enums with or without fields, and `NonZero`, the UTF-8 of `str`, and
//! the invariants of the types in [`num`], such as [`u16bec<V>`](num::u16bec) and [`u8in<MIN,
//! MAX>`](num::u8in), whose fields are private and whose constructors check them.
//!
//! A struct that derives `Pun` can only *require* values of its fields. `#[pun(valid(..))]` on a
//! field is a requirement, and so is a rule, because safe code can write any value to a `pub
//! version: u8`. `puns` proves or checks a requirement whenever it makes a value of the type,
//! though [`zeroed`] does not check rules. A requirement never proves what another type guarantees.
//! For example, a slice of a struct whose only field has `#[pun(valid(..=0x7f))]` is checked when
//! it is reinterpreted as a `str`, while a `[u8in<0, 0x7f>]` is proven to be one. For the same
//! reason, the accessors of regions truncate a region rather than trust the fields that give its
//! size. Two types have the same layout only if they require and guarantee the same values, so a
//! field `#[pun(valid(0..=1))] u8` is not a `bool`.
//!
//! A requirement that the source has too proves the same requirement of the target, but a check
//! of it may still fail on a value of the source. Where a union is proven by a field through a
//! requirement and checked all the same, its later fields may be tried, and what they read counts
//! as read (see [Unions](#unions)).
//!
//! ## Padding and `MaybeUninit`
//!
//! A pun is refused if a byte that the target requires to be initialized may be padding in the
//! source.
//!
//! `MaybeUninit<T>` is padding with the size and alignment of `T`, and it is only `Pun` if `T` is,
//! so that it never holds an `UnsafeCell`. As a target, it accepts anything. As a source, it proves
//! nothing except a target that accepts anything. So a `&mut MaybeUninit<T>` can only be made from
//! a source that may hold anything there, since whatever is written through it may be
//! uninitialized. A `&mut T` is made from a `MaybeUninit<T>` only if `T` requires no byte to be
//! initialized, since a byte that `T` requires would be read from padding.
//!
//! A rule that reads the bytes of its value by offset (`le`, `be`, a variable-length integer, a
//! checksum, a CRC, `matches`, or a `walk` whose expressions do) reads its padding as well. Such a
//! rule is checked when the source does not prove it. It is also checked with what it shares a
//! check with: in a variant of an enum that is checked as a whole, in an element of an array of
//! which another element is checked, and where every part of the target is checked. Its struct
//! must then have no byte that may be padding in the source.
//!
//! ## Unions
//!
//! The language has not settled the validity of unions ([`r-undefined.validity.union`][union
//! validity]). Some of the rules under discussion let a union hold any bytes, while others require
//! its bytes to be one of its fields, and safe code may write one field over another. So `puns`
//! accepts only what is sound under every one of these rules:
//!
//! - As a target, bytes are a union if they are one of its fields. This requires more than some of
//!   the rules do, and never less. A union is proven by any one field that the source proves, and
//!   otherwise it is checked field by field at run time.
//! - As a source, nothing is known about a union, not even that its bytes are initialized. Only the
//!   same union proves a union. A union may then hold a value even if none of its fields has one,
//!   so a value that holds it may exist, though no value of it can be made.
//!
//! A `repr(C)` union may be `packed` or `packed(N)`. Packing lowers its alignment to at most `N`,
//! which may lower its size, and moves no field: every field stays at offset 0
//! ([`r-layout.repr.c.union.size-align`][c union],
//! [`r-layout.repr.alignment.packed-fields`][alignment packed fields]). `puns` takes the size and
//! the alignment from the compiler. A field may then be at an address that is not aligned for its
//! type. `puns` checks such a field by reading its bytes, which needs no alignment, and makes no
//! reference to a field. Safe code reads it by copy, since a reference to a field that may be
//! unaligned does not compile.
//!
//! A field is only checked if no byte it reads may be padding in the source. If some field might
//! read padding and no field is proven, the pun is refused. Checking only the other fields would be
//! sound, but `puns` does not do it yet (see [Limitations](#limitations)).
//!
//! A union that is proven is not read. Some parts are checked together: the elements of an array,
//! and the fields of a variant of an enum that is checked as a whole. A union that is proven there
//! would be checked with the others, its fields tried in order. If that would read a byte that may
//! be padding in the source, or a byte of the same union, of which nothing is known, the pun is
//! refused. A byte that a field reads is one that it checks, a byte that a rule of a struct in it
//! loads, or any byte of a struct whose rule reads bytes by offset.
//!
//! ## Types without values
//!
//! `Infallible` and an enum without variants are types without values. No value of a type that has
//! values can be one of them, so such a pun is refused as never valid, and a variant or struct that
//! holds one has no value either. A slice of a type without values has one value, the empty one: a
//! pun that may give it an element is refused, unless the source ends with a slice of the same
//! elements at the same place. As a source, a variant without values needs no proof where an
//! enum is related to a target enum variant by variant, since no value is that variant. Against any
//! other target, the tag of the enum is read as a scalar that may hold the tag of that variant too,
//! and a struct that holds a type without values is related field by field, as any struct is.
//! Without the first rule, a variant of a `repr(u8)` enum whose field has no value would be proven
//! from a variant whose field does, because no byte of a zero-sized type is ever compared.
//!
//! No bytes are a variant without values of a target enum, or a field without values of a target
//! union. The analysis does not look at it, and a check reads nothing of it: bytes whose tag is
//! that variant's are refused, whatever its other fields would hold.
//!
//! A union none of whose fields has values is not a type without values as a source: a value of
//! it may exist ([Unions](#unions)), and so may a variant or a struct that holds it. An enum of the
//! `Rust` representation with such a variant is refused, since no sample of the variant can be made
//! to find its tag.
//!
//! ## What deriving `Pun` means
//!
//! `puns` knows a type by its layout and by what the type system enforces about its bytes, and
//! nothing more. This has two consequences, both by design.
//!
//! `PhantomData<T>` is a scalar with no bytes, for any `T`. Types that differ only in phantom
//! parameters or lifetimes have the same layout, and convert to each other freely. A type must not
//! derive `Pun` if its parameters mark something about its values: a unit, an invariant lifetime
//! that ties a value to a scope, or a marker of what a value was checked against.
//!
//! ```
//! use core::marker::PhantomData;
//!
//! #[derive(Clone, Copy)]
//! struct Meters;
//! #[derive(Clone, Copy)]
//! struct Feet;
//!
//! #[derive(puns::Pun, Clone, Copy)]
//! #[repr(C)]
//! struct Length<U> {
//!   value: u32,
//!   unit: PhantomData<U>,
//! }
//!
//! // Not what a unit is for: 3 meters are 3 feet.
//! let feet: Length<Feet> = puns::pun(Length::<Meters> { value: 3, unit: PhantomData });
//! assert_eq!(feet.value, 3);
//! ```
//!
//! Deriving `Pun` also makes every field as accessible as if it were public. A value can be made
//! from anything whose bytes are valid for its fields and rules, reinterpreted as anything its
//! bytes are, and written through a `&mut` to another type. The functions check what the derive was
//! told, and nothing that a constructor checks, so a type whose private fields keep an invariant
//! the derive does not know about must not derive `Pun`.
//!
//! ```
//! mod even {
//!   /// Only even numbers, says `new`.
//!   #[derive(puns::Pun, Clone, Copy)]
//!   #[repr(transparent)]
//!   pub struct Even(u32);
//!
//!   impl Even {
//!     pub const fn new(value: u32) -> Option<Self> {
//!       if value % 2 == 0 { Some(Self(value)) } else { None }
//!     }
//!
//!     pub const fn get(self) -> u32 {
//!       self.0
//!     }
//!   }
//! }
//!
//! assert!(even::Even::new(3).is_none());
//! // What `new` refuses, `pun` makes.
//! let odd: even::Even = puns::pun(3u32);
//! assert_eq!(odd.get(), 3);
//! ```
//!
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
//! cannot pun: the pun depends on a part of the source or of the target whose layout Rust leaves to the compiler (a struct or a tuple of the `Rust` representation, or an enum of it, `Option` and `Result` included, whose layout `puns` does not assume): another compiler, or `-Zrandomize-layout`, could arrange it otherwise. Only the same type at the same place proves it.
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
//!
//! # Aims
//!
//! An *aim* is a value at a place of the checked bytes that a struct points at, its *pointee*
//! ([`Aim`](layout::Aim), a rule of the struct). An aim says three things, and every combination
//! of them is an aim that `puns` checks and reads:
//!
//! - Where the pointee starts ([`Start`](layout::Start)): at the address of the struct plus an
//!   offset that an expression over the struct gives, an `i128` in two's complement, as for the
//!   self-relative pointers of [`nearest`](https://docs.rs/nearest); or at the *cursor* of the tail
//!   of the struct, where the pointee of the aim at the cursor before it ends, from the start of the
//!   tail, as for regions. The cursor is rounded up to the alignment of the pointee by its offset
//!   from the start of the struct, so that where a pointee is depends on the bytes, not on their
//!   address.
//! - How large it is ([`Extent`](layout::Extent)): the size of its type, sized or storing the
//!   length of its tail in its sized fields; a size in bytes that an expression gives; or the rest
//!   of the bytes, those of the tail at the cursor, and those that the check reads at a stored
//!   place.
//! - When it is checked ([`Check`](layout::Check)): with the struct (*eager*), or when it is read
//!   (*lazy*).
//!
//! The pointee is a [`Pointee`]: the value of a `Pun` type, or a [`Thin<T, M>`](Thin), a list or a
//! text in the format of `nearest`, a `T` right after an `M` that ends with the length of its tail,
//! at which the pointer points. Its type is named in the hidden list of the rules of the struct,
//! through which a check reaches the checks of the pointee, so that a type that points at itself
//! is checked by a recursive call, and through which a read names the type that it returns.
//!
//! - A check of a struct with an eager aim finds the pointee, in the bytes that the check reads:
//!   those of `from_bytes`, `from_head` and `from_tail`, or the source of a `try_` function. The
//!   pointee must be within them, aligned for its type, of a size that its type has, and every part
//!   of it is checked, its own eager aims included, one level deeper. A pointee may be before the
//!   struct, and may be shared by several aims. The check goes at most [`AIM_DEPTH`] levels deep,
//!   and checks pointees of at most twice as many bytes as it reads, so it ends on a cycle. Past
//!   either limit, it returns [`Error::Limit`]. It allocates nothing, and recurses once for each
//!   level, which takes about 10 KiB of stack in a build without optimizations, so that the
//!   deepest check of a list or a tree fits in 1 MiB.
//! - [`check`] checks bytes for a value at any depth, without recursion: the pointees that are
//!   still to be checked wait in a [`Worklist`] that the caller gives, depth first. It makes no
//!   reference, so the bytes may be at any address, and are checked as they will be once copied
//!   to an address of a given alignment. It can also require the bytes to be the compact image
//!   of the value, each pointee right after the one before it in that order, as a writer that
//!   copies the value makes them, and it says where bytes stop being a value ([`Failure`]).
//! - So a reference to a struct with an eager aim is a reference to a valid pointee, which code
//!   may read without a check. No function makes a value that holds an eager aim (`pun`,
//!   `try_pun`, `zeroed` and the other functions that return a value do not compile), nor gives a
//!   `&mut` to or from one (`pun_mut`, `try_pun_mut`, `from_bytes_mut` and the `_mut` parts), nor
//!   checks one from a source whose bytes may be padding (`try_pun_ref` from such a source). Only
//!   the same aim, at the same place of the source, of a pointee of the same identity, proves one,
//!   since its pointee was checked where the source was made. An instance of a generic type has an
//!   identity only if its parameters are bounded by `Pun` ([The same type](#the-same-type)), so a
//!   `Pair<O: Pun>` that points at a `Pair<O>` proves itself, and a `Pair<O>` with an unbounded `O`
//!   does not.
//! - A check of a struct with a lazy aim requires only that its pointee fits, so that the type of
//!   the pointee plays no part in the value of the struct, nor in a proof, unless its size is its
//!   own. What reads the pointee checks it then. Where safe code wrote the fields of the struct so
//!   that a region does not fit, the read truncates it to the tail, and a size whose expression
//!   fails takes what remains.
//!
//! The derive declares aims through two options ([Aims](macro@Pun#aims)), one for each place where
//! a pointee starts: `pointer(..)` on a field that holds where it is, and `regions(..)` on the last
//! field, whose tail the regions cut. They compose: a struct with regions may hold pointers, and
//! the pointee of a region may hold pointers too, whose pointees its read looks for within the
//! bytes of the struct, so that a pointer in a region may point at another region.
//!
//! A struct with an eager aim holds its field (the offset of a pointer, or the bytes of eager
//! regions) in an [`InPlace<T>`](InPlace), which no code makes, so no safe code makes a value of
//! the struct, which exists only where `puns` checked its pointee, or where unsafe code put it. A
//! struct with an eager aim is not `Clone`, `Copy` or `Default`, since an `InPlace` is none of
//! these. `tests/support/aims.rs` declares a self-relative pointer this way, as `nearest` does.
//!
//! The pointee is read by the methods that the derive writes for an aim with a name, which find it
//! as the check does. A lazy one is safe, and checks the pointee then. An eager one is `unsafe`:
//! the caller ensures that `puns` checked the value, or that unsafe code that wrote it upholds the
//! same. The read of an eager pointer takes a pointer to the value with the provenance of the bytes
//! of the pointee. A pointer made from a reference to the value has it under the tree borrows model
//! of Miri, but under stacked borrows, it is made from a pointer to the bytes that hold both, as
//! `tests/support/aims.rs` does.
//!
//! ```
//! use puns::{InPlace, Pun};
//!
//! /// A pointer to a `u32`, stored as the distance from its own address, in an `i32`.
//! #[derive(Pun)]
//! #[repr(transparent)]
//! struct Ptr {
//!   #[pun(pointer(target: u32 = off as u128))]
//!   off: InPlace<i32>,
//! }
//!
//! // A distance of 4, to the word after the pointer.
//! let words = [4u32, 7];
//! let bytes = puns::as_bytes(&words);
//! let ptr = puns::from_head::<Ptr>(bytes).unwrap().0;
//! // SAFETY: `puns` checked `ptr` with its pointee, in `bytes`, from which the pointer is made,
//! // and which are not written.
//! assert_eq!(unsafe { Ptr::target(bytes.as_ptr().cast()) }, &7);
//! // A distance of 8 is past the bytes, and -4 before them.
//! assert!(puns::from_head::<Ptr>(puns::as_bytes(&[8u32, 7])).is_err());
//! assert!(puns::from_head::<Ptr>(puns::as_bytes(&[(-4i32).cast_unsigned(), 7])).is_err());
//! ```
//!
//! # Cargo features
//!
//! ## The `unstable` feature
//!
//! Rust leaves parts of the layout of some types to the compiler
//! ([`r-layout.guarantees`][layout guarantees],
//! [`r-layout.repr.rust.unspecified`][rust unspecified]), which may choose differently in its next
//! version, or at random under [`-Zrandomize-layout`]. These parts are:
//!
//! - The order of the fields of a struct or tuple of the `Rust` representation, and therefore its
//!   size, so `(u8, u16, u8)` is 4 or 6 bytes.
//! - The position and the values of the tag, for an enum of the `Rust` representation.
//! - The niches of `Option` and `Result`, except where `core` documents them.
//!
//! `puns` reads what the compiler chose, so a pun is sound for the build that compiles it, whatever
//! the choice was. The `unstable` feature decides whether a pun must also behave the same under any
//! other choice. [`layout::UNSTABLE`] is `true` if the feature is enabled.
//!
//! The layouts that `puns` relies on without the feature are therefore of two kinds:
//!
//! - **Specified.** Rust documents them ([What Rust documents](#what-rust-documents)).
//! - **Assumed and checked when compiling.** `rustc` lays out an enum of the `Rust`
//!   representation by one algorithm, from the layouts of the fields of its variants, which Rust
//!   does not document. `puns` assumes that algorithm where its result is the same in every layout
//!   that Rust allows for the fields, computes the layout that it gives, and fails to compile if it
//!   is not the layout that `rustc` chose ([What `puns` assumes](#what-puns-assumes)).
//!
//! Every other layout that Rust leaves to the compiler is proven only by the same type, or field by
//! field ([Puns without the feature](#puns-without-the-feature)).
//!
//! ### The rule
//!
//! Without the `unstable` feature, a pun compiles only if `-Zrandomize-layout` cannot change what
//! it does (whether it compiles, what it checks, and which values a check accepts). The message of
//! a refusal is not part of this, and may name what the compiler chose. For example, reinterpreting
//! `(u8, u16, u8)` as `[u8; 4]` is refused because of its layout, or because of its size when it is
//! 6 bytes.
//!
//! With the feature, `puns` uses the layout that `rustc` chose. Within one build, a pun that
//! compiles without the feature also compiles with it, and does the same thing.
//!
//! The rule applies to a pun, not to a type. Reinterpreting `(bool, u8)` as bytes is proven
//! whichever byte holds the `bool`, but reinterpreting bytes as `(bool, u8)` would need a check of
//! one byte or the other, so that pun is refused.
//!
//! ### What a check may read
//!
//! A check never reads a byte of the source that holds a different field in another layout that
//! Rust allows. Such a check would read a different part of the same value, and could give a
//! different answer, so a check that reads a byte of a struct whose fields Rust may reorder is
//! refused. Only the bytes that a check reads count, as follows:
//!
//! - The check of an enum reads its tag, then the fields of the variant that the tag selects. A
//!   byte under a variant therefore counts only for the values of the source whose tag is that
//!   variant's. This holds where the source has an enum whose tag is at the same place, with the
//!   same size and byte order.
//! - The elements of an array share their checks. Where the source has elements of the same size at
//!   the same places, the check of every element reads the same bytes of its element as the check
//!   of the first one. Otherwise, every byte under the array counts.
//! - A checked rule may read any byte of its struct, so every byte of that struct counts.
//!
//! Which field a byte holds is not part of what a pun does. `pun::<[u8; 2], (u8, u8)>([1, 2])` is
//! `(1, 2)` or `(2, 1)`. Either is a valid value, and a pun promises a valid value and nothing
//! more, as `transmute` does. Which bytes a value of a struct of the `Rust` representation has is
//! the compiler's choice, with or without `puns`. But `puns` decides the result of a check on given
//! bytes, so that result is the same in every layout. For example, whether `try_pun::<(u8, u8),
//! F>((0, 2))` is `Ok`, for `F` a `repr(C)` struct of a `bool` and a `u8`, must not depend on which
//! byte holds the `0`.
//!
//! ### Puns without the feature
//!
//! | Pun, without `unstable` | | Why |
//! |---|---|---|
//! | `[u8; 2]` as `(u8, u8)`, and back | proven | bytes of any value, in either order |
//! | `[u8; 2]` as `(bool, bool)` | checked | both bytes are a `bool`, in either order |
//! | `(bool, u8)` as `[u8; 2]`, `as_bytes` | proven | both bytes are initialized, in either order |
//! | `[u8; 4]` as `(u8, u16)` | proven | it only requires initialized bytes, wherever its padding is |
//! | a `repr(C)` struct that holds a `(bool, u8)` as itself, or its first bytes as the `(bool, u8)` | proven | the same type at the same place |
//! | an enum of the `Rust` representation as itself | proven | a type has one layout in a program |
//! | `[bool; 2]` as `(bool, u8)` | proven | the `bool` may be either byte, and both bytes are a `bool` |
//! | `[u8; 2]` as `(bool, u8)` | refused | the byte that must be 0 or 1 is either byte |
//! | `(u8, u8)` as `[bool; 2]` | refused | the check of each element reads a byte that, in another layout, holds the other field |
//! | the first two bytes of `(u8, u16)` | refused | the second byte is padding in one layout and not in another |
//! | bytes as a `struct P { a: u8, b: u8 }` with the rule `a < b` | refused | the rule reads `a`, which is either byte |
//! | `(u8, u8)` as a `repr(C)` struct of a `bool` and a `u8` | refused | the check reads one field or the other, and `(0, 2)` passes in one order only |
//! | `(u8, u8)` as `NonZero<u16>` | refused | the check reads both bytes, and each may hold either field |
//! | a pun from `(u8, u16, u8)` or to it, other than to itself | refused | it is 4 or 6 bytes |
//! | bytes as `Option<bool>`, or as an enum of the `Rust` representation of which `puns` assumes the layout | checked | the layout is assumed, and checked when compiling ([What `puns` assumes](#what-puns-assumes)) |
//! | `Option<u16>` as a `repr(C)` struct of a `u16` tag and a `MaybeUninit<u16>` | proven | the tag 0 or 1, then the value, as assumed |
//! | bytes as `Option<(bool, u16)>`, or as an enum with a variant `A(bool, bool)` | refused | where the niche is depends on the order of the fields |
//!
//! A struct or tuple of the `Rust` representation whose size is the same in every layout, and that
//! has no rule, is proven field by field. Since a field may be anywhere, it must be proven at every
//! place where it may be. So two `bool`s prove a `(bool, u8)`, four initialized bytes prove a `(u8,
//! u16)`, and two `NonZero<u16>` prove a `(NonZero<u16>, u16)`. A struct with a rule, which names
//! fields, is proven only by itself, as an enum is. No check replaces a proof in a part whose
//! layout Rust leaves to the compiler, because the check would read where the compiler may not have
//! put the field, so whatever is not proven there does not compile. Such a part is still read by
//! the check of a union that a later field proves. The answer is then the same in every layout, and
//! the pun compiles only if every byte under the part is initialized in the source.
//!
//! ### The same type
//!
//! Without the feature, a part whose layout Rust leaves to the compiler, unless it is proven field
//! by field as above, is proven only by the same type at the same place. A type has one layout in a
//! program, but two types, however alike, may each be arranged differently ([`offset_of!`, "Layout
//! is subject to change"][offset_of layout]). `-Zrandomize-layout`, for example, arranges
//! `Wrapper<u8, u8>` and `Wrapper<u8, i8>` differently. Stable Rust cannot compare types in
//! `const`, since `TypeId` cannot be compared there and `type_name` is not a `const fn`.
//!
//! So `puns` identifies a type by a name, together with the identities of the types it is made of.
//! An identity has two uses, and no other. It tells whether two parts whose layout Rust leaves to
//! the compiler, and that are proven neither field by field nor by the layout that `puns` assumes,
//! are of the same type. And it names a type without its layout, so that the identity of a generic
//! type is made of the identities of its parameters, and a type that holds itself through a generic
//! type has one. A layout that Rust specifies, or that `puns` assumes, is compared by its parts,
//! never by an identity.
//! The primitive types, the types of [`num`], arrays, slices, tuples, `Option`, `Result`,
//! `MaybeUninit` and `ManuallyDrop` have identities. A part that is a `ManuallyDrop<T>` is of the
//! same type as a part that is a `T`, since `core` gives them one layout. A type made of a
//! `ManuallyDrop<T>` is not made of a `T`, though: `-Zrandomize-layout` arranges
//! `Wrapper<ManuallyDrop<u8>, u16, u32>` and `Wrapper<u8, u16, u32>` differently. A type that
//! derives `Pun` is named by where it is declared (the module, file, line and column), by its name,
//! and by the number of its derive in its crate. An instance of a generic type that derives `Pun`
//! is named, in addition, by the identity of each of its type parameters and the value of each of
//! its const parameters, in order, so a `Pair<i32>` proves a `Pair<i32>`, and not a `Pair<u32>`.
//! Its lifetimes do not count. The derive can name the identity of a type parameter only if the
//! parameter is bounded by `Pun` where it sees it: in its bounds (`<T: Pun>`), in the `where`
//! clause, in `#[pun(bound(..))]`, or by the bound that it adds for a field of type `T`. A generic
//! type with a type parameter that is not, such as one that is only in a `PhantomData<T>`, has no
//! identity, nor have a `PhantomData` and a [`Scalar`] implemented by hand, nor a type with a
//! parameter that has none. A part whose layout Rust leaves to the compiler and that holds one of
//! them therefore proves itself only with the feature, unless it is proven field by field, and the
//! refusal says so. A type whose every part has the same layout in every build, such as a generic
//! `repr(C)` struct of a `u8` and a `PhantomData<T>`, needs no identity. An identity can only
//! refuse a pun, because the layouts are compared as well, so two types that happened to share a
//! name would still be told apart by their layouts.
//!
//! ### What this relies on
//!
//! `rustc` places the fields of a struct of the `Rust` representation one after another, each at
//! the first offset its alignment allows, in an order of its choosing. The sizes it may give are
//! therefore the sizes of the possible orders of the fields. `puns` tries them all for a struct of
//! up to 6 fields that have a size or an alignment greater than 1. With more such fields, it
//! assumes that the size depends on the order, unless the fields need no padding in any order.
//! `-Zrandomize-layout` draws the order at random and does nothing else. No documentation states
//! that `rustc` places fields this way. Rust only bounds the offsets
//! ([`r-layout.repr.rust.layout`][rust layout]), and a compiler could put padding where no
//! alignment needs it. In that case the size that `puns` reads would differ from the size that
//! every order gives. The struct would no longer count as the same in every layout, so the pun
//! would fail to compile rather than change what it does.
//!
//! The alignment of a type is read, not assumed. Rust gives the alignment of each primitive per
//! platform, and makes a struct at least as aligned as its fields. `-Zrandomize-layout` does not
//! change alignment.
//!
//! ### What `puns` assumes
//!
//! Rust documents neither where the tag of an enum of the `Rust` representation is, nor its values,
//! nor how an enum without fields is stored ([`r-layout.repr.rust.unspecified`][rust unspecified]).
//! `rustc` lays them out by one algorithm, which has given the same layouts since Rust 1.65, from
//! the layouts of the fields of the variants. Without the `unstable` feature, `puns` assumes this
//! algorithm, as follows, where `-Zrandomize-layout` cannot change its result. `Option`, `Result`
//! and the enums that derive `Pun` follow it alike.
//!
//! - **An enum without fields** is its discriminant, in the least integer of 1, 2, 4, 8 or 16 bytes
//!   that holds every discriminant, signed if one is negative, in the byte order of the target. It
//!   has no byte if it has one variant or none.
//! - A variant is **absent** if its fields have no values and each has size 0 and alignment 1. It
//!   takes no byte and no value of the tag. If one variant is not absent, the enum is its fields.
//! - **A tag** is at offset 0, an unsigned integer as wide as the least alignment of the first
//!   field of each variant, among the fields that do not have both size 0 and alignment 1, if an
//!   integer of that width has that alignment on the target, and at least wide enough for the index
//!   of every variant. Each variant is its index there, and its fields start at the first offset
//!   after the tag that their alignment allows. So `Option<u16>` is a `u16` tag, 0 for `None` and
//!   1 for `Some`, then the value, and `Result<u32, u8>` a `u8` tag with `Ok` at 4 and `Err` at 1.
//! - **A niche** is a scalar in the fields of the largest variant (the last one, if several are as
//!   large), whose values Rust restricts. The other variants from the first to the last take
//!   consecutive values outside its *valid range*, the shortest range of values, wrapping past the
//!   greatest one to 0, that holds every value that Rust allows. One variant takes the value next to
//!   the range that is nearest to zero as a signed integer, the one before it on a tie, except
//!   after the range 0 to 1 of a `bool`, where it takes 2. Several variants take the values after
//!   the range, unless the range does not wrap and they fit before it, nearer to zero, or the
//!   values after it would wrap into it. So `None` is 2 in `Option<bool>`, 255 in
//!   `Option<Option<bool>>`, and `0xFFFF_FFFF` in `Option<char>`. Each other variant starts at 0 if
//!   it ends before the niche, and otherwise after it, where its alignment allows, and all must fit
//!   in the size of the largest variant. The niche of a type is the scalar with the most free
//!   values; the niche of an array is that of its first element.
//! - **Which one**: the niche if it makes the enum smaller than the tag, or as large with more free
//!   values left, and the tag otherwise.
//!
//! The assumption applies to an enum whose every variant has at most one field, or fields that
//! all have the same alignment and give the same bytes in every order (all bytes of any value
//! without rules, or all of the same layout, fixed at any depth), and none of which has values
//! that Rust restricts. The niche must be in a scalar whose values for Rust `puns` knows (a `bool`,
//! a `char`, a `NonZero`, an enum without fields, or the tag of an enum whose layout Rust
//! specifies or `puns` assumes), at the same place in every layout that Rust allows, and no other
//! scalar of its type may have as many free values. On stable Rust, only `core` and `std` make
//! niches (`bool`, `char`, `NonZero`, references and other pointers that are never null, the tags of
//! enums, and a few types of `std`), and any other type has the niches of its fields. So the values
//! that a scalar declared by hand ([`Scalar`]) accepts never give a niche: its `Spec` may declare
//! the type that its bytes are ([`Spec::stored_as`](layout::Spec::stored_as)), such as the
//! `NonZero` that a `repr(transparent)` struct holds, and the niche is then that type's, in its own
//! bytes. Otherwise the enum that would hold it there is not assumed. A variant whose fields have no value but a size is not assumed
//! either. Every other enum of the `Rust` representation keeps a layout that Rust leaves to the
//! compiler.
//!
//! Where the assumption applies, the layout of the enum is computed from the layouts of its fields
//! and the alignments of the integers of the target, and compared in a `const` with the one that
//! the experiments found (`Layout::assumed`, and for an enum without fields, the `SPECIFIED` of
//! the `Scalar` that the derive implements): the size, the alignment, the place and the width of
//! the tag, the value of each variant, and where the fields of each variant start. If they differ,
//! `Pun::LAYOUT` of the enum fails to compile, with the message "an enum of the `Rust`
//! representation is not laid out as `puns` assumes", and the note of `rustc` names the enum.
//! Enable the feature then, which uses the layout that `rustc` chose.
//!
//! The assumption never gives a layout that `puns` then uses. The layout of the enum is always the
//! one that the experiments found; the assumption only decides whether that layout counts as the
//! same in every layout that Rust allows, after the check shows that it is the predicted one. So a
//! wrong assumption, or a false [`Spec::stored_as`](layout::Spec::stored_as), fails to
//! compile, and is not unsound.
//!
//! The rule was measured on Rust 1.100.0 and nightly-2026-11-17, without `-Zrandomize-layout` and
//! with it under 16 seeds, and on `aarch64-apple-darwin`, `i686-unknown-linux-gnu` and
//! `s390x-unknown-linux-gnu`. The layouts that vary are those of variants of several fields: the
//! order of their fields, and the width of the tag where their alignments differ, which is why
//! the rule requires one alignment. The targets differ only by the alignments of their integers,
//! which the rule reads.
//!
//! ### What Rust documents
//!
//! The table lists each guarantee that `puns` relies on, and where Rust states it. "R" is the
//! [Reference][reference root], and "C" the documentation of [`core`].
//!
//! | Guarantee | Where |
//! |---|---|
//! | Layout may change with each compilation, and only what is listed is guaranteed | R [`r-layout.guarantees`][layout guarantees] |
//! | A size is a multiple of the alignment, and the stride of an array | R [`r-layout.properties.size`][size] |
//! | The sizes of `bool`, integers, floats and `char`, and of `usize` and `isize` | R [`r-layout.primitive.size`][primitive size], [`r-layout.primitive.size-align`][primitive size-align] |
//! | The alignment of a primitive is that of the platform, read and not assumed | R [`r-layout.primitive.platform-specific-alignment`][primitive align] |
//! | Any initialized bytes are an integer or a float | R [`r-type.numeric.validity`][numeric validity] |
//! | `bool` is one byte, 0 or 1 | R [`r-type.bool.layout`][bool layout], [`r-type.bool.repr`][bool repr] |
//! | `char` is a `u32` that is a Unicode scalar value | R [`r-type.char.value`][char value], [`r-type.char.layout`][char layout] |
//! | `[T; N]` is `N` elements one after the other, `[T]` is laid out as the array, and `str` as `[u8]` | R [`r-layout.array`][array], [`r-layout.slice`][slice], [`r-layout.str`][str] |
//! | `()` has size 0 and alignment 1. `!` has the same, and no value | R [`r-layout.tuple.unit`][unit], [`r-type.never.layout`][never layout] (nightly Reference) |
//! | A value is valid if its fields, or its elements, are. Padding may be uninitialized | R [`r-undefined.validity.struct`][validity struct], [`r-undefined.validity.undef`][validity undef] |
//! | An enum has a valid discriminant, and the fields of its variant are valid | R [`r-undefined.validity.enum`][validity enum] |
//! | A `repr(C)` struct has its fields in order, each at the next offset that its alignment allows | R [`r-layout.repr.c.struct`][c struct] |
//! | A `repr(C)` union has every field at 0, and `packed(N)` lowers its alignment without moving a field | R [`r-layout.repr.c.union.size-align`][c union], [`r-layout.repr.alignment.packed-fields`][alignment packed fields] |
//! | The validity of a union is not decided, so a target is one of its fields, and a source proves nothing | R [`r-undefined.validity.union`][union validity] |
//! | An enum without fields, `repr(C)` or of a primitive representation, is that integer, which holds the discriminant | R [`r-layout.repr.c.enum`][c enum], [`r-layout.repr.primitive.enum`][primitive enum], [`r-items.enum.discriminant.access-memory`][discriminant memory] |
//! | An enum with fields, `repr(C)`, of a primitive representation, or both ([RFC 2195]) | R [`r-layout.repr.c.adt`][c adt], [`r-layout.repr.primitive.adt`][primitive adt], [`r-layout.repr.primitive-c`][primitive c] |
//! | `repr(transparent)` has the layout of its field | R [`r-layout.repr.transparent.layout-abi`][transparent] |
//! | `packed(N)` and `align(N)` | R [`r-layout.repr.alignment`][alignment] |
//! | The `Rust` representation puts each field at a multiple of its alignment, and the type is at least as aligned as its fields | R [`r-layout.repr.rust.layout`][rust layout] |
//! | The fields of a struct of the `Rust` representation do not overlap, in an order that is not specified | R [`r-layout.repr.rust.struct`][rust struct] |
//! | A struct of the `Rust` representation whose fields have a size of 0 has none. So does an enum without variants, or of one variant whose fields have no size | R [`r-layout.repr.rust.struct-zst`][struct zst], [`r-layout.repr.rust.enum-empty-zst`][enum empty], [`r-layout.repr.rust.enum-struct-like-zst`][enum struct-like] |
//! | The `Rust` representation guarantees nothing else | R [`r-layout.repr.rust.unspecified`][rust unspecified] |
//! | A tuple has the `Rust` representation | R [`r-layout.tuple.def`][tuple] |
//! | The representation of a type does not change the layout of its fields | R [`r-layout.repr.inter-field`][inter-field] |
//! | A type has one layout in a program, and a similar type may have another | C [`offset_of!`, "Layout is subject to change"][offset_of layout], R [`r-type.struct.layout`][struct layout] |
//! | Where and how the discriminant of an enum of the `Rust` representation is stored is not defined | R [`r-items.enum.discriminant.type`][discriminant type], C [`mem::discriminant`] |
//! | The fields of a union of the `Rust` representation need not be at 0, so such a union is refused | R [`r-type.union.layout`][union layout] |
//! | `NonZero<T>` is `T`, except zero | C [`NonZero`, "Layout"][nonzero layout] |
//! | `Option<T>` is `T`, and `None` is zeros, for a `NonZero` and a `repr(transparent)` struct around one | C [`option`, "Representation"][option representation] |
//! | `Result<T, E>` has the size of `T`, and `Ok(t)` the bytes of `t`, when `T` is such a type and `E` has size 0 and alignment 1, or the reverse | C [`result`, "Representation"][result representation] |
//! | `ManuallyDrop<T>` has the layout and the valid values of `T` | C [`ManuallyDrop`] |
//! | `MaybeUninit<T>` has the size and the alignment of `T`, and any bytes are a value | C [`MaybeUninit`, "Layout"][maybe-uninit layout], [`MaybeUninit`, "Validity"][maybe-uninit validity] |
//! | `PhantomData<T>` has size 0 and alignment 1 | C [`PhantomData`, "Layout"][phantom layout] |
//!
//! Rust documents more than `puns` uses yet. `UnsafeCell`, `Cell`, `Wrapping`, `Saturating` and the
//! atomics have the layout of what they hold, but `puns` implements `Pun` for none of them. An enum
//! with two variants, one holding a type that `Option` is documented for and one with no sized
//! field, is ["ABI-compatible"][abi compatibility] with that type. That gives its size, but not
//! which bytes its other variant is.
//!
//! Niches are not guaranteed beyond the rows for `Option` and `Result` above. `Option<bool>` is one
//! byte with `None` at 2, but only because that is how `rustc` does it today, and the documentation
//! of `MaybeUninit` and `UnsafeCell` says that such optimizations vary. So `puns` assumes it, and
//! checks it when compiling ([What `puns` assumes](#what-puns-assumes)). Niches that Rust
//! documents only as "ABI-compatible" are assumed in the same way, not taken from that
//! documentation: `enum Ticket { Free, Paid(NonZero<u32>) }` is laid out like the corresponding
//! `Option`, and the check compares it with the layout that the assumption gives.
//!
//! ### How it is tested
//!
//! The tests of `puns` run both without the feature and with it.
//!
//! - Without it, a test records the verdict of every function on every pair of 31 types. These are
//!   the types of the table in [Puns without the feature](#puns-without-the-feature), along with
//!   what holds them: an enum and a union of a specified representation, a struct with a rule,
//!   and arrays. The record covers each pair whole, and in part (by `&mut` and from the end). It
//!   also records which byte patterns each type accepts from bytes, but not the message of a
//!   refusal, which may name what `rustc` chose. A rule and an enum checked as a whole each have a
//!   type in the record. CI compares the record with the one in the repository, without
//!   `-Zrandomize-layout` and with it under three seeds, and it must be the same each time.
//! - Compile-fail tests check each refusal without the feature, along with its message, and
//!   compile-pass tests check what compiles. Other compile-fail tests run with the feature, and the
//!   rest are the same both ways.
//! - The layout that `puns` assumes is checked in every build that evaluates the layout of an enum
//!   that it applies to, the tests included. Without the feature, a test reads `Option`, `Result`
//!   and derived enums of the `Rust` representation through it, and unit tests compare the values
//!   of niches with the ones that `rustc` gave in the measurements.
//! - With the feature, the tests that need it run as well: enums of the `Rust` representation,
//!   `Option` and `Result`, and the tests of the layout that `rustc` chose. CI also builds the
//!   example `codegen`, which needs it. The benchmark needs it too, and is run by hand.
//!
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
//! ([Limits](#limits)).
//!
//! # Portability
//!
//! `puns` builds for every width of `usize` that Rust supports, including 16 bits, and CI builds it
//! for `msp430-none-elf`, a 16-bit target. Miri cannot run a 16-bit target, so what must hold there
//! is asserted at compile time. The code assumes the following about `usize`:
//!
//! - A value is at most `isize::MAX` bytes, as Rust guarantees. So a size plus a small constant
//!   does not overflow, and an offset that a rule reads at is at most `isize::MAX` too. A `const`
//!   assertion checks that `isize::MAX` plus 16 does not overflow a `usize`.
//! - An offset, a length or a range that a rule reads is compared with the size of the value as a
//!   `u128`, on every target, before it becomes a `usize`.
//! - The tables of a [`layout::Crc`] fit in a value. They take 32 KiB, or a single table of 2 KiB
//!   where `usize` is 16 bits, and a `const` assertion checks that they are at most `isize::MAX`
//!   bytes. The CRCs are the same either way, and tests compare them with reference values. The
//!   table of [`crc32`], 16 KiB, fits on every width, since `isize::MAX` is 32 KiB minus 1 on 16
//!   bits.
//! - [`sum16`] folds its sum before it can overflow a `u64`. It folds every 2^20 blocks, or only at
//!   the end where `usize` cannot count that far. A `const` assertion checks that the blocks
//!   between two folds cannot overflow.
//! - An operation of a rule is computed in a register where its values fit one, and on 128 bits
//!   otherwise. The register has 64 bits where `usize` has 64 bits, and 32 bits otherwise. Every
//!   width computes the same values, so the choice only changes speed, and tests check both.
//!
//! What `unsafe` code assumes about the target (the alignment of a primitive, the width of `usize`,
//! the byte order) is asserted where it is used, or derived from the target. The tests run under
//! Miri on `i686-unknown-linux-gnu`, where `usize` is 32 bits and a `u64` is aligned to 4, and on
//! `s390x-unknown-linux-gnu`, which is big-endian.
//!
//! # Limitations
//!
//! ## Limits
//!
//! - The derive bounds its implementation by "the type of each field that depends on a parameter is
//!   `Pun`", except for the fields that name the type itself. Such a bound would make the type
//!   `Pun` if its field is, and the field `Pun` if the type is, and the compiler does not resolve
//!   that cycle. So a parameter of a recursive type is declared with whatever its fields need. Two
//!   generic types that hold each other would have their bounds found in a cycle (`overflow
//!   evaluating the requirement`). When the bounds that the types declare are enough, write
//!   `#[pun(bound())]` on both.
//! - The types of the fields of a struct must be at least as visible as the struct, because the
//!   list of fields is an associated type, and the privacy rules of Rust do not let it name a less
//!   visible type.
//! - A struct with more than about a hundred fields may need a higher `#![recursion_limit]` in the
//!   crate that validates it, and so may an enum with fields and more than about 120 variants,
//!   because validation recurses over the list of variants.
//! - The analysis follows arrays, slices and structs nested at most 32 deep, and refuses a pun of a
//!   type nested deeper ("type is nested too deeply").
//! - Some proofs search a bounded number of cases. Past the bound, what is not proven is checked at
//!   run time, or the pun is refused where it cannot be checked, and nothing is proven wrongly. A
//!   struct of the `Rust` representation is proven by another source only if its fields have at
//!   most 1,024 offsets in all, counting each offset that a field may have in some order. Otherwise
//!   only the same struct proves it. The values of a scalar made of parts of the source are
//!   compared in at most 16,384 steps, and a pattern over at most 4,096 runs of the source. A
//!   `&mut` to a part of an array whose elements have rules looks at no more than 64 elements, and
//!   is refused past them.
//! - A packed struct whose unsized field has trailing padding of its own is refused.
//! - The check of eager [aims](#aims) of a reference follows pointees at most [`AIM_DEPTH`] levels
//!   deep ([`check`] has no such bound), and pointees of at most twice the bytes that it reads,
//!   and returns [`Error::Limit`] past either.
//! - The build time of a view enum whose variants overlap grows with the square of the number of
//!   variants, or faster.
//! - `PhantomData<T>` is a [`Scalar`] for any `T`, and deriving `Pun` exposes private fields. [What
//!   deriving `Pun` means](#what-deriving-pun-means) says which types must not derive `Pun`.
//! - The evaluator of rules has a stack of 16 values and 8 variables. An expression has at most 256
//!   operations and at most 4 nested walks, and the derive says when an expression needs more, and
//!   how much more. A walk takes at most `8 * size + 128` steps, where `size` is the size of the
//!   value, or, for a `size` or `len` rule, the number of bytes from the start of the value to the
//!   end of the input.
//! - Unions must be `repr(C)`, packed or not, since the fields of other unions are not known to be
//!   at 0. As a source, nothing is known about a union (see [Unions](#unions)).
//! - The direct tag of an enum of the `Rust` representation, which is all the bytes before the
//!   first field of any variant, must be 1, 2, 4, 8 or 16 bytes, since `rustc` makes no other size,
//!   or the enum is refused. If `rustc` places the fields of a variant without values before the
//!   fields of the other variants, the build fails, because `puns` cannot observe where those
//!   fields are and does not guess.
//! - In an enum with fields and a primitive or `C` representation, a variant that holds a union
//!   none of whose fields has values has no sample from which `puns` reads its tag. Its
//!   discriminant must hold no macro and no attribute, or it must have none after a variant whose
//!   tag is known, or the enum is refused ([What the derive
//!   trusts](macro@Pun#what-the-derive-trusts)).
//!
//! ## Not done yet
//!
//! - **Precise mutable parts.** [`head_mut`] and [`tail_mut`] refuse any source in which a struct
//!   that has a rule overlaps the part, unless the target has the same struct with the same rules
//!   there, even if the rule reads other fields. They also refuse any enum that the part covers
//!   only partly, even one whose variants all accept what is written. A [`pun_mut`] of a whole
//!   value does not require this of rules, so a `&mut Plain` made from a `&mut S`, where `S` has a
//!   rule that `Plain` does not, can write a value that does not meet that rule, just as the fields
//!   of `S` can, since a rule is a [requirement](#guarantees-and-requirements).
//! - **Ranges from conditions.** A counter that only a comparison bounds, such as `i` in the walk
//!   condition `|i| i < count`, is computed on 128 bits, unless the expression also reads the value
//!   at that offset (`be(i, 1)`). The state of a walk inside two other walks is given no range.
//! - **Rules implied by other rules.** A rule is proven by the same rule, not by other rules that
//!   imply it. For example, `v % 6 == 0` implies `v % 3 == 0`, but does not prove it. Nor is a rule
//!   proven by the same rule in a struct of another extent, even when the rule does not read the
//!   extent, such as `a != b` in a struct of 2 bytes and in one of 3. The pun is then checked.
//! - **A rule that two variants of a view share, where neither variant accepts every value of the
//!   other.** For bytes that fail the rule, each variant computes it. A variant declared after them
//!   that accepts every value of both computes it once, and shares what succeeds as well.
//!   `Frame<Ipv4>` after two kinds of IPv4 packet is such a variant.
//! - **Checked parts of a typed value** (`try_head` and so on). A part is either proven or refused.
//!   For bytes, there are [`from_head`] and [`from_tail`].
//! - **Precise checks of an enum whose source is another type.** When only some of its variants are
//!   proven, or some values of the source are not a tag, the whole enum is checked at run time, its
//!   tag first, then the fields of its variant.
//! - **A checked pun from an enum to a type that is not an enum.** Take `try_pun` from the `Shape`
//!   of [Enums: what is refused](#enums-what-is-refused) to a struct that requires an initialized
//!   byte where one variant of `Shape` has padding. It is refused because of that padding byte,
//!   even if the check of the tag would exclude that variant, because as a source an enum is known
//!   by the bytes that every variant initializes, not variant by variant.
//! - **Checking some fields of a union.** When the check of one field would read a byte that may be
//!   padding, and no field is proven, the pun is refused.
//! - **Checking only what is not proven, among parts that are checked together.** The elements of
//!   an array share their check, and an enum is checked as a whole. A source that ends with a `str`
//!   where the target does not has every part of the target checked. The parts to check of a target
//!   past its first 1,024 are checked all together. A part that is proven there without being read
//!   (a union, an enum that holds one, a struct whose rule reads its padding) is then checked too.
//!   If that check would read a byte that may be uninitialized, the pun is refused, though the part
//!   needs no check.
//! - **Patterns other than `str` as types.** The engine accepts any automaton of at most 64 states
//!   as a type that guarantees it, but no such type can come from the derive, since safe code can
//!   write the fields of a type that derives `Pun`. A struct can require a pattern of its bytes
//!   with the rule `matches(DFA, start, end)`, which, like any rule, is proven only by the same
//!   rule.
//! - **A byte order that a value gives**, such as `EI_DATA` of ELF or the byte order mark of TIFF.
//!   A type has a byte order of its own (`u32be`, `i16le`, `f64be`, and so on), so no type reads
//!   its fields in the byte order that a value gives. A view enum chooses between a type of each
//!   byte order instead.
//! - **`default()`.** No function builds a value from the defaults of its fields (literals, sizes).
//! - **Fields of fields in rules.** A rule names the integer fields of its struct, not a field of a
//!   field (`header.length`) or an element of an array (`magic[0]`), which it reads with `be` or
//!   `le` at their offsets instead.
//! - **Signed and floating-point fields in rules.** Rules compute on unsigned integers. A signed or
//!   floating-point field is refused, and is read as bits with `le` or `be` instead. A getter reads
//!   fields, not bytes, and a value that only the bytes hold, such as a variable-length integer, is
//!   decoded by the user's code.
//! - **A size that is not in the value**, such as "to the end of whatever holds it" (an MP4 box of
//!   size 0).
//! - **Sums of types in sequences, and recursion**, such as `OPTIONAL` and `CHOICE` of ASN.1, and
//!   the containers of CBOR and MessagePack. A chain of types (`Then<A, B>`) is one sequence of
//!   types.
//! - **An arbitrary function in a rule.** A getter may call any `const fn`, but a rule may not. The
//!   validator is `const fn` code, generic over the type it validates, and on stable Rust 1.100
//!   such code cannot call a function that the type chooses. A call through a function pointer is
//!   refused ("function pointer calls are not allowed in constant functions"), and so is a call of
//!   a trait method (E0015, "cannot call non-const associated function"). Const traits are unstable
//!   (E0658). A user function could only be called if the validator stopped being `const`, or
//!   behind a check that runs only outside of `const`, which stable Rust cannot express. So what a
//!   rule computes is built in, and described by data where it varies: walks, variable-length
//!   integers, CRCs by their parameters, and sums. The analysis does not reason about that data,
//!   since a rule is proven only by the same rule, and a checksum only makes a check stricter.
//!
//! [properly aligned]: core::ptr#alignment
//! [valid]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity
//! [Reference]: https://doc.rust-lang.org/reference/type-layout.html
//! [reference root]: https://doc.rust-lang.org/reference/
//! [`core`]: https://doc.rust-lang.org/core/
//! [Miri]: https://github.com/rust-lang/miri
//! [RFC 2195]: https://rust-lang.github.io/rfcs/2195-really-tagged-unions.html
//! [`-Zrandomize-layout`]: https://doc.rust-lang.org/nightly/unstable-book/compiler-flags/randomize-layout.html
//! [`TransmuteFrom`]: https://doc.rust-lang.org/core/mem/trait.TransmuteFrom.html
//! [`ptr::metadata`]: https://doc.rust-lang.org/core/ptr/fn.metadata.html
//! [const traits]: https://doc.rust-lang.org/nightly/unstable-book/language-features/const-trait-impl.html
//! [`generic_const_exprs`]: https://doc.rust-lang.org/nightly/unstable-book/language-features/generic-const-exprs.html
//! [offset_of]: https://doc.rust-lang.org/core/mem/macro.offset_of.html
//! [offset_of layout]: https://doc.rust-lang.org/core/mem/macro.offset_of.html#layout-is-subject-to-change
//! [1.77]: https://blog.rust-lang.org/2024/03/21/Rust-1.77.0/
//! [1.79]: https://blog.rust-lang.org/2024/06/13/Rust-1.79.0/
//! [1.83]: https://blog.rust-lang.org/2024/11/28/Rust-1.83.0/
//! [`size_of_val_raw`]: https://doc.rust-lang.org/core/mem/fn.size_of_val_raw.html
//! [`align_of_val_raw`]: https://doc.rust-lang.org/core/mem/fn.align_of_val_raw.html
//! [`long_running_const_eval`]: https://doc.rust-lang.org/rustc/lints/listing/deny-by-default.html#long-running-const-eval
//! [`LINT_TERMINATOR_LIMIT`]: https://github.com/rust-lang/rust/blob/main/compiler/rustc_const_eval/src/const_eval/machine.rs
//! [`mem::discriminant`]: https://doc.rust-lang.org/core/mem/fn.discriminant.html
//! [layout guarantees]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.guarantees
//! [size]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.properties.size
//! [primitive size]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.primitive.size
//! [primitive size-align]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.primitive.size-align
//! [primitive align]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.primitive.platform-specific-alignment
//! [numeric validity]: https://doc.rust-lang.org/reference/types/numeric.html#r-type.numeric.validity
//! [bool layout]: https://doc.rust-lang.org/reference/types/boolean.html#r-type.bool.layout
//! [bool repr]: https://doc.rust-lang.org/reference/types/boolean.html#r-type.bool.repr
//! [char value]: https://doc.rust-lang.org/reference/types/char.html#r-type.char.value
//! [char layout]: https://doc.rust-lang.org/reference/types/char.html#r-type.char.layout
//! [array]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.array
//! [slice]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.slice
//! [str]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.str
//! [unit]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.tuple.unit
//! [never layout]: https://doc.rust-lang.org/nightly/reference/types/never.html#r-type.never.layout
//! [validity struct]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity.struct
//! [validity undef]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity.undef
//! [validity enum]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity.enum
//! [union validity]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity.union
//! [c struct]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.c.struct
//! [c union]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.c.union.size-align
//! [c enum]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.c.enum
//! [primitive enum]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.primitive.enum
//! [discriminant memory]: https://doc.rust-lang.org/reference/items/enumerations.html#r-items.enum.discriminant.access-memory
//! [c adt]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.c.adt
//! [primitive adt]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.primitive.adt
//! [primitive c]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.primitive-c
//! [transparent]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.transparent.layout-abi
//! [alignment]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.alignment
//! [alignment packed fields]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.alignment.packed-fields
//! [rust layout]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.rust.layout
//! [rust struct]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.rust.struct
//! [struct zst]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.rust.struct-zst
//! [enum empty]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.rust.enum-empty-zst
//! [enum struct-like]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.rust.enum-struct-like-zst
//! [rust unspecified]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.rust.unspecified
//! [tuple]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.tuple.def
//! [inter-field]: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.inter-field
//! [struct layout]: https://doc.rust-lang.org/reference/types/struct.html#r-type.struct.layout
//! [discriminant type]: https://doc.rust-lang.org/reference/items/enumerations.html#r-items.enum.discriminant.type
//! [union layout]: https://doc.rust-lang.org/reference/types/union.html#r-type.union.layout
//! [nonzero layout]: https://doc.rust-lang.org/core/num/struct.NonZero.html#layout-1
//! [option representation]: https://doc.rust-lang.org/core/option/index.html#representation
//! [result representation]: https://doc.rust-lang.org/core/result/index.html#representation
//! [`ManuallyDrop`]: https://doc.rust-lang.org/core/mem/struct.ManuallyDrop.html
//! [maybe-uninit layout]: https://doc.rust-lang.org/core/mem/union.MaybeUninit.html#layout-1
//! [maybe-uninit validity]: https://doc.rust-lang.org/core/mem/union.MaybeUninit.html#validity
//! [phantom layout]: https://doc.rust-lang.org/core/marker/struct.PhantomData.html#layout-1
//! [abi compatibility]: https://doc.rust-lang.org/core/primitive.fn.html#abi-compatibility
#![cfg_attr(not(test), no_std)]
// Every public item has documentation, as docs.rs counts it. CI checks that each item that rustdoc
// counts has an example.
#![deny(missing_docs)]

mod analysis;
mod assume;
mod discover;
#[cfg(feature = "verdict")]
#[doc(hidden)]
pub mod hook;
pub mod layout;
mod narrow;
#[cfg(test)]
mod narrow_tests;
pub mod num;
mod ptr;
mod std_enums;
#[cfg(test)]
mod tests;
mod thin;
mod validate;
#[doc(hidden)]
pub mod view;

use core::{marker::PhantomData, mem::ManuallyDrop};

use analysis::{
  Analysis, Exact, MUTABLE, Marks, TAIL_UNSIZED_SOURCE, TAIL_UNSIZED_TARGET, analyze, analyze_tail,
  mutable, mutable_part, mutable_unspecified,
};
use layout::{Dfa, Endian, Field, Layout, Spec};
pub use layout::{crc32, sum16};
/// Implements [`Pun`](trait@Pun) for a struct, an enum or a union, from its declaration and its
/// `#[pun(..)]` attributes.
///
/// The derive reads the layout of the type from the compiler (its size, its alignment, the offset
/// of each field, and the discriminants of an enum), and the [valid] values of the type from the
/// `Pun` implementations of its fields and from its attributes.
///
/// [valid]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity
///
/// **Structs.** A struct may have any representation, but a struct whose last field is unsized must
/// be `repr(C)` or `repr(transparent)`. With the `Rust` representation, Rust lets the compiler
/// choose the order of the fields, so without the `unstable` feature, a pun relies only on what
/// every order has in common (see [`layout::UNSTABLE`]). If every order makes the same bytes, as
/// for `(u8, [u8; 3])` or `(bool, bool)`, a pun may rely on every byte of the struct. Otherwise, as
/// a source, the struct is known to hold initialized bytes if it does in every order, and nothing
/// more. As a target, a struct whose size is the same in every order, without rules, is proven
/// field by field, each field at every place where it may be, and any other is proven only by the
/// same type ([Puns without the feature](crate#puns-without-the-feature)). A struct may also have
/// rules, which its attributes state (see "Attributes").
///
/// **Enums without fields.** An enum without fields may have any representation, but no padding.
/// Its valid values are its discriminants, as the compiler stores them. Rust does not specify how
/// they are stored with the `Rust` representation, so without the `unstable` feature, `puns`
/// assumes that it is the discriminant, in the least integer that holds them all, and checks it
/// when compiling ([What `puns` assumes](crate#what-puns-assumes)).
///
/// **Enums with fields.** An enum with fields may be `repr(C)`, `repr(u8)` or another integer,
/// both, or of the `Rust` representation. With `repr(C)` or an integer, its layout is the one that
/// [RFC 2195] specifies. Otherwise, the derive finds the layout that `rustc` chose by experimenting
/// in const evaluation on a value of each variant. An experiment that fails refuses the enum, so a
/// wrong layout is never found. What is known about a value of such an enum is its tag, so making
/// one from another type checks the tag, then what the fields of its variant require. With the
/// `Rust` representation and without the `unstable` feature, the layout is the one that `puns`
/// assumes, where it applies, and the experiments check it ([What `puns`
/// assumes](crate#what-puns-assumes)). Otherwise the enum proves only the same type, and only the
/// same type proves it.
///
/// [RFC 2195]: https://rust-lang.github.io/rfcs/2195-really-tagged-unions.html
///
/// **Enums without variants.** An enum without variants has no value, so no pun makes one.
///
/// **Views.** An enum whose variants each hold one shared reference, all with the same lifetime, is
/// not data but a *view* of data that is one of several types, such as `enum Frame<'a> { Ipv4(&'a
/// Ipv4), Arp(&'a Arp), Other(&'a [u8]) }`. The derive implements no `Pun` for it. Instead, it
/// generates two `const fn`s, as visible as the enum:
///
/// - `from_head(bytes: &[u8]) -> Result<(Self, &[u8]), Error>` returns the first variant that
///   `bytes` start with, and the bytes that follow it.
/// - `from_bytes(bytes: &[u8]) -> Result<Self, Error>` returns the first variant that `bytes` are
///   as a whole.
///
/// Both return [`Error::Validity`] if no variant fits. They return what trying the variants in
/// order would return, but they check each variant at most once, and only for what is not already
/// known about it. A variant is skipped as soon as the bytes do not match its first constrained
/// scalar, such as an EtherType. A variant whose values are all values of a later variant is
/// decided after that one, from it. It fails if that one fails, and is checked only for what that
/// one does not prove about it. So a variant `Frame<Ipv4<Udp>>` before `Frame<Ipv4>` is checked for
/// its protocol, without checking the checksums again. The error does not say why no variant fits.
/// A view has no `from_tail`, because its variants may be unsized. Its build time grows roughly in
/// proportion to the number of variants when each starts with a different constant, and with the
/// square of that number, or faster in a release build, when the values of each variant are also
/// values of the next.
///
/// **Unions.** A union must be `repr(C)`, and may be `packed` or `packed(N)`. [Unions](crate#unions)
/// says what a union accepts as a target and what it proves as a source.
///
/// **Generic types.** Where only the same type proves a part, an instance of a generic type is
/// the same type as another one with the same parameters, if each type parameter is bounded by
/// `Pun` in the bounds of the type, in its `where` clause, in `#[pun(bound(..))]`, or by the bound
/// that the derive adds for a field of that type. Otherwise it is not even the same type as itself
/// ([The same type](crate#the-same-type)).
///
/// **Byte order.** A field has the byte order of its type, and `u32be`, `i16le` and `f64be` from
/// [`num`] are numbers with a fixed byte order. A format whose values give their own byte order
/// (ELF, TIFF) needs one type for each byte order, and a view chooses between them.
///
/// # Attributes
///
/// The `#[pun(..)]` attribute takes the following options.
///
/// On a struct:
///
/// - `#[pun(size = EXPR)]`: the extent of a value is `EXPR` bytes. The extent is the size up to the
///   end of the slice, without the padding that may follow the slice. [`from_head`] takes that many
///   bytes, rounded up to the alignment of the struct.
/// - `#[pun(assert(EXPR))]`: `EXPR` must be true of a value. A struct may have several.
/// - `#[pun(get(name: Type = EXPR, ..))]`: generates `const fn name(&self) -> Type`, which returns
///   `(EXPR) as Type`, such as a bit field. The method is as visible as the struct. Here `EXPR` is
///   any Rust expression that a `const fn` may evaluate, in which the name of an unsigned integer
///   field, or `self.name`, is the value of that field as a `u128`, unless a `let` or a closure
///   declares that name. A field of another type is refused where the method is used. The field is
///   read by reference, or copied in a packed struct. `EXPR` is evaluated as Rust code, so the
///   method panics where that code panics, such as on a division by zero. A rule of the struct does
///   not prevent this, since safe code may change the fields of a value after it is checked.
/// - `#[pun(tag(Key = Type, ..))]`: implements [`Tag<Key>`](Tag) with `type Tag = Type`, so that a
///   tag of kind `Key` next to the struct must be a `Type`.
///
/// On a struct, an enum with fields that is not a view, or a union:
///
/// - `#[pun(bound(T: Trait, ..))]`: the bounds of the implementation of `Pun`, in addition to the
///   bounds of the type. They replace the bounds that the derive finds for the fields and the
///   targets of the aims, which require each of these types that depends on a type parameter to be
///   `Pun`, unless it names the type being derived. The derive still requires the type of each
///   typed region to be `Pun`, and a struct that is neither `repr(C)` nor `repr(transparent)` to
///   be `Sized`. Generic types that hold each other behind pointers need this, since the bounds
///   found for each would form a cycle, and so does an aim whose target needs a bound of its own.
///   Write `#[pun(bound())]` when the bounds of the type are enough.
///
/// On any type:
///
/// - `#[pun(crate = path)]`: the path of the `puns` crate, `::puns` by default. `#[pun(crate)]` is
///   `#[pun(crate = crate)]`, for the `puns` crate itself.
///
/// On a field of a struct, of an enum with fields that is not a view, or of a union:
///
/// - `#[pun(valid(0x45..=0x4f, 0x60, ..))]`: the field must have one of these values. Each item is
///   a value or a range of unsigned integer literals or constants. The value of the field is its
///   bits read as an unsigned integer in its byte order, so `0xff` is `-1i8`.
///
/// On the last field of a struct:
///
/// - `#[pun(len = EXPR)]`: the tail of the field has `EXPR` elements. The tail is the field if it
///   is an array, a slice or a `str`, whose elements are bytes, and otherwise the slice or `str`
///   that ends the unsized struct of the field.
///
/// ## Aims
///
/// An [aim](crate#aims) is a value at a place of the checked bytes that a struct points at, its
/// pointee. A struct declares its aims through two options, one for each place where a pointee
/// starts, and it may have any number of both: a struct with regions may point at other values,
/// and the pointee of a region may hold pointers. Each is a rule of the struct
/// ([`layout::Rule::Aim`]), checked as the crate documentation says. Either is eager, checked with
/// the struct, or lazy, checked when it is read.
///
/// - `#[pun(pointer(Type = EXPR))]`, on a field that holds where the pointee is: the struct points
///   at a `Type` that starts `EXPR` bytes from the start of the struct. `EXPR` is a rule whose value
///   is read as an `i128` in two's complement, so that a pointee may be before the struct: a signed
///   field `off` that holds the distance is `off as u128`. The pointee is of the size of its type,
///   and of the length that it stores in its sized fields if it is unsized, unless `size = EXPR`
///   follows, a size in bytes, or `size = ..`, the rest of the bytes. The aim is eager, and its
///   field is written [`InPlace<T>`](InPlace), a `T` in the layout and in rules, which may name it.
///   No code makes an `InPlace`, so no safe code makes a value of the struct, and code may read the
///   pointee through a reference to it without a check. `pointer(lazy Type = EXPR)` is a lazy
///   aim, whose field is of any type. The derive requires the type of the field and `Type` to be
///   `Pun`, as it requires the type of a field. With a name (`name: Type = EXPR`), a method reads
///   the pointee, as visible as the field:
///   - eager: `unsafe fn name(this: *const Self) -> &Type`, and, written `mut name: Type = EXPR`,
///     `unsafe fn name_mut(this: *mut Self) -> &mut Type` too. With `size = ..`, they also take
///     `end: *const u8`, where the bytes that hold the pointee end. They cannot be safe, since the
///     derive is: the type of a value does not tell that `puns` checked it. Their `# Safety` says
///     what the caller ensures.
///   - lazy: `fn name(&self, bytes: &[u8]) -> Result<&Type, Error>`, which finds the value in the
///     `bytes` that hold it, and checks the pointee there as [`from_bytes`] checks bytes.
/// - `#[pun(regions(first: Type = EXPR, second: Type, last))]`, on the last field: the bytes of the
///   field are consecutive regions, from the cursor, each rounded up to the alignment of its type by
///   its offset in the struct. A region with `= EXPR` is that many bytes, one without is of the size
///   of its type, and the last one without takes whatever remains. A region without a type is
///   bytes, `[u8]`. Regions are lazy, and `eager name: Type = EXPR` is an eager one, whose field is
///   then written `InPlace<[u8]>` (or an `InPlace` of an array of bytes). A method with each name, as
///   visible as the field, reads the region:
///   - without a type, `fn name(&self) -> &[u8]`, its bytes;
///   - lazy, `fn name(&self) -> Result<&Type, Error>`, the region as a `&Type` if its bytes are one,
///     which it checks as [`from_bytes`] checks bytes, with the pointees of its own aims within the
///     bytes of the struct;
///   - eager, `unsafe fn name(&self) -> &Type`, as for an eager pointer.
///
///   Since safe code may change the fields of a value after it is checked, a lazy region that does
///   not fit in the field is truncated, and a region whose expression fails takes the rest of the
///   field. A type parameter that only a region names is unused, which Rust refuses (E0392). A
///   `PhantomData<P>` field, which has no bytes, uses it.
///
/// ## Expressions
///
/// The `EXPR` of `size`, `assert`, `len`, `regions` and `aim` is a *rule*. A rule is compiled to a
/// constant program, which can be evaluated in a `const fn` and costs what its arithmetic costs.
///
/// A rule means what the same Rust expression would mean if every integer were a `u128`, or it is
/// refused. It is typed as Rust would type it, so arithmetic is on integers, the operands of `&&`
/// and `||` and the condition of an `if` are `bool`s, an `assert` is a `bool`, and a `size`, a
/// `len`, a region or an aim is an integer. Rust does not convert an integer to a `bool`, so
/// `flags & 1` is not an assertion, but `flags & 1 != 0` is.
///
/// A rule is made of:
///
/// - the fields of its struct that are integers (`u8`, `u16be`, `NonZero<u32>`, `i32`...), or
///   sized structs whose only field with a size is one of these and is as large as the struct. A
///   field is named by its name, or as `self.name` (`self.0` in a tuple struct). A signed integer
///   is named only under a cast, as in `off as u128` or `delta as u16`, which means what it means
///   in Rust: the value sign-extended to 128 bits, then truncated to the type of the cast, so that
///   `-1 as u128` is `u128::MAX`. Alone, it is refused, since a negative value is not a `u128`. A
///   floating-point number, a `bool`, a `char` or an enum is refused, since its bits are not its
///   value. A rule does not name the fields of a field or the elements of an array, but reads them
///   with `le` and `be`.
/// - `size_of_val(self)`, the extent of the value in bytes (its size up to the end of its slice,
///   without the padding that may follow the slice, which `core::mem::size_of_val` counts). A
///   `size` or a `len`, which defines it, does not read it.
/// - integer literals, with no suffix or the suffix of an unsigned type (`1u8`), and byte literals
///   (`b'a'`). Floating-point numbers, negative numbers, signed integers and `char`s are refused,
///   since they would mean something else.
/// - constants of an unsigned integer type, which Rust computes: `MAX_LEN`, `u16::MAX`, a const
///   parameter `N`, `size_of::<Header>()`, `offset_of!(Self, field)`, or `self::f(3)` for a `const
///   fn f`.
/// - the operators `+ - * / % & | ^ << >> == != < <= > >= && || !`. On an integer, `!` is a bitwise
///   not (`!3` is `0xffff..fffc`). On a `bool`, it is a logical not.
/// - casts: `as u8`, `as u16`, `as u32`, `as u64` and `as usize` keep the low bits, `as u128` keeps
///   them all, and `true as u8` is `1`. A signed field is sign-extended first. A cast to another
///   type is refused.
/// - blocks that name values, `{ let name = EXPR; .. EXPR }`, with at most 8 names at once. `let
///   name: u128` and `let name: bool` state the type.
/// - choices, `if EXPR { EXPR } else { EXPR }`.
/// - the functions below. A rule calls no other function by its name alone, and no method.
///
/// Where Rust would panic, the rule fails, which means that it does not hold: on an overflow of
/// `+`, `-` or `*`, a division or remainder by zero, a shift by 128 or more, and a read outside the
/// value. `<<` drops the bits it shifts out, as in Rust, so `(1 << 127) << 1` is `0`. As in Rust,
/// the right side of `&&` and `||` is computed only if the left side does not decide, and the
/// branch of an `if` that is not taken is not computed. What is not computed cannot fail, and costs
/// nothing, so `kind != 7 || be(40, 2) == 0` holds for a value that is not of kind 7, however
/// short. Also as in Rust, a `let` is computed whether its name is used or not, and a block fails
/// if one of its `let`s fails.
///
/// The evaluator has a stack of 16 values. `a + (b + (c + ..))` needs one more for each level,
/// which a `let` or a different order saves. The derive reports an expression that needs more.
///
/// A rule can read the bytes of the value beyond its fields. Its `start`, `end` and `offset`
/// arguments count bytes from the start of the value. A `size` or a `len` can read them too, and
/// [`from_head`] evaluates it over the bytes that are there, so `size = 2 + if length == 126 { 2 +
/// be(2, 2) } else { length }` takes WebSocket frames from a stream, and an MQTT packet is `size =
/// 1 + leb128_len(1) + leb128(1)`. `from_head` then validates the value it takes as a whole, rule
/// included, over the bytes of that value only, so a rule that reads beyond what it sizes fails.
/// Every read is bounded by the value, so a read or a range outside it fails.
///
/// A trailer is therefore a region, and "the last 4 bytes are the CRC of what comes before" is an
/// assertion. A checksum that covers several layers is an assertion of the struct that holds them
/// all. For example, the checksum of UDP and TCP covers the addresses of the IP header, so it is an
/// assertion of the IPv4 packet. The functions are:
///
/// - `le(offset, N)` and `be(offset, N)`: the unsigned integer of `N` bytes at `offset`,
///   little-endian or big-endian. `N` is a constant from 1 to 16.
/// - `leb128(offset)`, `quic(offset)` and `vlq(offset)`: the variable-length integer at `offset`.
///   LEB128 has 7 bits per byte, least significant first (Protocol Buffers, WebAssembly, MQTT).
///   QUIC (RFC 9000) has 1, 2, 4 or 8 bytes, as its two high bits say. VLQ is big-endian base 128
///   (ASN.1 tag numbers and object identifiers). `leb128_len(offset)`, `quic_len(offset)` and
///   `vlq_len(offset)` give its length in bytes. They fail if the integer does not end within the
///   value, or does not fit in a `u128`. A longer encoding than needed is read as it is, and a rule
///   such as `leb128_len(1) <= 4` can refuse it, but LEB128 fails past 19 bytes, the most that a
///   `u128` needs, even if the value fits.
/// - `crc32(start, end)`: the Ethernet CRC-32 of the bytes from `start` to `end`.
/// - `crc::<WIDTH, POLY, INIT, REFLECT, XOROUT>(start, end)`: any CRC of 1 to 64 bits, given by its
///   parameters in the catalogue of parametrised CRC algorithms (see [`layout::Crc`]). `crc::<16,
///   0x8005, 0xffff, true, 0>` is the Modbus CRC, and `crc::<32, 0x1edc6f41, 0xffffffff, true,
///   0xffffffff>` is CRC-32C.
/// - `crc32(start, end, from, to)` and `crc::<..>(start, end, from, to)`: the same, with the bytes
///   from `from` to `to` read as zeros, for a CRC over the header that holds it (GPT).
/// - `sum16(start, end)`: the ones' complement sum of the 16-bit words from `start` to `end`. It is
///   `0xffff` over data that holds its own IP checksum.
/// - `fletcher16(start, end)`: Fletcher-16. It is `0` over data that holds its own ISO 8473
///   checksum, as in OSPF and IS-IS.
/// - `adler32(start, end)`: Adler-32.
/// - `sum(start, end)`: the sum of the bytes (tar).
/// - `matches(DFA, start, end)`: whether an automaton accepts the bytes from `start` to `end`.
///   `DFA` is a `&'static` [`layout::Dfa`]: `Dfa::ASCII`, `Dfa::DIGITS`, `Dfa::OCTAL`, `Dfa::HEX`,
///   `Dfa::UTF8`, `Dfa::UTF8_NO_NUL`, or one of your own. It states a pattern that the struct
///   requires of its bytes, and like any rule, it proves nothing about them. A type that guarantees
///   a pattern, as `str` does, cannot be derived, since safe code can write its fields.
/// - `walk(START, |at| COND, |at| NEXT)`: what `START` becomes when it is replaced by `NEXT` for as
///   long as `COND` holds. A walk reaches something at an offset that depends on what comes before
///   it: after options, at the end of a chain of headers, or at the end of a string. The offset of
///   the first "type, length, value" whose type is 0 is `walk(0, |at| be(at, 1) != 0, |at| at + 2 +
///   be(at + 1, 1))`, and the size of a zero-terminated string is `size = walk(0, |at| be(at, 1) !=
///   0, |at| at + 1) + 1`. A walk may be in any rule or in another walk, and one expression may
///   hold several. A walk takes at most `8 * size + 128` steps, one for each bit of the `size`
///   bytes that it may read and of a `u128`, and fails beyond that. `size` is `size_of_val(self)`,
///   or, for a `size` or `len` rule, the number of bytes from the start of the value to the end of
///   the input. So a walk always ends, and a walk over the bits of a field fits whatever the size
///   of the value.
///
/// The state of a walk is one `u128`. A walk that carries two numbers, such as the kind of a header
/// and where it starts, packs them into it. Pack them at bit 64, as `(kind << 64) + start`, because
/// on a 64-bit target, the two halves are then two registers, and `at >> 64` and
/// `at & 0xffff_ffff_ffff_ffff` are each one register. Packed as `kind + (start << 8)`, reading
/// `start` would need a shift across registers, and writing it back a merge, on the path from one
/// step to the next. Where the low half is known not to carry, an addition to it is computed in
/// that half alone, without a carry into the high half. `examples/ethernet.rs` checks the IPv6
/// checksum behind any chain of extension headers this way.
///
/// # Compile-time errors
///
/// A derive compiles, and its implementation of `Pun` evaluates, only if all of the following
/// conditions hold.
///
/// For every type:
///
/// - No identifier in the item starts with `__Pun`, since these names are reserved.
/// - The type as compiled has the fields and the variants that the derive read, and each field has
///   the type that the derive read, as it names in the generated code ("What the derive trusts").
/// - Every `#[pun(..)]` option is one of those above, on an item that it applies to, and none is on
///   a variant or on a field of a view. An option is given once, except `bound`, `assert`, `tag`
///   and `get`, which add to what they gave before.
/// - The type of every field implements `Pun`, except in a view, where the type `T` of each
///   reference `&'a T` does.
/// - The layout of the type of a field with `valid` is a scalar of 1 to 16 bytes whose bytes are
///   always initialized. The layout of a [`Scalar`] is, as are those of integers, floating-point
///   numbers, `bool`, `char`, `NonZero`, enums without fields, and `ManuallyDrop` of one of them.
///   The layout of a struct, even a `repr(transparent)` one, is not.
/// - Every item of `valid` is an unsigned integer literal or a constant of an unsigned integer
///   type, no range of `valid` is empty, and every value of `valid` is a valid value of the type of
///   the field.
/// - The type of every field of a struct, an enum that is not a view, or a union is at least as
///   visible as the type (E0446).
/// - The bounds that the derive finds for a generic type do not form a cycle (E0275). Otherwise,
///   `#[pun(bound(..))]` states them.
///
/// For a struct:
///
/// - A struct whose last field is unsized is `repr(C)` or `repr(transparent)`.
/// - A `packed` struct does not end with an unsized field whose alignment is greater than the
///   packing.
/// - `len` and `regions` are on the last field, and the last field of a struct with `len` is an
///   array or unsized.
/// - Every rule is an expression as described in "Expressions", of the type that its option
///   requires, and each field it names is an unsigned integer, or a signed one under a cast, as
///   above. The `N` of each `le` and `be` is 1 to 16.
/// - The field of each eager aim, that of an eager `pointer` or of `regions` of which one is eager,
///   is written `InPlace<T>`, and is of the type [`InPlace<T>`](InPlace). The pointee of an aim of
///   its own size is sized, or stores its length in its sized fields: a `size` or `len` rule that
///   reads no byte by offset.
/// - A rule has at most 256 operations, needs a stack of at most 16 values, and has at most 8 names
///   at once. Its walks are nested at most 4 deep.
/// - The width of each `crc::<..>` is 1 to 64 bits, and its other parameters fit in that width.
/// - The automaton of each `matches` has 1 to 64 states, and the edges of each state are sorted and
///   disjoint.
///
/// For the methods that the derive generates for a struct, where they are used:
///
/// - The fields that a `get` getter names are unsigned integers.
/// - A struct with `regions` has no padding, and a region with a type meets the compile-time
///   conditions of [`from_bytes`] for that type.
///
/// For an enum:
///
/// - An enum without fields is at most 16 bytes, and has no padding, which an `align` greater than
///   the size of its discriminant adds.
/// - The variants of a view all hold a reference with the same named lifetime, `&'a T`.
/// - The type of each variant of a view meets the compile-time conditions of [`from_bytes`], where
///   `from_head` or `from_bytes` of the view is used.
/// - The compiler lays out an enum with fields of the `Rust` representation in a way that the
///   derive recognizes. The variant of a value is given by a tag of 1, 2, 4, 8 or 16 bytes before
///   the fields of every variant, or by a niche in a scalar among the fields of one variant. Any
///   other layout is refused as unsupported.
///
/// For a union:
///
/// - The union is `repr(C)`.
///
/// The derive reports an unknown, misplaced or repeated option, and an expression that is not a
/// rule, at the place of the fault. `cargo check` reports these errors, as well as type errors. The
/// other conditions are checked when the layout of the type is evaluated (E0080). For a type
/// without generic parameters, `cargo check` reports a violation. For a generic type, the
/// conditions are checked after monomorphization, so `cargo check` does not report a violation, but
/// `cargo build` does, for each instance that is used. The conditions of a generated method are
/// checked where the method is used, by `cargo build`.
///
/// # Analysis
///
/// *This section describes what the derive assumes, and what deriving `Pun` lets other code do.
/// Unsafe code may rely only on the documented contract of [`Pun`](trait@Pun), never on how this
/// derive meets it, since the analysis may change.*
///
/// A pun only gives a value of a type from bytes that are a valid value of it, in the sense that
/// [Soundness](crate#soundness) defines.
///
/// Deriving `Pun` makes every field as accessible as if it were public, and lets two types that
/// differ only in phantom parameters or lifetimes convert to each other. So a type whose private
/// fields keep an invariant, or whose parameters mark something about its values, must not derive
/// `Pun`, as [What deriving `Pun` means](crate#what-deriving-pun-means) shows. `#[pun(valid(..))]`
/// and rules are [requirements](crate#guarantees-and-requirements), which safe code can break, so
/// they never prove what another type guarantees.
///
/// Without the `unstable` feature, nothing that a pun does depends on a layout that Rust leaves to
/// the compiler, and with it, a pun uses the layout that `rustc` chose for this build ([The
/// `unstable` feature](crate#the-unstable-feature)).
///
/// ## What the derive trusts
///
/// The derive writes `unsafe impl`s in the crate that uses it, where safe code can make any path
/// name something else: `#[pun(crate = ..)]` names any module, `#[pun(crate)]` the root of the
/// crate, and `extern crate other as core;` makes `::core` another crate. So what a layout is made
/// of does not depend on any path, in the following ways:
///
/// - The functions that the generated code calls to make a layout are associated functions of
///   [`layout::Layout`], [`layout::Spec`], [`layout::Rule`], [`layout::Op`], [`layout::Field`],
///   [`layout::Enum`], [`layout::Variant`], [`layout::Crc`], [`layout::Checksum`],
///   [`layout::Varint`], [`layout::Aim`], [`layout::Start`], [`layout::Extent`],
///   [`layout::Check`] and the identity of a type, and methods of the values that they return.
///   Hidden constants of `Layout` fix these types, and [`InPlace`], with `fn(T)` types that no path
///   can replace, so a path that names a module or another type for one of them does not compile.
///   The description of an enum of the `Rust` representation, whose type the generated code names,
///   is what a function of `Layout` returns, so only the real type compiles.
/// - The offsets of the fields do not come from `offset_of!`, which is a macro named by a path.
///   They are measured on the type as compiled, by projecting a pointer to it (`&raw const
///   (*p).field`) and `byte_offset_from`, which read no memory. For a struct of the `Rust`
///   representation and for a union, which are `Sized`, the pointer is to an uninitialized value.
///   A `repr(C)` or `repr(transparent)` struct may be unsized, so the pointer is into uninitialized
///   bytes that hold it with a tail of no element: the smallest power of two from 4096 bytes up, a
///   local of const evaluation. A struct of any size is measured, and if const evaluation cannot
///   allocate those bytes, the build fails.
/// - The tag and the size of an enum with fields and a primitive or `C` representation are those
///   that [RFC 2195] specifies. The offsets of the fields of each variant are found on a sample of
///   the variant, which another function of `Layout` makes, by code that matches on it, with
///   `byte_offset_from`, a pointer method that no path names. A variant without a sample has the
///   offsets that RFC 2195 specifies, from the sizes and alignments of its fields. With the `Rust`
///   representation, a function of `Layout` finds the variants from samples in the same way. A fake `MaybeUninit`, or a fake
///   result type, named by a path does not have the type that these functions take or return, and
///   does not compile.
/// - The tags of an enum with fields and a primitive or `C` representation are those of an enum
///   without fields that the derive declares with the same representation and discriminants, in
///   which it replaces `Self` with the enum. It declares it next to its implementation, where a
///   name that a macro writes may name another item, so a discriminant may have another value
///   there. A function of [`layout::Variant`] compares each of its tags with the first bytes of a
///   sample of the variant, made as for the `Rust` representation, and fails the build if they
///   differ. A variant without a sample of which a value may exist, because it holds a union none
///   of whose fields has values, must have a discriminant that holds no macro and no attribute, or
///   none after a variant whose tag is known, since Rust then gives it the tag before it plus one.
///   Such a discriminant has the same value in both enums: the derive names only items that start
///   with `__Pun` there, which it refuses in the input.
/// - The trait that says where a field is (`Path`) is sealed, and the leaf at which a list of
///   fields places a field is checked against the layout.
/// - The field of each eager aim has the type [`InPlace<T>`](InPlace), where `T` is its type in the
///   layout, which the function that ties the fields to the type as compiled requires, below, as it
///   requires the type of every field. So a struct to which the derive gives an eager aim has no
///   value that safe code makes.
/// - The type of each field is written once, as an associated type of a trait that the derive
///   declares and implements for the type, where `Self` is the type, as in its declaration. The
///   rest of the generated code names the type of a field there. A function that is never called
///   compiles only if the type as compiled has the fields and the variants that the derive read,
///   by name and with no other (an exhaustive pattern), and if each field has exactly the type of
///   its associated type: a `&mut` to a `*mut` to the field admits no coercion, and no subtype. So a
///   macro in the type of a field, which expands in the generated code, where `Self` and the names
///   that start with `__Pun` name other items, gives the field one type, or does not compile.
///   These names are reserved.
///
/// The one exception is the identity of a type, which says whether two layouts are of the same
/// type. It is where the type is declared, written by a macro that the derive names through the
/// path of `puns`, and the identities of its parameters, so a crate that redirects that path can
/// give a type the identity of another. Two layouts count as the same type only if they are also
/// equal, so this cannot make a wrong layout. It can make a verdict depend on how `rustc` arranged
/// the two types in this compilation, which without the `unstable` feature no verdict otherwise
/// does.
///
/// Nothing else that the derive names through the path of `puns` makes a layout. A trait that it
/// implements through the path is the trait of `puns`: otherwise the type does not implement that
/// trait of `puns`, or the implementation of `Pun` does not compile. The lists of fields and of rules that it writes as types are checked against the layout. The
/// functions of a view, the accessors of the regions and the getters call functions named through
/// the path, in code that holds no `unsafe` block. A path that names other functions there changes
/// what they return, but whatever it is, it is a valid value of its type.
///
/// The rest of `::core` that the derive names (`Option`, `PhantomData`, `usize`, and so on) is
/// checked by the types of the traits it implements. `::core::ptr::null()` is the probe of a sized
/// type, which nothing reads, and `::core::panic!` is written only next to a compile error of the
/// derive. The derive trusts the compiler for what it parses, such as `repr`.
///
/// An attribute macro written below `#[derive(Pun)]` is expanded after the derive has read the
/// item, and can change the item that `rustc` compiles. The derive does not compile if such an
/// attribute adds, removes or renames a field, changes the type of a field, or adds or removes a
/// variant. A field added to a union adds values that the layout does not accept as a target,
/// which is sound, and the layout of a source union guarantees nothing, so such a field is not
/// refused. Where such an attribute changes the order of the fields or the `repr`, the layout is
/// that of the type as compiled, or the build fails:
///
/// - The offsets of the fields of a struct and of a union are measured, as above.
/// - The offsets of the fields of a variant of an enum with a primitive or `C` representation are
///   measured on a sample of the variant, made as for the `Rust` representation. The tags are
///   compared with the samples, as above, so reordered variants fail the build. In a copy of each
///   sample, the byte after the tag that the derive read is set to 0, then to 255, and the variant
///   of the copy, which `rustc` reads from the tag alone (`discover`, module documentation), must be
///   the same. A tag that is wider than the one that the derive read changes there, and the build
///   fails. A narrower one leaves a byte of padding or of a field where the derive reads its tag,
///   and the comparison with the samples or the size of the enum fails the build.
/// - An enum with fields of the `Rust` representation is found on samples made by the names of its
///   variants and of their fields, and its discovery asserts that the discriminant of each sample
///   is the index of its variant in the item that the derive read, so reordered variants fail the
///   build.
///
/// What such an attribute can still change without failing the build:
///
/// - A variant without a sample, which holds a union none of whose fields has values, keeps the
///   offsets that [RFC 2195] specifies, and so does every variant of an enum none of whose variants
///   has a sample, of which no tag width is checked.
/// - The derive takes from the `repr` that it read whether Rust specifies the layout, on which,
///   without the `unstable` feature, a verdict may rely ([The `unstable`
///   feature](crate#the-unstable-feature)). A struct or a union whose `repr(C)` such an attribute
///   removes has the offsets of this compilation, but is taken to have them in every compilation,
///   which Rust does not promise.
///
/// Measuring the offsets of the fields of a variant binds them in a pattern, which the `dead_code`
/// lint counts as reading them: a field of a variant of an enum with a primitive or `C`
/// representation is never reported as never read.
///
/// # Examples
///
/// A struct with rules, read from the front of bytes:
///
/// ```
/// use puns::{Pun, num::u16be};
///
/// /// A record: a kind from 1 to 3, a length of at most 64, and that many bytes.
/// #[derive(Pun)]
/// #[repr(C)]
/// #[pun(assert(length <= 64))]
/// struct Record {
///   #[pun(valid(1..=3))]
///   kind: u8,
///   length: u16be,
///   #[pun(len = length)]
///   data: [u8],
/// }
///
/// let (record, rest) = puns::from_head::<Record>(&[2, 0, 3, b'a', b'b', b'c', 9]).unwrap();
/// assert_eq!((record.kind, &record.data, rest), (2, &b"abc"[..], &[9][..]));
/// // A kind that is not one of its values, a length over 64.
/// assert!(puns::from_head::<Record>(&[4, 0, 0]).is_err());
/// assert!(puns::from_head::<Record>(&[1, 0, 65]).is_err());
/// ```
///
/// Getters of bit fields, and regions:
///
/// ```
/// use puns::Pun;
///
/// #[derive(Pun)]
/// #[repr(C)]
/// #[pun(get(version: u8 = flags >> 4, urgent: bool = flags & 1 != 0))]
/// struct Packet {
///   flags: u8,
///   header_len: u8,
///   #[pun(regions(header = header_len, body))]
///   rest: [u8],
/// }
///
/// let packet = puns::from_bytes::<Packet>(&[0x21, 2, 7, 7, 1, 2, 3]).unwrap();
/// assert_eq!((packet.version(), packet.urgent()), (2, true));
/// assert_eq!((packet.header(), packet.body()), (&[7, 7][..], &[1, 2, 3][..]));
/// ```
///
/// An enum with fields, and a view:
///
/// ```
/// use puns::Pun;
///
/// #[derive(Pun, Clone, Copy)]
/// #[repr(u8)]
/// enum Shape {
///   Dot,
///   Square(u8),
/// }
///
/// assert!(matches!(puns::from_bytes::<Shape>(&[1, 5]), Ok(Shape::Square(5))));
/// // 2 is not the tag of a variant.
/// assert!(puns::from_bytes::<Shape>(&[2, 0]).is_err());
///
/// #[derive(Pun)]
/// #[repr(C)]
/// struct Ping {
///   #[pun(valid(1))]
///   kind: u8,
///   sequence: u8,
/// }
///
/// #[derive(Pun)]
/// #[repr(C)]
/// struct Text {
///   #[pun(valid(2))]
///   kind: u8,
///   text: [u8],
/// }
///
/// #[derive(Pun)]
/// enum Message<'a> {
///   Ping(&'a Ping),
///   Text(&'a Text),
///   Other(&'a [u8]),
/// }
///
/// assert!(matches!(Message::from_bytes(&[1, 7]), Ok(Message::Ping(ping)) if ping.sequence == 7));
/// assert!(matches!(
///   Message::from_bytes(&[2, b'h', b'i']),
///   Ok(Message::Text(text)) if &text.text == b"hi"
/// ));
/// assert!(matches!(Message::from_bytes(&[3]), Ok(Message::Other(_))));
/// // A ping, then what follows it.
/// assert!(matches!(Message::from_head(&[1, 7, 9]), Ok((Message::Ping(_), &[9]))));
/// ```
///
/// A rule reads a signed integer only under a cast, since a negative `i8` is not a `u128`:
///
/// ```compile_fail,E0080
/// use puns::Pun;
///
/// #[derive(Pun)]
/// #[repr(C)]
/// #[pun(assert(level < 8))]
/// struct Level {
///   level: i8,
/// }
/// ```
///
/// Under a cast, it means what it means in Rust, so that a level of -1 is not less than 8:
///
/// ```
/// use puns::Pun;
///
/// #[derive(Pun)]
/// #[repr(C)]
/// #[pun(assert((level as u128) < 8))]
/// struct Level {
///   level: i8,
/// }
///
/// assert!(puns::from_bytes::<Level>(&[7]).is_ok());
/// assert!(puns::from_bytes::<Level>(&[(-1i8).cast_unsigned()]).is_err());
/// ```
///
/// The fields of a union are all at offset 0 only with `repr(C)`:
///
/// ```compile_fail
/// use puns::Pun;
///
/// #[derive(Pun, Clone, Copy)]
/// union Word {
///   bytes: [u8; 4],
///   word: u32,
/// }
/// ```
pub use puns_derive::Pun;
#[doc(hidden)]
pub use thin::{Metadata, Pointee, Thin};
pub use validate::{AIM_DEPTH, Pending};
use validate::{Checks, Input, validate};

/// Where the type `$name` is declared, its name, and `$n`, the number of the derive in its crate,
/// as its identity (`layout::Id`), for `#[derive(Pun)]`, which names `core` through this crate (a
/// crate may have another `core`). The place is the outermost macro call that declares the type,
/// which may declare two (one in each of two functions), and the number tells them apart.
#[doc(hidden)]
#[macro_export]
macro_rules! __site {
  ($name:ident, $n:literal) => {
    $crate::__core::concat!(
      $crate::__core::module_path!(),
      "::",
      $crate::__core::stringify!($name),
      "@",
      $crate::__core::file!(),
      ":",
      $crate::__core::line!(),
      ":",
      $crate::__core::column!(),
      "#",
      $n
    )
  };
}

#[doc(hidden)]
pub use core as __core;

// -------------------------------------------------------------------------------------------------
// Traits

/// A type that is one scalar value, whose valid values a [`Spec`] describes.
///
/// Every `Scalar` is a [`Pun`] type. The integer and floating-point types, `bool`, `char`,
/// `NonZero`, `()`, `PhantomData`, `PhantomPinned` and the types of [`num`] implement it, and
/// `#[derive(Pun)]` implements it for enums without fields.
///
/// # Safety
///
/// `Scalar` may be implemented by hand. `SPEC` must have the size of `Self`, which is checked at
/// compile time, and an implementation must guarantee that:
///
/// - Every value of `Self`, read as an unsigned integer with the size and byte order of `SPEC`, is
///   a value that `SPEC` accepts. If `SPEC` is made with [`Spec::maybe_uninit`] or
///   [`Spec::uninit`], a value of `Self` may instead hold any bytes, including uninitialized ones.
/// - Every value that `SPEC` accepts is a [valid] `Self`, and meets every invariant that the code
///   of `Self` relies on.
/// - No byte of `Self` is inside an [`UnsafeCell`](core::cell::UnsafeCell).
///
/// [`Spec::stored_as`] declares the type that the bytes of `Self` are, of its size,
/// so that `puns` knows the niche that `rustc` gives an `Option` of it, which is that type's. That
/// declaration is not part of this contract: a false one fails to compile, and is never unsound.
///
/// An implementation by hand leaves the hidden items at their defaults. If `T: Scalar`, unsafe code
/// may assume that `T::SPEC` describes the values of `T` exactly. `puns` makes a `T` from any bytes
/// that `SPEC` accepts, and takes the bytes of every `T` to be bytes that `SPEC` accepts, unless
/// `SPEC` is made with [`Spec::maybe_uninit`] or [`Spec::uninit`]. It then takes them to be any
/// bytes, uninitialized ones included.
///
/// # Examples
///
/// A type of a few bytes whose constructor checks its values:
///
/// ```
/// mod percent {
///   use puns::layout::{Endian, Spec};
///
///   /// A byte from 0 to 100.
///   #[derive(Clone, Copy)]
///   #[repr(transparent)]
///   pub struct Percent(u8);
///
///   impl Percent {
///     pub const fn new(value: u8) -> Option<Self> {
///       if value <= 100 { Some(Self(value)) } else { None }
///     }
///
///     pub const fn get(self) -> u8 {
///       self.0
///     }
///   }
///
///   // SAFETY: `Percent` is `repr(transparent)` over a `u8`, so it has the layout of a `u8` [1],
///   // and `SPEC` has its size. Its field is private, and `new` makes only the bytes from 0 to
///   // 100, which `SPEC` accepts. Each of them is a valid `Percent`, which holds no `UnsafeCell`.
///   //
///   // [1] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.transparent.layout-abi:
///   //
///   //     Structs and enums with this representation have the same layout and ABI as the only
///   //     non-size 0 non-alignment 1 field, if present, or unit otherwise.
///   unsafe impl puns::Scalar for Percent {
///     const SPEC: Spec = Spec::ranges::<Percent>(Endian::NATIVE, &[(0, 100)]);
///   }
/// }
///
/// use percent::Percent;
///
/// assert_eq!(puns::from_bytes::<Percent>(&[42]).unwrap().get(), 42);
/// assert_eq!(puns::from_bytes::<Percent>(&[101]).err(), Some(puns::Error::Validity));
/// // Proven: every `Percent` is a `u8`.
/// assert_eq!(puns::pun::<Percent, u8>(Percent::new(7).unwrap()), 7);
/// ```
///
/// [valid]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity
pub unsafe trait Scalar {
  /// The size, the byte order and the valid values of `Self`.
  const SPEC: Spec;
  /// See [`Pun::NULL_OPTIMIZED`].
  #[doc(hidden)]
  const NULL_OPTIMIZED: bool = false;
  /// The identity of the type (see `layout::Id`); none by default.
  #[doc(hidden)]
  const ID: layout::Id = layout::Id::NONE;
  /// Whether `puns` knows how values of the type are stored in every layout that Rust allows: Rust
  /// specifies it, or `puns` assumes it and checks it when compiling, as for an enum without fields
  /// of the `Rust` representation, which `#[derive(Pun)]` makes a `Scalar` ([What `puns`
  /// assumes](crate#what-puns-assumes)). Otherwise, as with the `unstable` feature, the layout is
  /// one that Rust leaves to the compiler (see [`layout::UNSTABLE`]).
  #[doc(hidden)]
  const SPECIFIED: bool = true;
}

/// A type of plain data, whose layout and valid values `puns` knows at compile time.
///
/// Every function of this crate takes `Pun` types. These are the integer and floating-point types,
/// `bool`, `char`, `NonZero`, `()`, `PhantomData`, `PhantomPinned`, arrays, slices, `str`, tuples
/// of up to 12 elements, `Option` and `Result` of `Pun` types, `MaybeUninit`, `ManuallyDrop`,
/// `Infallible`, the types of [`num`], every [`Scalar`], and the types that derive it. For a tuple,
/// struct or enum of the `Rust` representation, a pun without the `unstable` feature relies only on
/// what every layout that Rust allows has in common ([`layout::UNSTABLE`]).
///
/// The limit of 12 elements is arbitrary, and could be higher. It is the arity up to which `core`
/// implements its own traits for tuples ([trait implementations of tuples][core tuples]).
///
/// # Safety
///
/// `Pun` must not be implemented by hand, because its items are hidden, and may change in any
/// release. It is implemented by `#[derive(Pun)]`, and by `puns` for the types listed above, a
/// [`Scalar`] included.
///
/// An implementation guarantees that the layout it describes is the layout of `Self`: its size, its
/// alignment, the offset of each field, the values each scalar may hold, and its rules. Every value
/// of `Self` holds what the layout says every value holds, and every value that the layout accepts
/// is a valid `Self`. The lists of the implementation are those of the layout: its fields if it is a
/// struct, its variants if it is an enum or a union, and its element if it is an array or a slice.
/// `puns` reads no list of a layout of another kind, which may then be anything. The list of the
/// rules names the type of the target of each aim. `Metadata` is `()` exactly when the layout is
/// sized, and `usize` otherwise. No byte of `Self` is inside an
/// [`UnsafeCell`](core::cell::UnsafeCell).
///
/// If `T: Pun`, unsafe code may assume that the layout of `T` is exact in this sense, and that `T`
/// holds no `UnsafeCell`. The functions of `puns` are sound only under this assumption.
///
/// # Examples
///
/// ```
/// use puns::{Pun, num::u16le};
///
/// #[derive(Pun, Clone, Copy)]
/// #[repr(C)]
/// struct Point {
///   x: u16le,
///   y: u16le,
/// }
///
/// /// The first `T` of `bytes`, and what follows it.
/// fn first<T: Pun>(bytes: &[u8]) -> Option<(&T, &[u8])> {
///   puns::from_head(bytes).ok()
/// }
///
/// let (point, rest) = first::<Point>(&[1, 0, 2, 0, 9]).unwrap();
/// assert_eq!((point.x.get(), point.y.get(), rest), (1, 2, &[9][..]));
/// ```
///
/// [core tuples]: https://doc.rust-lang.org/1.99.0/core/primitive.tuple.html#trait-implementations-1
#[diagnostic::on_unimplemented(
  message = "`{Self}` is not a `Pun` type: `puns` does not know its layout and its valid values",
  label = "not a `Pun` type",
  note = "a struct, an enum or a union is one with `#[derive(Pun)]`, a scalar with `Scalar`"
)]
pub unsafe trait Pun {
  /// Type of the elements of an array or of a slice, `()` otherwise.
  #[doc(hidden)]
  type Elem: Pun;
  /// First field of a struct, `()` otherwise.
  #[doc(hidden)]
  type Fields: Fields;
  /// First variant of an enum with fields, `()` otherwise.
  #[doc(hidden)]
  type Variants: Variants;
  /// First of the rules of a struct that have an expression or are aims, `()` if none.
  #[doc(hidden)]
  type Rules: Rules;
  /// The metadata of a pointer to `Self`: `()` if `Self` is sized, and `usize`, the length of its
  /// tail, otherwise.
  #[doc(hidden)]
  type Metadata: Metadata;
  /// A null pointer to `Self`, whose slice length is [`ptr::PROBE_LEN`] if `Self` is unsized.
  /// It must be made with `as` casts only, since the pointer tricks are checked against it.
  #[doc(hidden)]
  const PROBE: *const Self;
  #[doc(hidden)]
  const LAYOUT: Layout;
  /// The identity of `Self` (see `layout::Id`), which `LAYOUT` carries where the analysis compares
  /// identities.
  ///
  /// The identity of a generic type is made of those of its parameters, not of their layouts, so
  /// that a type that holds an instance of a generic type of itself, behind a pointer, does not
  /// need its own layout to make the layout of that instance.
  #[doc(hidden)]
  const ID: layout::Id = Self::LAYOUT.id;
  /// Whether `core` guarantees that `Option<Self>` is `Self`, with `None` as all-zero bytes. That
  /// is so when `Self` is a `NonZero` integer, or a `repr(transparent)` struct around one
  /// ("Representation" in the documentation of `core::option`). The layout of `Option<Self>` is
  /// then the same from one compiler version to the next, and so is the layout of `Result<Self, E>`
  /// and `Result<E, Self>` where `E` has size 0 and alignment 1 ("Representation" in
  /// `core::result`). For any other `Self`, they need the `unstable` feature.
  #[doc(hidden)]
  const NULL_OPTIMIZED: bool = false;
}

/// A type that is carried next to a tag of kind `K`, and the values of that tag that select it.
///
/// A message carries a payload next to a field that tells its type, and an enum carries a variant
/// next to its discriminant. `K` names the kind of tag, because one type may be carried next to
/// tags of several kinds. By convention, `K` is the type of the tag when nothing is known about
/// what it carries. Bytes can be carried next to a tag of any kind `K`, and accept every value of
/// that tag.
///
/// `#[pun(tag(K = T))]` implements it (see [`macro@Pun`]). `Tag` is not an `unsafe` trait, because
/// the tag is a field like any other, whose values `puns` checks or proves.
///
/// # Examples
///
/// ```
/// use puns::num::{u8c, u16be};
/// use puns::{Pun, Tag};
///
/// /// The kind of a message, when its payload is not known.
/// #[derive(Pun, Copy, Clone)]
/// #[repr(transparent)]
/// struct Kind(u8);
///
/// #[derive(Pun)]
/// #[repr(C)]
/// struct Message<P: ?Sized + Tag<Kind> = [u8]> {
///   kind: P::Tag,
///   payload: P,
/// }
///
/// #[derive(Pun)]
/// #[repr(C)]
/// #[pun(tag(Kind = u8c<7>))]
/// struct Ping {
///   sequence: u16be,
/// }
///
/// // A message of any kind, and a message that must be a ping.
/// assert_eq!(puns::try_pun_ref::<_, Message>(&[9u8, 0, 1]).unwrap().kind.0, 9);
/// assert!(puns::try_pun_ref::<_, Message<Ping>>(&[9u8, 0, 1]).is_err());
/// assert!(puns::try_pun_ref::<_, Message<Ping>>(&[7u8, 0, 1]).is_ok());
/// ```
pub trait Tag<K: ?Sized>: Pun {
  /// The type of the tag next to a `Self`. Its valid values are the values of the tag that mean
  /// `Self`.
  type Tag: Pun + Copy;
}

/// Bytes are carried with any tag.
impl<K: Pun + Copy> Tag<K> for [u8] {
  type Tag = K;
}

/// A `T` that holds where the target of an eager [aim](crate#aims) is, the field of a struct with
/// `#[pun(pointer(Type = EXPR))]` ([`macro@Pun`]), which no code but `puns` makes, and `puns` makes
/// none.
///
/// It is `repr(transparent)` over `T`, which a struct reads with [`InPlace::get`], and whose
/// layout and values are those of the field in the layout of the struct. Its field is private, it
/// is not a [`Pun`] type, so no pun makes one, and it is neither `Clone`, `Copy` nor `Default`. So
/// no safe code makes a value of a struct that holds one: such a value exists only in bytes where
/// `puns` checked the targets of its aims, or where unsafe code put it, and code may read these
/// targets through a reference to it without a check.
///
/// # Examples
///
/// A pointer to a `u16` before it or after it, stored as the distance from its own address:
///
/// ```
/// use puns::{InPlace, Pun};
///
/// #[derive(Pun)]
/// #[repr(transparent)]
/// struct Near16 {
///   #[pun(pointer(u16 = off as u128))]
///   off: InPlace<i16>,
/// }
///
/// // The target, 7, then a pointer 2 bytes after it, which points back at it.
/// let words = [7, (-2i16).cast_unsigned()];
/// let (_, near) = puns::from_tail::<Near16>(puns::as_bytes(&words)).unwrap();
/// assert_eq!(near.off.get(), -2);
/// // Without the target in the bytes that are checked.
/// assert!(puns::from_bytes::<Near16>(&puns::as_bytes(&words)[2..]).is_err());
/// ```
///
/// Only `puns` could make one:
///
/// ```compile_fail,E0423
/// let _ = puns::InPlace(-2i16);
/// ```
#[repr(transparent)]
pub struct InPlace<T: ?Sized>(T);

impl<T: Copy> InPlace<T> {
  /// Returns the `T`.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::{InPlace, Pun};
  ///
  /// #[derive(Pun)]
  /// #[repr(transparent)]
  /// struct Next {
  ///   #[pun(pointer(u8 = off as u128))]
  ///   off: InPlace<u8>,
  /// }
  ///
  /// // A pointer to the byte after it.
  /// let next = puns::from_head::<Next>(&[1, 9]).unwrap().0;
  /// assert_eq!(next.off.get(), 1);
  /// ```
  #[inline]
  #[must_use]
  pub const fn get(&self) -> T {
    self.0
  }
}

/// A field of a struct, and the ones after it, as a list at the type level, for validation.
///
/// # Safety
///
/// An implementation must ensure that the list, from `Self` through `Next`, is the fields of the
/// layout of its type, in the order of their declarations, and that it ends with `()`, whose
/// `FIELD` is `None`. Only `#[derive(Pun)]` and this crate implement this trait.
#[doc(hidden)]
pub unsafe trait Fields {
  type Type: ?Sized + Pun;
  type Next: Fields;
  /// The last field of the list, which may be unsized (`()` except for the fields of a struct, of
  /// which only the last one may be).
  type Last: Fields;
  /// Where the field is in the target, given where the struct is.
  type Path<P: Path>: Path;
  /// `None` at the end of the list.
  const FIELD: Option<Field>;
}

/// A variant of an enum with fields, and the ones after it, as a list at the type level.
///
/// # Safety
///
/// An implementation must ensure that the list, from `Self` through `Next`, is the variants of the
/// layout of its enum, with their tags and their fields, in order, and that it ends with `()`,
/// whose `TAG` is `None`. Where the layout of the type is not an enum or a union, `puns` reads no
/// list of its variants, which may then be anything ([`Pun`](trait@Pun), section Safety). Only
/// `#[derive(Pun)]` and this crate implement this trait.
#[doc(hidden)]
pub unsafe trait Variants {
  /// First field of the variant.
  type Fields: Fields;
  type Next: Variants;
  /// The tag of the variant, `None` at the end of the list.
  const TAG: Option<u128>;
  /// The values of the tag of the variant if they are not `TAG` alone, sorted. They are those of an
  /// enum whose layout `rustc` chose (see `Layout::variant_ranges`), where `TAG` is its index.
  const RANGES: Option<&'static [(u128, u128)]> = None;
}

// SAFETY: `()` is the end of every list of variants, as the `# Safety` of `Variants` requires of
// the last item. Its `TAG` is `None`, so it stands for no variant, and the functions that walk a
// list stop at it (`validate_variants`, `validate_union`).
unsafe impl Variants for () {
  type Fields = ();
  type Next = ();
  const TAG: Option<u128> = None;
}

/// A rule of a struct that has an expression or is an aim, and the ones after it, as a list at the
/// type level, in the order of the rules, by which the validation applies each operation of each
/// expression in an instance of its own (see `validate::steps`), and reaches the checks of the
/// pointee of each aim.
///
/// Sealed. What a list says is checked against the layout of the struct, where it is used, so that
/// every rule that has an expression or is an aim is listed, once, in order, with every one of the
/// operations of each of its expressions, and with its pointee.
#[doc(hidden)]
pub trait Rules: sealed::Sealed {
  /// Index of the rule in the layout of the struct, `usize::MAX` at the end of the list.
  const RULE: usize;
  /// The operations of its first expression (`layout::Rule::ops`): of a `size`, `len` or
  /// `assert` rule, or where the pointee of an aim starts if it is stored. `()` if it has none.
  type Expr: Steps;
  /// The operations of its second expression (`layout::Rule::extent_ops`), the size of the
  /// pointee of an aim if it is given. `()` if it has none.
  type Extent: Steps;
  /// The pointee of an aim, and `()` for another rule.
  type Pointee: ?Sized + Pointee;
  type Next: Rules;
}

impl Rules for () {
  const RULE: usize = usize::MAX;
  type Expr = ();
  type Extent = ();
  type Pointee = ();
  type Next = ();
}

/// The rule `R` of a struct, whose first expression is `E`, then the list `N`. An aim has the
/// pointee `P`, and the second expression `X` if its size is given.
#[doc(hidden)]
pub struct RuleAt<const R: usize, E, N, P: ?Sized = (), X = ()>(
  PhantomData<(E, N, X)>,
  PhantomData<P>,
);

impl<const R: usize, E: Steps, N: Rules, P: ?Sized + Pointee, X: Steps> Rules
  for RuleAt<R, E, N, P, X>
{
  const RULE: usize = R;
  type Expr = E;
  type Extent = X;
  type Pointee = P;
  type Next = N;
}

/// What the operations of an expression are, as a type (the index of each, and the expressions of
/// each walk). A tree, so that the recursions down it are as deep as the logarithm of its length.
///
/// Sealed. Its indices are checked against the expression where it is applied: every operation,
/// once, in order (see `validate::covers`).
#[doc(hidden)]
pub trait Steps: sealed::Sealed {
  /// [`steps::NONE`], [`steps::ONE`], [`steps::TWO`] or [`steps::WALK`].
  const KIND: u8;
  /// The index of the operation (`ONE`, `WALK`).
  const AT: usize;
  /// The index of the first operation, and the one after the last.
  const FIRST: usize;
  const END: usize;
  /// The two parts (`TWO`), `()` otherwise.
  type A: Steps;
  type B: Steps;
  /// The condition of a walk and what follows a step (`WALK`), `()` otherwise.
  type Cond: Steps;
  type Then: Steps;
  /// Where they are, given where the walk is. It is `X` itself except for a walk. The functions
  /// that apply the operations name all of these, whatever the kind of `Self` (the compiler makes
  /// what a function names, used or not), so they name nothing new where they are not used.
  type InCond<X: steps::Ops>: steps::Ops;
  type InThen<X: steps::Ops>: steps::Ops;
}

/// The operations of expressions, as types (see [`Steps`]).
#[doc(hidden)]
pub mod steps {
  use core::marker::PhantomData;

  use super::{Pun, Rules, Steps, sealed::Sealed};
  use crate::layout::Op;

  /// Nothing (what the parts of an operation are).
  pub const NONE: u8 = 0;
  /// One operation, which is not a walk.
  pub const ONE: u8 = 1;
  /// Two parts, one after the other.
  pub const TWO: u8 = 2;
  /// A walk, and its expressions.
  pub const WALK: u8 = 3;

  /// The operation `I`.
  pub struct One<const I: usize>;

  /// `A`, then `B`.
  pub struct Two<A, B>(PhantomData<(A, B)>);

  /// The walk `I`, whose condition is `C` and what follows a step `N`.
  pub struct Walk<const I: usize, C, N>(PhantomData<(C, N)>);

  impl Sealed for () {}
  impl<const I: usize> Sealed for One<I> {}
  impl<A: Steps, B: Steps> Sealed for Two<A, B> {}
  impl<const I: usize, C: Steps, N: Steps> Sealed for Walk<I, C, N> {}
  impl<const R: usize, E: Steps, N: Rules, P: ?Sized + super::Pointee, X: Steps> Sealed
    for super::RuleAt<R, E, N, P, X>
  {
  }

  impl Steps for () {
    const KIND: u8 = NONE;
    const AT: usize = 0;
    const FIRST: usize = 0;
    const END: usize = 0;
    type A = ();
    type B = ();
    type Cond = ();
    type Then = ();
    type InCond<X: Ops> = X;
    type InThen<X: Ops> = X;
  }

  impl<const I: usize> Steps for One<I> {
    const KIND: u8 = ONE;
    const AT: usize = I;
    const FIRST: usize = I;
    const END: usize = I + 1;
    type A = ();
    type B = ();
    type Cond = ();
    type Then = ();
    type InCond<X: Ops> = X;
    type InThen<X: Ops> = X;
  }

  impl<A: Steps, B: Steps> Steps for Two<A, B> {
    const KIND: u8 = TWO;
    const AT: usize = 0;
    // Contiguous, or no range at all (checked by `validate::covers`).
    const FIRST: usize =
      if A::END == B::FIRST && A::KIND != NONE && B::KIND != NONE { A::FIRST } else { usize::MAX };
    const END: usize = B::END;
    type A = A;
    type B = B;
    type Cond = ();
    type Then = ();
    type InCond<X: Ops> = X;
    type InThen<X: Ops> = X;
  }

  impl<const I: usize, C: Steps, N: Steps> Steps for Walk<I, C, N> {
    const KIND: u8 = WALK;
    const AT: usize = I;
    const FIRST: usize = I;
    const END: usize = I + 1;
    type A = ();
    type B = ();
    type Cond = C;
    type Then = N;
    type InCond<X: Ops> = InWalk<X, Self, false>;
    type InThen<X: Ops> = InWalk<X, Self, true>;
  }

  /// An expression of the layout of a struct, as a type. It is the expression of a rule, or of a
  /// walk of one.
  pub trait Ops: Sealed {
    const OPS: &'static [Op];
    /// What its variables may be where it starts. The evaluator computes what fits 64 or 32 bits in
    /// that width (`narrow`).
    const ENTRY: Entry;
  }

  /// What the variables of an expression may be where it starts, and the number of walks that it
  /// is in (see `narrow`).
  #[derive(Copy, Clone)]
  pub struct Entry {
    pub(crate) vars: crate::narrow::Vars,
    pub(crate) walks: usize,
  }

  impl Entry {
    /// The expression of a rule, whose variables are not defined yet.
    pub(crate) const RULE: Entry =
      Entry { vars: [crate::narrow::Range::ANY; crate::layout::VARIABLES], walks: 0 };
  }

  /// The `WHICH`th expression of the rule `R::RULE` of `T` (`layout::Rule::expression`): 0 for
  /// its first, 1 for the size of the pointee of an aim.
  pub struct InRule<T: ?Sized, R, const WHICH: usize = 0>(PhantomData<R>, PhantomData<T>);

  /// The condition (or with `NEXT`, what follows a step) of the walk `W` of the expression `X`.
  pub struct InWalk<X, W, const NEXT: bool>(PhantomData<(X, W)>);

  impl<T: ?Sized + Pun, R: Rules, const WHICH: usize> Sealed for InRule<T, R, WHICH> {}
  impl<X: Ops, W: Steps, const NEXT: bool> Sealed for InWalk<X, W, NEXT> {}

  impl<T: ?Sized + Pun, R: Rules, const WHICH: usize> Ops for InRule<T, R, WHICH> {
    const OPS: &'static [Op] = T::LAYOUT.rule_ops(R::RULE, WHICH);
    const ENTRY: Entry = Entry::RULE;
  }

  impl<X: Ops, W: Steps, const NEXT: bool> Ops for InWalk<X, W, NEXT> {
    const OPS: &'static [Op] = match crate::layout::op_at(X::OPS, W::AT) {
      Some(Op::Walk { cond, next, .. }) => {
        if NEXT {
          next
        } else {
          cond
        }
      }
      _ => &[],
    };
    // Where the walk is in `X`, with its state.
    const ENTRY: Entry = crate::narrow::walk_entry(X::OPS, &X::ENTRY, W::AT);
  }
}

/// Implemented by this crate only, because what implements [`Path`], [`Rules`] and [`Steps`] is
/// trusted.
mod sealed {
  pub trait Sealed {}
}

impl sealed::Sealed for Root {}
impl<P: Path, F: ?Sized + Fields> sealed::Sealed for At<P, F> {}

/// Where something is in a target, for validation.
///
/// Sealed, because the code that validates a value trusts it.
#[doc(hidden)]
pub trait Path: sealed::Sealed {
  /// Index of its first leaf.
  const LEAF: usize;
}

#[doc(hidden)]
pub struct Root;

#[doc(hidden)]
pub struct At<P, F: ?Sized>(PhantomData<P>, PhantomData<F>);

impl Path for Root {
  const LEAF: usize = 0;
}

impl<P: Path, F: ?Sized + Fields> Path for At<P, F> {
  const LEAF: usize = P::LEAF
    + match F::FIELD {
      Some(field) => field.leaf(),
      None => 0,
    };
}

// SAFETY: `()` is the end of every list of fields, as the `# Safety` of `Fields` requires of the
// last item. Its `FIELD` is `None`, so it stands for no field, and the functions that walk a list
// stop at it (`validate_fields`).
unsafe impl Fields for () {
  type Type = ();
  type Next = ();
  type Last = ();
  type Path<P: Path> = P;
  const FIELD: Option<Field> = None;
}

// SAFETY: The layout is `Layout::scalar::<T>(T::SPEC)`. `identified` and `unspecified` change
// neither its size, its alignment, nor its values. It meets the `# Safety` of `Pun`:
// - Its size is the size of `T::SPEC`, which `Layout::scalar` asserts is `size_of::<T>()`. Its
//   alignment is `align_of::<T>()`. A scalar has no field.
// - Every value of `T` holds a value that `T::SPEC` accepts, by the `# Safety` of `Scalar`. If
//   `T::SPEC` is made with `Spec::maybe_uninit` or `Spec::uninit`, it holds any bytes instead,
//   uninitialized ones included. These are the values that `T::SPEC` is known to hold, which are
//   what the layout says every value holds. Every value that `T::SPEC` accepts is a valid `T`, by
//   the `# Safety` of `Scalar`.
// - A scalar has no element, field, variant or rule, and each of these lists is `()`.
// - No byte of `T` is inside an `UnsafeCell`, by the `# Safety` of `Scalar`.
unsafe impl<T: Scalar> Pun for T {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Rules = ();
  type Metadata = ();
  const PROBE: *const Self = core::ptr::null();
  const LAYOUT: Layout = {
    let layout = Layout::scalar::<T>(T::SPEC).identified(T::ID);
    if T::SPECIFIED { layout } else { layout.unspecified() }
  };
  const ID: layout::Id = T::ID;
  const NULL_OPTIMIZED: bool = T::NULL_OPTIMIZED;
}

// SAFETY: `Layout::array(&T::LAYOUT, N)` is `N` elements of the layout of `T`, the size of `T`
// apart, with the alignment of `T`: the layout of `[T; N]` [1]. An array is valid if each of its
// elements is a valid `T` [2], which is what `T::LAYOUT` describes, by the `# Safety` of `Pun` for
// `T`. `Elem` is `T`, and the other lists are `()`. Its bytes are those of its elements, none of
// which is inside an `UnsafeCell`, since `T: Pun`.
//
// [1] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#array-layout:
//
//     An array of `[T; N]` has a size of `size_of::<T>() * N` and the same alignment of `T`.
//     Arrays are laid out so that the zero-based `nth` element of the array is offset from the
//     start of the array by `n * size_of::<T>()` bytes.
//
// [2] Per https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.struct:
//
//     A struct, tuple, and array requires all fields/elements to be valid at their respective
//     type.
unsafe impl<T: Pun, const N: usize> Pun for [T; N] {
  type Elem = T;
  type Fields = ();
  type Variants = ();
  type Rules = ();
  type Metadata = ();
  const PROBE: *const Self = core::ptr::null();
  const LAYOUT: Layout = Layout::array(&T::LAYOUT, N).identified(Self::ID);
  const ID: layout::Id = layout::Id::sized("[T; N]", &[T::ID], N);
}

// SAFETY: `Layout::slice` is the layout of elements that have the layout of `T`, the size of `T`
// apart, with the alignment of `T`, as many as the length of the slice: the layout of `[T]` [1]. A
// slice is valid if each of its elements is a valid `T`, as an array is, which is what `T::LAYOUT`
// describes, by the `# Safety` of `Pun` for `T`. `Elem` is `T`, and the other lists are `()`. Its
// bytes are those of its elements, none of which is inside an `UnsafeCell`, since `T: Pun`.
//
// [1] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#slice-layout:
//
//     Slices have the same layout as the section of the array they slice.
unsafe impl<T: Pun> Pun for [T] {
  type Elem = T;
  type Fields = ();
  type Variants = ();
  type Rules = ();
  type Metadata = usize;
  const PROBE: *const Self = core::ptr::slice_from_raw_parts(core::ptr::null(), ptr::PROBE_LEN);
  // SAFETY: `Self::PROBE` is `<[T] as Pun>::PROBE`, as `Layout::slice` requires.
  const LAYOUT: Layout =
    unsafe { Layout::slice::<T>(Self::PROBE, &T::LAYOUT) }.identified(Self::ID);
  const ID: layout::Id = layout::Id::new("[T]", &[T::ID]);
}

// SAFETY: `Layout::pattern` with `Dfa::UTF8` is a tail of bytes at offset 0, with alignment 1,
// whose values are the bytes that the automaton of UTF-8 accepts. A `str` has the layout of
// `[u8]` [1]. The language requires only that its bytes are initialized [2], and the library
// requires that they are UTF-8. So every `str` holds UTF-8, and all UTF-8 bytes are a valid
// `str`. The lists are `()`. A `str` holds no `UnsafeCell`.
//
// [1] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#str-layout:
//
//     String slices are a UTF-8 representation of characters that have the same layout as slices
//     of type `[u8]`.
//
// [2] Per https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.str:
//
//     A `str` value is treated like `[u8]`, i.e. it must be initialized.
unsafe impl Pun for str {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Rules = ();
  type Metadata = usize;
  const PROBE: *const Self = <[u8] as Pun>::PROBE as *const str;
  // SAFETY: `str` is `str`, and `Self::PROBE` is `<[u8] as Pun>::PROBE` cast to `*const str` with
  // an `as` cast. Its tail has `PROBE_LEN` (3) bytes, whose size fits in an `isize`. These are the
  // requirements of `Layout::pattern`.
  const LAYOUT: Layout =
    unsafe { Layout::pattern::<str>(Self::PROBE, Dfa::UTF8) }.identified(Self::ID);
  const ID: layout::Id = layout::Id::new("str", &[]);
}

// SAFETY: `ManuallyDrop<T>` has the layout and the bit validity of `T` [1], so the layout of `T`,
// and its lists, are those of `ManuallyDrop<T>`. `identified` changes neither its size, its
// alignment, nor its values. It holds a `T` and nothing else, so it holds no `UnsafeCell`, since
// `T: Pun`.
//
// [1] Per https://doc.rust-lang.org/1.99.0/core/mem/struct.ManuallyDrop.html:
//
//     `ManuallyDrop<T>` is guaranteed to have the same layout and bit validity as `T`, and is
//     subject to the same layout optimizations as `T`.
unsafe impl<T: ?Sized + Pun> Pun for ManuallyDrop<T> {
  type Elem = T::Elem;
  type Fields = T::Fields;
  type Variants = T::Variants;
  type Rules = T::Rules;
  type Metadata = T::Metadata;
  const PROBE: *const Self = T::PROBE as *const Self;
  const LAYOUT: Layout = {
    if T::LAYOUT.unsized_ {
      // SAFETY: `T::LAYOUT.unsized_` is `true`, so `T` is a slice, `str`, or a struct whose last
      // field is one of these (see `ptr::metadata`), and so is `ManuallyDrop<T>`, a struct whose
      // only field is a `T`. `T::PROBE` is `slice_from_raw_parts(null(), PROBE_LEN)`, cast with
      // `as` casts only, and `Self::PROBE` casts it again with `as`. These are the requirements of
      // `verify`, which checks that a pointer to `ManuallyDrop<T>` is represented as a pointer to a
      // slice.
      unsafe { ptr::verify::<Self>(Self::PROBE) };
    }
    T::LAYOUT.identified(Self::ID)
  };
  const ID: layout::Id = layout::Id::new(layout::Id::MANUALLY_DROP, &[T::ID]);
  // `ManuallyDrop<T>` "is subject to the same layout optimizations as `T`".
  const NULL_OPTIMIZED: bool = T::NULL_OPTIMIZED;
}

// SAFETY: `Layout::padding::<Self>()` has the size and the alignment of `MaybeUninit<T>`, and every
// byte of it is padding. As a target it accepts any bytes, uninitialized ones included, and as a
// source it guarantees nothing. Any bit value is a valid `MaybeUninit<T>` [1], so the layout
// accepts only valid values, and every value holds what it guarantees. It has no element, field,
// variant or rule, and each list is `()`. `sized_like` and `identified` change neither its size,
// its alignment, nor its values. It holds a `T` or nothing, so it holds no `UnsafeCell`, since
// `T: Pun`.
//
// Safe code may write uninitialized bytes through a `&mut MaybeUninit<T>`. The analysis makes one
// from a `&mut S` only if every value of the target, padding included, is a valid `S` (the reverse
// relation that `mutable` checks), so not from a `&mut T` unless `T` requires no byte to be
// initialized.
//
// Without the `unstable` feature, the layout also records the size of `T`, which is the size of
// `MaybeUninit<T>`, as a part of what the analysis checks to be the same in every layout that
// Rust allows (`sized_like`). This is about the `unstable` feature, not about soundness.
//
// [1] Per https://doc.rust-lang.org/1.99.0/core/mem/union.MaybeUninit.html:
//
//     any bit value is valid for a `MaybeUninit<T>`
unsafe impl<T: Pun> Pun for core::mem::MaybeUninit<T> {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Rules = ();
  type Metadata = ();
  const PROBE: *const Self = core::ptr::null();
  #[cfg(not(feature = "unstable"))]
  const LAYOUT: Layout = Layout::padding::<Self>().sized_like(&T::LAYOUT).identified(Self::ID);
  #[cfg(feature = "unstable")]
  const LAYOUT: Layout = Layout::padding::<Self>();
  const ID: layout::Id = layout::Id::new("MaybeUninit", &[T::ID]);
}

// SAFETY: `Layout::uninhabited::<Self>()` has the size and the alignment of `Infallible`, and is an
// enum without variants, which accepts no value. `Infallible` is an enum without variants, so no
// value of it is valid [1]. So every value that the layout accepts is valid, and every value holds
// what the layout says, since there are none. The lists are `()`. It has no byte, so none is inside
// an `UnsafeCell`.
//
// [1] Per https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.enum:
//
//     An enum must have a valid discriminant, and all fields of the variant indicated by that
//     discriminant must be valid at their respective type.
unsafe impl Pun for core::convert::Infallible {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Rules = ();
  type Metadata = ();
  const PROBE: *const Self = core::ptr::null();
  const LAYOUT: Layout = Layout::uninhabited::<Self>().identified(Self::ID);
  const ID: layout::Id = layout::Id::new("!", &[]);
}

/// The `I`th field of the tuple `T`.
#[doc(hidden)]
pub struct TupleField<T, const I: usize>(PhantomData<T>);

macro_rules! tuple_field {
  ([$($all:ident)+] $index:tt $ty:ident $next:ty) => {
    // SAFETY: `FIELD` is the field of the layout of the tuple whose declaration index is
    // `$index` (`Layout::field`), the field of type `$ty`. `Next` is the field declared after it,
    // or `()` after the last. So the list from `TupleField<_, 0>` is the fields of the layout of
    // the tuple, in the order of their declarations, as the `# Safety` of `Fields` requires.
    unsafe impl<$($all: Pun),+> Fields for TupleField<($($all,)+), $index> {
      type Type = $ty;
      type Next = $next;
      type Last = ();
      type Path<P: Path> = At<P, Self>;
      const FIELD: Option<Field> = Some(*<($($all,)+) as Pun>::LAYOUT.field($index));
    }
  };
}

/// Lists the fields of the tuple of the elements `$all`, from `$ty`, of index `$index`, to the
/// last (`tuple_field!`).
macro_rules! tuple_fields {
  ([$($all:ident)+] $ty:ident $index:tt $next_ty:ident $next:tt $($rest:tt)*) => {
    tuple_field!([$($all)+] $index $ty TupleField<($($all,)+), $next>);
    tuple_fields!([$($all)+] $next_ty $next $($rest)*);
  };
  ([$($all:ident)+] $ty:ident $index:tt) => {
    tuple_field!([$($all)+] $index $ty ());
  };
}

/// Implements `Pun` for the tuple of the elements `$all`, whose indexes are `$index`.
macro_rules! tuple {
  ($($all:ident $index:tt)+) => {
    // SAFETY: The layout is `Layout::structure` of the elements of the tuple, at the offsets that
    // `offset_of!` gives, each with the layout of its type. `Layout::structure` takes the size and
    // the alignment of the tuple from the compiler, and asserts that no two fields overlap and that
    // each is within the size. A tuple is valid if each of its elements is valid [1], and every
    // byte that no element covers is padding. So the layout is that of the tuple, by the `# Safety`
    // of `Pun` for each element. `rust` and `identified` change neither the size, the alignment,
    // the offsets nor the values. `Fields` lists the elements in order (see `TupleField`). The
    // other lists are `()`. No byte of an element is in an `UnsafeCell`, since each is `Pun`, and a
    // byte of padding belongs to no element.
    //
    // [1] Per https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.struct:
    //
    //     A struct, tuple, and array requires all fields/elements to be valid at their
    //     respective type.
    unsafe impl<$($all: Pun),+> Pun for ($($all,)+) {
      type Elem = ();
      type Fields = TupleField<Self, 0>;
      type Variants = ();
      type Rules = ();
      type Metadata = ();
      const PROBE: *const Self = core::ptr::null();
      // SAFETY: A tuple is `Sized`, and `Layout::structure` requires nothing of a `Sized` type.
      const LAYOUT: Layout = unsafe {
        Layout::structure::<Self>(
          Self::PROBE,
          const {
            &Layout::sort_fields([
              $(Field::new(core::mem::offset_of!(Self, $index), &$all::LAYOUT)),+
            ])
          },
          &[],
        )
        // The `Rust` representation: what every order of its fields has.
        .rust(usize::MAX)
        .identified(Self::ID)
      };
      const ID: layout::Id = layout::Id::new("(..)", &[$($all::ID),+]);
    }

    tuple_fields!([$($all)+] $($all $index)+);
  };
}

// As many elements as `core` implements its own traits for (see the documentation of `Pun`).
tuple!(A 0);
tuple!(A 0 B 1);
tuple!(A 0 B 1 C 2);
tuple!(A 0 B 1 C 2 D 3);
tuple!(A 0 B 1 C 2 D 3 E 4);
tuple!(A 0 B 1 C 2 D 3 E 4 F 5);
tuple!(A 0 B 1 C 2 D 3 E 4 F 5 G 6);
tuple!(A 0 B 1 C 2 D 3 E 4 F 5 G 6 H 7);
tuple!(A 0 B 1 C 2 D 3 E 4 F 5 G 6 H 7 I 8);
tuple!(A 0 B 1 C 2 D 3 E 4 F 5 G 6 H 7 I 8 J 9);
tuple!(A 0 B 1 C 2 D 3 E 4 F 5 G 6 H 7 I 8 J 9 K 10);
tuple!(A 0 B 1 C 2 D 3 E 4 F 5 G 6 H 7 I 8 J 9 K 10 L 11);

macro_rules! impl_any {
  ($($ty:ty)+) => {$(
    // SAFETY: `Spec::any::<$ty>()` has the size of `$ty`, and accepts any initialized bytes. The
    // items of the `# Safety` of `Scalar` hold:
    // - Every value of `$ty` is initialized [2], and so is accepted. `()` has no byte.
    // - Any initialized bytes are a valid `$ty`: validity depends on the type [1], an integer is
    //   required only to be initialized [2], and `()` has no byte. `$ty` has no other invariant.
    // - `$ty` holds no `UnsafeCell`.
    //
    // [1] Per https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.def:
    //
    //     The Rust compiler assumes that all values produced during program execution are
    //     “valid”, and producing an invalid value is hence immediate UB. Whether a value is valid
    //     depends on the type:
    //
    // [2] Per https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.scalar:
    //
    //     An integer (`i*`/`u*`), floating point value (`f*`), or raw pointer must be
    //     initialized, i.e., must not be obtained from uninitialized memory.
    unsafe impl Scalar for $ty {
      const SPEC: Spec = Spec::any::<$ty>();
      const ID: layout::Id = layout::Id::new(stringify!($ty), &[]);
    }
  )+};
}

impl_any!(() u8 u16 u32 u64 u128 usize);

macro_rules! impl_bits {
  ($number:ident: $($ty:ty)+) => {$(
    // SAFETY: As in the implementations of `impl_any!`: `$ty` is an integer or a floating-point
    // type, whose valid values are all initialized bytes. `signed` and `not_a_number` change no
    // value, only how rules read it.
    unsafe impl Scalar for $ty {
      const SPEC: Spec = Spec::any::<$ty>().$number();
      const ID: layout::Id = layout::Id::new(stringify!($ty), &[]);
    }
  )+};
}

impl_bits!(signed: i8 i16 i32 i64 i128 isize);
impl_bits!(not_a_number: f32 f64);

// SAFETY: `PhantomData<T>` has a size of 0 [1], and so does `Spec::any::<Self>()`. A value of
// size 0 has no byte to hold or to accept, and `PhantomData<T>` has no invariant. It holds no
// `UnsafeCell`. What `T` marks is not an invariant of the bytes: a type that relies on it must not
// implement `Pun` (see the crate documentation, "Soundness").
//
// [1] Per https://doc.rust-lang.org/1.99.0/core/marker/struct.PhantomData.html:
//
//     `size_of::<PhantomData<T>>() == 0`
unsafe impl<T: ?Sized> Scalar for PhantomData<T> {
  const SPEC: Spec = Spec::any::<Self>();
}

// SAFETY: `PhantomPinned` is a struct without fields, of size 0, which `SPEC` asserts since `core`
// does not document it, and `Spec::any::<Self>()` has that size. A value of size 0 has no byte to
// hold or to accept, and `PhantomPinned` has no invariant of its bytes: it only makes a type that
// holds it not `Unpin`. It holds no `UnsafeCell`.
unsafe impl Scalar for core::marker::PhantomPinned {
  const SPEC: Spec = {
    assert!(size_of::<Self>() == 0, "`PhantomPinned` has a size of 0");
    Spec::any::<Self>()
  };
  const ID: layout::Id = layout::Id::new("PhantomPinned", &[]);
}

// SAFETY: `SPEC` has the size of `bool` and accepts the bytes 0 and 1, which are exactly the valid
// values of `bool` [1]. `bool` holds no `UnsafeCell`.
//
// [1] Per https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.bool:
//
//     A `bool` value must be `false` (`0`) or `true` (`1`).
unsafe impl Scalar for bool {
  const SPEC: Spec = Spec::ranges::<bool>(Endian::NATIVE, &[(0, 1)]).not_a_number().validity();
  const ID: layout::Id = layout::Id::new("bool", &[]);
}

// SAFETY: `SPEC` has the size of `char`, and accepts the values from 0 to `0xD7FF` and from
// `0xE000` to `0x10FFFF`, read in the byte order of the target. These are exactly the valid values
// of `char` [1], since `char::MAX` is `0x10FFFF`. `char` holds no `UnsafeCell`.
//
// [1] Per https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.char:
//
//     A `char` value must not be a surrogate (i.e., must not be in the range `0xD800..=0xDFFF`)
//     and must be equal to or less than `char::MAX`.
unsafe impl Scalar for char {
  const SPEC: Spec = Spec::ranges::<char>(Endian::NATIVE, &[(0, 0xD7FF), (0xE000, 0x10_FFFF)])
    .not_a_number()
    .validity();
  const ID: layout::Id = layout::Id::new("char", &[]);
}

macro_rules! impl_non_zero {
  ($($ty:ident)+) => {$(
    // SAFETY: `NonZero<$ty>` has the layout of `$ty`, whose valid values are all initialized
    // bytes, and is valid for every one of them but zero [1]. `SPEC` has the size of `$ty`, and
    // accepts every value from 1 to `$ty::MAX`, read in the byte order of the target: every
    // value but zero. `NonZero<$ty>` holds no `UnsafeCell`, and its code relies on no other
    // invariant.
    //
    // [1] Per https://doc.rust-lang.org/1.99.0/core/num/struct.NonZero.html:
    //
    //     `NonZero<T>` is guaranteed to have the same layout and bit validity as `T` with the
    //     exception that the all-zero bit pattern is invalid.
    unsafe impl Scalar for core::num::NonZero<$ty> {
      const SPEC: Spec = Spec::ranges::<$ty>(Endian::NATIVE, &[(1, $ty::MAX as u128)]).validity();
      const NULL_OPTIMIZED: bool = true;
      const ID: layout::Id = layout::Id::new(concat!("NonZero<", stringify!($ty), ">"), &[]);
    }
  )+};
}

impl_non_zero!(u8 u16 u32 u64 u128 usize);

macro_rules! impl_signed_non_zero {
  ($($ty:ident)+) => {$(
    // SAFETY: As for the unsigned `NonZero` types above. `SPEC` accepts the bytes of `$ty` read
    // as an unsigned integer from 1 to the greatest value of its size: every bit pattern but all
    // zeros, which is the only bit pattern of 0 in two's complement. `signed` changes no value:
    // the bits are the integer in two's complement, which rules read under a cast.
    unsafe impl Scalar for core::num::NonZero<$ty> {
      const SPEC: Spec = Spec::ranges::<$ty>(
        Endian::NATIVE,
        &[(1, u128::MAX >> (128 - 8 * size_of::<$ty>()))],
      )
      .signed()
      .validity();
      const NULL_OPTIMIZED: bool = true;
      const ID: layout::Id = layout::Id::new(concat!("NonZero<", stringify!($ty), ">"), &[]);
    }
  )+};
}

impl_signed_non_zero!(i8 i16 i32 i64 i128 isize);

// -------------------------------------------------------------------------------------------------
// `Error`

/// The reason a value cannot be reinterpreted as another type.
///
/// The functions that check at run time return it: the `try_` functions and the `from_`
/// functions.
///
/// # Examples
///
/// ```
/// use puns::Error;
///
/// let words = [0u32; 2];
/// // At an address aligned for a `u32`: one byte further is not.
/// let bytes: &[u8] = puns::as_bytes(&words);
///
/// assert_eq!(puns::from_bytes::<u32>(&bytes[..4]), Ok(&0));
/// assert_eq!(puns::from_bytes::<u32>(&bytes[..3]), Err(Error::Size));
/// assert_eq!(puns::from_bytes::<u32>(&bytes[1..5]), Err(Error::Alignment));
/// assert_eq!(puns::from_bytes::<bool>(&[2]), Err(Error::Validity));
/// assert_eq!(Error::Validity.to_string(), "value is not valid for the target type");
/// ```
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Error {
  /// The address of the value is not a multiple of the alignment of the target.
  Alignment,
  /// The size of the value is not a size that the target can have, or there are fewer bytes than
  /// the target needs.
  Size,
  /// The bytes are not a valid target, because a scalar holds a value that the target does not
  /// accept, a `str` is not UTF-8, a rule of the target does not hold or cannot be computed, the
  /// pointee of an aim does not fit, or the pointee of an eager aim is not a valid value within the
  /// bytes.
  Validity,
  /// The check of the pointees of eager aims ([`Check::Eager`](layout::Check::Eager)) stopped at its
  /// limit before it found the bytes valid or not: a pointee within another, this many deep
  /// ([`AIM_DEPTH`]), or pointees of more than twice as many bytes as are checked in all.
  Limit,
}

impl core::fmt::Display for Error {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    f.write_str(match self {
      Error::Alignment => "value is not aligned for the target type",
      Error::Size => "value does not have the size of the target type",
      Error::Validity => "value is not valid for the target type",
      Error::Limit => "the targets of the value are too deep or too large to check",
    })
  }
}

impl core::error::Error for Error {}

// -------------------------------------------------------------------------------------------------
// `Plan`

/// What is known at compile time of a pun from `S` to `D`.
struct Plan<S: ?Sized, D: ?Sized>(PhantomData<S>, PhantomData<D>);

/// What a function requires of a pun.
#[derive(Copy, Clone)]
struct Need {
  /// The target may be a part of the source, either its first bytes or, with `Plan::TAIL`, its
  /// last.
  part: bool,
  /// What is not proven is checked at run time.
  fallible: bool,
  /// The target is behind a reference.
  reference: bool,
}

impl Need {
  const VALUE: Need = Need { part: false, fallible: false, reference: false };
  const REF: Need = Need { reference: true, ..Need::VALUE };
  const PART: Need = Need { part: true, ..Need::VALUE };
  const PART_REF: Need = Need { part: true, ..Need::REF };
  const TRY: Need = Need { fallible: true, ..Need::VALUE };
  const TRY_REF: Need = Need { fallible: true, ..Need::REF };
}

/// Why nothing proves a part that only the same type proves, for a refusal of [`UNSPECIFIED`] where
/// the source or the target has such a part without an identity (`Layout::unidentified`). It goes
/// on with [`NO_IDENTITY`].
const UNIDENTIFIED: &str = "The source or the target has such a part that has no identity, so \
  that not even the same type proves it: ";

/// Why nothing proves an eager aim at a type that has no identity, for a refusal of a pun that
/// does not prove a rule of the target (`Layout::holds_eager`). It goes on with [`NO_IDENTITY`].
const UNIDENTIFIED_AIM: &str = "The source or the target has an eager aim at a pointee whose type \
  has no identity, so that not even the same aim proves it: ";

/// Which types have no identity, after [`UNIDENTIFIED`] or [`UNIDENTIFIED_AIM`].
const NO_IDENTITY: &str = "an instance of a generic type has an identity only if each of its type \
  parameters has one, which `PhantomData` and a `Scalar` implemented by hand do not, and a generic \
  type that derives `Pun` only if each of its type parameters is bounded by `Pun` where the derive \
  sees it (`<T: Pun>`, `where T: Pun`, or `#[pun(bound(T: Pun))]`)";

/// A pun that depends on a layout that Rust leaves to the compiler.
const UNSPECIFIED: &str = "the pun depends on a part of the source or of the target whose layout \
  Rust leaves to the compiler (a struct or a tuple of the `Rust` representation, or an enum of \
  it, `Option` and `Result` included, whose layout `puns` does not assume): another compiler, or \
  `-Zrandomize-layout`, could arrange it otherwise. Only the same type at the same place \
  proves it.\nAdd `#[repr(C)]` (or `#[repr(u8)]` to an enum), or enable the `unstable` feature \
  of `puns`";

/// A `&mut` whose writes are not known to leave a valid source in every layout that Rust allows.
const UNSPECIFIED_MUT: &str = "what may be written through the `&mut` is a valid source in the \
  layout that `rustc` chose, not in every layout that Rust allows: the source has a part whose \
  layout Rust leaves to the compiler (a struct, a tuple or an enum of the \
  `Rust` representation).\nAdd `#[repr(C)]` (or `#[repr(u8)]` to an enum), or \
  enable the `unstable` feature of `puns`";

/// A value that holds an eager aim, which no function makes.
const EAGER_BY_VALUE: &str = "the target holds an eager aim, whose pointee is checked with it and \
  read without a check: a value of it would not be at the place where its pointee was \
  checked.\nTake a reference to it with `try_pun_ref`, `from_bytes` or `from_head`";

/// A `&mut` to or from a value that holds an eager aim, which no function makes.
const EAGER_MUT: &str = "the source or the target holds an eager aim, whose pointee is checked \
  with it and read without a check: a write through a `&mut` could move it away from that \
  pointee";

/// A check of an eager aim whose pointee could be in bytes that are not initialized.
const EAGER_UNINIT: &str = "the target holds an eager aim, whose check reads its pointee in the \
  bytes of the source, and some of them may be padding.\nCheck the bytes of the source instead, \
  with `from_bytes` or `from_head`";

/// Why a pun of which `analysis` is the analysis does not meet the `need`, if it does not.
/// `source` and `target` are the layouts of the two types, which explain a refusal.
const fn unmet(
  analysis: &Analysis,
  need: Need,
  source: &Layout,
  target: &Layout,
) -> Option<Message> {
  let Analysis { fit, relation, aligned, shift } = *analysis;
  match fit.exact {
    _ if need.part && fit.shrink => {}
    _ if need.part => {
      return Some(Message::new(
        "the source can be smaller than the target (if it has a slice, it may be empty)",
      ));
    }
    Exact::Always => {}
    Exact::Sometimes if need.fallible => {}
    Exact::Sometimes => {
      return Some(Message::new(
        "the size of the source may not be a size of the target, depending on the length of \
         its slice.\nUse `try_pun_ref` or `try_pun_mut` to check it at run time",
      ));
    }
    Exact::Never(why) if fit.shrink => {
      return Some(Message::new(why).str(
        ".\nUse `head` or `tail` (or their `_ref` and `_mut` forms) to take a part of the source",
      ));
    }
    Exact::Never(why) => return Some(Message::new(why)),
  }
  // Offsets of the target.
  if let Some(offset) = relation.unspecified {
    let message = Message::new(UNSPECIFIED).at(offset.saturating_sub(shift));
    // Looked for only here, so that no pun that compiles pays for it.
    if source.unidentified() || target.unidentified() {
      return Some(message.str(".\n").str(UNIDENTIFIED).str(NO_IDENTITY));
    }
    return Some(message);
  }
  if let Some(offset) = relation.uninit {
    return Some(
      Message::new("a byte that the target requires to be initialized is padding in the source")
        .at(offset.saturating_sub(shift)),
    );
  }
  if let Some(offset) = relation.disjoint {
    return Some(
      Message::new("no value of the source is valid for the target")
        .at(offset.saturating_sub(shift)),
    );
  }
  if let (Some(offset), false) = (relation.partial, need.fallible) {
    let message = Message::new(
      "some values of the source are not valid for the target, or do not prove one of its \
       rules.\nUse a `try_*` function to check them at run time",
    )
    .at(offset.saturating_sub(shift));
    // Looked for only here, so that no pun that compiles pays for it.
    if source.holds_eager(true) || target.holds_eager(true) {
      return Some(message.str(".\n").str(UNIDENTIFIED_AIM).str(NO_IDENTITY));
    }
    return Some(message);
  }
  if need.reference && !aligned && !need.fallible {
    return Some(Message::new(
      "the target has a greater alignment than the source.\nUse `try_pun_ref` or \
       `try_pun_mut` to check the address at run time",
    ));
  }
  None
}

impl<S: ?Sized + Pun, D: ?Sized + Pun> Plan<S, D> {
  const ANALYSIS: Analysis = analyze(&S::LAYOUT, &D::LAYOUT);

  /// The analysis of the last bytes of the source as the target.
  const TAIL_ANALYSIS: Analysis = analyze_tail(&S::LAYOUT, &D::LAYOUT);

  const MUTABLE: bool = mutable(&S::LAYOUT, &D::LAYOUT, &Self::ANALYSIS.fit);

  /// Fail to compile if `message` says why.
  const fn require(message: Option<Message>) {
    if let Some(message) = message {
      message.panic();
    }
  }

  /// Why the pun does not meet the `need`, if it does not.
  const fn check(need: Need) -> Option<Message> {
    if let Some(message) = Self::eager(need) {
      return Some(message);
    }
    unmet(&Self::ANALYSIS, need, &S::LAYOUT, &D::LAYOUT)
  }

  /// Why the eager aims of the target do not let the pun meet the `need`, if they do not: no value
  /// holds one, and a check follows one only in initialized bytes.
  const fn eager(need: Need) -> Option<Message> {
    if !D::LAYOUT.holds_eager(false) {
      return None;
    }
    if !need.reference {
      return Some(Message::new(EAGER_BY_VALUE));
    }
    if need.fallible && !S::LAYOUT.initialized() {
      return Some(Message::new(EAGER_UNINIT));
    }
    None
  }

  /// Why no `&mut` is made from a `&mut` to an `S` to a `D`, for their eager aims, if it is not.
  const fn eager_mut() -> Option<Message> {
    if S::LAYOUT.holds_eager(false) || D::LAYOUT.holds_eager(false) {
      Some(Message::new(EAGER_MUT))
    } else {
      None
    }
  }

  /// Why a `&mut` to a `D` cannot be made from a `&mut` to an `S`, when [`Plan::MUTABLE`] says
  /// so.
  const NOT_MUTABLE: &'static str =
    if mutable_unspecified(&S::LAYOUT, &D::LAYOUT, &Self::ANALYSIS.fit) {
      UNSPECIFIED_MUT
    } else {
      MUTABLE
    };

  /// Why the pun of the last bytes of the source does not meet the `need` (a part), if it does
  /// not.
  const fn check_tail(need: Need) -> Option<Message> {
    if let Some(message) = Self::eager(need) {
      return Some(message);
    }
    if S::LAYOUT.is_unsized() {
      return Some(Message::new(TAIL_UNSIZED_SOURCE));
    }
    if D::LAYOUT.is_unsized() {
      return Some(Message::new(TAIL_UNSIZED_TARGET));
    }
    match unmet(&Self::TAIL_ANALYSIS, Need { reference: false, ..need }, &S::LAYOUT, &D::LAYOUT) {
      None if need.reference && !Self::TAIL_ANALYSIS.aligned => Some(Message::new(
        "the target has a greater alignment than the source.\nUse `tail` to copy it, or \
         `from_tail` to take it from bytes, whose address is checked at run time",
      )),
      verdict => verdict,
    }
  }

  const BY_VALUE: () = Self::require(Self::check(Need::VALUE));
  const BY_REF: () = Self::require(Self::check(Need::REF));
  const BY_MUT: () = {
    let () = Self::BY_REF;
    Self::require(Self::eager_mut());
    if !Self::MUTABLE {
      Message::new(Self::NOT_MUTABLE).panic();
    }
  };
  const HEAD: () = Self::require(Self::check(Need::PART));
  const HEAD_REF: () = Self::require(Self::check(Need::PART_REF));
  const TAIL: () = Self::require(Self::check_tail(Need::PART));
  const TAIL_REF: () = Self::require(Self::check_tail(Need::PART_REF));
  const HEAD_MUT: () = Self::require(Self::check_head_mut());
  const TAIL_MUT: () = Self::require(Self::check_tail_mut());
  const TRY: () = Self::require(Self::check(Need::TRY));
  const TRY_REF: () = Self::require(Self::check(Need::TRY_REF));
  const FROM_TAIL: () = Self::require(Self::check_from_tail());
  const FROM_TAIL_MUT: () = {
    let () = Self::FROM_TAIL;
    Self::require(Self::eager_mut());
    if !Self::MUTABLE {
      Message::new(Self::NOT_MUTABLE).panic();
    }
  };

  /// Why the target cannot be taken from the end of the source (bytes) at run time, if it
  /// cannot.
  const fn check_from_tail() -> Option<Message> {
    if D::LAYOUT.is_unsized() {
      return Some(Message::new(
        "the target is unsized: where it starts cannot be found from the end of the bytes.\nUse \
         `from_head` or `from_bytes`",
      ));
    }
    Self::check(Need::TRY_REF)
  }

  /// Why a `&mut` to the first bytes of the source cannot be handed out, if it cannot: what a
  /// `&` cannot, and what [`mutable_part`] refuses.
  const fn check_head_mut() -> Option<Message> {
    if let Some(message) = Self::check(Need::PART_REF) {
      return Some(message);
    }
    if let Some(message) = Self::eager_mut() {
      return Some(message);
    }
    match mutable_part(&S::LAYOUT, &D::LAYOUT, &Self::ANALYSIS.fit, 0) {
      Some(why) => Some(Message::new(why)),
      None => None,
    }
  }

  /// Why a `&mut` to the last bytes of the source cannot be handed out, if it cannot.
  const fn check_tail_mut() -> Option<Message> {
    if let Some(message) = Self::check_tail(Need::PART_REF) {
      return Some(message);
    }
    if let Some(message) = Self::eager_mut() {
      return Some(message);
    }
    let Analysis { fit, shift, .. } = Self::TAIL_ANALYSIS;
    match mutable_part(&S::LAYOUT, &D::LAYOUT, &fit, shift) {
      Some(why) => Some(Message::new(why)),
      None => None,
    }
  }

  const TRY_MUT: () = {
    let () = Self::TRY_REF;
    Self::require(Self::eager_mut());
    if !Self::MUTABLE {
      Message::new(Self::NOT_MUTABLE).panic();
    }
  };

  /// Pointer to the target at `source`, when there is nothing to check (`BY_REF`,
  /// `HEAD_REF`).
  #[inline(always)]
  const fn cast(source: *const S) -> *const D {
    let (head, stride, exact) = const {
      // The slice length of the target is `head + len * stride`, exactly or rounded down.
      let (source, target) = (S::LAYOUT, D::LAYOUT);
      match (target.stride(), Self::ANALYSIS.fit.exact) {
        (0, _) => (0, 0, true),
        (by, Exact::Always) => ((source.head() - target.head()) / by, source.stride() / by, true),
        _ => (0, 0, false),
      }
    };
    let len = if exact {
      head + ptr::metadata(source) * stride
    } else {
      let (layout, target) = const { (S::LAYOUT, D::LAYOUT) };
      let extent = layout.head() + ptr::metadata(source) * layout.stride();
      (extent - target.head()) / target.stride()
    };
    ptr::from_raw_parts::<D>(source.cast(), len)
  }

  /// Pointer to the target at the end of `source`, when there is nothing to check (`TAIL_REF`,
  /// where both are sized).
  #[inline(always)]
  const fn tail_cast(source: *const S) -> *const D {
    let shift = const { Self::TAIL_ANALYSIS.shift };
    // `wrapping_add` requires nothing of `source`. If `source` points to an `S`, the result is in
    // it, since `shift` is `size_of::<S>() - size_of::<D>()` (`analyze_tail`).
    ptr::from_raw_parts::<D>(source.cast::<u8>().wrapping_add(shift), 0)
  }

  /// Pointer to the target at `source`, with every check that could not be made at compile
  /// time (`TRY_REF`).
  ///
  /// # Safety
  ///
  /// The caller must ensure that:
  ///
  /// - `source` points to a valid `S`, valid for reads.
  /// - `Plan::<S, D>::TRY_REF` compiles.
  #[inline(always)]
  const unsafe fn try_cast(source: *const S) -> Result<*const D, Error> {
    // SAFETY: `source` points to a valid `S`, valid for reads, by contract on the caller.
    // `Plan::<S, D>::TRY_REF` compiles, so `check` found the analysis to meet a need that is
    // fallible, and no check that it marks reads a byte that may be padding in `S`. So every byte
    // that such a check reads is initialized. These are the requirements of `try_cast_by` with the
    // marks of this pun, which are every part of `D` that a valid `S` may not be valid for.
    unsafe { Self::try_cast_by::<Self>(source) }
  }

  /// [`Plan::try_cast`], checking what `C` marks rather than what the analysis of this pun does.
  ///
  /// # Safety
  ///
  /// The caller must ensure that:
  ///
  /// - `source` points to an `S` that is valid for reads.
  /// - Every byte of that `S` that a check marked by `C` reads is initialized.
  ///
  /// The result is a valid `D` only if `C` marks every part of `D` that the bytes of `source` may
  /// not be valid for. [`Plan`] itself is such a `C` if `source` is a valid `S`. Otherwise `C` is
  /// the marks of a source type that the bytes are a valid value of, with the extent of `source`.
  #[inline(always)]
  const unsafe fn try_cast_by<C: Checks>(source: *const S) -> Result<*const D, Error> {
    let layout = const { S::LAYOUT };
    // The size of a value that exists: no overflow.
    let extent = layout.head() + ptr::metadata(source) * layout.stride();
    // The pointees of eager aims are looked for in the bytes of the source.
    let mut input = Input::new(source.cast::<u8>(), extent);
    // SAFETY: The requirements of `try_cast_in` are those of this call, and the bytes of `input`
    // are those of `source`, which are valid for reads and initialized where a marked check reads
    // them.
    unsafe { Self::try_cast_in::<C>(source, &mut input) }
  }

  /// [`Plan::try_cast_by`], which looks for the pointees of eager aims in the bytes of `input`,
  /// which hold the source.
  ///
  /// # Safety
  ///
  /// The caller must ensure the requirements of [`Plan::try_cast_by`], and that the bytes of
  /// `input` are in one allocation, valid for reads and initialized, and hold those of `source`, or
  /// are those of `source` and the bytes that a marked check reads in them are.
  #[inline(always)]
  const unsafe fn try_cast_in<C: Checks>(
    source: *const S,
    input: &mut Input,
  ) -> Result<*const D, Error> {
    let (layout, target) = const { (S::LAYOUT, D::LAYOUT) };
    let addr = source.cast::<u8>();
    if const { !Self::ANALYSIS.aligned } && !ptr::is_aligned(addr, target.align()) {
      return Err(Error::Alignment);
    }
    // The size of a value that exists: no overflow.
    let extent = layout.head() + ptr::metadata(source) * layout.stride();
    if extent < target.head() {
      return Err(Error::Size);
    }
    let rest = extent - target.head();
    let len = match target.stride() {
      0 if rest == 0 => 0,
      stride if stride != 0 && rest.is_multiple_of(stride) => rest / stride,
      _ => return Err(Error::Size),
    };
    // An unsized target of greater alignment may end with more padding than the source has room
    // for, even if its slice has a size of 0 (`[()]`).
    if const { D::LAYOUT.is_unsized() && D::LAYOUT.align() > S::LAYOUT.align() } {
      match (target.size_of_val(len), layout.size_of_val(ptr::metadata(source))) {
        (Some(needed), Some(available)) if needed <= available => {}
        _ => return Err(Error::Size),
      }
    }
    let target = ptr::from_raw_parts::<D>(addr, len);
    // SAFETY: This satisfies the safety contract of `validate`, which does not require `target` to
    // be aligned. `target` is at the address of `source`, and its extent is the extent of `source`,
    // which is checked above. Its size is at most the size of `source`. This is checked above if
    // `D` is unsized and more aligned than `S`. Otherwise `D` is sized, and its size is its extent,
    // or it is unsized and its extent is rounded up to an alignment that divides that of `S`. So
    // `target` is valid for reads of a `D`, by contract on the caller. Every byte that a marked
    // check reads is initialized, by contract on the caller, and the bytes of `input` hold it.
    if !unsafe { validate::<C, Root, D>(target, input) } {
      return Err(if input.limit { Error::Limit } else { Error::Validity });
    }
    Ok(target)
  }
}

impl<S: ?Sized + Pun, D: ?Sized + Pun> Checks for Plan<S, D> {
  const MARKS: Marks = Self::ANALYSIS.relation.marks;
}

/// The most bytes of a compile-time error message. The rest of a longer one is cut.
pub(crate) const MESSAGE_LEN: usize = 1024;

/// A compile-time error message, without `format!`.
struct Message {
  bytes: [u8; MESSAGE_LEN],
  len: usize,
}

impl Message {
  const fn new(what: &str) -> Self {
    Self { bytes: [0; MESSAGE_LEN], len: 0 }.str("\ncannot pun: ").str(what)
  }

  const fn str(mut self, str: &str) -> Self {
    // `panic` cuts the message at any byte, which is between two characters only in ASCII.
    assert!(str.is_ascii(), "a message of `puns` is not ASCII");
    let (bytes, mut i) = (str.as_bytes(), 0);
    while i < bytes.len() && self.len < self.bytes.len() {
      self.bytes[self.len] = bytes[i];
      (self.len, i) = (self.len + 1, i + 1);
    }
    self
  }

  /// Add the offset of what is wrong.
  const fn at(self, offset: usize) -> Self {
    let (mut digits, mut len, mut value) = ([0; 20], 0, offset);
    loop {
      len += 1;
      digits[20 - len] = b'0' + (value % 10) as u8;
      value /= 10;
      if value == 0 {
        break;
      }
    }
    // SAFETY: Each of the last `len` bytes of `digits` is `b'0' + value % 10`, an ASCII digit.
    // ASCII is valid UTF-8, as `from_utf8_unchecked` requires.
    let digits = unsafe { core::str::from_utf8_unchecked(digits.split_at(20 - len).1) };
    self.str(".\nThis is at byte ").str(digits).str(" of the target")
  }

  const fn panic(self) -> ! {
    let this = self.str(".\n");
    // SAFETY: The first `len` bytes of `this.bytes` are the bytes of `str`s, one after the
    // other, cut where they reach `MESSAGE_LEN` bytes, and the digits of `at`. `Message::str`,
    // which `Message::new` calls too, asserts that each `str` is ASCII, so the cut is between two
    // characters, and the bytes are UTF-8, as `from_utf8_unchecked` requires.
    panic!("{}", unsafe { core::str::from_utf8_unchecked(this.bytes.split_at(this.len).0) })
  }
}

// -------------------------------------------------------------------------------------------------
// By value

/// Reinterprets a value of type `S` as a value of type `D`.
///
/// The result has the bytes of `value`. As with [`core::mem::transmute`], the padding of the result
/// is not guaranteed to keep them. Nothing is checked at run time.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - `size_of::<S>() == size_of::<D>()`.
/// - Every byte that `D` requires to be initialized is initialized in every value of `S`.
/// - Every value of `S` is a [valid](crate#soundness) `D`.
/// - Without the `unstable` feature, whether the call compiles, what it checks and which values a
///   check accepts do not depend on a layout that Rust leaves to the compiler.
///
/// These conditions are checked after monomorphization, as
/// [the crate documentation](crate#compile-time-errors) explains.
///
/// # Examples
///
/// ```
/// assert_eq!(puns::pun::<[bool; 2], u16>([true, true]), 0x0101);
/// ```
///
/// ```compile_fail,E0080
/// // Not every `u8` is a `bool`.
/// let _: bool = puns::pun(2u8);
/// ```
#[inline(always)]
pub const fn pun<S: Pun + Copy, D: Pun + Copy>(value: S) -> D {
  const { Plan::<S, D>::BY_VALUE };
  // SAFETY: `Plan::<S, D>::BY_VALUE` compiled, so the analysis meets `Need::VALUE`. Its fit is
  // `Exact::Always`: `S` and `D` are `Copy`, so `Sized`, and `size_of::<S>() == size_of::<D>()`.
  // Its relation records nothing: no byte that `D` requires that may be padding in `S`, no disjoint
  // or partial part, no part that Rust leaves to the compiler. So every value of `S` is a valid `D`
  // (`Relation::infallible`). This is what `pun_unchecked` requires.
  unsafe { pun_unchecked(value) }
}

/// Reinterprets the first bytes of a value of type `S` as a value of type `D`.
///
/// The result has the first `size_of::<D>()` bytes of `value`. As with [`core::mem::transmute`],
/// the padding of the result is not guaranteed to keep them. Nothing is checked at run time.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - `size_of::<S>() >= size_of::<D>()`.
/// - Every byte that `D` requires to be initialized is initialized in the first
///   `size_of::<D>()` bytes of every value of `S`.
/// - The first `size_of::<D>()` bytes of every value of `S` are a [valid](crate#soundness) `D`.
/// - Without the `unstable` feature, whether the call compiles, what it checks and which values a
///   check accepts do not depend on a layout that Rust leaves to the compiler.
///
/// These conditions are checked after monomorphization, as
/// [the crate documentation](crate#compile-time-errors) explains.
///
/// # Examples
///
/// ```
/// assert_eq!(puns::head::<[u8; 4], [u8; 2]>([1, 2, 3, 4]), [1, 2]);
/// ```
///
/// ```compile_fail,E0080
/// // The second byte may be padding: Rust does not specify the order of the fields of a tuple,
/// // and `rustc` puts the `u16` after a byte of padding.
/// let _: [u8; 2] = puns::head((0u8, 0u16));
/// ```
#[inline(always)]
pub const fn head<S: Pun + Copy, D: Pun + Copy>(value: S) -> D {
  const { Plan::<S, D>::HEAD };
  // SAFETY: `Plan::<S, D>::HEAD` compiled, so the analysis meets `Need::PART`. Its fit can shrink:
  // `size_of::<S>() >= size_of::<D>()`. Its relation, of the first `size_of::<D>()` bytes of `S`,
  // records nothing (as in `pun`). So the first `size_of::<D>()` bytes of every value of `S` are a
  // valid `D`. This is what `pun_unchecked` requires.
  unsafe { pun_unchecked(value) }
}

/// Reinterprets the last bytes of a value of type `S` as a value of type `D`.
///
/// The result has the last `size_of::<D>()` bytes of `value`, which start `size_of::<S>() -
/// size_of::<D>()` bytes after its start. As with [`core::mem::transmute`], the padding of the
/// result is not guaranteed to keep them. Nothing is checked at run time.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - `size_of::<S>() >= size_of::<D>()`.
/// - Every byte that `D` requires to be initialized is initialized in the last `size_of::<D>()`
///   bytes of every value of `S`.
/// - The last `size_of::<D>()` bytes of every value of `S` are a [valid](crate#soundness) `D`.
/// - Without the `unstable` feature, whether the call compiles, what it checks and which values a
///   check accepts do not depend on a layout that Rust leaves to the compiler.
///
/// These conditions are checked after monomorphization, as
/// [the crate documentation](crate#compile-time-errors) explains.
///
/// # Examples
///
/// ```
/// assert_eq!(puns::tail::<[u8; 4], [u8; 2]>([1, 2, 3, 4]), [3, 4]);
/// // Two `bool`s are two initialized bytes.
/// assert_eq!(puns::tail::<[bool; 3], u16>([false, true, true]), 0x0101);
/// ```
///
/// ```compile_fail,E0080
/// // Not every `u8` is a `bool`.
/// let _: bool = puns::tail::<[u8; 2], bool>([1, 7]);
/// ```
///
/// ```compile_fail,E0080
/// #[derive(puns::Pun, Clone, Copy)]
/// #[repr(C)]
/// struct Padded {
///   a: u16,
///   b: u8,
/// }
///
/// // The last byte is padding.
/// let _: u8 = puns::tail(Padded { a: 0, b: 0 });
/// ```
#[inline(always)]
pub const fn tail<S: Pun + Copy, D: Pun + Copy>(value: S) -> D {
  const { Plan::<S, D>::TAIL };
  let shift = const { Plan::<S, D>::TAIL_ANALYSIS.shift };
  // SAFETY: `Plan::<S, D>::TAIL` compiled, so the analysis of the last bytes meets `Need::PART`.
  // Both types are sized, `size_of::<S>() >= size_of::<D>()`, and `shift` is `size_of::<S>() -
  // size_of::<D>()` (`analyze_tail`). Its relation records nothing (as in `pun`): the
  // `size_of::<D>()` bytes of every value of `S` from `shift` are a valid `D`. So:
  // - `add(shift)` stays within `value`, as `add` requires.
  // - The pointer is valid for reads of `size_of::<D>()` bytes, which end where `value` ends, and
  //   points to a valid `D`. These are the requirements of `read_unaligned` [1], which needs no
  //   alignment.
  //
  // [1] Per https://doc.rust-lang.org/1.99.0/core/ptr/fn.read_unaligned.html:
  //
  //     Behavior is undefined if any of the following conditions are violated:
  //
  //     - `src` must be valid for reads.
  //     - `src` must point to a properly initialized value of type `T`.
  unsafe { (&raw const value).cast::<u8>().add(shift).cast::<D>().read_unaligned() }
}

/// Reinterprets a value of type `S` as a value of type `D`, after checking what the analysis
/// could not prove.
///
/// The result has the bytes of `value`. As with [`core::mem::transmute`], the padding of the result
/// is not guaranteed to keep them. Only what the analysis could not prove is checked at run time.
///
/// # Errors
///
/// Returns [`Error::Validity`] if the bytes of `value` are not a [valid](crate#soundness) `D`.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - `size_of::<S>() == size_of::<D>()`.
/// - Every byte that `D` requires to be initialized is initialized in every value of `S`.
/// - Each part of `D` (each scalar that is not in an enum or a union, some variant of each enum,
///   and some field of each union) accepts what some value of `S` holds at its place. `D` is
///   [inhabited] if `S` is.
/// - Without the `unstable` feature, whether the call compiles, what it checks and which values a
///   check accepts do not depend on a layout that Rust leaves to the compiler.
///
/// These conditions are checked after monomorphization, as
/// [the crate documentation](crate#compile-time-errors) explains.
///
/// # Examples
///
/// ```
/// assert_eq!(puns::try_pun::<u8, bool>(1), Ok(true));
/// assert_eq!(puns::try_pun::<u8, bool>(2), Err(puns::Error::Validity));
/// ```
///
/// ```compile_fail,E0080
/// // A byte that is always 2 is never a `bool`.
/// let _ = puns::try_pun::<puns::num::u8c<2>, bool>(Default::default());
/// ```
///
/// [inhabited]: https://doc.rust-lang.org/reference/glossary.html#inhabited
#[inline]
pub const fn try_pun<S: Pun + Copy, D: Pun + Copy>(value: S) -> Result<D, Error> {
  const { Plan::<S, D>::TRY };
  // `D` holds no eager aim (`Plan::TRY`), so nothing is looked for in these bytes.
  let mut input = Input::new((&raw const value).cast(), size_of::<S>());
  // SAFETY: `Plan::<S, D>::TRY` compiled, so the analysis meets `Need::TRY`. `S` and `D` are
  // `Copy`, so `Sized`, and the fit is `Exact::Always`: they have the same size. So `value` is
  // valid for reads of a `D`, which `validate` does not need aligned. The relation records no byte
  // that may be padding in `S` and that `D` requires to be initialized, or that a check marked by
  // the analysis reads. So every byte that `validate` reads is initialized.
  if !unsafe { validate::<Plan<S, D>, Root, D>((&raw const value).cast(), &mut input) } {
    return Err(Error::Validity);
  }
  // SAFETY: `S` and `D` have the same size. `validate` found every part of `D` that the analysis
  // marked to be valid in `value`. Every part that it did not mark is valid in every value of `S`,
  // since the relation records no disjoint part and nothing that Rust leaves to the compiler. So
  // `value` is a valid `D`, as `pun_unchecked` requires.
  Ok(unsafe { pun_unchecked(value) })
}

/// Reinterprets the first `size_of::<D>()` bytes of `value` as a `D`.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `D` is not larger than `S`.
/// - The first `size_of::<D>()` bytes of `value` are a valid `D`.
#[inline(always)]
const unsafe fn pun_unchecked<S: Copy, D: Copy>(value: S) -> D {
  #[repr(C)]
  union Overlay<S: Copy, D: Copy> {
    source: S,
    target: D,
  }
  // SAFETY: The union is `repr(C)`, so both fields are at offset 0 [1], and reading `target` reads
  // the first `size_of::<D>()` bytes of `value` as a `D` [2]. `D` is not larger than `S`, and these
  // bytes are a valid `D`, by contract on the caller.
  //
  // [1] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.c.union.size-align:
  //
  //     Each field lives at byte offset 0 from the beginning of the union.
  //
  // [2] Per https://doc.rust-lang.org/1.99.0/reference/items/unions.html#r-items.union.fields.read:
  //
  //     Reading a union field reads the bits of the union at the field's type.
  unsafe { Overlay { source: value }.target }
}

// -------------------------------------------------------------------------------------------------
// By reference

/// Reinterprets a reference to an `S` as a reference to a `D`.
///
/// The returned reference points to the same bytes as `value`. If `D` ends with a slice, its length
/// is chosen so that the result covers the same bytes as `value`, up to the end of the slice of
/// `value`. Padding after that slice is not included. Nothing is checked at run time.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - Whatever the length of its slice, every value of `S` has a size that `D` can have. For sized
///   types, this means `size_of::<S>() == size_of::<D>()`.
/// - `D` needs no greater alignment than `S`. Use [`try_pun_ref`] to check the address at run
///   time.
/// - Every byte that `D` requires to be initialized is initialized in every value of `S`.
/// - Every value of `S` is a [valid](crate#soundness) `D`.
/// - Without the `unstable` feature, whether the call compiles, what it checks and which values a
///   check accepts do not depend on a layout that Rust leaves to the compiler.
///
/// These conditions are checked after monomorphization, as
/// [the crate documentation](crate#compile-time-errors) explains.
///
/// # Examples
///
/// ```
/// let bytes: &[u8] = puns::pun_ref(&[0x0102u16, 0x0304][..]);
/// assert_eq!(bytes.len(), 4);
/// let text: &[u8] = puns::pun_ref("héllo");
/// assert_eq!(text.len(), 6);
/// ```
///
/// ```compile_fail,E0080
/// // Bytes may be at any address, a `u16` may not.
/// let _: &u16 = puns::pun_ref(&[0u8; 2]);
/// ```
#[inline(always)]
pub const fn pun_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> &D {
  const { Plan::<S, D>::BY_REF };
  // SAFETY: `Plan::<S, D>::BY_REF` compiled, so the analysis meets `Need::REF`: the fit is
  // `Exact::Always`, `D` is no more aligned than `S`, and the relation is infallible.
  // `Plan::cast` returns a pointer to the address of `value`, with a tail length that gives the
  // target the extent of `value`. The reference made from it is valid [1]:
  // - It is not null, because `value` is a reference at the same address.
  // - It is aligned, because `value` is aligned for `S`, `D` is no more aligned than `S`, and
  //   alignments are powers of two.
  // - It is not dangling. The target has the extent of `value` and no greater alignment, so its
  //   size, which is its extent rounded up to its alignment, is at most the size of `value`. So it
  //   is within the allocation of `value`, and its size fits in an `isize`.
  // - It points to a valid `D`, because the relation is infallible: every value of `S` is a valid
  //   `D`.
  // The reference has the lifetime of the shared borrow `value`. Neither `S` nor `D` holds an
  // `UnsafeCell` (`Pun`, section Safety), so nothing writes the bytes while it lives.
  //
  // [1] Per https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.reference-box:
  //
  //     A reference or `Box<T>` must be aligned and non-null, it cannot be dangling, and it must
  //     point to a valid value (in case of dynamically sized types, using the actual dynamic type
  //     of the pointee as determined by the metadata).
  unsafe { &*Plan::<S, D>::cast(value) }
}

/// Reinterprets a mutable reference to an `S` as a mutable reference to a `D`.
///
/// The returned reference points to the same bytes as `value`, as with [`pun_ref`]. Since any `D`
/// may be written through it, every `D` must also be a valid `S`, except for the rules of `S`.
/// Nothing is checked at run time.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - Whatever the length of its slice, every value of `S` has a size that `D` can have. For sized
///   types, this means `size_of::<S>() == size_of::<D>()`.
/// - `D` needs no greater alignment than `S`. Use [`try_pun_mut`] to check the address at run
///   time.
/// - Every byte that `D` requires to be initialized is initialized in every value of `S`.
/// - Every value of `S` is a [valid](crate#soundness) `D`.
/// - Every value of `D` is a valid `S`, apart from the rules of `S`. A write through the returned
///   reference may break a rule of `S`, as a write to a field of `S` may, since a rule is a
///   [requirement](crate#guarantees-and-requirements). In particular, every byte that `S` requires
///   to be initialized is initialized in every value of `D`.
/// - Without the `unstable` feature, whether the call compiles, what it checks and which values a
///   check accepts do not depend on a layout that Rust leaves to the compiler.
///
/// These conditions are checked after monomorphization, as
/// [the crate documentation](crate#compile-time-errors) explains.
///
/// # Examples
///
/// ```
/// let mut value = 0x0102u16;
/// puns::pun_mut::<_, [u8; 2]>(&mut value)[0] = 0xff;
/// // The first byte is the low one on a little-endian target, and the high one otherwise.
/// assert!(value == 0x01ff || value == 0xff02);
/// ```
///
/// ```compile_fail,E0080
/// // A 2 could be written through the result where a `bool` is.
/// let _: &mut u8 = puns::pun_mut(&mut true);
/// ```
#[inline(always)]
pub const fn pun_mut<S: ?Sized + Pun, D: ?Sized + Pun>(value: &mut S) -> &mut D {
  const { Plan::<S, D>::BY_MUT };
  let source: *mut S = value;
  // SAFETY: `Plan::<S, D>::BY_MUT` compiled, so `Plan::<S, D>::BY_REF` did too. As in `pun_ref`,
  // the pointer that `Plan::cast` returns is not null, is aligned for `D`, is in the allocation of
  // `value`, and points to a valid `D`. It is made from `source`, a pointer that may write, so
  // writes through it are allowed. The reference has the lifetime of the exclusive borrow `value`,
  // so nothing else reads or writes the bytes while it lives. What may be written through it is any
  // `D`, padding included. `Plan::MUTABLE` is `true` (`mutable`): every value of `D`, padding
  // included, is a valid `S`. So `value` is a valid `S` when the borrow ends.
  unsafe { &mut *Plan::<S, D>::cast(source).cast_mut() }
}

/// Reinterprets a reference to the first bytes of an `S` as a reference to a `D`.
///
/// The returned reference points to the start of `value`. A sized `D` covers the first
/// `size_of::<D>()` bytes. A `D` that ends with a slice takes as many elements as fit in `value`.
/// Nothing is checked at run time.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - Every value of `S` is at least as large as the smallest `D`. For sized types, this means
///   `size_of::<S>() >= size_of::<D>()`.
/// - `D` needs no greater alignment than `S`.
/// - Every byte that `D` requires to be initialized is initialized in the bytes that it covers
///   in every value of `S`.
/// - The bytes that `D` covers in every value of `S` are a [valid](crate#soundness) `D`.
/// - Without the `unstable` feature, whether the call compiles, what it checks and which values a
///   check accepts do not depend on a layout that Rust leaves to the compiler.
///
/// These conditions are checked after monomorphization, as
/// [the crate documentation](crate#compile-time-errors) explains.
///
/// # Examples
///
/// ```
/// let head: &[u8; 2] = puns::head_ref(&[1u8, 2, 3]);
/// assert_eq!(head, &[1, 2]);
/// let pairs: &[[u8; 2]] = puns::head_ref(&[1u8, 2, 3, 4, 5]);
/// assert_eq!(pairs, &[[1, 2], [3, 4]]);
/// ```
///
/// ```compile_fail,E0080
/// // An empty slice has no first byte.
/// let _: &u8 = puns::head_ref::<[u8], u8>(&[1, 2]);
/// ```
#[inline(always)]
pub const fn head_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> &D {
  const { Plan::<S, D>::HEAD_REF };
  // SAFETY: `Plan::<S, D>::HEAD_REF` compiled, so the analysis meets `Need::PART_REF`. The fit can
  // shrink: the sized part of `S` is at least that of `D`. So `Plan::cast` returns a pointer to the
  // address of `value` whose extent is at most that of `value`: the extent of `value` if the fit is
  // `Exact::Always`, else as many elements as fit. The reference is valid, as in `pun_ref`:
  // - It is not null, because `value` is a reference at the same address.
  // - It is aligned, because `D` is no more aligned than `S` (`aligned`).
  // - It is not dangling, because a smaller extent and no greater alignment make a size at most
  //   the size of `value`.
  // - It points to a valid `D`, because the relation of the bytes of `D` records nothing.
  // The reference has the lifetime of the shared borrow `value`, and neither type holds an
  // `UnsafeCell` (`Pun`, section Safety).
  unsafe { &*Plan::<S, D>::cast(value) }
}

/// Reinterprets a reference to the last bytes of an `S` as a reference to a `D`.
///
/// The returned reference points `size_of::<S>() - size_of::<D>()` bytes after the start of
/// `value`, and covers the last `size_of::<D>()` bytes. Nothing is checked at run time.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - `S` is sized, because where the last bytes of an unsized value start is known only at run
///   time. Use [`from_tail`] to take a value from the end of bytes.
/// - `D` is sized.
/// - `size_of::<S>() >= size_of::<D>()`.
/// - `align_of::<D>() <= align_of::<S>()`. The offset of the result is then a multiple of the
///   alignment of `D`.
/// - Every byte that `D` requires to be initialized is initialized in the last `size_of::<D>()`
///   bytes of every value of `S`.
/// - The last `size_of::<D>()` bytes of every value of `S` are a [valid](crate#soundness) `D`.
/// - Without the `unstable` feature, whether the call compiles, what it checks and which values a
///   check accepts do not depend on a layout that Rust leaves to the compiler.
///
/// These conditions are checked after monomorphization, as
/// [the crate documentation](crate#compile-time-errors) explains.
///
/// # Examples
///
/// ```
/// let tail: &[u8; 2] = puns::tail_ref(&[1u8, 2, 3]);
/// assert_eq!(tail, &[2, 3]);
/// let last: &u16 = puns::tail_ref(&[1u16, 2, 3]);
/// assert_eq!(*last, 3);
/// ```
///
/// ```compile_fail,E0080
/// // An unsized source.
/// let _: &u8 = puns::tail_ref::<[u8], u8>(&[1, 2]);
/// ```
///
/// ```compile_fail,E0080
/// // A greater alignment.
/// let _: &u16 = puns::tail_ref(&[0u8; 4]);
/// ```
#[inline(always)]
pub const fn tail_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> &D {
  const { Plan::<S, D>::TAIL_REF };
  // SAFETY: `Plan::<S, D>::TAIL_REF` compiled, so the analysis of the last bytes meets
  // `Need::PART_REF`. Both types are sized, and `shift` is `size_of::<S>() - size_of::<D>()`
  // (`analyze_tail`). `Plan::tail_cast` returns a pointer to the `size_of::<D>()` bytes of `value`
  // from `shift`, which end where `value` ends. The reference is valid, as in `pun_ref`:
  // - It is not null, because it points within `value`, a reference.
  // - It is aligned, because the analysis found `D` no more aligned than `S`, and `shift` a
  //   multiple of the alignment of `D` (`aligned`).
  // - It is not dangling, because it is within `value`.
  // - It points to a valid `D`, because the relation of the last bytes records nothing.
  // The reference has the lifetime of the shared borrow `value`, and neither type holds an
  // `UnsafeCell` (`Pun`, section Safety).
  unsafe { &*Plan::<S, D>::tail_cast(value) }
}

/// Reinterprets a mutable reference to the first bytes of an `S` as a mutable reference to a `D`.
///
/// The returned reference covers the bytes that [`head_ref`] covers. Any `D` may be written
/// through it, so what is written must leave a valid `S`. Nothing is checked at run time.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - Every value of `S` is at least as large as the smallest `D`. For sized types, this means
///   `size_of::<S>() >= size_of::<D>()`.
/// - `D` needs no greater alignment than `S`.
/// - Every byte that `D` requires to be initialized is initialized in the bytes that it covers
///   in every value of `S`.
/// - The bytes that `D` covers in every value of `S` are a [valid](crate#soundness) `D`.
/// - Every value of `D`, written over the bytes that it covers, leaves a valid `S`, whatever the
///   other bytes of `S` hold.
/// - No struct of `S` that has a rule (a `size`, a `len`, an `assert` or a region) overlaps the
///   covered bytes, unless that struct is in no enum, lies within the covered bytes, and is in `D`
///   at the same place with the same rules.
/// - Each enum of `S` lies either wholly within the covered bytes or wholly outside them,
///   whatever the length of the slice of `S`.
/// - No `str` of `S` overlaps the covered bytes, unless both `S` and `D` end with a slice.
/// - Without the `unstable` feature, whether the call compiles, what it checks and which values a
///   check accepts do not depend on a layout that Rust leaves to the compiler.
///
/// These conditions are checked after monomorphization, as
/// [the crate documentation](crate#compile-time-errors) explains.
///
/// # Examples
///
/// ```
/// let mut words = [0x0102u16, 0x0304];
/// *puns::head_mut::<_, u16>(&mut words) = 7;
/// assert_eq!(words, [7, 0x0304]);
/// let mut bytes = [0u8; 5];
/// puns::head_mut::<_, [[u8; 2]]>(&mut bytes[..]).fill([1, 2]);
/// assert_eq!(bytes, [1, 2, 1, 2, 0]);
/// ```
///
/// ```compile_fail,E0080
/// // A `u8` can be 2, which is not a `bool`.
/// let _: &mut u8 = puns::head_mut(&mut [true, false]);
/// ```
///
/// ```compile_fail,E0080
/// #[derive(puns::Pun)]
/// #[repr(u8)]
/// enum Shape {
///   Circle { radius: u16 },
///   Rect(u8, bool),
/// }
///
/// // Only the tag of the shape, without the fields of its variant.
/// let _: &mut u8 = puns::head_mut(&mut Shape::Rect(1, true));
/// ```
#[inline(always)]
pub const fn head_mut<S: ?Sized + Pun, D: ?Sized + Pun>(value: &mut S) -> &mut D {
  const { Plan::<S, D>::HEAD_MUT };
  let source: *mut S = value;
  // SAFETY: `Plan::<S, D>::HEAD_MUT` compiled, so the analysis meets `Need::PART_REF`, and
  // `mutable_part` found nothing to refuse. As in `head_ref`, the pointer that `Plan::cast` returns
  // is not null, is aligned for `D`, is within `value`, and points to a valid `D`. It is made from
  // `source`, a pointer that may write. The reference has the lifetime of the exclusive borrow
  // `value`, so nothing else reads or writes the bytes while it lives. Any `D`, padding included,
  // may be written through it. `mutable_part` found that every value of `D` is valid for what `S`
  // has over its bytes, and that nothing of `S` outside them depends on them. So `value` is a valid
  // `S` when the borrow ends.
  unsafe { &mut *Plan::<S, D>::cast(source).cast_mut() }
}

/// Reinterprets a mutable reference to the last bytes of an `S` as a mutable reference to a `D`.
///
/// The returned reference covers the bytes that [`tail_ref`] covers. Any `D` may be written through
/// it, so what is written must leave a valid `S`. Nothing is checked at run time.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - `S` is sized, because where the last bytes of an unsized value start is known only at run
///   time. Use [`from_tail_mut`] to take a value from the end of bytes.
/// - `D` is sized.
/// - `size_of::<S>() >= size_of::<D>()`.
/// - `align_of::<D>() <= align_of::<S>()`.
/// - Every byte that `D` requires to be initialized is initialized in the last `size_of::<D>()`
///   bytes of every value of `S`.
/// - The last `size_of::<D>()` bytes of every value of `S` are a [valid](crate#soundness) `D`.
/// - Every value of `D`, written over the bytes that it covers, leaves a valid `S`, whatever the
///   other bytes of `S` hold.
/// - No struct of `S` that has a rule (a `size`, a `len`, an `assert` or a region) overlaps the
///   covered bytes, unless that struct is in no enum, lies within the covered bytes, and is in `D`
///   at the same place with the same rules.
/// - Each enum of `S` lies either wholly within the covered bytes or wholly outside them.
/// - Without the `unstable` feature, whether the call compiles, what it checks and which values a
///   check accepts do not depend on a layout that Rust leaves to the compiler.
///
/// These conditions are checked after monomorphization, as
/// [the crate documentation](crate#compile-time-errors) explains.
///
/// # Examples
///
/// ```
/// let mut words = [0x0102u16, 0x0304, 0x0506];
/// *puns::tail_mut::<_, [u16; 2]>(&mut words) = [7, 8];
/// assert_eq!(words, [0x0102, 7, 8]);
/// ```
///
/// ```compile_fail,E0080
/// #[derive(puns::Pun)]
/// #[repr(C)]
/// #[pun(assert(a < b))]
/// struct Ordered {
///   a: u8,
///   b: u8,
/// }
///
/// // Writing `b` could break the rule.
/// let _: &mut u8 = puns::tail_mut(&mut Ordered { a: 1, b: 2 });
/// ```
#[inline(always)]
pub const fn tail_mut<S: ?Sized + Pun, D: ?Sized + Pun>(value: &mut S) -> &mut D {
  const { Plan::<S, D>::TAIL_MUT };
  let source: *mut S = value;
  // SAFETY: `Plan::<S, D>::TAIL_MUT` compiled, so the analysis of the last bytes meets
  // `Need::PART_REF`, and `mutable_part` found nothing to refuse at `shift`. As in `tail_ref`, the
  // pointer that `Plan::tail_cast` returns is not null, is aligned for `D`, is within `value`, and
  // points to a valid `D`. It is made from `source`, a pointer that may write. The reference has
  // the lifetime of the exclusive borrow `value`. As in `head_mut`, any `D` written through it
  // leaves `value` a valid `S`, by what `mutable_part` found of the bytes from `shift`.
  unsafe { &mut *Plan::<S, D>::tail_cast(source).cast_mut() }
}

/// Reinterprets a reference to an `S` as a reference to a `D`, after checking what the analysis
/// could not prove.
///
/// The returned reference points to the same bytes as `value`. Only what the analysis could not
/// prove is checked at run time: the size, if it depends on the length of a slice; the address,
/// if `D` is more aligned than `S`; and the values of `D` that the bytes of `S` do not guarantee.
///
/// In a constant, a call compiles only if `D` is no more aligned than `S`, because const evaluation
/// cannot read an address to check it.
///
/// # Errors
///
/// Returns [`Error::Alignment`] if `value` is not aligned for `D`, [`Error::Size`] if no `D` has
/// the extent of `value` (its size without the padding that may follow its slice) or if that `D` is
/// larger than `value`, and [`Error::Validity`] if the bytes are not a [valid](crate#soundness)
/// `D`. The checks are made in this order, so a value that fails two of them gives the error of the
/// first.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - Some length of `value` gives it a size that `D` can have. For sized types, this means
///   `size_of::<S>() == size_of::<D>()`.
/// - Every byte that `D` requires to be initialized is initialized in every value of `S`.
/// - Each part of `D` (each scalar that is not in an enum or a union, some variant of each enum,
///   and some field of each union) accepts what some value of `S` holds at its place. `D` is
///   [inhabited] if `S` is.
/// - Without the `unstable` feature, whether the call compiles, what it checks and which values a
///   check accepts do not depend on a layout that Rust leaves to the compiler.
///
/// These conditions are checked after monomorphization, as
/// [the crate documentation](crate#compile-time-errors) explains.
///
/// # Examples
///
/// ```
/// assert_eq!(puns::try_pun_ref::<[u8], str>(b"caf\xc3\xa9"), Ok("café"));
/// assert!(puns::try_pun_ref::<[u8], str>(b"caf\xc3").is_err());
/// ```
///
/// ```compile_fail,E0080
/// // One byte of the source is padding, whatever the order of its fields.
/// let _ = puns::try_pun_ref::<_, [u8; 4]>(&(0u8, 0u16));
/// ```
///
/// [inhabited]: https://doc.rust-lang.org/reference/glossary.html#inhabited
#[inline]
pub const fn try_pun_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> Result<&D, Error> {
  const { Plan::<S, D>::TRY_REF };
  // SAFETY: `value` is a reference, so it points to a valid `S`, valid for reads.
  // `Plan::<S, D>::TRY_REF` compiled above. These are the requirements of `try_cast`.
  match unsafe { Plan::<S, D>::try_cast(value) } {
    // SAFETY: `Plan::<S, D>::TRY_REF` compiled, so the analysis meets `Need::TRY_REF`, and
    // `Plan::try_cast` returned `Ok`. `target` is the address of `value`, with a tail length that
    // gives it the extent of `value`. The reference is valid, as in `pun_ref`:
    // - It is not null, because `value` is a reference at the same address.
    // - It is aligned. Either the analysis found `D` no more aligned than `S`, or
    //   `Plan::try_cast_by` checked the address.
    // - It is not dangling. Its extent is that of `value`. If `D` is unsized and more aligned than
    //   `S`, `Plan::try_cast_by` checked that its size is at most the size of `value`. Otherwise
    //   either `D` is sized, and its size is its extent, or `D` is unsized and its extent is
    //   rounded up to an alignment that divides that of `S`. Either way its size is at most the
    //   size of `value`.
    // - It points to a valid `D`. `validate` found every part that the analysis marked to be
    //   valid. Every other part is valid in every value of `S`, since the relation records no
    //   byte read from padding, no disjoint part, and nothing that Rust leaves to the compiler.
    // The reference has the lifetime of the shared borrow `value`, and neither type holds an
    // `UnsafeCell` (`Pun`, section Safety).
    Ok(target) => Ok(unsafe { &*target }),
    Err(error) => Err(error),
  }
}

/// Reinterprets a mutable reference to an `S` as a mutable reference to a `D`, after checking
/// what the analysis could not prove.
///
/// The returned reference points to the same bytes as `value`, and is checked at run time as
/// [`try_pun_ref`] checks it. Since any `D` may be written through the result, every `D` must also
/// be a valid `S`, as for [`pun_mut`].
///
/// In a constant, a call compiles only if `D` is no more aligned than `S`, because const evaluation
/// cannot read an address to check it.
///
/// # Errors
///
/// Returns [`Error::Alignment`] if `value` is not aligned for `D`, [`Error::Size`] if no `D` has
/// the extent of `value` (its size without the padding that may follow its slice) or if that `D` is
/// larger than `value`, and [`Error::Validity`] if the bytes are not a [valid](crate#soundness)
/// `D`. The checks are made in this order, so a value that fails two of them gives the error of the
/// first.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - Some length of `value` gives it a size that `D` can have. For sized types, this means
///   `size_of::<S>() == size_of::<D>()`.
/// - Every byte that `D` requires to be initialized is initialized in every value of `S`.
/// - Each part of `D` (each scalar that is not in an enum or a union, some variant of each enum,
///   and some field of each union) accepts what some value of `S` holds at its place. `D` is
///   [inhabited] if `S` is.
/// - Every value of `D` is a valid `S`, apart from the rules of `S`. A write through the returned
///   reference may break a rule of `S`, as a write to a field of `S` may, since a rule is a
///   [requirement](crate#guarantees-and-requirements). In particular, every byte that `S` requires
///   to be initialized is initialized in every value of `D`.
/// - Without the `unstable` feature, whether the call compiles, what it checks and which values a
///   check accepts do not depend on a layout that Rust leaves to the compiler.
///
/// These conditions are checked after monomorphization, as
/// [the crate documentation](crate#compile-time-errors) explains.
///
/// # Examples
///
/// ```
/// let mut byte = 1u8;
/// let flag: &mut bool = puns::try_pun_mut(&mut byte).unwrap();
/// *flag = false;
/// assert_eq!(byte, 0);
///
/// let mut two = 2u8;
/// assert_eq!(puns::try_pun_mut::<u8, bool>(&mut two), Err(puns::Error::Validity));
/// ```
///
/// ```compile_fail,E0080
/// // A `u8` can be written through the result, which a `bool` cannot hold.
/// let _ = puns::try_pun_mut::<bool, u8>(&mut true);
/// ```
///
/// [inhabited]: https://doc.rust-lang.org/reference/glossary.html#inhabited
#[inline]
pub const fn try_pun_mut<S: ?Sized + Pun, D: ?Sized + Pun>(value: &mut S) -> Result<&mut D, Error> {
  const { Plan::<S, D>::TRY_MUT };
  let source: *mut S = value;
  // SAFETY: `source` is made from a reference, so it points to a valid `S`, valid for reads.
  // `Plan::<S, D>::TRY_MUT` compiled above, and it requires `Plan::<S, D>::TRY_REF`. These are the
  // requirements of `try_cast`.
  match unsafe { Plan::<S, D>::try_cast(source) } {
    // SAFETY: `Plan::<S, D>::TRY_MUT` compiled, so `Plan::<S, D>::TRY_REF` did too, and
    // `Plan::MUTABLE` is `true`. As in `try_pun_ref`, `target` is aligned for `D`, is within
    // `value`, and points to a valid `D`. It is made from `source`, a pointer that may write. The
    // reference has the lifetime of the exclusive borrow `value`. As in `pun_mut`, every value of
    // `D`, padding included, is a valid `S`, so `value` is a valid `S` when the borrow ends.
    Ok(target) => Ok(unsafe { &mut *target.cast_mut() }),
    Err(error) => Err(error),
  }
}

/// Returns a value of type `T` whose fields and elements are all zero bytes.
///
/// As with [`core::mem::zeroed`], the padding of the result is not guaranteed to be zero. The rules
/// of `T` (see [`macro@Pun`]) are not checked, so the result may break them.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - A `T` whose bytes are all zero is a [valid](crate#soundness) `T`, apart from its rules.
/// - Without the `unstable` feature, whether the call compiles, what it checks and which values a
///   check accepts do not depend on a layout that Rust leaves to the compiler.
///
/// These conditions are checked after monomorphization, as
/// [the crate documentation](crate#compile-time-errors) explains.
///
/// # Examples
///
/// ```
/// assert_eq!(puns::zeroed::<(u32, [bool; 2])>(), (0, [false; 2]));
/// ```
///
/// ```compile_fail,E0080
/// let _ = puns::zeroed::<core::num::NonZero<u8>>();
/// ```
#[inline(always)]
#[must_use]
pub const fn zeroed<T: Pun + Copy>() -> T {
  const {
    assert!(T::LAYOUT.zeroable(), "zero is not a valid value of this type");
  };
  const {
    assert!(!T::LAYOUT.holds_eager(false), "a value of this type would hold an eager aim");
  };
  // SAFETY: `zeroed` requires that bytes that are all zero are a valid `T` [1]. The assertion above
  // found that the layout of `T` accepts them (`Layout::zeroable`), and every value that the
  // layout accepts is a valid `T` (`Pun`, section Safety). Rules are requirements of `puns`, not
  // of the language, and the result may break them.
  //
  // [1] Per https://doc.rust-lang.org/1.99.0/core/mem/fn.zeroed.html:
  //
  //     There is no guarantee that an all-zero byte-pattern represents a valid value of some type
  //     `T`.
  unsafe { core::mem::zeroed() }
}

/// Returns the value of an unsigned integer field, as the expressions of `#[pun(get(..))]` read
/// it.
///
/// # Compile-time errors
///
/// A call compiles only if `T` is an unsigned integer of 1 to 16 bytes, in any byte order, or a
/// sized struct whose only field of nonzero size is such an integer and covers the struct. This
/// is checked after monomorphization, as [the crate documentation](crate#compile-time-errors)
/// explains.
#[doc(hidden)]
#[inline(always)]
pub const fn int<T: Pun>(value: &T) -> u128 {
  let spec = const {
    match T::LAYOUT.as_integer() {
      Some(spec) => spec,
      None => panic!(
        "a getter reads the unsigned integer fields of its struct as `u128`s, and this field is \
         not one (a signed or floating-point number, a `bool`, a `char` or an enum is not)"
      ),
    }
  };
  // SAFETY: `as_integer` returned `spec`, the scalar of `T` or of its only field with a size, at
  // offset 0 and of the size of `T`. This satisfies the safety contract of `Spec::load`:
  // - `spec.is_integer()` is `true`, which `as_integer` requires.
  // - `value` is a reference, so valid for reads of `size_of::<T>()` bytes, which is
  //   `spec.size`. `spec` is never uninitialized, and every value of `T` holds what its layout
  //   says (`Pun`, section Safety), so these bytes are initialized.
  unsafe { spec.load(core::ptr::from_ref::<T>(value).cast()) }
}

/// Returns the bytes of a value.
///
/// `as_bytes(value)` is [`pun_ref::<S, [u8]>(value)`](pun_ref), with identical behavior. Since the
/// target is named, only `S` is inferred. The result covers `value` up to the end of its slice,
/// without any padding after the slice. Nothing is checked at run time.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - Every byte that the result covers is initialized in every value of `S`.
/// - Without the `unstable` feature, whether the call compiles, what it checks and which values a
///   check accepts do not depend on a layout that Rust leaves to the compiler.
///
/// These conditions are checked after monomorphization, as
/// [the crate documentation](crate#compile-time-errors) explains.
///
/// # Examples
///
/// ```
/// assert_eq!(puns::as_bytes(&[1u16.to_be(), 2u16.to_be()]), &[0, 1, 0, 2]);
/// ```
///
/// ```compile_fail,E0080
/// // One byte is padding, whatever the order of the fields.
/// let _ = puns::as_bytes(&(0u8, 0u16));
/// ```
#[inline(always)]
pub const fn as_bytes<S: ?Sized + Pun>(value: &S) -> &[u8] {
  // The body of `pun_ref`, not a call to it: a call would evaluate the constant inside `puns`
  // again, and a refusal would end with notes that point there instead of at the caller.
  const { Plan::<S, [u8]>::BY_REF };
  // SAFETY: As in `pun_ref`, with `[u8]` for `D`.
  unsafe { &*Plan::<S, [u8]>::cast(value) }
}

/// Reinterprets bytes as a reference to a `D`, after checking them.
///
/// `from_bytes(bytes)` is [`try_pun_ref::<[u8], D>(bytes)`](try_pun_ref), with identical behavior.
/// It fixes the source to bytes, which `try_pun_ref` would infer from its argument. For example,
/// the source of `try_pun_ref(&[1, 0])` is an `[i32; 2]`, because an integer literal without a
/// suffix is an `i32` unless something says otherwise. It also takes one type parameter instead of
/// two.
///
/// It checks the address if `D` needs an alignment greater than 1, then the size, then the values
/// of `D` that bytes do not guarantee.
///
/// In a constant, a call compiles only if `D` has an alignment of 1, because const evaluation
/// cannot read the address of the bytes to check it.
///
/// # Errors
///
/// Returns [`Error::Alignment`] if `bytes` is not aligned for `D`, [`Error::Size`] if the length
/// of `bytes` is not a size that `D` can have, and [`Error::Validity`] if the bytes are not a
/// [valid](crate#soundness) `D`.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - `D` is [inhabited].
/// - Without the `unstable` feature, whether the call compiles, what it checks and which values a
///   check accepts do not depend on a layout that Rust leaves to the compiler.
///
/// These conditions are checked after monomorphization, as
/// [the crate documentation](crate#compile-time-errors) explains.
///
/// # Examples
///
/// ```
/// use puns::{Error, Pun};
///
/// #[derive(Pun, Debug, PartialEq)]
/// #[repr(C)]
/// struct Position {
///   x: u8,
///   visible: bool,
/// }
///
/// assert_eq!(puns::from_bytes(&[7, 1]), Ok(&Position { x: 7, visible: true }));
/// assert_eq!(puns::from_bytes::<Position>(&[7, 2]), Err(Error::Validity));
/// assert_eq!(puns::from_bytes::<Position>(&[7, 1, 0]), Err(Error::Size));
/// assert_eq!(puns::from_bytes::<str>(b"caf\xc3\xa9"), Ok("caf\u{e9}"));
/// ```
///
/// The source must be bytes, so a literal of another integer type does not compile:
///
/// ```compile_fail,E0308
/// let _ = puns::from_bytes::<[u8; 2]>(&[7i32, 1]);
/// ```
///
/// No bytes are a value of a type without values:
///
/// ```compile_fail,E0080
/// let _ = puns::from_bytes::<core::convert::Infallible>(&[]);
/// ```
///
/// [inhabited]: https://doc.rust-lang.org/reference/glossary.html#inhabited
#[inline]
pub const fn from_bytes<D: ?Sized + Pun>(bytes: &[u8]) -> Result<&D, Error> {
  // The body of `try_pun_ref`, not a call to it, for the reason given in `as_bytes`.
  const { Plan::<[u8], D>::TRY_REF };
  // SAFETY: As in `try_pun_ref`, with `[u8]` for `S`.
  match unsafe { Plan::<[u8], D>::try_cast(bytes) } {
    // SAFETY: As in `try_pun_ref`, with `[u8]` for `S`.
    Ok(target) => Ok(unsafe { &*target }),
    Err(error) => Err(error),
  }
}

/// Reinterprets mutable bytes as a mutable reference to a `D`, after checking them.
///
/// `from_bytes_mut(bytes)` is [`try_pun_mut::<[u8], D>(bytes)`](try_pun_mut), with identical
/// behavior, and exists for the same reasons as [`from_bytes`], whose checks it makes. Since any
/// `D` may be written through the result, every byte of a `D` must be initialized, as bytes are.
///
/// In a constant, a call compiles only if `D` has an alignment of 1, because const evaluation
/// cannot read the address of the bytes to check it.
///
/// # Errors
///
/// Returns [`Error::Alignment`] if `bytes` is not aligned for `D`, [`Error::Size`] if the length
/// of `bytes` is not a size that `D` can have, and [`Error::Validity`] if the bytes are not a
/// [valid](crate#soundness) `D`.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - `D` is [inhabited].
/// - Every byte of every value of `D` is initialized.
/// - Without the `unstable` feature, whether the call compiles, what it checks and which values a
///   check accepts do not depend on a layout that Rust leaves to the compiler.
///
/// These conditions are checked after monomorphization, as
/// [the crate documentation](crate#compile-time-errors) explains.
///
/// # Examples
///
/// ```
/// let mut bytes = [0u8, 0, 0, 1];
/// *puns::from_bytes_mut::<[bool; 2]>(&mut bytes[2..]).unwrap() = [true, false];
/// assert_eq!(bytes, [0, 0, 1, 0]);
/// assert!(puns::from_bytes_mut::<[bool; 2]>(&mut [2, 0]).is_err());
/// ```
///
/// ```compile_fail,E0080
/// // A `(u8, u16)` has a byte of padding.
/// let _ = puns::from_bytes_mut::<(u8, u16)>(&mut [0; 4]);
/// ```
///
/// [inhabited]: https://doc.rust-lang.org/reference/glossary.html#inhabited
#[inline]
pub const fn from_bytes_mut<D: ?Sized + Pun>(bytes: &mut [u8]) -> Result<&mut D, Error> {
  // The body of `try_pun_mut`, not a call to it, for the reason given in `as_bytes`.
  const { Plan::<[u8], D>::TRY_MUT };
  let source: *mut [u8] = bytes;
  // SAFETY: As in `try_pun_mut`, with `[u8]` for `S`.
  match unsafe { Plan::<[u8], D>::try_cast(source) } {
    // SAFETY: As in `try_pun_mut`, with `[u8]` for `S`.
    Ok(target) => Ok(unsafe { &mut *target.cast_mut() }),
    Err(error) => Err(error),
  }
}

// -------------------------------------------------------------------------------------------------
// Parsing

impl<D: ?Sized + Pun> Plan<[u8], D> {
  /// Returns the `D` that the bytes at `bytes` are, all of them, checked as [`from_bytes`] checks
  /// bytes, with the pointees of its eager aims in the bytes of `input`.
  ///
  /// # Safety
  ///
  /// The caller must ensure that `bytes` is valid for reads of its initialized bytes, and that the
  /// bytes of `input` are in one allocation, valid for reads and initialized, and hold them.
  #[inline(always)]
  const unsafe fn try_cast_whole(bytes: *const [u8], input: &mut Input) -> Result<*const D, Error> {
    // SAFETY: The bytes are a valid `[u8]`, valid for reads and initialized, by contract on the
    // caller, and so are those of `input`, which hold them. These are the requirements of
    // `try_cast_in`, with the marks of the analysis of bytes as a `D`: every part of `D` that some
    // bytes are not valid for.
    unsafe { Self::try_cast_in::<Self>(bytes, input) }
  }

  /// Returns the `D` at the start of the `n` bytes at `addr`, and its size, after every check
  /// that could not be made at compile time (`from_head`, `from_head_mut`).
  ///
  /// # Safety
  ///
  /// The caller must ensure that:
  ///
  /// - `addr` is valid for reads of `n` initialized bytes.
  #[inline(always)]
  const unsafe fn front(addr: *const u8, n: usize) -> Result<(*const D, usize), Error> {
    // SAFETY: `addr` is valid for reads of `n` initialized bytes, by contract on the caller, as
    // `front_by` requires. `C` is `Self`, whose marks are those of the analysis of bytes as a `D`:
    // every part of `D` that some bytes are not valid for. So a pointer that it returns is a
    // valid `D`.
    unsafe { Self::front_by::<Self>(addr, n, usize::MAX) }
  }

  /// Returns the `D` at the start of the `n` bytes at `addr`, and its size, as [`Plan::front`]
  /// does, but checks what `C` marks rather than everything that bytes do not prove.
  ///
  /// A `D` whose extent (its size without the trailing padding of a slice) is not `extent` is not
  /// valid, unless `extent` is `usize::MAX`, which allows any extent.
  ///
  /// # Safety
  ///
  /// The caller must ensure that:
  ///
  /// - `addr` is valid for reads of `n` initialized bytes.
  ///
  /// The returned pointer is a valid `D` only if `C` marks every part of `D` that the bytes may
  /// not be valid for. It does if `C` is [`Plan`] itself, whose marks are those of any bytes as a
  /// `D`. It also does if `C` holds the marks of the analysis of a source `S` as a `D`, and the
  /// first `extent` bytes at `addr` are a valid `S`.
  #[inline(always)]
  const unsafe fn front_by<C: Checks>(
    addr: *const u8,
    n: usize,
    extent: usize,
  ) -> Result<(*const D, usize), Error> {
    let (layout, rule) = const { (D::LAYOUT, D::LAYOUT.length_rule()) };
    let (head, stride) = (layout.head(), layout.stride());
    if n < head {
      return Err(Error::Size);
    }
    if const { D::LAYOUT.align() > 1 } && !ptr::is_aligned(addr, layout.align()) {
      return Err(Error::Alignment);
    }
    let len = match rule {
      _ if stride == 0 => 0,
      // As many elements as fit, trailing padding included.
      None => match n & !(layout.align() - 1) {
        room if room >= head => (room - head) / stride,
        _ => return Err(Error::Size),
      },
      Some(rule) => {
        // SAFETY: This satisfies the safety contract of `validate::length`. `addr` is valid for
        // reads of `n` initialized bytes, by contract on the caller. `n >= head`, which is
        // checked above, and the sized fields of `D`, and of the struct in it that has the rule,
        // are in the first `head` bytes. `D` has such a rule: `length_rule` returned it.
        let Some(value) = (unsafe { validate::length::<D>(addr, n) }) else {
          return Err(Error::Validity);
        };
        // From the size of the struct that has the rule, down to a number of elements: in a
        // `usize` (where the value is a field, the optimizer knows that it fits one), else a
        // size or a length that no bytes have.
        let before = head - rule.base;
        if value > usize::MAX as u128 {
          let whole = rule.elements || (value - before as u128).is_multiple_of(stride as u128);
          return Err(if whole { Error::Size } else { Error::Validity });
        }
        let value = value as usize;
        let len = match () {
          () if rule.elements => value,
          () if value >= before && (value - before).is_multiple_of(stride) => {
            (value - before) / stride
          }
          () => return Err(Error::Validity),
        };
        if len > (n - head) / stride {
          return Err(Error::Size);
        }
        len
      }
    };
    // At most the `n` bytes: no overflow.
    if extent != usize::MAX && head + len * stride != extent {
      return Err(Error::Validity);
    }
    // With its trailing padding, if any.
    let size = match layout.size_of_val(len) {
      Some(size) if size <= n => size,
      _ => return Err(Error::Size),
    };
    let target = ptr::from_raw_parts::<D>(addr, len);
    // The targets of eager aims are looked for in the `n` bytes.
    let mut input = Input::new(addr, n);
    // SAFETY: This satisfies the safety contract of `validate`. `target` is at `addr`, and its
    // size, `size`, is at most `n`, which is checked above. So it is valid for reads of a `D`, by
    // contract on the caller. Every one of the `n` bytes is initialized, by contract on the
    // caller, so every byte that a check reads is.
    if !unsafe { validate::<C, Root, D>(target, &mut input) } {
      return Err(if input.limit { Error::Limit } else { Error::Validity });
    }
    Ok((target, size))
  }

  /// Returns the `D` at the end of the `n` bytes at `addr`, and where it starts, after every check
  /// that could not be made at compile time (`from_tail`, `from_tail_mut`).
  ///
  /// # Safety
  ///
  /// The caller must ensure that:
  ///
  /// - `addr` is valid for reads of `n` initialized bytes.
  /// - `D` is `Sized`.
  #[inline(always)]
  const unsafe fn back(addr: *const u8, n: usize) -> Result<(usize, *const D), Error> {
    let layout = const { D::LAYOUT };
    // The targets of eager aims are looked for in the `n` bytes, before the `D` too.
    let mut input = Input::new(addr, n);
    let Some(start) = n.checked_sub(layout.size()) else { return Err(Error::Size) };
    // SAFETY: `start <= n`, and `addr` is valid for reads of `n` bytes, by contract on the
    // caller. So `addr..addr + start` is in bounds of one allocation, as `add` requires.
    let addr = unsafe { addr.add(start) };
    if const { D::LAYOUT.align() > 1 } && !ptr::is_aligned(addr, layout.align()) {
      return Err(Error::Alignment);
    }
    let target = ptr::from_raw_parts::<D>(addr, 0);
    // SAFETY: This satisfies the safety contract of `validate`. `D` is `Sized`, by contract on the
    // caller, and `target` is the `size_of::<D>()` bytes from `start`, which end at `n`. So it is
    // valid for reads of a `D`, and its bytes are initialized, by contract on the caller.
    if !unsafe { validate::<Self, Root, D>(target, &mut input) } {
      return Err(if input.limit { Error::Limit } else { Error::Validity });
    }
    Ok((start, target))
  }
}

/// Reinterprets the start of `bytes` as a `D`, and returns it with the bytes that follow it.
///
/// A sized `D` takes `size_of::<D>()` bytes. A `D` that ends with a slice takes the size that its
/// `size` or `len` rule gives (see [`macro@Pun`]), or else as many elements as fit, trailing
/// padding included. `from_head` checks the size, the address if `D` needs an alignment greater
/// than 1, and the values of `D` that bytes do not guarantee.
///
/// In a constant, a call compiles only if `D` has an alignment of 1, or if `bytes` is too short to
/// hold the sized part of `D`, which is checked first. Const evaluation cannot read the address of
/// the bytes to check it.
///
/// # Errors
///
/// Returns [`Error::Size`] if `bytes` is shorter than what the `D` takes, [`Error::Alignment`] if
/// `bytes` is not aligned for `D`, and [`Error::Validity`] if the `size` or `len` rule of `D`
/// cannot be computed or gives a size that `D` cannot have, or if the bytes taken are not a
/// [valid](crate#soundness) `D`.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - `D` is [inhabited].
/// - Without the `unstable` feature, whether the call compiles, what it checks and which values a
///   check accepts do not depend on a layout that Rust leaves to the compiler.
///
/// These conditions are checked after monomorphization, as
/// [the crate documentation](crate#compile-time-errors) explains.
///
/// # Examples
///
/// ```
/// use puns::Pun;
/// use puns::num::u16be;
///
/// #[derive(Pun)]
/// #[repr(C)]
/// #[pun(size = length)]
/// struct Chunk {
///   kind: u8,
///   length: u16be,
///   data: [u8],
/// }
///
/// let (chunk, rest) = puns::from_head::<Chunk>(&[7, 0, 5, b'h', b'i', 1, 2, 3]).unwrap();
/// assert_eq!((chunk.kind, &chunk.data, rest), (7, &b"hi"[..], &[1, 2, 3][..]));
/// ```
///
/// [inhabited]: https://doc.rust-lang.org/reference/glossary.html#inhabited
#[inline]
pub const fn from_head<D: ?Sized + Pun>(bytes: &[u8]) -> Result<(&D, &[u8]), Error> {
  const { Plan::<[u8], D>::TRY_REF };
  // SAFETY: `bytes` is valid for reads of `bytes.len()` bytes, which are initialized, since a
  // `u8` is. This is what `Plan::front` requires.
  match unsafe { Plan::<[u8], D>::front(bytes.as_ptr(), bytes.len()) } {
    // SAFETY: `Plan::front` returned `Ok`, so `target` is a `D` at the address of `bytes`, whose
    // size is `size`, at most `bytes.len()`. The reference is valid, as in `pun_ref`:
    // - It is not null, because `bytes` is a reference at the same address.
    // - It is aligned. The address was checked if `D` is aligned to more than 1.
    // - It is not dangling, because it is within `bytes`.
    // - It points to a valid `D`. `validate` found every part that the analysis of bytes as a
    //   `D` marked to be valid. `Plan::<[u8], D>::TRY_REF` compiled, so the relation records no
    //   disjoint part and nothing that Rust leaves to the compiler: every other part accepts any
    //   initialized bytes.
    // The reference has the lifetime of the shared borrow `bytes`, and `D` holds no `UnsafeCell`
    // (`Pun`, section Safety).
    Ok((target, size)) => Ok((unsafe { &*target }, bytes.split_at(size).1)),
    Err(error) => Err(error),
  }
}

/// Where and why [`check`] found that bytes are not a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Failure {
  /// Why: [`Error::Validity`] if the bytes are not a value, or another error of [`from_head`]:
  /// [`Error::Size`], [`Error::Alignment`], or [`Error::Limit`], which says nothing about the bytes,
  /// if the worklist was full and could not grow, or the pointees took more than twice the bytes.
  pub error: Error,
  /// Where, in bytes from the start of the bytes: for [`Error::Validity`], the innermost scalar,
  /// enum or struct whose own check fails (its valid values, its tag, its rules), or else the
  /// pointee that holds it, from the start of what is stored before its value ([`Pointee`]), or
  /// the struct whose eager aim does not lead to a pointee, or, in a compact check, where the bytes
  /// should have ended. 0 for another error.
  pub at: usize,
}

/// Where a [`check`] keeps the pointees that are still to be checked: a slice of [`Pending`]
/// entries, or a vector that grows.
///
/// A check needs at most one entry for each struct that holds an eager aim at a stored place in
/// the bytes, `bytes.len() / 4` for pointers of 4 bytes, and usually as many as wait at once down
/// the deepest path of a depth-first walk.
///
/// # Examples
///
/// ```
/// use puns::{Pending, Worklist};
///
/// /// Entries on the heap, as many as the check needs.
/// struct Grows(Vec<Pending>);
///
/// impl Worklist for Grows {
///   fn items(&mut self) -> &mut [Pending] {
///     &mut self.0
///   }
///
///   fn grow(&mut self) -> bool {
///     self.0.resize(self.0.len().max(8) * 2, Pending::default());
///     true
///   }
/// }
/// ```
pub trait Worklist {
  /// Returns the entries, every one of which the check may write.
  fn items(&mut self) -> &mut [Pending];

  /// Makes room for more entries, keeping the ones of [`Worklist::items`] at the start, and returns
  /// whether it did. A slice cannot.
  fn grow(&mut self) -> bool {
    false
  }
}

impl Worklist for [Pending] {
  fn items(&mut self) -> &mut [Pending] {
    self
  }
}

impl<const N: usize> Worklist for [Pending; N] {
  fn items(&mut self) -> &mut [Pending] {
    self
  }
}

/// Checks that `bytes` start with a `D`, with the pointees of its eager aims, without making a
/// reference to them, and without recursion: the bytes may be at any address, and the pointees
/// nested at any depth.
///
/// The bytes are checked as they will be at an address aligned to `align`, as when they are
/// copied there after the check: the `D`, and each eager pointee, must be aligned by its offset
/// in the bytes, to at most `align`. The pointees that are still to be checked wait in `work`
/// ([`Worklist`]), so the check goes deeper than [`AIM_DEPTH`], which bounds the recursion of
/// [`from_head`]. The `D` is checked first, then its pointees, depth first, each struct before its
/// pointees and each aim of a struct before the next.
///
/// With `compact`, the bytes must also be the compact image of the `D`: each pointee starts at the
/// first offset after the one before it in that order where it is aligned (after the `D` for the
/// first), with what is stored before its value ([`Pointee`]) right before it, and the last one
/// ends where the bytes end. Such bytes are what a writer that copies a value depth first, each
/// pointee at the next aligned place, makes, and no pointee in them is shared.
///
/// When the bytes are not a `D`, the check is made again to find where they stop being one
/// ([`Failure`]), so that the check of a valid `D` takes no more time than [`from_head`].
///
/// # Errors
///
/// Returns a [`Failure`] with [`Error::Size`] if `bytes` is shorter than a `D`,
/// [`Error::Alignment`] if `D` needs an alignment greater than `align`, [`Error::Validity`] if the
/// bytes are not a `D` with its pointees (or not its compact image), and [`Error::Limit`] if
/// `work` was full and could not grow, or the pointees took more than twice the bytes, which says
/// nothing about the bytes.
///
/// # Panics
///
/// Panics if `align` is not a power of two.
///
/// # Compile-time errors
///
/// A call compiles only under the compile-time conditions of [`from_head`].
///
/// # Examples
///
/// ```
/// use core::num::NonZero;
///
/// use puns::{Error, Failure, InPlace, Pending, Pun, Thin};
///
/// /// A pointer to a text: an `i16` from its own address to an `i16` length, then the bytes.
/// #[derive(Pun)]
/// #[repr(transparent)]
/// struct Text {
///   #[pun(pointer(Thin<str, i16> = off as u128))]
///   off: InPlace<NonZero<i16>>,
/// }
///
/// // The pointer, the length, then "hi", at an odd address.
/// let mut image = [0u8; 7];
/// image[1..3].copy_from_slice(&2i16.to_ne_bytes());
/// image[3..5].copy_from_slice(&2i16.to_ne_bytes());
/// image[5..7].copy_from_slice(b"hi");
/// let mut work = [Pending::default(); 1];
/// assert_eq!(puns::check::<Text, _>(&image[1..], 2, true, &mut work), Ok(()));
/// // A text that is not UTF-8, refused where its length is.
/// image[6] = 0xff;
/// let failure = Failure { error: Error::Validity, at: 2 };
/// assert_eq!(puns::check::<Text, _>(&image[1..], 2, true, &mut work), Err(failure));
/// ```
pub fn check<D: Pun, W: Worklist + ?Sized>(
  bytes: &[u8],
  align: usize,
  compact: bool,
  work: &mut W,
) -> Result<(), Failure> {
  const { Plan::<[u8], D>::TRY_REF };
  assert!(align.is_power_of_two(), "the alignment of the bytes is a power of two");
  if bytes.len() < size_of::<D>() {
    return Err(Failure { error: Error::Size, at: 0 });
  }
  if align_of::<D>() > align {
    return Err(Failure { error: Error::Alignment, at: 0 });
  }
  let (addr, n) = (bytes.as_ptr(), bytes.len());
  // SAFETY: `bytes` is valid for reads of its `n` initialized bytes, at least the size of `D`, as
  // `walk` requires.
  match unsafe { validate::drive::<Plan<[u8], D>, D, W>(addr, n, align, compact, work) } {
    Err(Failure { error: Error::Validity, .. }) => {
      // Made again, recording where the check fails.
      // SAFETY: As above.
      match unsafe { validate::drive::<validate::Locate, D, W>(addr, n, align, compact, work) } {
        Err(failure) => Err(failure),
        Ok(()) => Err(Failure { error: Error::Validity, at: 0 }),
      }
    }
    result => result,
  }
}

/// Returns `true` if `bytes` can neither be nor start with a `D`, as the first scalar that `D`
/// restricts tells (an EtherType, a version), in which case [`from_bytes`] and [`from_head`] would
/// check it, and fail.
///
/// A view of data that is one of several types (an enum of references, see `#[derive(Pun)]`) tests
/// this before it decides each of its variants (`view::decide`). The same bytes are then compared
/// with what each variant requires there, which the optimizer merges into one decision, instead of
/// each conversion computing sizes and rules before it reads that scalar.
#[doc(hidden)]
#[inline(always)]
#[must_use]
pub const fn rejects<D: ?Sized + Pun>(bytes: &[u8]) -> bool {
  let Some((offset, spec)) = (const { D::LAYOUT.key() }) else { return false };
  // The key is in the sized part of `D`: if it is not in `bytes`, neither is a `D`.
  if bytes.len() < offset + spec.size {
    return true;
  }
  // SAFETY: `offset + spec.size <= bytes.len()`, which is checked above. So `add(offset)` stays in
  // `bytes`, as `add` requires. The pointer is valid for reads of `spec.size` bytes of `bytes`,
  // which are initialized, as `Spec::matches` requires. It need not be aligned.
  !unsafe { spec.matches(bytes.as_ptr().add(offset)) }
}

/// Reinterprets the end of `bytes` as a `D`, and returns the bytes before it, followed by the `D`.
///
/// The `D` is the last `size_of::<D>()` bytes. `from_tail` checks the size, the address if `D`
/// needs an alignment greater than 1, and the values of `D` that bytes do not guarantee.
///
/// In a constant, a call compiles only if `D` has an alignment of 1, or if `bytes` is shorter than
/// a `D`, which is checked first. Const evaluation cannot read the address of the bytes to check
/// it.
///
/// # Errors
///
/// Returns [`Error::Size`] if `bytes` is shorter than `size_of::<D>()`, [`Error::Alignment`] if
/// the last `size_of::<D>()` bytes are not aligned for `D`, and [`Error::Validity`] if they are
/// not a [valid](crate#soundness) `D`.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - `D` is sized, because where an unsized `D` starts cannot be found from the end of the bytes.
///   Use [`from_head`] or [`from_bytes`].
/// - `D` is [inhabited].
/// - Without the `unstable` feature, whether the call compiles, what it checks and which values a
///   check accepts do not depend on a layout that Rust leaves to the compiler.
///
/// These conditions are checked after monomorphization, as
/// [the crate documentation](crate#compile-time-errors) explains.
///
/// # Examples
///
/// ```
/// use puns::Pun;
/// use puns::num::u16be;
///
/// /// A trailer: the length of what precedes it, and a check.
/// #[derive(Pun)]
/// #[repr(C)]
/// struct Trailer {
///   length: u16be,
///   check: u8,
/// }
///
/// let (body, trailer) = puns::from_tail::<Trailer>(&[b'h', b'i', 0, 2, 0xff]).unwrap();
/// assert_eq!((body, trailer.length.get(), trailer.check), (&b"hi"[..], 2, 0xff));
/// assert_eq!(puns::from_tail::<Trailer>(&[0, 2]).err(), Some(puns::Error::Size));
/// ```
///
/// ```compile_fail,E0080
/// // An unsized target.
/// let _ = puns::from_tail::<[u16]>(&[0; 4]);
/// ```
///
/// [inhabited]: https://doc.rust-lang.org/reference/glossary.html#inhabited
#[inline]
pub const fn from_tail<D: ?Sized + Pun>(bytes: &[u8]) -> Result<(&[u8], &D), Error> {
  const { Plan::<[u8], D>::FROM_TAIL };
  // SAFETY: `bytes` is valid for reads of `bytes.len()` bytes, which are initialized, since a
  // `u8` is. `Plan::<[u8], D>::FROM_TAIL` compiled, and `check_from_tail` refuses an unsized
  // `D`. These are the requirements of `Plan::back`.
  match unsafe { Plan::<[u8], D>::back(bytes.as_ptr(), bytes.len()) } {
    // SAFETY: `Plan::back` returned `Ok`, so `target` is the sized `D` of the last `size_of::<D>()`
    // bytes of `bytes`. The reference is valid, as in `from_head`: it is not null, its address was
    // checked if `D` is aligned to more than 1, it is within `bytes`, and `validate` found every
    // part that the analysis marked to be valid. It has the lifetime of the shared borrow `bytes`.
    Ok((start, target)) => Ok((bytes.split_at(start).0, unsafe { &*target })),
    Err(error) => Err(error),
  }
}

/// Reinterprets the start of mutable `bytes` as a `D`, and returns it with the bytes that follow
/// it, both mutable.
///
/// It takes and checks what [`from_head`] takes and checks. Since any `D` may be written through
/// the result, every byte of a `D` must be initialized, as bytes are.
///
/// In a constant, a call compiles only if `D` has an alignment of 1, or if `bytes` is too short to
/// hold the sized part of `D`, which is checked first. Const evaluation cannot read the address of
/// the bytes to check it.
///
/// # Errors
///
/// Returns [`Error::Size`] if `bytes` is shorter than what the `D` takes, [`Error::Alignment`] if
/// `bytes` is not aligned for `D`, and [`Error::Validity`] if the `size` or `len` rule of `D`
/// cannot be computed or gives a size that `D` cannot have, or if the bytes taken are not a
/// [valid](crate#soundness) `D`.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - `D` is [inhabited].
/// - Every byte of every value of `D` is initialized.
/// - Without the `unstable` feature, whether the call compiles, what it checks and which values a
///   check accepts do not depend on a layout that Rust leaves to the compiler.
///
/// These conditions are checked after monomorphization, as
/// [the crate documentation](crate#compile-time-errors) explains.
///
/// # Examples
///
/// ```
/// let mut bytes = [1u8, 0, 7, 7];
/// let (flags, rest) = puns::from_head_mut::<[bool; 2]>(&mut bytes).unwrap();
/// flags[1] = true;
/// rest[0] = 9;
/// assert_eq!(bytes, [1, 1, 9, 7]);
/// assert!(puns::from_head_mut::<[bool; 2]>(&mut [1, 2, 0]).is_err());
/// ```
///
/// ```compile_fail,E0080
/// // A `(u8, u16)` has a byte of padding.
/// let _ = puns::from_head_mut::<(u8, u16)>(&mut [0; 4]);
/// ```
///
/// [inhabited]: https://doc.rust-lang.org/reference/glossary.html#inhabited
#[inline]
pub const fn from_head_mut<D: ?Sized + Pun>(
  bytes: &mut [u8],
) -> Result<(&mut D, &mut [u8]), Error> {
  const { Plan::<[u8], D>::TRY_MUT };
  // SAFETY: `bytes` is valid for reads of `bytes.len()` bytes, which are initialized, since a
  // `u8` is. This is what `Plan::front` requires.
  match unsafe { Plan::<[u8], D>::front(bytes.as_mut_ptr().cast_const(), bytes.len()) } {
    Ok((target, size)) => {
      let len = ptr::metadata(target);
      let (front, rest) = bytes.split_at_mut(size);
      let target = ptr::from_raw_parts::<D>(front.as_mut_ptr().cast_const(), len).cast_mut();
      // SAFETY: `target` has the address and the tail length of the `D` that `Plan::front`
      // validated, so its size is `size`. It is not null, aligned, within `front`, and points to a
      // valid `D`, as in `from_head`. It is made from `front`, the exclusive borrow of the first
      // `size` bytes, which is not used again, so nothing else reads or writes them while the
      // reference lives. Any `D`, padding included, may be written through it. `Plan::MUTABLE` is
      // `true`, so every value of `D` is valid bytes: `D` has no padding.
      Ok((unsafe { &mut *target }, rest))
    }
    Err(error) => Err(error),
  }
}

/// Reinterprets the end of mutable `bytes` as a `D`, and returns the bytes before it, followed by
/// the `D`, both mutable.
///
/// It takes and checks what [`from_tail`] takes and checks. Since any `D` may be written through
/// the result, every byte of a `D` must be initialized, as bytes are.
///
/// In a constant, a call compiles only if `D` has an alignment of 1, or if `bytes` is shorter than
/// a `D`, which is checked first. Const evaluation cannot read the address of the bytes to check
/// it.
///
/// # Errors
///
/// Returns [`Error::Size`] if `bytes` is shorter than `size_of::<D>()`, [`Error::Alignment`] if
/// the last `size_of::<D>()` bytes are not aligned for `D`, and [`Error::Validity`] if they are
/// not a [valid](crate#soundness) `D`.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - `D` is sized. Use [`from_head_mut`] or [`from_bytes_mut`] for an unsized `D`.
/// - `D` is [inhabited].
/// - Every byte of every value of `D` is initialized.
/// - Without the `unstable` feature, whether the call compiles, what it checks and which values a
///   check accepts do not depend on a layout that Rust leaves to the compiler.
///
/// These conditions are checked after monomorphization, as
/// [the crate documentation](crate#compile-time-errors) explains.
///
/// # Examples
///
/// ```
/// let mut bytes = [7u8, 7, 0, 1];
/// let (rest, flags) = puns::from_tail_mut::<[bool; 2]>(&mut bytes).unwrap();
/// flags[0] = true;
/// rest[0] = 9;
/// assert_eq!(bytes, [9, 7, 1, 1]);
/// assert!(puns::from_tail_mut::<[bool; 2]>(&mut [1, 2]).is_err());
/// ```
///
/// ```compile_fail,E0080
/// // A `(u8, u16)` has a byte of padding.
/// let _ = puns::from_tail_mut::<(u8, u16)>(&mut [0; 4]);
/// ```
///
/// [inhabited]: https://doc.rust-lang.org/reference/glossary.html#inhabited
#[inline]
pub const fn from_tail_mut<D: ?Sized + Pun>(
  bytes: &mut [u8],
) -> Result<(&mut [u8], &mut D), Error> {
  const { Plan::<[u8], D>::FROM_TAIL_MUT };
  // SAFETY: `bytes` is valid for reads of `bytes.len()` bytes, which are initialized, since a
  // `u8` is. `Plan::<[u8], D>::FROM_TAIL_MUT` compiled, so `Plan::<[u8], D>::FROM_TAIL` did too,
  // and `check_from_tail` refuses an unsized `D`. These are the requirements of `Plan::back`.
  match unsafe { Plan::<[u8], D>::back(bytes.as_mut_ptr().cast_const(), bytes.len()) } {
    Ok((start, _)) => {
      let (rest, back) = bytes.split_at_mut(start);
      let target = ptr::from_raw_parts::<D>(back.as_mut_ptr().cast_const(), 0).cast_mut();
      // SAFETY: `target` is the address of the sized `D` that `Plan::back` validated, and `back` is
      // its `size_of::<D>()` bytes. It is not null, aligned, within `back`, and points to a valid
      // `D`, as in `from_tail`. It is made from `back`, the exclusive borrow of these bytes, which
      // is not used again. As in `from_head_mut`, `Plan::MUTABLE` is `true`, so any `D` written
      // through it is valid bytes.
      Ok((rest, unsafe { &mut *target }))
    }
    Err(error) => Err(error),
  }
}

/// Returns the bytes of the region of the tail of `value` that the aim at the cursor `L`, an item
/// of the rules of the struct of `S`, cuts: a region of `#[pun(regions(..))]` without a type (see
/// `#[derive(Pun)]`).
///
/// The type system does not enforce regions. If the fields of `value` give regions that do not fit
/// in its tail, they are truncated. A region whose expression fails takes the rest of the tail.
///
/// # Compile-time errors
///
/// A call compiles only if the rule `L` of the struct of `S` is an aim at the cursor at `[u8]`,
/// every byte of every value of `S`, up to the end of its slice, is initialized and, without the
/// `unstable` feature, this does not depend on a layout that Rust leaves to the compiler. This is
/// checked after monomorphization, as [the crate documentation](crate#compile-time-errors)
/// explains.
#[doc(hidden)]
#[inline(always)]
pub const fn region<S: ?Sized + Pun, L: Rules>(value: &S) -> &[u8] {
  // The bytes of the value can be read: it has no padding.
  const { Plan::<S, [u8]>::BY_REF };
  let head = const {
    assert!(
      matches!(S::LAYOUT.rule(L::RULE), Some(layout::Rule::Aim(aim))
        if matches!(aim.start, layout::Start::Next) && aim.id.equals(&<[u8] as Pun>::ID)),
      "no such region"
    );
    S::LAYOUT.tail_shape().start
  };
  let (_, tail, size) = validate::tail_of::<S>(value);
  let base = core::ptr::from_ref::<S>(value).cast::<u8>();
  // SAFETY: This satisfies the safety contract of `validate::region_at`:
  // - `value` is a reference, so it is valid for reads of its `size` bytes while the borrow lives,
  //   and they are not written during it. `size` is the extent of the value, up to the end of its
  //   slice (`tail_of`). `Plan::<S, [u8]>::BY_REF` compiled, so every byte of `S` up to the end of
  //   its slice is initialized: bytes take no padding.
  // - `head` is where the tail starts, and `tail` its size in bytes (`tail_shape`, `tail_of`).
  //   The tail is the last part of the struct, so `head + tail <= size`. The search starts at
  //   0, which is at most `tail`.
  let (start, n) =
    unsafe { validate::region_at::<S, S::Rules>(base, size, (head, tail), L::RULE, 0) };
  // SAFETY: `start` and `n` are a region within the tail, which ends at most `size` bytes from
  // `base` (`region_at`). So `add(start)` stays within the bytes of `value`. This satisfies the
  // safety contract of `slice::from_raw_parts` [1]: the `n` bytes from there are in one
  // allocation, valid for reads while the borrow of `value` lives, so non-null [2], initialized,
  // and not written during it, as above. A `u8` is aligned at any address, and `n` bytes fit in an
  // `isize`, since they are in an allocation.
  //
  // [1] Per https://doc.rust-lang.org/1.99.0/core/slice/fn.from_raw_parts.html:
  //
  //     Behavior is undefined if any of the following conditions are violated:
  //
  //     - `data` must be non-null, valid for reads for `len * size_of::<T>()` many bytes, and
  //       it must be properly aligned. [...]
  //     - `data` must point to `len` consecutive properly initialized values of type `T`.
  //     - The memory referenced by the returned slice must not be mutated for the duration of
  //       lifetime `'a`, except inside an `UnsafeCell`.
  //     - The total size `len * size_of::<T>()` of the slice must be no larger than
  //       `isize::MAX`, and adding that size to `data` must not "wrap around" the address
  //       space.
  //
  // [2] Per https://doc.rust-lang.org/1.99.0/core/ptr/index.html#safety:
  //
  //     A null pointer is never valid for reads/writes.
  unsafe { core::slice::from_raw_parts(base.add(start), n) }
}

/// Returns the pointee of the lazy aim at the cursor `L`, an item of the rules of the struct of
/// `S`, in `value`, as a `&L::Pointee` if its bytes are one: a region of `#[pun(regions(..))]`
/// with a type (see `#[derive(Pun)]`).
///
/// The bytes of the region are found as [`region`] finds them, and checked as [`from_bytes`]
/// checks bytes, but the pointees of the eager aims of the region are looked for in the bytes of
/// `value`, so that a pointer in a region may point at another region of the same struct.
///
/// # Errors
///
/// As for [`from_bytes`] on the bytes of the region.
///
/// # Compile-time errors
///
/// A call compiles only if the rule `L` of the struct of `S` is an aim at the cursor, under the
/// compile-time conditions of [`region`] for `S` and of [`from_bytes`] for the pointee.
#[doc(hidden)]
#[inline(always)]
pub const fn region_as<S: ?Sized + Pun, L: Rules>(
  value: &S,
) -> Result<&<L::Pointee as Pointee>::Value, Error> {
  const {
    let () = Plan::<S, [u8]>::BY_REF;
    let () = Plan::<[u8], <L::Pointee as Pointee>::Value>::TRY_REF;
  };
  let head = const {
    assert!(
      matches!(S::LAYOUT.rule(L::RULE), Some(layout::Rule::Aim(aim))
        if matches!(aim.start, layout::Start::Next) && is_pointee::<L::Pointee>(&aim)),
      "no such region"
    );
    S::LAYOUT.tail_shape().start
  };
  let (_, tail, size) = validate::tail_of::<S>(value);
  let base = core::ptr::from_ref::<S>(value).cast::<u8>();
  // SAFETY: As in `region`.
  let (start, n) =
    unsafe { validate::region_at::<S, S::Rules>(base, size, (head, tail), L::RULE, 0) };
  // SAFETY: As in `region`, the `n` bytes at `start` are within the bytes of `value`.
  let bytes = core::ptr::slice_from_raw_parts(unsafe { base.add(start) }, n);
  let mut input = Input::new(base, size);
  // SAFETY: `bytes` is valid for reads of `n` initialized bytes, which are a valid `[u8]`. So is
  // the `[u8]` of the `size` bytes of `value`, which hold them, which `input` spans. These are the
  // requirements of `try_cast_in`, with the marks of the analysis of bytes as the pointee.
  match unsafe { Plan::<[u8], <L::Pointee as Pointee>::Value>::try_cast_whole(bytes, &mut input) } {
    // SAFETY: `try_cast_whole` returned `Ok`, so the pointee is a valid value at `bytes`, aligned,
    // within the bytes of `value`, which are borrowed for the lifetime of the result, as in
    // `from_bytes`.
    Ok(pointee) => Ok(unsafe { &*pointee }),
    Err(error) => Err(error),
  }
}

/// Returns whether `aim` is an aim at a `P`: a pointee of its size and alignment, and of its
/// identity if the aim knows it.
const fn is_pointee<P: ?Sized + Pointee>(aim: &layout::Aim) -> bool {
  aim.size == P::POINTEE_SIZE
    && aim.align == P::POINTEE_ALIGN
    && (!aim.id.known() || aim.id.equals(&P::POINTEE_ID))
}

/// Returns a pointer to the pointee of the eager aim `L`, an item of the rules of the struct of
/// `S`, for the value of `S` at `holder`, with the length of its tail if it is unsized ([`aim`]).
///
/// # Safety
///
/// The caller must ensure the requirements of [`aim`] but the ones on `'a`.
///
/// # Compile-time errors
///
/// As for [`aim`].
#[inline(always)]
const unsafe fn pointee<S: ?Sized + Pun, L: Rules>(
  holder: *const S,
  end: *const u8,
) -> *const <L::Pointee as Pointee>::Value {
  let (head, start, extent) = const {
    let Some(layout::Rule::Aim(aim)) = S::LAYOUT.rule(L::RULE) else {
      panic!("the rule is not an eager aim at this type")
    };
    assert!(
      matches!(aim.check, layout::Check::Eager) && is_pointee::<L::Pointee>(&aim),
      "the rule is not an eager aim at this type"
    );
    validate::pointee_checked::<S, L>();
    (S::LAYOUT.tail_shape().start, aim.start, aim.extent)
  };
  let (_, tail, size) = validate::tail_of::<S>(holder);
  let base = holder.cast::<u8>();
  let (value, wanted) = match start {
    layout::Start::Next => {
      // SAFETY: `holder` points to a value of `S`, valid for reads, by contract on the caller, so
      // its `size` bytes, the extent of the value (`tail_of`), are valid for reads, and the bytes
      // that its rules read, and of its tail, are initialized. `head` and `tail` are its tail,
      // which is its last part, so `head + tail <= size`. The search starts at 0. These are the
      // requirements of `region_at`.
      let (start, n) =
        unsafe { validate::region_at::<S, S::Rules>(base, size, (head, tail), L::RULE, 0) };
      // SAFETY: `start + n <= size`: the pointee is within the bytes of the value.
      (unsafe { base.add(start) }, n)
    }
    layout::Start::Stored(_) => {
      // SAFETY: As above, the requirements of `pointer_at` hold.
      let Some(at) = (unsafe { validate::pointer_at::<S, L>(base, size) }) else {
        // SAFETY: The expression of the aim succeeds over the bytes of the value, by contract on
        // the caller.
        unsafe { core::hint::unreachable_unchecked() }
      };
      // The value starts after what the aim points at, the length of a `Thin`.
      let value = base.wrapping_offset(at as isize).wrapping_add(<L::Pointee as Pointee>::VALUE_AT);
      (value, ptr::address(end).wrapping_sub(ptr::address(value)))
    }
  };
  let len = match extent {
    // SAFETY: A valid pointee, aligned, is at `value`, which stores its length, before it or in
    // its sized fields, by contract on the caller, and its type stores it, which
    // `pointee_checked` asserts of the pointee of an aim of its own size. These are the
    // requirements of `validate::own_length`.
    layout::Extent::Own => Some(unsafe { validate::own_length::<L::Pointee>(value) }),
    layout::Extent::Given(_) => {
      // SAFETY: As above, the requirements of `eval` hold for the size of the pointee.
      let given = unsafe { validate::given::<S, L>(base, size) };
      match given {
        Some(n) if n <= usize::MAX as u128 => {
          validate::sized_as::<<L::Pointee as Pointee>::Value>(n as usize)
        }
        _ => None,
      }
    }
    layout::Extent::Rest => validate::sized_as::<<L::Pointee as Pointee>::Value>(wanted),
  };
  let Some(len) = len else {
    // SAFETY: The pointee has the size that the aim gives, one that its type has, by contract on
    // the caller.
    unsafe { core::hint::unreachable_unchecked() }
  };
  ptr::from_raw_parts::<<L::Pointee as Pointee>::Value>(value, len)
}

/// Returns the pointee of the eager aim `L`, an item of the rules of the struct of `S`, for the
/// value of `S` at `holder`, as a `&L::Pointee`.
///
/// This is the read of the pointee of an eager aim, which the methods that the derive writes for
/// a named aim call ([`macro@Pun`]). The pointee is where the check of the struct found it: at the
/// address of `holder`, plus the value of the expression of the aim, an `i128` in two's
/// complement, or at the cursor of the tail of the struct. Its size is that of its type, with the
/// length that it stores in its sized fields if it is unsized, the size that the aim gives, or,
/// for one at the cursor, the rest of the tail. A pointee at a stored place that takes the rest of
/// the bytes ends at `end`, which is ignored otherwise.
///
/// It cannot be safe: any crate can declare an eager aim through the derive, which is safe, so the
/// type of a value does not tell that its pointee was checked. A reference to a value with an
/// eager aim that a function of `puns` returned does, and so does one to a value within it.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `holder` points to a value of `S`, valid for reads, whose bytes that its rules read, and the
///   bytes of its tail, are initialized, and over which the expressions of the aim succeed.
/// - Where the aim says, there is a value of `L::Pointee`, aligned, that is valid as Rust requires
///   of the referent of a `&L::Pointee`, of the size that the aim gives, and, if it is of its own
///   size and unsized, whose `size` or `len` rule, evaluated over its sized fields, gives the
///   length of its tail. A pointee at a stored place that takes the rest of the bytes ends at
///   `end`.
/// - `holder` has the provenance of the bytes of that pointee. One made from a reference to the
///   value alone has it under the tree borrows model of Miri, but not under stacked borrows, where a
///   reference gives access to the bytes of its referent only: there, `holder` is made from a
///   pointer to bytes that hold both, such as the bytes that `puns` checked.
/// - The bytes of the value and of the pointee are not written for `'a`, except inside an
///   `UnsafeCell`.
///
/// A value that a function of `puns` checked with its eager aims meets the first two, such as one
/// that a reference that `puns` returned points to, or a value within one. Unsafe code that writes
/// a value with an eager aim, as `nearest` writes its regions, keeps them for each value that it
/// reads this way. Rules of the pointee that do not give its length, such as an `assert`, are not
/// needed: the read relies on none.
///
/// # Compile-time errors
///
/// A call compiles only if the rule `L` of the struct of `S` is an eager aim at an `L::Pointee`,
/// of its size and alignment, and of its identity if the aim knows it, and a pointee of its own
/// size is sized or stores the length of its tail in its sized fields: a `size` or `len` rule
/// that reads no byte by offset, as the check of the aim requires.
///
/// # Examples
///
/// ```
/// use puns::{InPlace, Pun};
///
/// #[derive(Pun)]
/// #[repr(transparent)]
/// struct Next {
///   #[pun(pointer(next: u8 = off as u128))]
///   off: InPlace<u8>,
/// }
///
/// let bytes = [1, 9];
/// let next = puns::from_head::<Next>(&bytes).unwrap().0;
/// // SAFETY: `puns` checked `next` with its pointee, and the pointer has the provenance of the
/// // bytes that hold both, which are not written while it is used.
/// let pointee = unsafe { Next::next(bytes.as_ptr().with_addr(core::ptr::from_ref(next).addr()).cast()) };
/// assert_eq!(*pointee, 9);
/// ```
#[doc(hidden)]
#[inline(always)]
#[must_use]
pub const unsafe fn aim<'a, S: ?Sized + Pun, L: Rules>(
  holder: *const S,
  end: *const u8,
) -> &'a <L::Pointee as Pointee>::Value {
  // SAFETY: The requirements of `pointee` are those of this call. The pointee at the address that
  // it returns, with its length, is valid and aligned, and is not written for `'a`, by contract on
  // the caller, so it can be borrowed for `'a`.
  unsafe { &*pointee::<S, L>(holder, end) }
}

/// Returns the pointee of the eager aim `L`, an item of the rules of the struct of `S`, for the
/// value of `S` at `holder`, as a `&mut L::Pointee`: what [`aim`] returns, to write.
///
/// # Safety
///
/// The caller must ensure the requirements of [`aim`] but the last, and that for `'a`, the bytes of
/// the pointee are accessed only through the result, and the bytes of the value are not written.
///
/// A reference that `puns` returned is shared, so the bytes of a value that `puns` checked are not
/// for this function while it lives: it is for the bytes that unsafe code owns, as `nearest` owns
/// a region. What is written through the result is what later reads of the pointee rely on.
///
/// # Compile-time errors
///
/// As for [`aim`].
#[doc(hidden)]
#[inline(always)]
#[must_use]
pub const unsafe fn aim_mut<'a, S: ?Sized + Pun, L: Rules>(
  holder: *mut S,
  end: *const u8,
) -> &'a mut <L::Pointee as Pointee>::Value {
  // SAFETY: The requirements of `pointee` are those of this call. The pointee at the address that
  // it returns, with its length, is valid and aligned, and accessed only through the result for
  // `'a`, by contract on the caller, so it can be borrowed mutably for `'a`. The pointer is made
  // from `holder`, a `*mut S`, so it may write.
  unsafe { &mut *pointee::<S, L>(holder.cast_const(), end).cast_mut() }
}

/// Returns the pointee of the lazy aim at a stored place `L`, an item of the rules of the struct of
/// `S`, for `value`, which is in `bytes`, as a `&L::Pointee` if the bytes where it is are one.
///
/// The pointee is where the check of the struct finds it: at the address of `value` plus the value
/// of the expression of the aim, an `i128` in two's complement, within `bytes`. Its size is that
/// of its type, with the length that it stores if it is unsized, the size that the aim gives, or
/// the rest of `bytes`. It is checked as [`from_bytes`] checks bytes, and the pointees of its own
/// eager aims are looked for in `bytes`. This is the read that the derive writes for a lazy aim at
/// a stored place, which takes the bytes, since a reference to the value covers only its own.
///
/// # Errors
///
/// Returns [`Error::Validity`] if `value` is not within `bytes`, or if the pointee does not start
/// within them, or its expression fails, [`Error::Size`] if its size does not fit in them, or is
/// not one that its type has, and the errors of [`from_bytes`] on the bytes of the pointee.
///
/// # Compile-time errors
///
/// A call compiles only if the rule `L` of the struct of `S` is a lazy aim at a stored place at an
/// `L::Pointee`, under the compile-time conditions of [`from_bytes`] for the pointee.
#[doc(hidden)]
#[inline]
pub fn aim_in<'a, S: ?Sized + Pun, L: Rules>(
  value: &S,
  bytes: &'a [u8],
) -> Result<&'a <L::Pointee as Pointee>::Value, Error> {
  const {
    let () = Plan::<[u8], <L::Pointee as Pointee>::Value>::TRY_REF;
    let () = Plan::<[u8], <L::Pointee as Pointee>::Prefix>::TRY_REF;
  };
  let extent = const {
    match S::LAYOUT.rule(L::RULE) {
      Some(layout::Rule::Aim(aim))
        if matches!((aim.start, aim.check), (layout::Start::Stored(_), layout::Check::Lazy))
          && is_pointee::<L::Pointee>(&aim) =>
      {
        aim.extent
      }
      _ => panic!("the rule is not a lazy aim at a stored place at this type"),
    }
  };
  let before = size_of::<<L::Pointee as Pointee>::Prefix>();
  let (_, _, size) = validate::tail_of::<S>(value);
  let base = core::ptr::from_ref::<S>(value).cast::<u8>();
  let from = match base.addr().checked_sub(bytes.as_ptr().addr()) {
    Some(from) if from <= bytes.len() && size <= bytes.len() - from => from,
    _ => return Err(Error::Validity),
  };
  // SAFETY: `value` is a reference, valid for reads of its `size` bytes, whose bytes that its rules
  // read are initialized, since it is a valid `S`, as `pointer_at` requires.
  let Some(at) = (unsafe { validate::pointer_at::<S, L>(base, size) }) else {
    return Err(Error::Validity);
  };
  // Where the value starts, after the length of a `Thin`, and what is stored before it.
  let to = match (from as i128).checked_add(at + <L::Pointee as Pointee>::VALUE_AT as i128) {
    Some(to) if to >= before as i128 && to <= bytes.len() as i128 => to.cast_unsigned() as usize,
    _ => return Err(Error::Validity),
  };
  let mut input = Input::new(bytes.as_ptr(), bytes.len());
  if before > 0 {
    // SAFETY: `before <= to <= bytes.len()`.
    let prefix =
      core::ptr::slice_from_raw_parts(unsafe { bytes.as_ptr().add(to - before) }, before);
    // SAFETY: `prefix` is valid for reads of `before` initialized bytes of `bytes`, which `input`
    // spans, as `try_cast_whole` requires.
    let checked =
      unsafe { Plan::<[u8], <L::Pointee as Pointee>::Prefix>::try_cast_whole(prefix, &mut input) };
    checked?;
  }
  let room = bytes.len() - to;
  let wanted = match extent {
    layout::Extent::Own => validate::Size::Own,
    layout::Extent::Given(_) => {
      // SAFETY: As for `pointer_at`, the requirements of `given` hold.
      match unsafe { validate::given::<S, L>(base, size) } {
        Some(n) => validate::Size::Bytes(n),
        None => return Err(Error::Validity),
      }
    }
    layout::Extent::Rest => validate::Size::Rest,
  };
  // SAFETY: The `room` bytes from `to` are the end of `bytes`, valid for reads and initialized, and
  // the prefix of the pointee is before them, in `bytes`, as `measure` requires.
  let Some((_, n)) =
    (unsafe { validate::measure::<L::Pointee>(wanted, bytes.as_ptr().add(to), room) })
  else {
    return Err(Error::Size);
  };
  // SAFETY: `to + n <= bytes.len()`.
  let pointee = core::ptr::slice_from_raw_parts(unsafe { bytes.as_ptr().add(to) }, n);
  // SAFETY: `pointee` is valid for reads of `n` initialized bytes of `bytes`, which `input` spans,
  // as `try_cast_whole` requires.
  match unsafe { Plan::<[u8], <L::Pointee as Pointee>::Value>::try_cast_whole(pointee, &mut input) }
  {
    // SAFETY: As in `from_bytes`: a valid pointee, aligned, within `bytes`, borrowed for `'a`.
    Ok(pointee) => Ok(unsafe { &*pointee }),
    Err(error) => Err(error),
  }
}
