// The README is the crate page. Its links to the chapters of the reference name their files on
// GitHub, and rustdoc keeps the first definition of a link, so these, before it, make them
// intra-doc.
//! [valid]: reference::how_puns_decides#what-is-a-valid-value
//! [kinds]: reference::how_puns_decides#proven-checked-and-refused
//! [pairs]: reference::how_puns_decides#why-the-pair-decides
//! [requirements]: reference::how_puns_decides#requirements
//! [layouts]: reference::how_puns_decides#layouts-and-rules
//! [msrv]: reference::rust_version_and_targets#minimum-rust-version
//! [rules]: reference::rules
//! [aims]: reference::aims
//! [performance]: reference::performance
//! [comparison]: reference::performance#compared-with-zerocopy-and-bytemuck
//! [rust]: reference::what_puns_relies_on
//! [reference]: mod@reference
//! [concepts]: reference::how_puns_decides
//! [functions]: reference::api
//! [formats]: reference::in_practice
//! [limits]: reference::limits
//! [trust]: reference::trust
//! [target]: reference::rust_version_and_targets
//! [not-yet]: reference::limits#not-yet
//! [Compile-time errors]: reference::errors
//! [Soundness]: reference::soundness
//! [testing]: reference::testing
//! [deriving]: reference::soundness#what-deriving-pun-means
//! [unstable]: reference::layouts
#![doc = include_str!("../README.md")]
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
pub mod reference;
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
/// Implements [`Pun`](trait@Pun) for a struct, an enum or a union, or the reading functions of a
/// view, from its declaration and its `#[pun(..)]` attributes.
///
/// The derive reads the layout of the type from the compiler: its size, its alignment, the offset
/// of each field, and the discriminants of an enum. It takes the [valid] values of the type from
/// the `Pun` implementations of its fields and from its attributes. [In
/// practice](crate::reference::in_practice) teaches the attributes from examples, and [The language
/// of rules](crate::reference::rules) specifies the expressions that they take. [What the derive
/// trusts](crate::reference::trust) says how the derive finds the layout of a type, so that it
/// depends on no path that the deriving crate can redefine, and [How a part with an unspecified
/// layout is proven](crate::reference::layouts#how-a-part-with-an-unspecified-layout-is-proven)
/// what a pun of a type of the `Rust` representation relies on.
///
/// The derive compiles, and its implementation of `Pun` evaluates, only if the type meets the
/// conditions of its section below, of its attributes, and of [Compile-time
/// errors](#compile-time-errors), which apply to every type and say where each error is reported.
///
/// [valid]: crate::reference::how_puns_decides#what-is-a-valid-value
///
/// # Structs
///
/// A struct may have any representation, and may have [rules](crate::reference::rules), which its
/// attributes state. A struct of the `Rust` representation has an [unspecified
/// layout](crate::reference::how_puns_decides#unspecified-layouts): the compiler chooses the order
/// of its fields.
///
/// The derive compiles a struct only if it meets these conditions:
///
/// - A struct whose last field is unsized must be `repr(C)` or `repr(transparent)`. These say where
///   that field is, while the derive finds the fields of any other struct on a value, which an
///   unsized struct cannot be.
/// - A `packed` struct must not end with an unsized field whose alignment is greater than the
///   packing. Such a field ends with padding of its own, while `puns` takes the size of an unsized
///   value to be the end of its trailing slice, rounded up to the alignment of the value. The build
///   fails with "unsupported layout".
///
/// # Enums without fields
///
/// An enum without fields is a [`Scalar`], whose valid values are its discriminants as the compiler
/// stores them, whatever its representation. With the `Rust` representation, the compiler chooses
/// how they are stored, so the enum has an [unspecified
/// layout](crate::reference::how_puns_decides#unspecified-layouts). Without the `unstable` feature,
/// `puns` assumes that it is the discriminant, in the least integer that holds them all, and checks
/// it when compiling ([What `puns` assumes](crate::reference::layouts#what-puns-assumes)).
///
/// The derive compiles an enum without fields only if it meets these conditions:
///
/// - The enum must be at most 16 bytes, since `puns` reads a scalar as an unsigned integer of at
///   most 128 bits.
/// - The enum must have no padding, which an `align` greater than the size of its discriminant
///   adds. The build fails with an error of const evaluation that reads the padding ("memory is
///   uninitialized").
///
/// An enum without variants is [uninhabited], so no pun makes one.
///
/// [uninhabited]: crate::reference::how_puns_decides#uninhabited-types
///
/// # Enums with fields
///
/// An enum with fields may be `repr(C)`, `repr(u8)` or another integer, both, or of the `Rust`
/// representation. With `repr(C)` or an integer, its layout is the one that [RFC 2195] specifies.
/// With the `Rust` representation, it is the one that `rustc` chose. A value of such an enum is
/// known by its [tag](layout::Enum), so making one from another type checks the tag, then what the
/// fields of its variant require. With the `Rust` representation, the compiler chooses the tag, so
/// the enum has an [unspecified layout](crate::reference::how_puns_decides#unspecified-layouts).
/// Without the `unstable` feature, its layout is the one that `puns` assumes, where the assumption
/// applies, and the experiments below check it ([What `puns`
/// assumes](crate::reference::layouts#what-puns-assumes)). Otherwise the enum proves only the same
/// type, and only the same type proves it.
///
/// A `repr(transparent)` enum is read as one of the `Rust` representation, as [Not
/// decided](crate::reference::limits#not-decided) explains.
///
/// The derive finds where the fields of a variant are, and its tag, on the *sample* of the variant:
/// its least value, which holds the least value of each field. The least value of an enum is that
/// of its first variant that has values. The derive compiles an enum with fields only if it meets
/// these conditions:
///
/// - With the `Rust` representation, `rustc` must give the variant of a value by a tag of 1, 2, 4,
///   8 or 16 bytes before the fields of every variant, or by a
///   [niche](crate::reference::what_puns_relies_on#the-tag-of-an-enum-of-the-rust-representation)
///   in a scalar among the fields of one variant. Any other layout is refused as unsupported.
/// - With a primitive or `C` representation, the derive cannot make the least value of a variant
///   that holds a union none of whose fields has values, and reads its tag from its discriminant
///   alone. If a value of such a variant may exist, its discriminant must be
///   written without a macro or an attribute, or be left out after a variant whose tag the derive
///   reads. Rust gives a variant without a discriminant the tag before it plus one.
///
/// The `dead_code` lint never reports a field of a variant of an enum with fields that derives
/// `Pun` as never read, whatever its representation. The derive binds these fields in a pattern
/// to measure their offsets, which the lint counts as reading them.
///
/// [RFC 2195]: https://rust-lang.github.io/rfcs/2195-really-tagged-unions.html
///
/// # Views
///
/// A *view* is an enum each of whose variants is a tuple variant that holds one shared reference,
/// `&'a T`, such as the `Message` of the examples below. It reads data that may be one of several
/// types. An enum with a variant of another form, such as `A { r: &'a T }`, is not a view. For a
/// view, the derive implements no `Pun`, but two `const fn`s, as visible as the enum:
///
/// - `from_head(bytes: &[u8]) -> Result<(Self, &[u8]), Error>` returns the first variant that
///   `bytes` start with, and the bytes that follow it.
/// - `from_bytes(bytes: &[u8]) -> Result<Self, Error>` returns the first variant that `bytes` are
///   as a whole.
///
/// Both return [`Error::Validity`] if no variant fits, without saying why. A view has no
/// `from_tail`, because its variants may be unsized.
///
/// The derive compiles a view only if it meets these conditions:
///
/// - The variants must all hold a reference with the same named lifetime, `&'a T`.
/// - Where `from_head` or `from_bytes` of the view is used, the type of each variant must meet the
///   compile-time conditions of [`from_bytes`].
///
/// ## What the view functions check
///
/// The functions of a view return what trying the variants in order would return, but check each
/// variant at most once, and only for what is not already known about it. A variant is skipped as
/// soon as the bytes do not match its first constrained scalar, such as an EtherType. A variant is
/// decided after its *base*: the first later variant to which [`pun_ref`] of it is proven, and from
/// which [`try_pun_ref`] to it compiles. It fails if its base fails, and is checked only for what
/// its base does not prove. In the view `AnyFrame` of [`ethernet.rs`], a
/// variant of `Frame<Ipv4<Udp>>`, an Ethernet frame that holds UDP over IPv4, comes before one of
/// `Frame<Ipv4>`. A UDP frame is then checked for its protocol, without the IPv4 checksum that the
/// `Frame<Ipv4>` variant checks, so a wrong IPv4 checksum is computed once. Where two variants
/// share a rule and neither accepts every value of the other, a later variant that accepts every
/// value of both, such as `Frame<Ipv4>` after two kinds of IPv4 packet, computes the rule once, and
/// shares what succeeds.
///
/// [Build time](crate::reference::performance#build-time) measures how the build time of a view
/// grows with its number of variants.
///
/// [`ethernet.rs`]: https://github.com/uael/puns/blob/v0.2.4/puns/examples/ethernet.rs
///
/// # Unions
///
/// [Unions](crate::reference::soundness#unions) says what a union accepts as a target and what it
/// proves as a source. The derive compiles a union only if it meets these conditions:
///
/// - The union must be `repr(C)`, since the fields of a union of another representation need not
///   be at offset 0 ([`r-type.union.layout`]). It may be `packed` or `packed(N)`, which lowers its
///   alignment and moves no field.
/// - No field may hold an eager [aim](#aims): which of its fields the bytes of a union are would
///   depend on pointees that a check checks after it.
///
/// [`r-type.union.layout`]: https://doc.rust-lang.org/1.99.0/reference/types/union.html#r-type.union.layout
///
/// # Generic types
///
/// Where only the same type proves a part ([The same
/// type](crate::reference::layouts#the-same-type)), an instance of a generic type is the same type
/// as another one with the same parameters, if each type parameter is bounded by `Pun` in the
/// bounds of the type, in its `where` clause, in `#[pun(bound(..))]`, or by the bound that the
/// derive adds for a field of that type. Otherwise it is not even the same type as itself.
///
/// # Attributes
///
/// The `#[pun(..)]` attribute takes the options below, each on the items that its entry names.
/// Each option may be given once, except `bound`, `assert`, `tag` and `get`, which add to what they
/// gave before. No option applies to a variant, or to a field of a view.
///
/// ## `size`
///
/// `#[pun(size = EXPR)]`, on a struct: the
/// [extent](crate::reference::how_puns_decides#layouts-and-rules) of a value is `EXPR` bytes, which
/// [`from_head`] reads to take the value from the front of bytes ([a header with
/// options](crate::reference::in_practice#a-header-with-options)).
///
/// ## `assert`
///
/// `#[pun(assert(EXPR))]`, on a struct: `EXPR` is true of a value.
///
/// ## `len`
///
/// `#[pun(len = EXPR)]`, on the last field of a struct: `EXPR` is the number of elements of the
/// field if it is an array, a slice or a `str`, whose elements are its bytes. Otherwise it is the
/// number of elements of the array, slice or `str` that ends the struct of the field, or the
/// struct that ends that struct, and so on. The derive compiles `len` only if the field is or ends
/// with one of these, as a length needs elements ("`len` is about a last field that is or ends
/// with an array, a slice or a `str`").
///
/// ## Aims
///
/// An [aim](crate::reference::aims) is a value at a place of the checked bytes that a struct points
/// at, its pointee. A struct declares its aims through two options, one for each place where a
/// pointee is, and it may have any number of both: a struct with regions may point at other
/// values, and the pointee of a region may hold pointers. Each is a rule of the struct
/// ([`layout::Rule::Aim`]), checked as [Aims](crate::reference::aims) says. The type of a pointee
/// is a [`Pointee`]: a `Pun` type, or a [`Thin<T, M>`](Thin), which stores the length of an unsized
/// `T` before it.
///
/// Both options write an aim the same way: `eager` or `lazy`, then the name and the type of the
/// pointee. An eager aim is checked with the struct, and its field is an [`InPlace<T>`](InPlace),
/// a `T` in the layout and in rules, which may name it. No code makes an `InPlace`, so no safe code
/// makes a value of the struct, and code reads the pointee of a value without a check. A lazy aim
/// is checked when it is read, and its field is of any type. Without `eager` or `lazy`, a pointer
/// is eager and a region is lazy. With a name, a method reads the pointee, as visible as the field:
///
/// - eager: `fn name(&self) -> &Type`, safe, which reads through the reference to the value, which
///   has the provenance of the bytes of the pointee under the tree borrows model of Miri. The
///   struct also has a constant `AIMS`, as visible as the struct and hidden from its
///   documentation, whose field `name`, as visible as the field of the aim, names the aim for
///   [`aim`] and [`aim_mut`], which read the pointee from a pointer, to read or to write
///   ([`AimOf`]).
/// - lazy, at a pointer: `fn name(&self, bytes: &[u8]) -> Result<&Type, Error>`, which finds the
///   value in the `bytes` that hold it, and checks the pointee there as [`from_bytes`] checks
///   bytes.
/// - lazy, at a region: `fn name(&self) -> Result<&Type, Error>`, the region as a `&Type` if its
///   bytes are one, which it checks as [`from_bytes`] checks bytes, with the pointees of its own
///   aims within the bytes of the struct; and, for a region without a type, `fn name(&self) ->
///   &[u8]`, its bytes ([a typed region](crate::reference::in_practice#a-region-with-a-type)).
///
/// The derive bounds the type of a pointee by [`Pointee`] where it depends on a type parameter,
/// so that a `Pair<T>` may point at a `Thin<T, u32>`, which is a `Pointee` exactly when `T` is
/// `Pun`. [`bound`](#bound) gives the bounds instead, this one included.
///
/// The derive compiles an aim only if it meets these conditions:
///
/// - The field of each eager aim, that of an eager `pointer` or of `regions` of which one is eager,
///   is of the type [`InPlace<T>`](InPlace), by any name.
/// - The pointee of an aim whose size is what it stores is sized, a [`Thin`], or stores its length
///   in its sized fields: a `size` or `len` rule that reads no byte by offset. A pointee whose
///   length `len = EXPR` gives is unsized, and a `Thin`, which stores its length, is given neither
///   a length nor a size.
///
/// Where a method of a region is used, it compiles only if it meets these conditions:
///
/// - The struct must have no padding, except after its slice if it ends with one, since the method
///   reads bytes of the value, which padding may leave uninitialized. Without the `unstable`
///   feature, this must not depend on a layout that Rust leaves to the compiler ([layout
///   independence](crate::reference::layouts#layout-independence)).
/// - The type of the region must meet the compile-time conditions of [`from_bytes`] for that type.
///
/// ### `pointer`
///
/// `#[pun(pointer(name: Type = EXPR))]`, on a field: the struct points at a `Type` that is `EXPR`
/// bytes from the start of the struct, where a `Thin` has its length. `EXPR` is a rule whose value
/// is read as an `i128` in two's complement, so that a pointee may be before the struct. Without
/// `= EXPR`, the distance is the integer of the field, as its layout says, so a signed field gives
/// a negative one. `EXPR` may read other fields instead: the field of an eager aim is then the
/// `InPlace` that makes the struct one that safe code does not make. The pointee is as large as
/// what it stores says: the size of a sized type, or of an unsized one with the length that a
/// `Thin` stores, or that its own `size` or `len` rule gives from its sized fields. After it,
/// `len = EXPR` gives the length of an unsized pointee, `size = EXPR` its size in bytes, and
/// `size = ..` makes it end where the tail of the struct ends. The name may be left out
/// (`pointer(Type = EXPR)`), for an aim that no method reads. The derive requires the type of the
/// field to be `Pun`, as it requires the type of a field, and the pointee to be a `Pointee`.
///
/// ### `regions`
///
/// `#[pun(regions(first: Type = EXPR, second: Type, last))]`, on the last field of a struct: the
/// bytes of the field are consecutive
/// [regions](crate::reference::how_puns_decides#layouts-and-rules), from the cursor, the value of
/// each aligned for its type by its offset in the struct, after what it stores before it. A region
/// with `= EXPR` is that many bytes, one without is as large as what its type stores, and the last
/// one without takes whatever remains. A region without a type is bytes, `[u8]`. The field of eager
/// regions is written `InPlace<[u8]>` (or an `InPlace` of an array of bytes).
///
/// Since safe code may change the fields of a value after it is checked, a lazy region that does
/// not fit in the field is truncated, and a region whose expression fails takes the rest of the
/// field. A type parameter that only a region names is unused, which Rust refuses (E0392). A
/// `PhantomData<P>` field, which has no bytes, uses it.
///
/// ## `valid`
///
/// `#[pun(valid(0x45..=0x4f, 0x60))]`, on a field of a struct, of an enum with fields that is not a
/// view, or of a union: the field has one of these values, its bits read as an unsigned integer in
/// its byte order, so `0xff` is `-1i8`. Each item is a value or a range of unsigned integer
/// literals, or of constants of an unsigned integer type. A range may omit either bound: the start
/// is then 0, and the end the greatest unsigned integer of the size of the field. Like a rule,
/// `valid` is a [requirement](crate::reference::how_puns_decides#requirements): safe code can write
/// any value to the field.
///
/// The derive compiles `valid` only if it meets these conditions:
///
/// - No range may be empty, nor go past the greatest unsigned integer of the size of the field
///   ("empty range", "scalar range is empty or out of bounds").
/// - Each value must be valid for the type of the field ("restricted to values that are not valid
///   for the type"). So `..` is refused on a `bool`, whose values are only 0 and 1.
/// - The type of the field must have the layout of a scalar of 1 to 16 bytes that are always
///   initialized, since `puns` reads the value of the field as an unsigned integer of at most 128
///   bits ("a constrained scalar must be 1 to 16 bytes wide"). Integers, floating-point numbers,
///   `bool`, `char`, `NonZero`, enums without fields, the types of [`num`], every other [`Scalar`]
///   of 1 to 16 bytes that are always initialized, and `ManuallyDrop` of one of these have this
///   layout. `()` and `PhantomData`, which have no byte, do not.
/// - Or the type of the field must be a struct that is one integer and requires nothing: its
///   layout is one that Rust specifies, it has no rule and no restricted value, and it is that
///   integer and fields of size 0, the integer at offset 0 with the size of the struct, such as a
///   `#[repr(transparent)]` struct around one ([`Layout::restricted`](layout::Layout::restricted)).
///   It is then restricted as that integer ("only scalars, and structs that Rust lays out as one
///   integer and require nothing, can be restricted").
///
/// ## `get`
///
/// `#[pun(get(name: Type = EXPR, ..))]`, on a struct: generates `const fn name(&self) -> Type`,
/// which returns `(EXPR) as Type`, such as a bit field, and is as visible as the struct. `EXPR` is
/// not a rule but any Rust expression that a `const fn` may evaluate. In it, the name of an
/// unsigned integer field, or `self.name`, is the value of that field as a `u128`, unless a `let`
/// or a closure declares that name. The field is read by reference, or copied in a packed struct.
/// The method panics where its code panics, such as on a division by zero, even on a value that met
/// the rules of the struct when it was checked, since a rule is a
/// [requirement](crate::reference::how_puns_decides#requirements) that a write may break.
///
/// Where the method is used, it compiles only if the fields that `EXPR` names are unsigned
/// integers. Unlike a [rule](crate::reference::rules#fields-in-a-rule), a getter does not read a
/// signed field under a cast.
///
/// ## `tag`
///
/// `#[pun(tag(Key = Type, ..))]`, on a struct: implements [`Tag<Key>`](Tag) with `type Tag =
/// Type`. Where the struct is carried next to a tag `Key`, that tag must be a `Type`.
///
/// ## `bound`
///
/// `#[pun(bound(T: Trait, ..))]`, on a struct, an enum with fields that is not a view, or a union:
/// the bounds of the implementation of `Pun`, besides those of the type.
///
/// Without `bound`, the derive requires each field type that depends on a type parameter to be
/// `Pun`. It leaves out a field type that names the type being derived. Such a bound would make
/// the type `Pun` if its field is, and the field `Pun` if the type is, and the compiler does not
/// resolve that cycle. So a parameter of a recursive type is declared with what its fields need.
///
/// `bound` replaces the bounds that the derive finds for the fields and the pointees of the
/// [aims](#aims), which require each of these types that depends on a type parameter to be `Pun`,
/// or a [`Pointee`] for a pointee. The derive still requires the type of each region with a type to
/// be `Pun`, and a struct that is neither `repr(C)` nor `repr(transparent)` to be `Sized`. The
/// bounds are written as given, so a bound that the type states already is not repeated there.
///
/// Two generic types that hold each other behind pointers have their bounds found in a cycle,
/// which the compiler reports as "overflow evaluating the requirement" (E0275), and so does an aim
/// whose pointee needs a bound of its own. Each of them then needs `bound`, or `#[pun(bound())]`
/// when the bounds that the types declare are enough:
///
/// ```
/// use puns::Pun;
///
/// // Without `bound()`, `Ping<T>` would be `Pun` if its pointee `Pong<T>` is, which needs its own
/// // pointee `Ping<T>`, and so `Ping<T>` again (E0275). `T: Pun` is enough.
/// #[derive(Pun)]
/// #[repr(C)]
/// #[pun(bound())]
/// struct Ping<T: Pun> {
///   value: T,
///   #[pun(pointer(lazy pong: Pong<T>))]
///   pong: i32,
/// }
///
/// #[derive(Pun)]
/// #[repr(C)]
/// #[pun(bound())]
/// struct Pong<T: Pun> {
///   value: T,
///   #[pun(pointer(lazy ping: Ping<T>))]
///   ping: i32,
/// }
///
/// // A ping that points 8 bytes ahead, at a pong that points back at it.
/// let words: [i32; 4] = [7, 8, 9, -8];
/// let bytes = puns::as_bytes(&words);
/// let ping = puns::from_head::<Ping<i32>>(bytes).unwrap().0;
/// let pong = ping.pong(bytes).unwrap();
/// assert_eq!(pong.value, 9);
/// assert_eq!(pong.ping(bytes).unwrap().value, 7);
/// ```
///
/// ## `crate`
///
/// `#[pun(crate = path)]`, on any type: the path of the `puns` crate, `::puns` by default.
/// `#[pun(crate)]` is `#[pun(crate = crate)]`, for the `puns` crate itself. A crate that reaches
/// `puns` through another crate, or under another name, gives that path:
///
/// ```
/// use puns as bytes;
///
/// #[derive(bytes::Pun)]
/// #[repr(C)]
/// #[pun(crate = bytes)]
/// struct Pair {
///   a: u8,
///   b: u8,
/// }
///
/// let pair: &[u8; 2] = bytes::pun_ref(&Pair { a: 1, b: 2 });
/// assert_eq!(pair, &[1, 2]);
/// ```
///
/// # Compile-time errors
///
/// Besides the conditions of its section and of its attributes, the derive compiles a type only if
/// it meets these conditions:
///
/// - No identifier in the item may start with `__Pun`, since these names are reserved.
/// - The type as compiled must have the fields and the variants that the derive read, and each
///   field must have the type that the derive read, as it names in the generated code ([What the
///   derive trusts](crate::reference::trust)).
/// - The type of every field must implement `Pun`, except in a view, where the type `T` of each
///   reference `&'a T` does.
/// - The type of every field of a struct, an enum that is not a view, or a union must be at least
///   as visible as the type (E0446), because the list of fields is an associated type, and the
///   privacy rules of Rust do not let it name a less visible type.
///
/// The derive reports these errors at the place of the fault, so `cargo check` reports them too:
///
/// - an unknown, misplaced or repeated option, or one that does not parse;
/// - an expression that is not a rule, or that exceeds a [limit of the
///   evaluator](crate::reference::rules#limits-of-the-evaluator);
/// - a value of `valid` that is negative, or a literal that is not an unsigned integer;
/// - a `pointer` with a size other than `len = EXPR`, `size = EXPR` or `size = ..`, or with `len`
///   or `regions` on its field;
/// - an identifier that starts with `__Pun`;
/// - a union that is not `repr(C)`, and a struct that ends with a slice or a `str` but is neither
///   `repr(C)` nor `repr(transparent)`;
/// - a view whose references do not have the same lifetime.
///
/// `cargo check` also reports type errors. The other conditions are checked when the layout of the
/// type is evaluated (E0080). For a type without generic parameters, `cargo check` reports a
/// violation. For a generic type, the conditions are checked after monomorphization, so `cargo
/// check` does not report a violation, but `cargo build` does, for each instance that is used. The
/// conditions of a generated method are checked where the method is used, by `cargo build`.
/// [Compile-time errors](crate::reference::errors) says how to read these errors.
///
/// # Examples
///
/// An enum with fields:
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
/// ```
///
/// A view of a message that is a ping, a text, or anything else:
///
/// ```
/// use puns::Pun;
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
/// A union of the `Rust` representation is refused, since its fields need not be at offset 0:
///
/// ```compile_fail
/// use puns::Pun;
///
/// #[derive(Pun, Clone, Copy)]
/// union Word { // Refused.
///   bytes: [u8; 4],
///   word: u32,
/// }
/// ```
///
/// ```text
/// error: `Pun` is derived for unions of `repr(C)`, whose fields are all at offset 0
/// ```
///
/// To fix it, declare the union `repr(C)`:
///
/// ```
/// use puns::Pun;
///
/// #[derive(Pun, Clone, Copy)]
/// #[repr(C)]
/// union Word {
///   bytes: [u8; 4],
///   word: u32,
/// }
///
/// assert!(puns::from_bytes::<Word>(&[1, 2, 3, 4]).is_ok());
/// ```
pub use puns_derive::Pun;
pub use thin::{Metadata, Pointee, Thin};
pub use validate::{AIM_PARTS, Pending};
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
/// Every `Scalar` is a [`Pun`] type. `puns` implements `Scalar` for the integer and floating-point
/// types, `bool`, `char`, `NonZero`, `()`, `PhantomData<T>` for any `T`, `PhantomPinned` and the
/// types of [`num`], and `#[derive(Pun)]` for enums without fields. A type may implement it by hand
/// by defining `SPEC` only: the other items are hidden, since `puns` sets them and may change them
/// in any release.
///
/// # Safety
///
/// `puns` checks at compile time that `SPEC` has the size of `Self`. It makes a `Self` from any
/// bytes that `SPEC` accepts, and takes every `Self` to hold what `SPEC` is known to hold, so
/// behavior is undefined unless:
///
/// - Every value of `Self`, read as an unsigned integer of the size and byte order of `SPEC`, is
///   one that `SPEC` accepts. With [`Spec::maybe_uninit`] or [`Spec::uninit`], it may be any bytes,
///   uninitialized ones included.
/// - Every value that `SPEC` accepts is a [valid] `Self` that meets every invariant that the code
///   of `Self` relies on.
/// - No byte of `Self` is inside an [`UnsafeCell`](core::cell::UnsafeCell).
///
/// Unsafe code may therefore assume that `T::SPEC` describes the values of any `T: Scalar` exactly.
///
/// [`Spec::stored_as`] declares the type that the bytes of `Self` are, of its size, so that `puns`
/// knows the niche that `rustc` gives an `Option` of it, which is that type's. That declaration is
/// not part of this contract: a false one fails to compile, and is never unsound.
///
/// # Examples
///
/// A number whose constructor checks it:
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
/// [valid]: https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity
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
  /// assumes](crate::reference::layouts#what-puns-assumes)). Otherwise, as with the `unstable`
  /// feature, the layout is one that Rust leaves to the compiler (see [`layout::UNSTABLE`]).
  #[doc(hidden)]
  const SPECIFIED: bool = true;
}

/// A type whose layout and valid values `puns` knows at compile time.
///
/// Every function of this crate takes `Pun` types. `puns` implements `Pun` for every [`Scalar`],
/// and for the types of `core` that it supports: arrays, slices, `str`, tuples of up to 12
/// elements, `Option` and `Result` of `Pun` types, `MaybeUninit`, `ManuallyDrop` and `Infallible`.
/// `#[derive(Pun)]` implements it for structs and enums of any representation, uninhabited ones
/// included, for `repr(C)` and `repr(transparent)` structs that end with a slice, and for `repr(C)`
/// unions, packed or not. A tuple, struct or enum of the `Rust` representation has an [unspecified
/// layout](crate::reference::layouts).
///
/// The limit of 12 elements is arbitrary, and could be higher. It is the arity up to which `core`
/// implements its own traits for tuples ([trait implementations of tuples][core tuples]).
///
/// # Safety
///
/// `Pun` must not be implemented by hand. Its items are hidden, because they may change in any
/// release: the layout of `Self` (`Pun::LAYOUT`, a [`Layout`]), lists of its fields, variants,
/// element and rules, as types, for the checks at run time, and a pointer to `Self` that is never
/// dereferenced (`Pun::PROBE`). A layout makes no value of a type, and stands for `Self` only
/// through this implementation, which guarantees that:
///
/// - The layout is that of `Self`: its size, its alignment, the offset of each field, the values
///   that each scalar may hold, and its rules.
/// - Every value of `Self` holds what the layout says every value holds, apart from its rules, and
///   every value that the layout accepts apart from its rules is a valid `Self`. Rules are
///   requirements of `puns`, not of the language: safe code may make a value that breaks them.
/// - The lists are the layout's: its fields if it is a struct, its variants if it is an enum or a
///   union, and its element if it is an array or a slice. `puns` reads no other list, which may
///   then be anything. The list of the rules names the type of the pointee of each aim. The checks
///   compare the rules and the place of each field with the layout, and trust the rest.
/// - `Metadata` is `()` exactly when the layout is sized, and `usize` otherwise.
/// - No byte of `Self` is inside an [`UnsafeCell`](core::cell::UnsafeCell).
/// - If `Self` is unsized, `PROBE` is `slice_from_raw_parts(null(), PROBE_LEN)` cast to
///   `*const Self` with `as` casts only. Nothing reads the `PROBE` of a sized `Self`.
/// - If the layout has an eager [aim](crate::reference::aims), a reference to a value of `Self`
///   that safe code can reach meets the requirements of [`aim`] for each eager aim, the reference
///   itself being the holder: where the aim says, there is a valid pointee, which the reference
///   has the provenance of, and which is not written while it lives. The derive meets this with
///   the [`InPlace`] of the aim, which no safe code makes ("Values made by unsafe code" in its
///   documentation).
///
/// Unsafe code may assume these of any `T: Pun`, as the functions of `puns` do for their soundness.
/// It may rely on nothing else, such as how the derive meets them, which may change.
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
  /// If `Self` is unsized, `slice_from_raw_parts(null(), PROBE_LEN)` cast to `*const Self` with
  /// `as` casts only ([`ptr::PROBE_LEN`]), since the pointer tricks are checked against it.
  /// Nothing reads it otherwise.
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

/// A type carried next to a tag `K`, and the values of that tag that select it.
///
/// A payload is carried next to the field that tells its type, as a variant is next to its
/// discriminant. `K` names that tag, because one type may be carried next to several tags, such as
/// the type fields of two protocols. By convention, `K` is the type of the tag as it is read when
/// its payload is not known, as `Kind` below. Bytes (`[u8]`) can be carried next to any tag `K`
/// that is `Pun + Copy`, and accept every value of it.
///
/// `#[pun(tag(K = T))]` implements `Tag` ([`macro@Pun`]). It is not an `unsafe` trait, because the
/// tag is a field like any other, whose values `puns` checks or proves.
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
  /// The type of the tag next to a `Self`, whose valid values are those that mean `Self`.
  type Tag: Pun + Copy;
}

/// Bytes are carried with any tag.
impl<K: Pun + Copy> Tag<K> for [u8] {
  type Tag = K;
}

/// A `T` that holds where the pointee of an eager [aim](crate::reference::aims) is, the field of a
/// struct with an eager `#[pun(pointer(..))]` or eager regions ([`macro@Pun`]), which no code but
/// `puns` makes.
///
/// `puns` makes one only in the samples from which it finds the layout of an enum of the `Rust`
/// representation that holds one, such as an `Option` of a pointer: values of the enum in
/// `MaybeUninit`s, in constants, which its analysis reads at compile time, and which give no
/// value and no reference to safe code.
///
/// It is `repr(transparent)` over `T`, which a struct reads with [`InPlace::get`], and whose
/// layout and values are those of the field in the layout of the struct. Its field is private, it
/// is not a [`Pun`] type, so no pun makes one, and it is neither `Clone`, `Copy` nor `Default`. So
/// no safe code makes a value of a struct that holds one, nor a reference to one but through
/// `puns`, which gives a reference to such a value only where it checked its pointees, or where
/// the same aim at the same place of a source proves them: the reads of the pointees, which the
/// derive writes, are safe, and check nothing.
///
/// Without it, safe code could make a value whose aim leads nowhere, and `pun_ref` would make it a
/// reference to a type with the same aim, which the same aim proves, and whose read would then
/// follow it.
///
/// # Values made by unsafe code
///
/// Unsafe code that makes a value of a struct that holds an `InPlace`, such as `nearest`, which
/// writes its pointers and their pointees in its regions, or that gives a `&mut` to one, takes the
/// obligation that `puns` meets: while safe code may reach a reference to the value, its eager aims
/// lead to valid pointees, of the sizes that the aims give, which are not written but through
/// such reads, and whose bytes the reference to the value has the provenance of.
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
///   #[pun(pointer(pointee: u16 = off as u128))]
///   off: InPlace<i16>,
/// }
///
/// // The pointee, 7, then a pointer 2 bytes after it, which points back at it.
/// let words = [7, (-2i16).cast_unsigned()];
/// let (_, near) = puns::from_tail::<Near16>(puns::as_bytes(&words)).unwrap();
/// assert_eq!((near.off.get(), *near.pointee()), (-2, 7));
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

impl<T: ?Sized + core::fmt::Debug> core::fmt::Debug for InPlace<T> {
  /// Formats the `T`, which makes no `InPlace`.
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    f.debug_tuple("InPlace").field(&&self.0).finish()
  }
}

/// The `T` of an [`InPlace<T>`](InPlace), which the derive names for the field of an eager aim,
/// whatever the spelling of its type (an alias, a path).
///
/// Sealed: `InPlace<T>` is the one type that implements it. The derive names it through the path
/// of `puns`, which may name another trait, so it does not rely on it alone: `Layout::in_place`
/// ties the field to the `InPlace<T>` of `puns` for the `T` that this trait gives.
#[doc(hidden)]
#[diagnostic::on_unimplemented(
  message = "the field of an eager aim is an `InPlace<T>`, not `{Self}`",
  label = "not an `InPlace<T>`",
  note = "an `InPlace<T>` is a `T` that only `puns` makes, so that no safe code makes a value of a \
          struct whose pointee `puns` did not check"
)]
pub trait InPlaceOf: sealed::InPlace {
  /// The `T`.
  type Inner: ?Sized;
}

impl<T: ?Sized> sealed::InPlace for InPlace<T> {}

impl<T: ?Sized> InPlaceOf for InPlace<T> {
  type Inner = T;
}

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
/// An implementation must ensure that the list, from `Self` through `Next` up to the first item
/// whose `FIELD` is `None`, is the fields of the layout of its type, in the order of their
/// declarations. `()` has the `FIELD` `None`, and ends every list that reaches it. Only
/// `#[derive(Pun)]` and this crate implement this trait.
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

/// An item of the list of rules `L`, by where it is in the list ([`First`], [`After`]).
///
/// The reads of a pointee ([`aim`], [`aim_mut`], and those of regions) name the item of its aim by
/// where it is in the list of the rules of its struct, never by the item itself, so that the
/// pointee that they read is the one of that struct, which its check found, and not a type that
/// the caller names. The derive counts the rules for each aim ([`AimOf`]). Sealed.
#[doc(hidden)]
pub trait Item<L: Rules>: sealed::Sealed {
  /// The item.
  type Of: Rules;
}

/// The first item of a list of rules, which is the first rule of a struct that has an expression
/// or is an aim. Only the derive names it ([`AimOf`]).
#[doc(hidden)]
pub struct First;

/// The item after the one that `K` names, in a list of rules: `After<First>` is the second rule of
/// a struct that has an expression or is an aim, `After<After<First>>` the third, and so on. Only
/// the derive names it ([`AimOf`]).
#[doc(hidden)]
pub struct After<K>(PhantomData<K>);

impl<L: Rules> Item<L> for First {
  type Of = L;
}

impl<L: Rules, K: Item<L::Next>> Item<L> for After<K> {
  type Of = K::Of;
}

/// An eager [aim](crate::reference::aims) of the struct `S`, by its name, for [`aim`] and
/// [`aim_mut`].
///
/// A struct that derives [`Pun`](macro@Pun) with an eager aim that has a name has a constant
/// `AIMS`, as visible as the struct and hidden from its documentation, which holds one `AimOf` for
/// each such aim, a field of that name, as visible as the field of the aim. So `Next::AIMS.next` is
/// the aim `next` of `Next`, whatever the other rules of the struct, and names no aim of another
/// type. `K` is where the aim is among the rules of `S`, which the derive counts.
///
/// # Examples
///
/// ```
/// use puns::{InPlace, Pun};
///
/// #[derive(Pun)]
/// #[repr(C)]
/// #[pun(assert(first < second))]
/// struct Two {
///   #[pun(pointer(left: u8 = 2))]
///   first: InPlace<u8>,
///   #[pun(pointer(right: u8 = 3))]
///   second: InPlace<u8>,
/// }
///
/// let bytes = [0, 1, 7, 9];
/// let two = puns::from_head::<Two>(&bytes).unwrap().0;
/// let holder = bytes.as_ptr().with_addr(core::ptr::from_ref(two).addr()).cast();
/// // SAFETY: `puns` checked `two` with its pointees, and the pointer has the provenance of the
/// // bytes that hold them, which are not written while it is used.
/// let left = unsafe { puns::aim(holder, Two::AIMS.left) };
/// // SAFETY: As above.
/// let right = unsafe { puns::aim(holder, Two::AIMS.right) };
/// assert_eq!((*left, *right), (7, 9));
/// ```
pub struct AimOf<S: ?Sized, K>(PhantomData<fn() -> (*const S, K)>);

impl<S: ?Sized, K> AimOf<S, K> {
  /// The aim, which the derive writes in `AIMS`.
  #[doc(hidden)]
  pub const NEW: Self = Self(PhantomData);
}

impl<S: ?Sized, K> Clone for AimOf<S, K> {
  fn clone(&self) -> Self {
    *self
  }
}

impl<S: ?Sized, K> Copy for AimOf<S, K> {}

impl<S: ?Sized, K> core::fmt::Debug for AimOf<S, K> {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    f.write_str("AimOf")
  }
}

/// The pointee of the item that `K` names in the list of the rules of `S`.
type PointeeOf<S, K> = <<K as Item<<S as Pun>::Rules>>::Of as Rules>::Pointee;

/// The value of the pointee of the item that `K` names in the list of the rules of `S`.
type ValueOf<S, K> = <PointeeOf<S, K> as Pointee>::Value;

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
  impl Sealed for super::First {}
  impl<K> Sealed for super::After<K> {}

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

/// Implemented by this crate only, because what implements [`Path`], [`Rules`], [`Steps`] and
/// [`InPlaceOf`] is trusted.
mod sealed {
  pub trait Sealed {}

  /// The seal of [`InPlaceOf`](super::InPlaceOf), apart, so that the errors about the others do
  /// not list `InPlace`.
  pub trait InPlace {}
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
// apart, with the alignment of `T`: the layout of `[T; N]` [1]. An array is valid if, and only if,
// each of its elements is a valid `T`. The Rust Reference states the "only if" [2]. `puns` relies
// on the "if", which no documentation states (`docs/what_puns_relies_on.md`, "A value whose fields
// are valid"). A valid `T` is what `T::LAYOUT` describes, by the `# Safety` of `Pun` for `T`.
// `Elem` is `T`, and the other lists are `()`. Its bytes are those of its elements, none of which
// is inside an `UnsafeCell`, since `T: Pun`.
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
// slice is valid if, and only if, each of its elements is a valid `T`, as an array is. No
// documentation states the "if", on which `puns` relies (`docs/what_puns_relies_on.md`, "A value
// whose fields are valid"). A valid `T` is what `T::LAYOUT` describes, by the `# Safety` of `Pun`
// for `T`. `Elem` is `T`, and the other lists are `()`. Its bytes are those of its elements, none
// of which is inside an `UnsafeCell`, since `T: Pun`.
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
// requires that they are UTF-8 [3]. So every `str` holds UTF-8, and all UTF-8 bytes are a valid
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
//
// [3] Per https://doc.rust-lang.org/1.99.0/core/primitive.str.html#invariant:
//
//     Rust libraries may assume that string slices are always valid UTF-8.
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
      // SAFETY: `T::LAYOUT.unsized_` is `true`, so `T` is unsized, and `T::PROBE` is
      // `slice_from_raw_parts(null(), PROBE_LEN)` cast to `*const T` with `as` casts only (`Pun`,
      // section Safety). That cast compiles only if a pointer to `T` carries a slice length, so `T`
      // is a slice, `str`, or a struct whose last field is one of these, and so is
      // `ManuallyDrop<T>`, a struct whose only field is a `T`. `Self::PROBE` casts `T::PROBE` again
      // with `as`. These are the requirements of `verify`, which checks that a pointer to
      // `ManuallyDrop<T>` is represented as a pointer to a slice.
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
    // each is within the size. A tuple is valid if, and only if, each of its elements is valid, and
    // every byte that no element covers is padding. The Rust Reference states the "only if" [1].
    // `puns` relies on the "if", which no documentation states (`docs/what_puns_relies_on.md`, "A
    // value whose fields are valid"). So the layout is that of the tuple, by the `# Safety` of
    // `Pun` for each element. `rust` and `identified` change neither the size, the alignment, the
    // offsets nor the values. `Fields` lists the elements in order (see `TupleField`). The other
    // lists are `()`. No byte of an element is in an `UnsafeCell`, since each is `Pun`, and a byte
    // of padding belongs to no element.
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
// implement `Pun` (`docs/soundness.md`, "What deriving `Pun` means").
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
/// The `try_` functions and the `from_` functions return it when a check at run time fails, and so
/// do the methods that read the pointee of a lazy [aim](crate::reference::aims), a [typed
/// region](macro@Pun#regions) among them, and the functions of a [view](macro@Pun#views), which
/// `#[derive(Pun)]` generates. It does not say which field or rule failed. Where several checks
/// would fail, a function returns the error of the first, in the order that its page gives. A view
/// returns `Validity` whenever no variant fits, even for bytes that are too short or not aligned
/// for any variant.
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
#[non_exhaustive]
pub enum Error {
  /// The address of the value is not a multiple of the target's alignment.
  Alignment,
  /// The size of the value is not one that the target can have, or there are fewer bytes than the
  /// target needs.
  Size,
  /// The bytes are not a [valid] target, or do not meet its [requirements]: a scalar holds a value
  /// that the target does not accept, a `str` is not UTF-8, a rule of the target does not hold or
  /// cannot be computed, the pointee of an aim does not fit, or the pointee of an eager aim is not
  /// a valid value within the bytes.
  ///
  /// [valid]: crate::reference::how_puns_decides#what-is-a-valid-value
  /// [requirements]: crate::reference::how_puns_decides#requirements
  Validity,
  /// The check of the pointees of eager aims ([`Check::Eager`](layout::Check::Eager)) stopped at
  /// its limit before it found the bytes valid or not: pointees within one another of more parts
  /// than [`AIM_PARTS`], pointees of more than twice as many bytes as are checked in all, or, for
  /// [`check`], a [`Worklist`] that was full and could not grow.
  Limit,
}

impl core::fmt::Display for Error {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    f.write_str(match self {
      Error::Alignment => "value is not aligned for the target type",
      Error::Size => "value has a size that the target type cannot have, or is too short for it",
      Error::Validity => "value is not valid for the target type",
      Error::Limit => "the pointees of the value are too deep or too large to check",
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
const UNSPECIFIED: &str = "the pun depends on a part of the source or of the target with an \
  unspecified layout (a struct or a tuple of the `Rust` representation, or an enum of it, `Option` \
  and `Result` included, whose layout `puns` does not assume): another compiler, or \
  `-Zrandomize-layout`, could arrange it otherwise. Only the same type at the same place proves \
  it.\nAdd `#[repr(C)]` (or `#[repr(u8)]` to an enum), or enable the `unstable` feature of `puns`";

/// A `&mut` whose writes would have to leave a valid source that has a part with an unspecified
/// layout, where the analysis does not tell what a write leaves.
const UNSPECIFIED_MUT: &str = "what may be written through the `&mut` must leave a valid source, \
  which has a part with an unspecified layout (a struct, a tuple or an enum of the `Rust` \
  representation): the analysis does not tell what a write leaves there.\nAdd `#[repr(C)]` (or \
  `#[repr(u8)]` to an enum), or enable the `unstable` feature of `puns`";

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
        ".\nUse `head` or `tail` (or their `_ref` and `_mut` forms) to take the first or last \
         bytes of the source",
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
    // The checked functions of a whole value do not take the first bytes of a larger source.
    return Some(Message::new(if need.part {
      "the target has a greater alignment than the source.\nUse `head` to copy it, or \
       `from_head` to take it from bytes, whose address is checked at run time"
    } else {
      "the target has a greater alignment than the source.\nUse `try_pun_ref` or \
       `try_pun_mut` to check the address at run time"
    }));
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
  /// not be valid for. [`Plan`] itself is such a `C` if `source` is a valid `S` and
  /// `Plan::<S, D>::TRY_REF` compiles: an analysis that refuses the pun may stop before it marks
  /// every part. Otherwise `C` is the marks of the analysis of a source type as `D` that records no
  /// byte that `D` requires to be initialized and that may be padding, no disjoint part and nothing
  /// that Rust leaves to the compiler, and the bytes are a valid value of that source type, with
  /// the extent of `source`.
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
/// The result has the bytes of `value`, except that, as with [`core::mem::transmute`], the bytes of
/// its padding may differ. Nothing is checked at run time.
///
/// # Compile-time errors
///
/// A call [compiles] only under the conditions [same extent], [initialized bytes], [proven target],
/// [no eager aim] and [layout independence].
///
/// # Examples
///
/// ```
/// assert_eq!(puns::pun::<[bool; 2], u16>([true, true]), 0x0101);
/// ```
///
/// [Reinterpreting a value](crate::reference::api#reinterpreting-a-value) shows a real use:
/// the bits of an `f32`.
///
/// A pun that needs a check is refused, since `pun` checks nothing ([its message and fix][check
/// message]):
///
/// ```compile_fail,E0080
/// // Not every `u8` is a `bool`.
/// let _: bool = puns::pun(2u8);
/// ```
///
/// [compiles]: crate::reference::errors
/// [same extent]: crate::reference::api#sizes
/// [initialized bytes]: crate::reference::api#initialized-bytes
/// [proven target]: crate::reference::api#proven-target
/// [layout independence]: crate::reference::layouts#layout-independence
/// [check message]: crate::reference::errors#values-and-padding
/// [no eager aim]: crate::reference::api#no-eager-aim
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
/// The result has the first `size_of::<D>()` bytes of `value`, which are the covered bytes, except
/// that the bytes of its padding may differ, as with [`pun`]. Nothing is checked at run time.
///
/// # Compile-time errors
///
/// A call [compiles] only under the conditions [room for the target], [initialized bytes], [proven
/// target], [no eager aim] and [layout independence].
///
/// # Examples
///
/// ```
/// assert_eq!(puns::head::<[u8; 4], [u8; 2]>([1, 2, 3, 4]), [1, 2]);
/// ```
///
/// [Taking the front or the back](crate::reference::api#taking-the-front-or-the-back) shows a
/// real use: the low byte of a little-endian number.
///
/// A byte that may be padding in the covered bytes is refused ([its message and fix][padding
/// message]):
///
/// ```compile_fail,E0080
/// // Rust does not specify the order of the fields of a tuple. Without the `unstable` feature, the
/// // pun is refused for that. With it, where `u16` has alignment 2, the second byte is padding,
/// // since `rustc` puts the `u16` after a byte of padding.
/// let _: [u8; 2] = puns::head((0u8, 0u16));
/// ```
///
/// [compiles]: crate::reference::errors
/// [room for the target]: crate::reference::api#sizes
/// [initialized bytes]: crate::reference::api#initialized-bytes
/// [proven target]: crate::reference::api#proven-target
/// [layout independence]: crate::reference::layouts#layout-independence
/// [padding message]: crate::reference::errors#reading-an-error
/// [no eager aim]: crate::reference::api#no-eager-aim
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
/// The result has the last `size_of::<D>()` bytes of `value`, which are the covered bytes and start
/// `size_of::<S>() - size_of::<D>()` bytes after its start, except that the bytes of its padding
/// may differ, as with [`pun`]. Nothing is checked at run time.
///
/// # Compile-time errors
///
/// A call [compiles] only under the conditions [room for the target], [initialized bytes], [proven
/// target], [no eager aim] and [layout independence].
///
/// # Examples
///
/// ```
/// assert_eq!(puns::tail::<[u8; 4], [u8; 2]>([1, 2, 3, 4]), [3, 4]);
/// // Two `bool`s are two initialized bytes.
/// assert_eq!(puns::tail::<[bool; 3], u16>([false, true, true]), 0x0101);
/// ```
///
/// [Taking the front or the back](crate::reference::api#taking-the-front-or-the-back) shows a
/// real use: the low half of a big-endian number.
///
/// A pun that needs a check is refused, since `tail` checks nothing ([its message and fix][check
/// message]):
///
/// ```compile_fail,E0080
/// // Not every `u8` is a `bool`.
/// let _: bool = puns::tail::<[u8; 2], bool>([1, 7]);
/// ```
///
/// So is a byte that may be padding in the covered bytes ([its message and fix][padding message]):
///
/// ```compile_fail,E0080
/// #[derive(puns::Pun, Clone, Copy)]
/// #[repr(C)]
/// struct Padded {
///   a: u16,
///   b: u8,
/// }
///
/// // Where `u16` has alignment 2, the last byte is padding.
/// let _: u8 = puns::tail(Padded { a: 0, b: 0 });
/// ```
///
/// [compiles]: crate::reference::errors
/// [room for the target]: crate::reference::api#sizes
/// [initialized bytes]: crate::reference::api#initialized-bytes
/// [proven target]: crate::reference::api#proven-target
/// [layout independence]: crate::reference::layouts#layout-independence
/// [check message]: crate::reference::errors#values-and-padding
/// [padding message]: crate::reference::errors#reading-an-error
/// [no eager aim]: crate::reference::api#no-eager-aim
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

/// Reinterprets a value of type `S` as a value of type `D`, after checking what [the analysis]
/// could not prove.
///
/// The result has the bytes of `value`, as with [`pun`].
///
/// # Errors
///
/// Returns [`Error::Validity`] if the bytes are not a [valid] `D`, or do not meet a [requirement]
/// of `D` that `S` does not prove ([Why the pair decides]).
///
/// # Compile-time errors
///
/// A call [compiles] only under the conditions [same extent], [initialized bytes], [accepted by a
/// check], [no eager aim] and [layout independence].
///
/// # Examples
///
/// ```
/// assert_eq!(puns::try_pun::<u8, bool>(1), Ok(true));
/// assert_eq!(puns::try_pun::<u8, bool>(2), Err(puns::Error::Validity));
/// ```
///
/// [Reinterpreting a value](crate::reference::api#reinterpreting-a-value) shows a real use: a
/// `u32` that is a `char` only if it is a Unicode scalar value.
///
/// A pair where no value of the source is a valid target is refused, since no check could accept
/// it ([its message and fix][check message]):
///
/// ```compile_fail,E0080
/// // A byte that is always 2 is never a `bool`.
/// let _ = puns::try_pun::<puns::num::u8c<2>, bool>(Default::default());
/// ```
///
/// [valid]: crate::reference::how_puns_decides#what-is-a-valid-value
/// [requirement]: crate::reference::how_puns_decides#requirements
/// [Why the pair decides]: crate::reference::how_puns_decides#why-the-pair-decides
/// [the analysis]: crate::reference::how_puns_decides#layouts-and-rules
/// [compiles]: crate::reference::errors
/// [same extent]: crate::reference::api#sizes
/// [initialized bytes]: crate::reference::api#initialized-bytes
/// [accepted by a check]: crate::reference::api#accepted-by-a-check
/// [layout independence]: crate::reference::layouts#layout-independence
/// [check message]: crate::reference::errors#values-and-padding
/// [no eager aim]: crate::reference::api#no-eager-aim
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
  //     Reading a union field reads the bits of the union at the field’s type.
  unsafe { Overlay { source: value }.target }
}

// -------------------------------------------------------------------------------------------------
// By reference

/// Reinterprets a reference to an `S` as a reference to a `D`.
///
/// The returned reference points to the same bytes as `value`. If `D` ends with a slice, its length
/// is chosen so that the result covers the same bytes as `value`, up to the end of the slice of
/// `value`. Padding after that slice is not included. If the elements of the slice have size 0,
/// every length covers these bytes, and the length is 0. Nothing is checked at run time. Where
/// this function is refused because the pun needs a check of the values of `D`, of the length of a
/// slice or of the address, [`try_pun_ref`] makes that check at run time.
///
/// # Compile-time errors
///
/// A call [compiles] only under the conditions [same extent], [alignment], [initialized bytes],
/// [proven target] and [layout independence].
///
/// # Examples
///
/// ```
/// let bytes: &[u8] = puns::pun_ref(&[0x0102u16, 0x0304][..]);
/// assert_eq!(bytes.len(), 4);
/// let text: &[u8] = puns::pun_ref("héllo");
/// assert_eq!(text.len(), 6);
/// // Any length of a slice of `()` covers the same bytes, and the result has none.
/// let units: &[()] = puns::pun_ref(&[(); 3][..]);
/// assert_eq!(units.len(), 0);
/// ```
///
/// A target more aligned than the source is refused ([its message and fix][alignment message]):
///
/// ```compile_fail,E0080
/// // Bytes may be at any address, a `u16` may not.
/// let _: &u16 = puns::pun_ref(&[0u8; 2]);
/// ```
///
/// [compiles]: crate::reference::errors
/// [same extent]: crate::reference::api#sizes
/// [alignment]: crate::reference::api#alignment
/// [initialized bytes]: crate::reference::api#initialized-bytes
/// [proven target]: crate::reference::api#proven-target
/// [layout independence]: crate::reference::layouts#layout-independence
/// [alignment message]: crate::reference::errors#alignment
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
/// The returned reference points to the same bytes as `value`, as with [`pun_ref`]. A write through
/// it may break a rule of `S`, but not a `valid(..)` range ([Writing through `&mut`]). Nothing is
/// checked at run time. Where this function is refused because the pun needs a check, as for
/// [`pun_ref`], [`try_pun_mut`] makes that check at run time.
///
/// # Compile-time errors
///
/// A call [compiles] only under the conditions [same extent], [alignment], [initialized bytes],
/// [proven target], [writes leave a valid source], [no eager aim] and [layout independence].
///
/// # Examples
///
/// ```
/// let mut value = 0x0102u16;
/// puns::pun_mut::<_, [u8; 2]>(&mut value)[0] = 0xff;
/// // The first byte in memory, whichever half of the number it is on this machine.
/// let mut expected = 0x0102u16.to_ne_bytes();
/// expected[0] = 0xff;
/// assert_eq!(value.to_ne_bytes(), expected);
/// ```
///
/// A `&mut` through which a value that is not a valid source could be written is refused ([its
/// message and fix][write message]):
///
/// ```compile_fail,E0080
/// // A 2 could be written through the result where a `bool` is.
/// let _: &mut u8 = puns::pun_mut(&mut true);
/// ```
///
/// [compiles]: crate::reference::errors
/// [same extent]: crate::reference::api#sizes
/// [alignment]: crate::reference::api#alignment
/// [initialized bytes]: crate::reference::api#initialized-bytes
/// [proven target]: crate::reference::api#proven-target
/// [writes leave a valid source]: crate::reference::api#writes-leave-a-valid-source
/// [layout independence]: crate::reference::layouts#layout-independence
/// [writing through `&mut`]: crate::reference::api#writing-through-mut
/// [write message]: crate::reference::errors#writing-through-mut
/// [no eager aim]: crate::reference::api#no-eager-aim
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
  // included, is accepted by the layout of `S` apart from its rules, so it is a valid `S` (`Pun`,
  // section Safety). So `value` is a valid `S` when the borrow ends.
  unsafe { &mut *Plan::<S, D>::cast(source).cast_mut() }
}

/// Reinterprets a reference to the first bytes of an `S` as a reference to a `D`.
///
/// The returned reference points to the start of `value`. A sized `D` covers the first
/// `size_of::<D>()` bytes. A `D` that ends with a slice takes as many elements as fit in the
/// [extent] of `value`, or none if they have size 0. Nothing is checked at run time.
///
/// # Compile-time errors
///
/// A call [compiles] only under the conditions [room for the target], [alignment], [initialized
/// bytes], [proven target] and [layout independence].
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
/// A source that may be smaller than the target is refused ([its message and fix][sizes message]):
///
/// ```compile_fail,E0080
/// // An empty slice has no first byte.
/// let _: &u8 = puns::head_ref::<[u8], u8>(&[1, 2]);
/// ```
///
/// [extent]: crate::reference::how_puns_decides#layouts-and-rules
/// [compiles]: crate::reference::errors
/// [room for the target]: crate::reference::api#sizes
/// [alignment]: crate::reference::api#alignment
/// [initialized bytes]: crate::reference::api#initialized-bytes
/// [proven target]: crate::reference::api#proven-target
/// [layout independence]: crate::reference::layouts#layout-independence
/// [sizes message]: crate::reference::errors#sizes
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
/// `value`, and covers its last `size_of::<D>()` bytes. Since `D` is no more aligned than `S`, this
/// offset is a multiple of the alignment of `D`. Nothing is checked at run time.
///
/// # Compile-time errors
///
/// A call [compiles] only under the conditions [room for the target], [sized source and target],
/// [alignment], [initialized bytes], [proven target] and [layout independence].
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
/// An unsized source is refused ([its message and fix][last bytes message]):
///
/// ```compile_fail,E0080
/// // An unsized source.
/// let _: &u8 = puns::tail_ref::<[u8], u8>(&[1, 2]);
/// ```
///
/// So is a target more aligned than the source ([its message and fix][alignment message]):
///
/// ```compile_fail,E0080
/// // A greater alignment.
/// let _: &u16 = puns::tail_ref(&[0u8; 4]);
/// ```
///
/// [compiles]: crate::reference::errors
/// [room for the target]: crate::reference::api#sizes
/// [sized source and target]: crate::reference::api#sizes
/// [alignment]: crate::reference::api#alignment
/// [initialized bytes]: crate::reference::api#initialized-bytes
/// [proven target]: crate::reference::api#proven-target
/// [layout independence]: crate::reference::layouts#layout-independence
/// [alignment message]: crate::reference::errors#alignment
/// [last bytes message]: crate::reference::errors#the-last-bytes-of-a-value
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
/// The returned reference covers the bytes that [`head_ref`] covers. Nothing is checked at run
/// time.
///
/// # Compile-time errors
///
/// A call [compiles] only under the conditions [room for the target], [alignment], [initialized
/// bytes], [proven target], [writes leave a valid source], [no eager aim] and [layout
/// independence].
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
/// A `&mut` through which a value that is not a valid source could be written is refused, and so is
/// one that covers an enum only partly ([its message and fix][write message]):
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
///
/// [compiles]: crate::reference::errors
/// [room for the target]: crate::reference::api#sizes
/// [alignment]: crate::reference::api#alignment
/// [initialized bytes]: crate::reference::api#initialized-bytes
/// [proven target]: crate::reference::api#proven-target
/// [writes leave a valid source]: crate::reference::api#writes-leave-a-valid-source
/// [layout independence]: crate::reference::layouts#layout-independence
/// [write message]: crate::reference::errors#writing-through-mut
/// [no eager aim]: crate::reference::api#no-eager-aim
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
  // has over its bytes, apart from its rules, and that nothing of `S` outside them depends on them.
  // A value that the layout of `S` accepts apart from its rules is a valid `S` (`Pun`, section
  // Safety). So `value` is a valid `S` when the borrow ends.
  unsafe { &mut *Plan::<S, D>::cast(source).cast_mut() }
}

/// Reinterprets a mutable reference to the last bytes of an `S` as a mutable reference to a `D`.
///
/// The returned reference covers the bytes that [`tail_ref`] covers. Nothing is checked at run
/// time.
///
/// # Compile-time errors
///
/// A call [compiles] only under the conditions [room for the target], [sized source and target],
/// [alignment], [initialized bytes], [proven target], [writes leave a valid source], [no eager aim]
/// and [layout independence].
///
/// # Examples
///
/// ```
/// let mut words = [0x0102u16, 0x0304, 0x0506];
/// *puns::tail_mut::<_, [u16; 2]>(&mut words) = [7, 8];
/// assert_eq!(words, [0x0102, 7, 8]);
/// ```
///
/// [Writing through `&mut`](crate::reference::api#writing-through-mut) shows a real use: the
/// text of a note, without its flag.
///
/// A `&mut` that overlaps a struct with a rule is refused ([its message and fix][write message]):
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
///
/// [compiles]: crate::reference::errors
/// [room for the target]: crate::reference::api#sizes
/// [sized source and target]: crate::reference::api#sizes
/// [alignment]: crate::reference::api#alignment
/// [initialized bytes]: crate::reference::api#initialized-bytes
/// [proven target]: crate::reference::api#proven-target
/// [writes leave a valid source]: crate::reference::api#writes-leave-a-valid-source
/// [layout independence]: crate::reference::layouts#layout-independence
/// [write message]: crate::reference::errors#writing-through-mut
/// [no eager aim]: crate::reference::api#no-eager-aim
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

/// Reinterprets a reference to an `S` as a reference to a `D`, after checking what [the analysis]
/// could not prove.
///
/// The returned reference points to the same bytes as `value`. At run time, it checks the address,
/// if `D` is more aligned than `S`; the size, if it depends on the length of a slice; and the
/// values of `D` that the bytes of `S` do not prove.
///
/// # Errors
///
/// Returns, in the order of the checks, [`Error::Alignment`] if `value` is not aligned for `D`,
/// [`Error::Size`] if no `D` has the [extent] of `value` or if that `D` is larger than `value`, and
/// [`Error::Validity`] if the bytes are not a [valid] `D`, or do not meet a [requirement] of `D`
/// that `S` does not prove ([Why the pair decides]). The first check that fails gives the error.
/// It returns [`Error::Limit`] if `D` holds eager [aims] whose pointees, one within another, have
/// more than [`AIM_PARTS`] parts of their layouts, or more bytes than twice those checked.
///
/// # Compile-time errors
///
/// A call [compiles] only under the conditions [an extent in common], [no address check in a
/// constant], [initialized bytes], [accepted by a check], [bytes for a pointee] and [layout
/// independence].
///
/// # Examples
///
/// ```
/// assert_eq!(puns::try_pun_ref::<[u8], str>(b"caf\xc3\xa9"), Ok("café"));
/// assert_eq!(puns::try_pun_ref::<[u8], str>(b"caf\xc3"), Err(puns::Error::Validity));
/// ```
///
/// A byte that may be padding in the source is refused, since no check reads it ([its message and
/// fix][padding message]):
///
/// ```compile_fail,E0080
/// // One byte of the source is padding, whatever the order of its fields.
/// let _ = puns::try_pun_ref::<_, [u8; 4]>(&(0u8, 0u16));
/// ```
///
/// [valid]: crate::reference::how_puns_decides#what-is-a-valid-value
/// [requirement]: crate::reference::how_puns_decides#requirements
/// [Why the pair decides]: crate::reference::how_puns_decides#why-the-pair-decides
/// [the analysis]: crate::reference::how_puns_decides#layouts-and-rules
/// [extent]: crate::reference::how_puns_decides#layouts-and-rules
/// [compiles]: crate::reference::errors
/// [an extent in common]: crate::reference::api#sizes
/// [initialized bytes]: crate::reference::api#initialized-bytes
/// [accepted by a check]: crate::reference::api#accepted-by-a-check
/// [no address check in a constant]: crate::reference::api#in-a-constant
/// [layout independence]: crate::reference::layouts#layout-independence
/// [padding message]: crate::reference::errors#reading-an-error
/// [aims]: crate::reference::aims
/// [bytes for a pointee]: crate::reference::api#bytes-for-a-pointee
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
    //   `S`, `Plan::check_by`, which `Plan::try_cast_by` calls, checked that its size is at most
    //   the size of `value`. Otherwise either `D` is sized, and its size is its extent, or `D` is
    //   unsized and its extent is rounded up to an alignment that divides that of `S`. Either way
    //   its size is at most the size of `value`.
    // - It points to a valid `D`. `validate` found every part that the analysis marked to be
    //   valid. Every other part is valid in every value of `S`, since the relation records no
    //   byte that `D` requires to be initialized and that may be padding, no disjoint part, and
    //   nothing that Rust leaves to the compiler.
    // The reference has the lifetime of the shared borrow `value`, and neither type holds an
    // `UnsafeCell` (`Pun`, section Safety).
    Ok(target) => Ok(unsafe { &*target }),
    Err(error) => Err(error),
  }
}

/// Reinterprets a mutable reference to an `S` as a mutable reference to a `D`, after checking what
/// [the analysis] could not prove.
///
/// The returned reference points to the same bytes as `value`, and is checked at run time as
/// [`try_pun_ref`] checks it. As with [`pun_mut`], a write through it may break a rule of `S`, but
/// not a `valid(..)` range.
///
/// # Errors
///
/// Returns the errors of [`try_pun_ref`], in the same order.
///
/// # Compile-time errors
///
/// A call [compiles] only under the conditions [an extent in common], [no address check in a
/// constant], [initialized bytes], [accepted by a check], [writes leave a valid source], [no eager
/// aim] and [layout independence].
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
/// A `&mut` through which a value that is not a valid source could be written is refused ([its
/// message and fix][write message]):
///
/// ```compile_fail,E0080
/// // A `u8` can be written through the result, which a `bool` cannot hold.
/// let _ = puns::try_pun_mut::<bool, u8>(&mut true);
/// ```
///
/// [the analysis]: crate::reference::how_puns_decides#layouts-and-rules
/// [compiles]: crate::reference::errors
/// [an extent in common]: crate::reference::api#sizes
/// [initialized bytes]: crate::reference::api#initialized-bytes
/// [accepted by a check]: crate::reference::api#accepted-by-a-check
/// [writes leave a valid source]: crate::reference::api#writes-leave-a-valid-source
/// [no address check in a constant]: crate::reference::api#in-a-constant
/// [layout independence]: crate::reference::layouts#layout-independence
/// [write message]: crate::reference::errors#writing-through-mut
/// [no eager aim]: crate::reference::api#no-eager-aim
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
/// Unlike [`core::mem::zeroed`], it is safe: a `T` that zero bytes do not make does not compile. As
/// with `core::mem::zeroed`, the padding of the result is not guaranteed to be zero. The [rules] of
/// `T` are not checked, so the result may break them.
///
/// # Compile-time errors
///
/// A call [compiles] only if:
///
/// - A `T` whose bytes are all zero is a [valid] `T` and meets its `valid(..)` ranges. Its rules
///   are not checked.
/// - Without the `unstable` feature, the zero bytes make no part whose representation Rust leaves
///   to the compiler and `puns` does not assume, other than a struct or a tuple: an enum of the
///   `Rust` representation, `Option` or `Result` ([layout independence]).
/// - `T` holds no eager [aim], since no function makes a value that holds one ([no eager aim]).
///
/// # Examples
///
/// ```
/// assert_eq!(puns::zeroed::<(u32, [bool; 2])>(), (0, [false; 2]));
/// ```
///
/// Counters that start at zero, with an `Option` of a `NonZero` that zero bytes make `None`:
///
/// ```
/// use core::num::NonZero;
///
/// #[derive(puns::Pun, Clone, Copy)]
/// #[repr(C)]
/// struct Counters {
///   packets: u64,
///   errors: u32,
///   first_error: Option<NonZero<u32>>,
/// }
///
/// let counters: Counters = puns::zeroed();
/// assert_eq!((counters.packets, counters.errors, counters.first_error), (0, 0, None));
/// ```
///
/// A type that zero bytes do not make is refused ([its message and fix][zeros message]):
///
/// ```compile_fail,E0080
/// let _ = puns::zeroed::<core::num::NonZero<u8>>();
/// ```
///
/// [valid]: crate::reference::how_puns_decides#what-is-a-valid-value
/// [rules]: crate::reference::how_puns_decides#layouts-and-rules
/// [compiles]: crate::reference::errors
/// [zeros message]: crate::reference::errors#zeros
/// [layout independence]: crate::reference::layouts#layout-independence
/// [no eager aim]: crate::reference::api#no-eager-aim
/// [aim]: crate::reference::aims
#[inline(always)]
#[must_use]
pub const fn zeroed<T: Pun + Copy>() -> T {
  const {
    assert!(
      T::LAYOUT.zeroable(false),
      "zero is not a valid value of this type, or does not meet its `valid(..)` ranges"
    );
    assert!(
      layout::UNSTABLE || T::LAYOUT.zeroable(true),
      "zero is a value of this type only by the tags that `rustc` chose for an enum of the `Rust` \
       representation, `Option` or `Result` in it, whose layout `puns` does not assume: another \
       compiler, or `-Zrandomize-layout`, could choose others.\nGive the enum a primitive \
       representation, such as `#[repr(u8)]`, where it is yours, or enable the `unstable` feature \
       of `puns`"
    );
  };
  const {
    assert!(!T::LAYOUT.holds_eager(false), "a value of this type would hold an eager aim");
  };
  // SAFETY: `zeroed` requires that bytes that are all zero are a valid `T` [1]. The first assertion
  // above found that the layout of `T` accepts them apart from its rules (`Layout::zeroable`), and
  // every value that the layout accepts apart from its rules is a valid `T` (`Pun`, section
  // Safety). Rules are requirements of `puns`, not of the language, and the result may break them.
  //
  // [1] Per https://doc.rust-lang.org/1.99.0/core/mem/fn.zeroed.html:
  //
  //     There is no guarantee that an all-zero byte-pattern represents a valid value of some type
  //     `T`. [...] Using `zeroed` on such types causes immediate undefined behavior because the
  //     Rust compiler assumes that there always is a valid value in a variable it considers
  //     initialized.
  unsafe { core::mem::zeroed() }
}

/// Returns the value of an unsigned integer field, as the expressions of `#[pun(get(..))]` read
/// it.
///
/// # Compile-time errors
///
/// A call compiles only if `T` is an unsigned integer of 1 to 16 bytes, in any byte order, or a
/// sized struct whose only field of nonzero size is such an integer and covers the struct. This
/// is checked after monomorphization, as [the reference](crate::reference::errors)
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
/// `as_bytes(value)` is [`pun_ref::<S, [u8]>(value)`](pun_ref), with identical behavior, so the
/// result has the length that `pun_ref` gives a slice. Only `S` is a type parameter, since the
/// target is `[u8]`. Nothing is checked at run time.
///
/// # Compile-time errors
///
/// A call [compiles] only under the conditions [initialized bytes] and [layout independence]. Here,
/// every byte that the result covers is initialized in every value of `S`.
///
/// # Examples
///
/// ```
/// assert_eq!(puns::as_bytes(&[1u16.to_be(), 2u16.to_be()]), &[0, 1, 0, 2]);
/// ```
///
/// [Reading again after a write](crate::reference::in_practice#reading-again-after-a-write) shows a
/// real use: checking a value again from its bytes.
///
/// A value with a byte of padding is refused ([its message and fix][padding message]):
///
/// ```compile_fail,E0080
/// // One byte is padding, whatever the order of the fields.
/// let _ = puns::as_bytes(&(0u8, 0u16));
/// ```
///
/// [compiles]: crate::reference::errors
/// [initialized bytes]: crate::reference::api#initialized-bytes
/// [layout independence]: crate::reference::layouts#layout-independence
/// [padding message]: crate::reference::errors#reading-an-error
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
/// `from_bytes(bytes)` is [`try_pun_ref::<[u8], D>(bytes)`](try_pun_ref), with identical behavior,
/// and one type parameter, which fixes the source to bytes ([Reading
/// bytes](crate::reference::api#reading-bytes) says why).
///
/// # Errors
///
/// Returns, in the order of the checks, [`Error::Alignment`] if `D` needs an alignment greater than
/// 1 and `bytes` is not aligned for it, [`Error::Size`] if no `D` has the length of `bytes` as its
/// [extent] or if that `D` is larger than `bytes`, [`Error::Validity`] if the bytes are not a
/// [valid] `D` or do not meet its [requirements], and [`Error::Limit`] if `D` holds eager [aims]
/// whose pointees, one within another, have more than [`AIM_PARTS`] parts of their layouts, or more
/// bytes than twice those checked.
///
/// # Compile-time errors
///
/// A call [compiles] only under the conditions [no address check in a constant] and [layout
/// independence], and if:
///
/// - `D` is [inhabited], and the elements of its trailing slice, if any, have a value or have size
///   0. An empty slice of elements without a value is a value of `D`, but no bytes are such an
///   element, so the pun is refused.
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
/// No bytes are a value of an uninhabited type ([its message][check message]):
///
/// ```compile_fail,E0080
/// let _ = puns::from_bytes::<core::convert::Infallible>(&[]);
/// ```
///
/// [valid]: crate::reference::how_puns_decides#what-is-a-valid-value
/// [requirements]: crate::reference::how_puns_decides#requirements
/// [extent]: crate::reference::how_puns_decides#layouts-and-rules
/// [inhabited]: crate::reference::how_puns_decides#uninhabited-types
/// [compiles]: crate::reference::errors
/// [no address check in a constant]: crate::reference::api#in-a-constant
/// [layout independence]: crate::reference::layouts#layout-independence
/// [check message]: crate::reference::errors#values-and-padding
/// [aims]: crate::reference::aims
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
/// behavior, and exists for the same reasons as [`from_bytes`].
///
/// # Errors
///
/// Returns the errors of [`from_bytes`], in the same order.
///
/// # Compile-time errors
///
/// A call [compiles] only under the conditions [no address check in a constant], [writes leave a
/// valid source], [no eager aim] and [layout independence], and if:
///
/// - `D` is [inhabited], and the elements of its trailing slice, if any, have a value or have size
///   0. An empty slice of elements without a value is a value of `D`, but no bytes are such an
///   element, so the pun is refused.
///
/// # Examples
///
/// ```
/// let mut bytes = [0u8, 0, 0, 1];
/// *puns::from_bytes_mut::<[bool; 2]>(&mut bytes[2..]).unwrap() = [true, false];
/// assert_eq!(bytes, [0, 0, 1, 0]);
/// assert_eq!(puns::from_bytes_mut::<[bool; 2]>(&mut [2, 0]), Err(puns::Error::Validity));
/// ```
///
/// A target with padding is refused, since a write through the `&mut` could leave uninitialized
/// bytes ([its message and fix][write message]):
///
/// ```compile_fail,E0080
/// // A `(u8, u16)` has a byte of padding.
/// let _ = puns::from_bytes_mut::<(u8, u16)>(&mut [0; 4]);
/// ```
///
/// [inhabited]: crate::reference::how_puns_decides#uninhabited-types
/// [compiles]: crate::reference::errors
/// [writes leave a valid source]: crate::reference::api#writes-leave-a-valid-source
/// [no address check in a constant]: crate::reference::api#in-a-constant
/// [layout independence]: crate::reference::layouts#layout-independence
/// [write message]: crate::reference::errors#writing-through-mut
/// [no eager aim]: crate::reference::api#no-eager-aim
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
  ///
  /// A returned pointer is a valid `D` if `Plan::<[u8], D>::TRY_REF` compiles ([`Plan::front_by`]).
  #[inline(always)]
  const unsafe fn front(addr: *const u8, n: usize) -> Result<(*const D, usize), Error> {
    // SAFETY: `addr` is valid for reads of `n` initialized bytes, by contract on the caller, as
    // `front_by` requires. `C` is `Self`, whose marks are those of the analysis of bytes as a `D`.
    // Where `Plan::<[u8], D>::TRY_REF` compiles, as the callers require first, these are every
    // part of `D` that some bytes are not valid for, so a pointer that it returns is a valid `D`.
    unsafe { Self::front_by::<Self>(addr, n, usize::MAX) }
  }

  /// Returns the `D` at the start of the `n` bytes at `addr`, and its size, as [`Plan::front`]
  /// does, but checks what `C` marks rather than everything that bytes do not prove.
  ///
  /// A `D` whose [extent](crate::reference::how_puns_decides#layouts-and-rules) is not `extent` is
  /// not valid, unless `extent` is `usize::MAX`, which allows any extent.
  ///
  /// # Safety
  ///
  /// The caller must ensure that:
  ///
  /// - `addr` is valid for reads of `n` initialized bytes.
  ///
  /// The returned pointer is a valid `D` only if `C` marks every part of `D` that the bytes may not
  /// be valid for. It does if `C` is [`Plan`] itself and `Plan::<[u8], D>::TRY_REF` compiles, since
  /// the marks are then those of any bytes as a `D`. It also does if `C` holds the marks of an
  /// analysis of a source `S` as a `D` that records no byte that `D` requires to be initialized and
  /// that may be padding, no disjoint part and nothing that Rust leaves to the compiler, and the
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
    // The pointees of eager aims are looked for in the `n` bytes.
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
    // The pointees of eager aims are looked for in the `n` bytes, before the `D` too.
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
/// A sized `D` takes `size_of::<D>()` bytes. A `D` that ends with a slice and has a `size` or `len`
/// rule (see [`macro@Pun`]) takes its [extent], which the rule gives, rounded up to the alignment
/// of `D`: the bytes must also hold the padding that follows its slice. Without such a rule, it
/// takes as many elements as fit, trailing padding included, or none if they have size 0.
///
/// # Errors
///
/// Returns, in the order of the checks:
///
/// 1. [`Error::Size`] if `bytes` is shorter than the fields of `D` before its slice, or than a
///    sized `D`;
/// 2. [`Error::Alignment`] if `D` needs an alignment greater than 1 and `bytes` is not aligned for
///    it;
/// 3. [`Error::Validity`] if the `size` or `len` rule of `D` cannot be computed, or gives a size
///    that `D` cannot have;
/// 4. [`Error::Size`] if `bytes` is shorter than what the `D` takes;
/// 5. [`Error::Validity`] if the bytes taken are not a [valid] `D` or do not meet its
///    [requirements], and [`Error::Limit`] if `D` holds eager [aims] whose pointees, one within
///    another, have more than [`AIM_PARTS`] parts of their layouts, or more bytes than twice those
///    checked.
///
/// # Compile-time errors
///
/// A call [compiles] only under the conditions [no address check in a constant] and [layout
/// independence], and if:
///
/// - `D` is [inhabited], and the elements of its trailing slice, if any, have a value or have size
///   0. An empty slice of elements without a value is a value of `D`, but no bytes are such an
///   element, so the pun is refused.
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
/// [valid]: crate::reference::how_puns_decides#what-is-a-valid-value
/// [requirements]: crate::reference::how_puns_decides#requirements
/// [extent]: crate::reference::how_puns_decides#layouts-and-rules
/// [inhabited]: crate::reference::how_puns_decides#uninhabited-types
/// [compiles]: crate::reference::errors
/// [no address check in a constant]: crate::reference::api#in-a-constant
/// [layout independence]: crate::reference::layouts#layout-independence
/// [aims]: crate::reference::aims
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

/// Where and why [`check`] or [`check_compact`] found that bytes are not a value.
///
/// # Examples
///
/// ```
/// use puns::{Error, Failure, Pun};
///
/// #[derive(Pun)]
/// #[repr(C)]
/// struct Flags {
///   a: bool,
///   b: bool,
/// }
///
/// // The second byte is no `bool`.
/// let failure = puns::check::<Flags, 1, _>(&[1, 2], &mut []).unwrap_err();
/// assert_eq!(failure, Failure { error: Error::Validity, at: 1 });
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Failure {
  /// Why: [`Error::Validity`] if the bytes are not a value, [`Error::Size`] if they are fewer than
  /// its size, or [`Error::Limit`], which says nothing about the bytes, if the worklist was full
  /// and could not grow, or the pointees took more than twice the bytes.
  pub error: Error,
  /// Where, in bytes from the start of the bytes: for [`Error::Validity`], where the check first
  /// fails, in the order of [`check`]. That is the innermost scalar, enum or struct whose own check
  /// fails (its valid values, its tag, its rules), or else the pointee that holds it, from the
  /// start of what is stored before its value ([`Pointee`]), the struct whose eager aim does not
  /// lead to a pointee, or, in a compact check, where the bytes should have ended. A pointee whose
  /// length is not one of its type, such as a negative length, fails where it starts. 0 for another
  /// error.
  pub at: usize,
}

/// Where a [`check`] keeps the pointees that are still to be checked: a slice of [`Pending`]
/// entries, or a vector that grows.
///
/// A check needs at most one entry for each eager aim at a stored place in the bytes, and one for
/// each slice whose elements hold such aims, `bytes.len() / 4` for pointers of 4 bytes, and usually
/// as many as wait at once while the check goes down its deepest path.
///
/// # Safety
///
/// The check keeps entries in the slice that [`Worklist::items`] returns, and reads them back, so
/// an implementation must ensure that:
///
/// - `items` returns the same entries each time it is called, unchanged, unless `grow` was called
///   in between.
/// - After a call of `grow`, `items` returns the entries that it returned before, unchanged, as its
///   first ones, and more entries than before if `grow` returned `true`.
///
/// A slice and an array, which cannot grow, meet these. So does a vector that `grow` resizes,
/// since a resize keeps its elements.
///
/// # Examples
///
/// ```
/// use puns::{Pending, Worklist};
///
/// /// Entries on the heap, as many as the check needs.
/// struct Grows(Vec<Pending>);
///
/// // SAFETY: `items` is the vector, which only `grow` changes, and `grow` makes it longer, keeping
/// // its elements.
/// unsafe impl Worklist for Grows {
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
pub unsafe trait Worklist {
  /// Returns the entries, every one of which the check may write.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::{Pending, Worklist};
  ///
  /// let mut work = [Pending::default(); 4];
  /// assert_eq!(work.items().len(), 4);
  /// ```
  fn items(&mut self) -> &mut [Pending];

  /// Makes room for more entries, keeping the ones of [`Worklist::items`] at the start, and returns
  /// whether it did. A slice cannot.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::{Pending, Worklist};
  ///
  /// let mut work = [Pending::default(); 4];
  /// assert!(!work.grow());
  /// ```
  fn grow(&mut self) -> bool {
    false
  }
}

// SAFETY: `items` returns the slice, which only the check writes, and `grow` does not grow it.
unsafe impl Worklist for [Pending] {
  fn items(&mut self) -> &mut [Pending] {
    self
  }
}

// SAFETY: As for a slice.
unsafe impl<const N: usize> Worklist for [Pending; N] {
  fn items(&mut self) -> &mut [Pending] {
    self
  }
}

/// A worklist of [`check`] as a trait object, which it may not be itself (a slice), so that one
/// driver of the check serves every worklist.
struct Dyn<'w, W: ?Sized>(&'w mut W);

// SAFETY: The calls are those of `W`, which meets the `# Safety` of `Worklist`.
unsafe impl<W: Worklist + ?Sized> Worklist for Dyn<'_, W> {
  fn items(&mut self) -> &mut [Pending] {
    self.0.items()
  }

  fn grow(&mut self) -> bool {
    self.0.grow()
  }
}

/// Checks that `bytes` start with a `D`, with the pointees of its eager aims, as they will be at
/// an address aligned to `ALIGN`.
///
/// The check makes no reference to the bytes, which may be at any address, and the pointees may be
/// nested at any depth.
///
/// The bytes are checked as they will be once copied to an address aligned to `ALIGN`: the `D`
/// and the types of each eager pointee must be aligned to at most `ALIGN`, and each pointee is
/// aligned by its offset in the bytes, to the alignment of its aim, which may be higher (that of a
/// [`Thin`] counts the size of its length). The check makes no recursion that grows with the data:
/// the pointees wait in `work` ([`Worklist`]), so it goes as deep as the data does, with a stack
/// that the types bound, where [`from_head`] stops at [`AIM_PARTS`].
///
/// The order of the check is that of a depth-first walk in which the parts of each value come
/// before its pointees: the `D`, then the pointees of its eager aims, in the order of the aims,
/// each with its own pointees before the next. The elements of a slice whose elements hold eager
/// aims are each a value of its own, so the pointees of one come before the next element. Where
/// the bytes stop being a `D` is where this order first meets a part that is not valid
/// ([`Failure`]).
///
/// # Errors
///
/// Returns a [`Failure`] with [`Error::Size`] if `bytes` is shorter than a `D`,
/// [`Error::Validity`] if the bytes are not a `D` with its pointees, and [`Error::Limit`] if `work`
/// was full and could not grow, or the pointees took more than twice the bytes, which says nothing
/// about the bytes.
///
/// # Compile-time errors
///
/// A call compiles only under the compile-time conditions of [`from_head`], and if `ALIGN` is a
/// power of two, at least the alignment of `D`.
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
/// assert_eq!(puns::check::<Text, 2, _>(&image[1..], &mut work), Ok(()));
/// // A text that is not UTF-8, refused where its length is.
/// image[6] = 0xff;
/// let failure = Failure { error: Error::Validity, at: 2 };
/// assert_eq!(puns::check::<Text, 2, _>(&image[1..], &mut work), Err(failure));
/// ```
pub fn check<D: Pun, const ALIGN: usize, W: Worklist + ?Sized>(
  bytes: &[u8],
  work: &mut W,
) -> Result<(), Failure> {
  checked::<D, ALIGN>(bytes, false, &mut Dyn(work))
}

/// Checks that `bytes` are the compact image of a `D` once at an address aligned to `ALIGN`.
///
/// It checks what [`check`] checks, and that each pointee at a stored place starts at the first
/// offset after the one before it, in the order of the check, where it is aligned (after the `D`
/// for the first), with what is stored before its value ([`Pointee`]) right before it, and that
/// the last one ends where the bytes end.
///
/// Such bytes are what a writer that copies a value depth first, each pointee at the next aligned
/// place, makes, and no pointee in them is shared: no two aims lead to one, and none leads back.
///
/// # Errors
///
/// As for [`check`]. A pointee that is not where a compact image has it fails where the struct
/// whose aim leads to it is, and bytes that go on after the last pointee fail where it ends.
///
/// # Compile-time errors
///
/// As for [`check`].
///
/// # Examples
///
/// ```
/// use core::num::NonZero;
///
/// use puns::{Error, Failure, InPlace, Pending, Pun};
///
/// /// A pointer to a `u16`, stored as the distance from its own address.
/// #[derive(Pun)]
/// #[repr(transparent)]
/// struct Ptr {
///   #[pun(pointer(u16 = off as u128))]
///   off: InPlace<NonZero<i16>>,
/// }
///
/// let mut work = [Pending::default(); 1];
/// // The pointer, then its pointee right after it.
/// assert_eq!(puns::check_compact::<Ptr, 2, _>(puns::as_bytes(&[2i16, 7]), &mut work), Ok(()));
/// // A byte more, past the pointee: no longer the compact image.
/// let failure = Failure { error: Error::Validity, at: 4 };
/// let longer = puns::check_compact::<Ptr, 2, _>(puns::as_bytes(&[2i16, 7, 0]), &mut work);
/// assert_eq!(longer, Err(failure));
/// ```
pub fn check_compact<D: Pun, const ALIGN: usize, W: Worklist + ?Sized>(
  bytes: &[u8],
  work: &mut W,
) -> Result<(), Failure> {
  checked::<D, ALIGN>(bytes, true, &mut Dyn(work))
}

/// [`check`], or [`check_compact`] if `compact`, with one driver for every worklist.
fn checked<D: Pun, const ALIGN: usize>(
  bytes: &[u8],
  compact: bool,
  work: &mut dyn Worklist,
) -> Result<(), Failure> {
  const {
    let () = Plan::<[u8], D>::TRY_REF;
    assert!(ALIGN.is_power_of_two(), "the alignment of the bytes is a power of two");
    assert!(align_of::<D>() <= ALIGN, "the target needs a greater alignment than the bytes have");
  };
  if bytes.len() < size_of::<D>() {
    return Err(Failure { error: Error::Size, at: 0 });
  }
  let root = (size_of::<D>(), validate::root_stage::<D>());
  // SAFETY: `bytes` is valid for reads of its initialized bytes, at least the size of `D`, and the
  // root is that of `D`, as `drive` requires.
  unsafe { validate::drive(bytes.as_ptr(), bytes.len(), (ALIGN, compact), root, work) }
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
/// The `D` is the last `size_of::<D>()` bytes.
///
/// # Errors
///
/// Returns, in the order of the checks, [`Error::Size`] if `bytes` is shorter than
/// `size_of::<D>()`, [`Error::Alignment`] if `D` needs an alignment greater than 1 and the last
/// `size_of::<D>()` bytes are not aligned for it, [`Error::Validity`] if the bytes are not a
/// [valid] `D` or do not meet its [requirements], and [`Error::Limit`] if `D` holds eager [aims]
/// whose pointees, one within another, have more than [`AIM_PARTS`] parts of their layouts, or more
/// bytes than twice those checked.
///
/// # Compile-time errors
///
/// A call [compiles] only under the conditions [no address check in a constant] and [layout
/// independence], and if:
///
/// - `D` is [inhabited].
/// - `D` is sized, because where an unsized `D` starts cannot be found from the end of the bytes.
///   [`from_head`] and [`from_bytes`] take an unsized `D`.
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
/// An unsized target is refused ([its message and fix][last bytes message]):
///
/// ```compile_fail,E0080
/// // An unsized target.
/// let _ = puns::from_tail::<[u16]>(&[0; 4]);
/// ```
///
/// [valid]: crate::reference::how_puns_decides#what-is-a-valid-value
/// [requirements]: crate::reference::how_puns_decides#requirements
/// [inhabited]: crate::reference::how_puns_decides#uninhabited-types
/// [compiles]: crate::reference::errors
/// [no address check in a constant]: crate::reference::api#in-a-constant
/// [layout independence]: crate::reference::layouts#layout-independence
/// [last bytes message]: crate::reference::errors#the-last-bytes-of-a-value
/// [aims]: crate::reference::aims
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
/// It takes and checks what [`from_head`] takes and checks.
///
/// # Errors
///
/// Returns the errors of [`from_head`], in the same order.
///
/// # Compile-time errors
///
/// A call [compiles] only under the conditions [no address check in a constant], [writes leave a
/// valid source], [no eager aim] and [layout independence], and if:
///
/// - `D` is [inhabited], and the elements of its trailing slice, if any, have a value or have size
///   0. An empty slice of elements without a value is a value of `D`, but no bytes are such an
///   element, so the pun is refused.
///
/// # Examples
///
/// ```
/// let mut bytes = [1u8, 0, 7, 7];
/// let (flags, rest) = puns::from_head_mut::<[bool; 2]>(&mut bytes).unwrap();
/// flags[1] = true;
/// rest[0] = 9;
/// assert_eq!(bytes, [1, 1, 9, 7]);
/// assert_eq!(puns::from_head_mut::<[bool; 2]>(&mut [1, 2, 0]).err(), Some(puns::Error::Validity));
/// ```
///
/// A target with padding is refused, as by [`from_bytes_mut`] ([its message and fix][write
/// message]):
///
/// ```compile_fail,E0080
/// // A `(u8, u16)` has a byte of padding.
/// let _ = puns::from_head_mut::<(u8, u16)>(&mut [0; 4]);
/// ```
///
/// [inhabited]: crate::reference::how_puns_decides#uninhabited-types
/// [compiles]: crate::reference::errors
/// [writes leave a valid source]: crate::reference::api#writes-leave-a-valid-source
/// [no address check in a constant]: crate::reference::api#in-a-constant
/// [layout independence]: crate::reference::layouts#layout-independence
/// [write message]: crate::reference::errors#writing-through-mut
/// [no eager aim]: crate::reference::api#no-eager-aim
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
/// It takes and checks what [`from_tail`] takes and checks.
///
/// # Errors
///
/// Returns the errors of [`from_tail`], in the same order.
///
/// # Compile-time errors
///
/// A call [compiles] only under the conditions [no address check in a constant], [writes leave a
/// valid source], [no eager aim] and [layout independence], and if:
///
/// - `D` is [inhabited].
/// - `D` is sized. [`from_head_mut`] and [`from_bytes_mut`] take an unsized `D`.
///
/// # Examples
///
/// ```
/// let mut bytes = [7u8, 7, 0, 1];
/// let (rest, flags) = puns::from_tail_mut::<[bool; 2]>(&mut bytes).unwrap();
/// flags[0] = true;
/// rest[0] = 9;
/// assert_eq!(bytes, [9, 7, 1, 1]);
/// assert_eq!(puns::from_tail_mut::<[bool; 2]>(&mut [1, 2]).err(), Some(puns::Error::Validity));
/// ```
///
/// A target with padding is refused, as by [`from_bytes_mut`] ([its message and fix][write
/// message]):
///
/// ```compile_fail,E0080
/// // A `(u8, u16)` has a byte of padding.
/// let _ = puns::from_tail_mut::<(u8, u16)>(&mut [0; 4]);
/// ```
///
/// [inhabited]: crate::reference::how_puns_decides#uninhabited-types
/// [compiles]: crate::reference::errors
/// [writes leave a valid source]: crate::reference::api#writes-leave-a-valid-source
/// [no address check in a constant]: crate::reference::api#in-a-constant
/// [layout independence]: crate::reference::layouts#layout-independence
/// [write message]: crate::reference::errors#writing-through-mut
/// [no eager aim]: crate::reference::api#no-eager-aim
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

/// Returns the bytes of the region of the tail of `value` that the aim at the cursor that `K` names
/// in the list of the rules of `S` ([`Item`]) cuts: a region of `#[pun(regions(..))]` without a
/// type (see `#[derive(Pun)]`).
///
/// The type system does not enforce regions. If the fields of `value` give regions that do not fit
/// in its tail, they are truncated. A region whose expression fails takes the rest of the tail.
///
/// # Compile-time errors
///
/// A call compiles only if the rule that `K` names is an aim at the cursor at `[u8]`, and every
/// byte of every value of `S`, up to the end of its slice, is initialized and, without the
/// `unstable` feature, this does not depend on a layout that Rust leaves to the compiler. This is
/// checked after monomorphization, as [the reference](crate::reference::errors) explains.
#[doc(hidden)]
#[inline(always)]
pub const fn region<S: ?Sized + Pun, K: Item<S::Rules>>(value: &S) -> &[u8] {
  // The bytes of the value can be read: it has no padding.
  const { Plan::<S, [u8]>::BY_REF };
  let head = const {
    assert!(
      matches!(S::LAYOUT.rule(K::Of::RULE), Some(layout::Rule::Aim(aim))
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
  let step =
    unsafe { validate::region_at::<S, S::Rules>(base, size, (head, tail), K::Of::RULE, 0) };
  let (start, n) = (step.value, step.size);
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

/// Returns the pointee of the lazy aim at the cursor that `K` names in the list of the rules of `S`
/// ([`Item`]), in `value`, as a reference to its value if its bytes are one: a region of
/// `#[pun(regions(..))]` with a type (see `#[derive(Pun)]`).
///
/// The bytes of the region are found as [`region`] finds them, and checked as [`from_bytes`]
/// checks bytes, after what the pointee stores before its value, but the pointees of the eager aims
/// of the region are looked for in the bytes of `value`, so that a pointer in a region may point at
/// another region of the same struct.
///
/// # Errors
///
/// As for [`from_bytes`] on the bytes of the region, and [`Error::Size`] if they are not of a size
/// that the pointee has.
///
/// # Compile-time errors
///
/// A call compiles only if the rule that `K` names is an aim at the cursor, under the compile-time
/// conditions of [`region`] for `S` and of [`from_bytes`] for the pointee.
#[doc(hidden)]
#[inline(always)]
pub const fn region_as<S: ?Sized + Pun, K: Item<S::Rules>>(
  value: &S,
) -> Result<&ValueOf<S, K>, Error> {
  const { Plan::<S, [u8]>::BY_REF };
  let head = const {
    assert!(
      matches!(S::LAYOUT.rule(K::Of::RULE), Some(layout::Rule::Aim(aim))
        if matches!(aim.start, layout::Start::Next)),
      "no such region"
    );
    S::LAYOUT.tail_shape().start
  };
  let (_, tail, size) = validate::tail_of::<S>(value);
  let base = core::ptr::from_ref::<S>(value).cast::<u8>();
  // SAFETY: As in `region`.
  let step =
    unsafe { validate::region_at::<S, S::Rules>(base, size, (head, tail), K::Of::RULE, 0) };
  // A pointee that does not fit, past the tail, has no size of its type. Otherwise, what it stores
  // before its value is in the tail, before it.
  if step.len.is_none() {
    return Err(Error::Size);
  }
  let (start, n) = (step.value, step.size);
  let mut input = Input::new(base, size);
  // SAFETY: `start + n <= size` (`region_at`), and the pointee is the one of the list of `S`, whose
  // step `region_at` took, so if it has a size of its type, what it stores before its value is in
  // the tail, before `start`. So the pointee is within the bytes of `value`, which are valid for
  // reads and initialized, since `Plan::<S, [u8]>::BY_REF` compiled, and which `input` spans, as
  // `lazy` requires.
  match unsafe { lazy::<PointeeOf<S, K>>(base.add(start), n, &mut input) } {
    // SAFETY: `lazy` returned `Ok`, so the pointee is a valid value at that address, aligned,
    // within the bytes of `value`, which are borrowed for the lifetime of the result, as in
    // `from_bytes`.
    Ok(pointee) => Ok(unsafe { &*pointee }),
    Err(error) => Err(error),
  }
}

/// Returns the value of the pointee `P` whose value is the `n` bytes at `value`, if they and what
/// `P` stores before them are one, checked as [`from_bytes`] checks bytes, what is stored before
/// first, with the pointees of their eager aims in the bytes of `input`.
///
/// # Safety
///
/// The caller must ensure that the `n` bytes at `value`, and the bytes of the prefix of `P` before
/// them, are within the bytes of `input`, in one allocation, valid for reads and initialized.
const unsafe fn lazy<P: ?Sized + Pointee>(
  value: *const u8,
  n: usize,
  input: &mut Input,
) -> Result<*const P::Value, Error> {
  const {
    let () = Plan::<[u8], P::Value>::TRY_REF;
    let () = Plan::<[u8], P::Prefix>::TRY_REF;
  };
  let before = const { size_of::<P::Prefix>() };
  if before > 0 {
    // SAFETY: The prefix is the `before` bytes before `value`, within the bytes of `input`, by
    // contract on the caller.
    let prefix = core::ptr::slice_from_raw_parts(unsafe { value.sub(before) }, before);
    // SAFETY: `prefix` is valid for reads of `before` initialized bytes, which `input` spans, as
    // `try_cast_whole` requires.
    if let Err(error) = unsafe { Plan::<[u8], P::Prefix>::try_cast_whole(prefix, input) } {
      return Err(error);
    }
  }
  // The length that a `Thin` stores is the length of its value, which is of that size.
  if const { P::LENGTH.is_some() } {
    // SAFETY: The `n` bytes at `value` are valid for reads and initialized, with the prefix before
    // them, as `measure` requires.
    match unsafe { validate::measure::<P>(validate::Size::Pointee, value, n) } {
      Some((Some(_), size)) if size == n => {}
      _ => return Err(Error::Size),
    }
  }
  let bytes = core::ptr::slice_from_raw_parts(value, n);
  // SAFETY: `bytes` is valid for reads of `n` initialized bytes, which `input` spans, as
  // `try_cast_whole` requires.
  unsafe { Plan::<[u8], P::Value>::try_cast_whole(bytes, input) }
}

/// Returns a pointer to the value of the pointee of the eager aim `L`, an item of the list of the
/// rules of `S`, for the value of `S` at `holder`, with the length of its tail if it is unsized
/// ([`aim`]).
///
/// # Safety
///
/// The caller must ensure the requirements of [`aim`] but the ones on `'a`, with `L` the item that
/// `K` names there, and that `L` is an item of the list of the rules of `S` (`S::Rules`).
///
/// # Compile-time errors
///
/// As for [`aim`].
#[inline(always)]
const unsafe fn pointee<S: ?Sized + Pun, L: Rules>(
  holder: *const S,
) -> *const <L::Pointee as Pointee>::Value {
  let (head, start, extent) = const {
    let Some(layout::Rule::Aim(aim)) = S::LAYOUT.rule(L::RULE) else {
      panic!("the rule is not an eager aim")
    };
    assert!(matches!(aim.check, layout::Check::Eager), "the rule is not an eager aim");
    validate::pointee_checked::<S, L>();
    (S::LAYOUT.tail_shape().start, aim.start, aim.extent)
  };
  let (_, tail, size) = validate::tail_of::<S>(holder);
  let base = holder.cast::<u8>();
  let (value, len) = match start {
    layout::Start::Next => {
      // SAFETY: `holder` points to a value of `S`, valid for reads, by contract on the caller, so
      // its `size` bytes, the extent of the value (`tail_of`), are valid for reads, and the bytes
      // that its rules read, and of its tail, are initialized. `head` and `tail` are its tail,
      // which is its last part, so `head + tail <= size`. The search starts at 0. These are the
      // requirements of `region_at`.
      let step =
        unsafe { validate::region_at::<S, S::Rules>(base, size, (head, tail), L::RULE, 0) };
      // SAFETY: `step.value <= size`, within the bytes of the holder.
      (unsafe { base.add(step.value) }, step.len)
    }
    layout::Start::Stored(_) => {
      // SAFETY: As above, the requirements of `pointer_at` hold.
      let Some(at) = (unsafe { validate::pointer_at::<S, L>(base, size) }) else {
        // SAFETY: The expression of the aim succeeds over the bytes of the value, by contract on
        // the caller.
        unsafe { core::hint::unreachable_unchecked() }
      };
      // The value is after what the aim points at, the length of a `Thin`, within the bytes that
      // hold the holder, by contract on the caller, so the offset fits an `isize`.
      let off = at.wrapping_add(<L::Pointee as Pointee>::VALUE_AT as i128) as isize;
      let value = base.wrapping_offset(off);
      let len = match extent {
        // SAFETY: The value of a pointee that stores its length, aligned and valid, is at `value`,
        // after what the pointee stores before it, by contract on the caller, and it stores its
        // length (`pointee_checked`). These are the requirements of `own_length`.
        layout::Extent::Pointee => Some(unsafe { validate::own_length::<L::Pointee>(value) }),
        // SAFETY: As above, the requirements of `given` hold.
        layout::Extent::Length(_) => match unsafe { validate::given::<S, L>(base, size) } {
          Some(len) if len <= usize::MAX as u128 => Some(len as usize),
          _ => None,
        },
        // SAFETY: As above.
        layout::Extent::Bytes(_) => match unsafe { validate::given::<S, L>(base, size) } {
          Some(n) if n <= usize::MAX as u128 => {
            validate::sized_as::<<L::Pointee as Pointee>::Value>(n as usize)
          }
          _ => None,
        },
        // To the end of the tail of the holder, which is after the value, by contract on the
        // caller.
        layout::Extent::Rest => {
          let end = if const { S::LAYOUT.is_unsized() } { size } else { head + tail };
          #[expect(
            clippy::cast_sign_loss,
            reason = "the value is before the end, as the check found"
          )]
          let rest = end.cast_signed().wrapping_sub(off) as usize;
          validate::sized_as::<<L::Pointee as Pointee>::Value>(rest)
        }
      };
      (value, len)
    }
  };
  let Some(len) = len else {
    // SAFETY: The pointee has a size that its type has, as the check of the holder found, by
    // contract on the caller.
    unsafe { core::hint::unreachable_unchecked() }
  };
  ptr::from_raw_parts::<<L::Pointee as Pointee>::Value>(value, len)
}

/// Returns the value of the pointee of the eager aim that `K` names in the list of the rules of
/// `S`, for `value`: the read of the method `name(&self)` that the derive writes for an eager aim
/// with a name ([`macro@Pun`]).
///
/// It is safe because `S` is a [`Pun`](trait@Pun) type, whose `# Safety` requires every value of
/// `S` that safe code reaches to hold the pointee of each of its eager aims, as [`aim`] requires
/// of its holder, with a reference to the value for its provenance: the tree borrows model of Miri
/// gives a reference the provenance of the bytes of the pointee, and `puns` relies on it here
/// ([Aims](crate::reference::aims)). The derive meets that `# Safety` with the [`InPlace`] of the
/// aim, which no safe code makes, and which no path that `#[pun(crate = ..)]` names replaces ([What
/// the derive trusts](crate::reference::trust)). The method calls this function with no `unsafe`
/// block, so a path that names another function there calls safe code.
///
/// # Compile-time errors
///
/// As for [`aim`].
#[doc(hidden)]
#[inline(always)]
#[must_use]
pub const fn aim_of<S: ?Sized + Pun, K: Item<S::Rules>>(value: &S) -> &ValueOf<S, K> {
  // SAFETY: `value` is a reference to a value of `S`, a `Pun` type, so by the `# Safety` of `Pun`,
  // it meets the requirements of `aim` for the eager aims of `S`, one of which `K` names, which
  // `pointee` asserts. The pointee is not written while `value` lives, which the result borrows.
  unsafe { aim(value, AimOf::<S, K>::NEW) }
}

/// Returns the value of the pointee of the eager aim of `S` that the second argument names, for
/// the value of `S` at `holder`, as a reference.
///
/// The pointee is where the check of the struct found it: at the address of `holder`, plus the
/// value of the expression of the aim, an `i128` in two's complement, or at the cursor of the tail
/// of the struct, with the length that it stores, that the aim gives, or of the size that the aim
/// gives, or of what remains of the tail of the struct. The aim is named by a field of the
/// constant `AIMS` that the derive writes ([`AimOf`]).
///
/// The method `name(&self)` that the derive writes for the aim reads the same pointee through the
/// reference to the value, which has the provenance of the bytes of the pointee under the tree
/// borrows model of Miri, but not under stacked borrows ([Aims](crate::reference::aims)). This
/// function takes a pointer, so that a caller can give it that provenance under either model.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `holder` points to a value of `S`, valid for reads, whose bytes that its rules read, and the
///   bytes of its tail, are initialized, and over which the expressions of the aim succeed.
/// - Where the aim says, there is a value of its pointee, aligned, that is valid as Rust requires
///   of the referent of a reference to it, of the size that the aim gives, and, if its extent is
///   what it stores and it is unsized, the length before it (a `Thin`), or that its `size` or `len`
///   rule gives from its sized fields, is the length of its tail.
/// - `holder` has the provenance of the bytes of that pointee.
/// - The bytes of the value and of the pointee are not written for `'a`, except inside an
///   `UnsafeCell`.
///
/// A value that a function of `puns` checked with its eager aims meets the first two, such as one
/// that a reference that `puns` returned points to, or a value within one. Unsafe code that writes
/// a value with an eager aim, as `nearest` writes its regions, keeps them for each value that it
/// writes, since safe code reads its pointees. Rules of the pointee that do not give its length,
/// such as an `assert`, are not needed: the read relies on none.
///
/// # Compile-time errors
///
/// A call compiles only if the aim is eager, and a pointee whose extent is what it stores is sized,
/// a `Thin`, or stores the length of its tail in its sized fields, as the check of the aim
/// requires.
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
/// assert_eq!(*next.next(), 9);
/// // The same, with the provenance of the bytes that hold both, as stacked borrows requires.
/// let holder = bytes.as_ptr().with_addr(core::ptr::from_ref(next).addr()).cast();
/// // SAFETY: `puns` checked `next` with its pointee, and the pointer has the provenance of the
/// // bytes that hold both, which are not written while it is used.
/// let pointee = unsafe { puns::aim(holder, Next::AIMS.next) };
/// assert_eq!(*pointee, 9);
/// ```
#[inline(always)]
#[must_use]
pub const unsafe fn aim<'a, S: ?Sized + Pun, K: Item<S::Rules>>(
  holder: *const S,
  _: AimOf<S, K>,
) -> &'a ValueOf<S, K> {
  // SAFETY: The requirements of `pointee` are those of this call, and `K::Of` is an item of the
  // list of `S` (`Item`). The pointee at the address that it returns, with its length, is valid
  // and aligned, and is not written for `'a`, by contract on the caller, so it can be borrowed for
  // `'a`.
  unsafe { &*pointee::<S, K::Of>(holder) }
}

/// Returns the value of the pointee of the eager aim of `S` that the second argument names, for
/// the value of `S` at `holder`, as a mutable reference: what [`aim`] returns, to write.
///
/// # Safety
///
/// The caller must ensure the requirements of [`aim`] but the last, and that for `'a`, the bytes of
/// the pointee are accessed only through the result, and the bytes of the value are not written.
/// The bytes of a value that `puns` checked may hold pointees that several aims share, since a
/// check that is not compact allows it, and a reference that `puns` returned is shared, so these
/// bytes are not for this function: it is for the bytes that unsafe code owns, as `nearest` owns a
/// region, whose pointees it does not share. What is written through the result is what later
/// reads of the pointee rely on, safe ones included.
///
/// # Compile-time errors
///
/// As for [`aim`].
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
/// let mut bytes = [1, 9];
/// assert!(puns::from_head::<Next>(&bytes).is_ok());
/// let holder = bytes.as_mut_ptr().cast::<Next>();
/// // SAFETY: `puns` checked the `Next` at the start of `bytes` with its pointee. The pointer has
/// // the provenance of both, the pointee is accessed only through the result while it is used,
/// // and a `u8` stays one.
/// unsafe { *puns::aim_mut(holder, Next::AIMS.next) += 1 };
/// assert_eq!(bytes, [1, 10]);
/// ```
#[inline(always)]
#[must_use]
pub const unsafe fn aim_mut<'a, S: ?Sized + Pun, K: Item<S::Rules>>(
  holder: *mut S,
  _: AimOf<S, K>,
) -> &'a mut ValueOf<S, K> {
  // SAFETY: The requirements of `pointee` are those of this call, and `K::Of` is an item of the
  // list of `S` (`Item`). The pointee at the address that it returns, with its length, is valid
  // and aligned, and accessed only through the result for `'a`, by contract on the caller, so it
  // can be borrowed mutably for `'a`. The pointer is made from `holder`, a `*mut S`, so it may
  // write.
  unsafe { &mut *pointee::<S, K::Of>(holder.cast_const()).cast_mut() }
}

/// Returns the pointee of the lazy aim at a stored place that `K` names in the list of the rules of
/// `S` ([`Item`]), for `value`, which is in `bytes`, as a reference to its value if the bytes where
/// it is are one.
///
/// The pointee is where the check of the struct finds it: at the address of `value` plus the value
/// of the expression of the aim, an `i128` in two's complement, within `bytes`. Its size is that
/// of its type, with the length that it stores if it is unsized, the length or the size that the
/// aim gives, or what remains of the tail of the struct. What it stores before its value is checked
/// first, then the value, as [`from_bytes`] checks bytes, and the pointees of their own eager aims
/// are looked for in `bytes`. This is the read that the derive writes for a lazy aim at a stored
/// place, which takes the bytes, since a reference to the value covers only its own.
///
/// # Errors
///
/// Returns [`Error::Validity`] if `value` is not within `bytes`, if the pointee does not start
/// within them, or if an expression of the aim fails, [`Error::Size`] if its size does not fit in
/// them, or is not one that its type has, and the errors of [`from_bytes`] on its bytes.
///
/// # Compile-time errors
///
/// A call compiles only if the rule that `K` names is a lazy aim at a stored place, under the
/// compile-time conditions of [`from_bytes`] for the pointee.
#[doc(hidden)]
#[inline]
pub fn aim_in<'a, S: ?Sized + Pun, K: Item<S::Rules>>(
  value: &S,
  bytes: &'a [u8],
) -> Result<&'a ValueOf<S, K>, Error> {
  const {
    let Some(layout::Rule::Aim(aim)) = S::LAYOUT.rule(K::Of::RULE) else {
      panic!("the rule is not a lazy aim at a stored place")
    };
    assert!(
      matches!((aim.start, aim.check), (layout::Start::Stored(_), layout::Check::Lazy)),
      "the rule is not a lazy aim at a stored place"
    );
    validate::pointee_checked::<S, K::Of>();
  };
  let (_, _, size) = validate::tail_of::<S>(value);
  let base = core::ptr::from_ref::<S>(value).cast::<u8>();
  let from = match base.addr().checked_sub(bytes.as_ptr().addr()) {
    Some(from) if from <= bytes.len() && size <= bytes.len() - from => from,
    _ => return Err(Error::Validity),
  };
  let mut input = Input::new(bytes.as_ptr(), bytes.len());
  // SAFETY: The struct is read through `bytes`, at `from` bytes from their start: its `size` bytes
  // are within `bytes` (checked above), so they are initialized `u8`s, valid for reads, whatever
  // padding the struct has, in the allocation that `input` spans, and they hold the value. These
  // are the requirements of `stored_at`.
  let Some((to, wanted)) =
    (unsafe { validate::stored_at::<S, K::Of>(bytes.as_ptr().add(from), size, &input) })
  else {
    return Err(Error::Validity);
  };
  // SAFETY: `prefix <= to <= bytes.len()` (`stored_at`), so `addr` is in `bytes`.
  let addr = unsafe { bytes.as_ptr().add(to) };
  // SAFETY: The `bytes.len() - to` bytes at `addr` are the end of `bytes`, valid for reads and
  // initialized, and the prefix of the pointee is before them, as `measure` requires.
  let Some((_, n)) =
    (unsafe { validate::measure::<PointeeOf<S, K>>(wanted, addr, bytes.len() - to) })
  else {
    return Err(Error::Size);
  };
  // SAFETY: The `n` bytes at `addr`, and the prefix before them, are within `bytes`, valid for
  // reads and initialized, which `input` spans, as `lazy` requires.
  match unsafe { lazy::<PointeeOf<S, K>>(addr, n, &mut input) } {
    // SAFETY: As in `from_bytes`: a valid pointee, aligned, within `bytes`, borrowed for `'a`.
    Ok(pointee) => Ok(unsafe { &*pointee }),
    Err(error) => Err(error),
  }
}
