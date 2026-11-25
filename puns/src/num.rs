//! Numbers of a given byte order, and integers restricted to some values.
//!
//! # Byte order
//!
//! [`u16le`] is a little-endian `u16`, which stores its least significant byte first, and
//! [`u16be`] a big-endian one, which stores it last. There are such types for the unsigned
//! integers of 16, 24, 32, 48, 64 and 128 bits, for `i16` to `i128`, and for `f32` and `f64`.
//!
//! Each type is a byte array, so any bytes of its size are a valid value, and its alignment is 1:
//! it can be at any address. A `repr(C)` struct without `align(..)` whose fields are these types,
//! bytes, or such structs, has no padding and can be at any address too. A field of alignment 1
//! may still hold padding of its own, as an enum `repr(u8)` with fields may.
//!
//! Integers are compared, ordered and formatted as their values. A floating-point number is
//! formatted as its value and has no order, and two of them are equal if their bytes are. So,
//! unlike with `f64`, a NaN equals a NaN of the same bytes, and `-0.0` is not `0.0`.
//!
//! The rules of `#[derive(Pun)]` read a signed integer under a cast, `field as u128`,
//! sign-extended, and do not read a floating-point number, whose bits are not its value as an
//! integer ([Fields in a rule](crate::reference::rules#fields-in-a-rule)).
//!
//! Reading and comparing numbers of each byte order:
//!
//! ```
//! use puns::num::{f64le, i16be, u16be, u16le, u24be};
//!
//! // The same bytes in each byte order.
//! assert_eq!(u16le::from_bytes([1, 2]).get(), 0x0201);
//! assert_eq!(u16be::from_bytes([1, 2]).get(), 0x0102);
//! assert_eq!(u16be::new(0x0102).to_bytes(), [1, 2]);
//!
//! // One byte into an array, which an alignment of 1 allows.
//! let bytes = [0, 1, 2, 3];
//! let x: &u24be = puns::from_head(&bytes[1..]).unwrap().0;
//! assert_eq!(x.get(), 0x01_0203);
//!
//! assert_eq!(i16be::from_bytes([0xff, 0xfe]).get(), -2);
//!
//! // Floating-point numbers are equal if their bytes are.
//! assert_eq!(f64le::new(f64::NAN), f64le::new(f64::NAN));
//! assert_ne!(f64le::new(-0.0), f64le::new(0.0));
//! ```
//!
//! # Constant types and ranges
//!
//! A *constant type* is an integer whose only valid value is its parameter `V`: [`u8c<V>`](u8c),
//! and one for each unsigned type above, such as [`u32bec<V>`](u32bec). It holds a magic number, a
//! version or a type tag. A pun of it as its unsigned type, such as `u32bec<V>` as `u32be`, is
//! proven, since `V` is a valid value of that type.
//!
//! [`u8in<MIN, MAX>`](u8in) is a `u8` from `MIN` to `MAX`.
//!
//! Constant types and `u8in` [guarantee] their values: their field is private, and a value is made
//! only by their constructors or by a pun that proves or checks its bytes. So a pun can rely on
//! them, as it cannot on a [requirement].
//!
//! ```
//! use puns::{Pun, num::{u16be, u32bec}};
//!
//! /// The start of a Java class file.
//! #[derive(Pun)]
//! #[repr(C)]
//! struct Class {
//!   magic: u32bec<0xcafe_babe>,
//!   minor: u16be,
//!   major: u16be,
//! }
//!
//! let class: &Class = puns::from_bytes(&[0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 65]).unwrap();
//! // Java 21.
//! assert_eq!(class.major.get(), 65);
//! // Other bytes than the magic number fail the check.
//! assert!(puns::from_bytes::<Class>(&[0; 8]).is_err());
//! ```
//!
//! # Integers that are not zero
//!
//! [`NonZero<T>`](NonZero) is an integer of 2, 4, 8 or 16 bytes of this module that is not zero,
//! whose `Option` has its size.
//!
//! [guarantee]: crate::reference::concepts#what-is-a-valid-value
//! [requirement]: crate::reference::concepts#requirements

use core::{fmt, marker::PhantomData};

use crate::{
  Scalar,
  layout::{Endian, Id, Spec},
};

/// Expands to `$le` for `Little`, and to `$be` for `Big`.
macro_rules! endian {
  (Little, $le:expr, $be:expr) => {
    $le
  };
  (Big, $le:expr, $be:expr) => {
    $be
  };
}

/// Expands to `$narrow` for an integer of fewer bytes than its `$raw` (`u24` and `u48`), which
/// some values of `$raw` do not fit, and to `$exact` otherwise.
macro_rules! narrower {
  (u32 3, $narrow:expr, $exact:expr) => {
    $narrow
  };
  (u64 6, $narrow:expr, $exact:expr) => {
    $narrow
  };
  ($raw:ident $size:tt, $narrow:expr, $exact:expr) => {
    $exact
  };
}

/// Expands to the text that the examples of an integer of `$size` bytes write.
///
/// With `$endian`, it is the value whose bytes in memory are 1, 2, 3 and so on in that byte
/// order. With `bytes`, it is those bytes. The size is matched as a token, since `ints!` takes it
/// as a `tt`, and a `literal` fragment matches no literal.
macro_rules! sample {
  (Little 2) => {
    "0x0201"
  };
  (Big 2) => {
    "0x0102"
  };
  (bytes 2) => {
    "[1, 2]"
  };
  (Little 3) => {
    "0x03_0201"
  };
  (Big 3) => {
    "0x01_0203"
  };
  (bytes 3) => {
    "[1, 2, 3]"
  };
  (Little 4) => {
    "0x0403_0201"
  };
  (Big 4) => {
    "0x0102_0304"
  };
  (bytes 4) => {
    "[1, 2, 3, 4]"
  };
  (Little 6) => {
    "0x0605_0403_0201"
  };
  (Big 6) => {
    "0x0102_0304_0506"
  };
  (bytes 6) => {
    "[1, 2, 3, 4, 5, 6]"
  };
  (Little 8) => {
    "0x0807_0605_0403_0201"
  };
  (Big 8) => {
    "0x0102_0304_0506_0708"
  };
  (bytes 8) => {
    "[1, 2, 3, 4, 5, 6, 7, 8]"
  };
  (Little 16) => {
    "0x100f_0e0d_0c0b_0a09_0807_0605_0403_0201"
  };
  (Big 16) => {
    "0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10"
  };
  (bytes 16) => {
    "[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]"
  };
}

/// Defines, for each `$name $literal: $raw[$size] $endian`, an unsigned integer `$name` of
/// `$size` bytes of the byte order `$endian`, read as a `$raw`, and the integer `$literal<V>` of
/// that type whose only valid value is `V`.
macro_rules! ints {
  ($($name:ident $literal:ident: $raw:ident[$size:tt] $endian:ident;)+) => {$(
    #[doc = narrower!(
      $raw $size,
      concat!(
        "A ", endian!($endian, "little", "big"), "-endian unsigned integer of ", stringify!($size),
        " bytes, read as a `", stringify!($raw), "`."
      ),
      concat!("A ", endian!($endian, "little", "big"), "-endian `", stringify!($raw), "`.")
    )]
    ///
    /// # Examples
    ///
    /// ```
    #[doc = concat!("use puns::num::", stringify!($name), ";")]
    ///
    /// let bytes = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
    /// // One byte into the array.
    #[doc = concat!(
      "let x: &", stringify!($name), " = puns::from_head(&bytes[1..]).unwrap().0;"
    )]
    #[doc = concat!("assert_eq!(x.get(), ", sample!($endian $size), ");")]
    #[doc = concat!("assert_eq!(align_of::<", stringify!($name), ">(), 1);")]
    /// ```
    #[expect(non_camel_case_types, reason = "named like the primitive integers they stand for")]
    #[derive(Copy, Clone, PartialEq, Eq, Hash, Default)]
    #[repr(transparent)]
    pub struct $name([u8; $size]);

    impl $name {
      /// The greatest value.
      ///
      /// # Examples
      ///
      /// ```
      #[doc = concat!("use puns::num::", stringify!($name), ";")]
      ///
      #[doc = concat!(
        "assert_eq!(", stringify!($name), "::MAX.to_bytes(), [0xff; ", stringify!($size), "]);"
      )]
      /// // As many bits as the bytes have.
      #[doc = concat!(
        "assert_eq!(", stringify!($name), "::MAX.get(), ", stringify!($raw), "::MAX >> (",
        stringify!($raw), "::BITS - 8 * ", stringify!($size), "));"
      )]
      /// ```
      pub const MAX: Self = Self([0xff; $size]);

      /// Creates `value`.
      ///
      #[doc = narrower!($raw $size, concat!(
        "# Panics\n\nPanics if `value` does not fit in ", stringify!($size), " bytes.\n\n",
        "# Examples\n"
      ), "# Examples\n")]
      ///
      /// ```
      #[doc = concat!("use puns::num::", stringify!($name), ";")]
      ///
      #[doc = concat!("let x = ", stringify!($name), "::new(", sample!($endian $size), ");")]
      #[doc = concat!("assert_eq!(x.to_bytes(), ", sample!(bytes $size), ");")]
      #[doc = concat!("assert_eq!(x.get(), ", sample!($endian $size), ");")]
      /// ```
      #[inline]
      pub const fn new(value: $raw) -> Self {
        let (mut bytes, mut i) = ([0; $size], 0);
        while i < $size {
          bytes[endian!($endian, i, $size - 1 - i)] = (value >> (8 * i)) as u8;
          i += 1;
        }
        let this = Self(bytes);
        assert!(this.get() == value, "value is out of range");
        this
      }

      /// Returns the value.
      ///
      /// # Examples
      ///
      /// ```
      #[doc = concat!("use puns::num::", stringify!($name), ";")]
      ///
      #[doc = concat!(
        "assert_eq!(", stringify!($name), "::from_bytes(", sample!(bytes $size), ").get(), ",
        sample!($endian $size), ");"
      )]
      /// ```
      #[inline]
      pub const fn get(self) -> $raw {
        let (mut value, mut i) = (0, 0);
        while i < $size {
          value |= (self.0[endian!($endian, i, $size - 1 - i)] as $raw) << (8 * i);
          i += 1;
        }
        value
      }

      /// Returns its bytes.
      ///
      /// # Examples
      ///
      /// ```
      #[doc = concat!("use puns::num::", stringify!($name), ";")]
      ///
      #[doc = concat!(
        "assert_eq!(", stringify!($name), "::new(", sample!($endian $size), ").to_bytes(), ",
        sample!(bytes $size), ");"
      )]
      /// ```
      #[inline]
      pub const fn to_bytes(self) -> [u8; $size] {
        self.0
      }

      /// Creates a value from its bytes.
      ///
      /// # Examples
      ///
      /// ```
      #[doc = concat!("use puns::num::", stringify!($name), ";")]
      ///
      #[doc = concat!(
        "assert_eq!(", stringify!($name), "::from_bytes(", sample!(bytes $size), "), ",
        stringify!($name), "::new(", sample!($endian $size), "));"
      )]
      #[doc = concat!(
        "assert_eq!(", stringify!($name), "::from_bytes([0xff; ", stringify!($size), "]), ",
        stringify!($name), "::MAX);"
      )]
      /// ```
      #[inline]
      pub const fn from_bytes(bytes: [u8; $size]) -> Self {
        Self(bytes)
      }
    }

    // SAFETY: `$name` is `repr(transparent)` over `[u8; $size]`, so it has its layout [1], and
    // `SPEC` has its size. The items of the `# Safety` of `Scalar` hold:
    // - Every value of `$name` is initialized bytes, all of which `Spec::any` accepts.
    // - Every initialized `[u8; $size]` is a valid `$name`, whose one field is all that it requires
    //   to be valid. The Rust Reference requires the field to be valid [2]. `puns` relies on this
    //   being all, which no documentation states (`docs/rust.md`, "A value whose fields are
    //   valid"). No code of `$name` relies on what its bytes are: `from_bytes` makes one of any
    //   bytes.
    // - `$name` holds no `UnsafeCell`.
    //
    // [1] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.transparent.layout-abi:
    //
    //     Structs and enums with this representation have the same layout and ABI as the only
    //     non-size 0 non-alignment 1 field, if present, or unit otherwise.
    //
    // [2] Per https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.struct:
    //
    //     A struct, tuple, and array requires all fields/elements to be valid at their respective
    //     type.
    unsafe impl Scalar for $name {
      const SPEC: Spec = Spec::any::<Self>().with_endian(Endian::$endian);
      const ID: Id = Id::new(stringify!($name), &[]);
    }

    impl From<$name> for $raw {
      #[inline]
      fn from(value: $name) -> Self {
        value.get()
      }
    }

    impl PartialOrd for $name {
      #[inline]
      fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
      }
    }

    impl Ord for $name {
      #[inline]
      fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.get().cmp(&other.get())
      }
    }

    impl fmt::Debug for $name {
      fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.get(), f)
      }
    }

    impl fmt::Display for $name {
      fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.get(), f)
      }
    }

    impl fmt::LowerHex for $name {
      fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::LowerHex::fmt(&self.get(), f)
      }
    }

    #[doc = concat!("A [`", stringify!($name), "`] whose only valid value is `V`.")]
    ///
    #[doc = narrower!($raw $size, concat!(
      "# Compile-time errors\n\nA pun of a type that holds it [compiles](crate::reference::errors) ",
      "only if `V` fits in ", stringify!($size), " bytes:\n\n",
      "```compile_fail,E0080\nuse puns::num::", stringify!($literal), ";\n\n",
      "// One more than the greatest value of ", stringify!($size), " bytes.\n",
      "let _ = puns::from_bytes::<", stringify!($literal), "<{ 1 << (8 * ", stringify!($size),
      ") }>>(&[0; ", stringify!($size), "]);\n```\n\n# Examples\n"
    ), "# Examples\n")]
    ///
    /// ```
    #[doc = concat!("use puns::num::{", stringify!($name), ", ", stringify!($literal), "};")]
    ///
    #[doc = concat!("type Magic = ", stringify!($literal), "<", sample!($endian $size), ">;")]
    ///
    #[doc = concat!("assert!(puns::from_bytes::<Magic>(&", sample!(bytes $size), ").is_ok());")]
    /// assert_eq!(
    #[doc = concat!("  puns::from_bytes::<Magic>(&[0; ", stringify!($size), "]),")]
    ///   Err(puns::Error::Validity),
    /// );
    /// // Proven: nothing is checked.
    #[doc = concat!("let x: ", stringify!($name), " = puns::pun(Magic::new());")]
    #[doc = concat!("assert_eq!(x.get(), ", sample!($endian $size), ");")]
    /// ```
    #[expect(non_camel_case_types, reason = "named like the primitive integers they stand for")]
    #[derive(Copy, Clone, PartialEq, Eq, Hash)]
    #[repr(transparent)]
    pub struct $literal<const V: $raw>($name);

    impl<const V: $raw> $literal<V> {
      /// Creates `V`.
      ///
      #[doc = narrower!($raw $size, concat!(
        "# Panics\n\nPanics if `V` does not fit in ", stringify!($size), " bytes.\n\n",
        "# Examples\n"
      ), "# Examples\n")]
      ///
      /// ```
      #[doc = concat!("use puns::num::", stringify!($literal), ";")]
      ///
      #[doc = concat!(
        "let magic = ", stringify!($literal), "::<", sample!($endian $size), ">::new();"
      )]
      #[doc = concat!("assert_eq!(puns::as_bytes(&magic), ", sample!(bytes $size), ");")]
      /// ```
      #[inline]
      pub const fn new() -> Self {
        Self($name::new(V))
      }

      /// Returns `V`.
      ///
      /// # Examples
      ///
      /// ```
      #[doc = concat!("use puns::num::", stringify!($literal), ";")]
      ///
      #[doc = concat!(
        "assert_eq!(", stringify!($literal), "::<", sample!($endian $size), ">::default().get(), ",
        sample!($endian $size), ");"
      )]
      /// ```
      #[inline]
      pub const fn get(self) -> $raw {
        V
      }
    }

    // SAFETY: `$literal<V>` is `repr(transparent)` over `$name`, so it has its layout [1], and
    // `SPEC` has its size. The items of the `# Safety` of `Scalar` hold:
    // - Its field is private, and `new` is the only code that makes one. `new` makes
    //   `$name::new(V)`, whose bytes, read in the byte order `$endian`, are `V`. `SPEC` accepts
    //   `V`. Its evaluation calls `new`, so a `V` that does not fit in `$size` bytes does not
    //   compile.
    // - The bytes of `V` are a valid `$name`, which any initialized bytes are. Code of
    //   `$literal<V>` relies on its value being `V`, which is the only value `SPEC` accepts.
    // - `$literal<V>` holds no `UnsafeCell`.
    //
    // [1] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.transparent.layout-abi:
    //
    //     Structs and enums with this representation have the same layout and ABI as the only
    //     non-size 0 non-alignment 1 field, if present, or unit otherwise.
    unsafe impl<const V: $raw> Scalar for $literal<V> {
      const SPEC: Spec = {
        let _ = Self::new();
        Spec::ranges::<Self>(Endian::$endian, const { &[(V as u128, V as u128)] })
      };
      // The identity keeps `V as usize`, which may drop high bits. Two types whose `V` differ only
      // there have the same identity, but different layouts, and the analysis compares the
      // layouts.
      const ID: Id = Id::sized(stringify!($literal), &[], V as usize);
    }

    impl<const V: $raw> Default for $literal<V> {
      #[inline]
      fn default() -> Self {
        Self::new()
      }
    }

    impl<const V: $raw> fmt::Debug for $literal<V> {
      fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&V, f)
      }
    }
  )+};
}

/// Implements `From<$raw>` for each `$name: $raw`, by `$name::new`.
///
/// `u24le`, `u24be`, `u48le` and `u48be` have none, since `new` panics on the values of their
/// `$raw` that do not fit.
macro_rules! from {
  ($($name:ident: $raw:ident;)+) => {$(
    impl From<$raw> for $name {
      #[inline]
      fn from(value: $raw) -> Self {
        Self::new(value)
      }
    }
  )+};
}

from! {
  u16le: u16; u16be: u16; u32le: u32; u32be: u32;
  u64le: u64; u64be: u64; u128le: u128; u128be: u128;
}

ints! {
  u16le u16lec: u16[2] Little;
  u16be u16bec: u16[2] Big;
  u24le u24lec: u32[3] Little;
  u24be u24bec: u32[3] Big;
  u32le u32lec: u32[4] Little;
  u32be u32bec: u32[4] Big;
  u48le u48lec: u64[6] Little;
  u48be u48bec: u64[6] Big;
  u64le u64lec: u64[8] Little;
  u64be u64bec: u64[8] Big;
  u128le u128lec: u128[16] Little;
  u128be u128bec: u128[16] Big;
}

/// The method of the `Spec` of a number of `numbers!` that holds a `$raw`, by how the rules of
/// `#[derive(Pun)]` read it: not at all for a floating-point number, whose bits are not its value,
/// and under a cast for a signed integer.
macro_rules! reading {
  (f32) => {
    Spec::not_a_number
  };
  (f64) => {
    Spec::not_a_number
  };
  ($raw:ident) => {
    Spec::signed
  };
}

/// Defines, for each `$name: $raw[$size] $endian = $value`, a number `$name` of `$size` bytes of
/// the byte order `$endian`, read as a `$raw`. Its examples use the value `$value`.
macro_rules! numbers {
  ($($name:ident: $raw:ident[$size:literal] $endian:ident = $value:literal;)+) => {$(
    #[doc = concat!("A ", endian!($endian, "little", "big"), "-endian `", stringify!($raw), "`.")]
    ///
    /// # Examples
    ///
    /// ```
    #[doc = concat!("use puns::num::", stringify!($name), ";")]
    ///
    #[doc = concat!(
      "let bytes = (", stringify!($value), stringify!($raw), ").",
      endian!($endian, "to_le_bytes", "to_be_bytes"), "();"
    )]
    #[doc = concat!("let x: &", stringify!($name), " = puns::from_bytes(&bytes).unwrap();")]
    #[doc = concat!("assert_eq!(x.get(), ", stringify!($value), ");")]
    #[doc = concat!("assert_eq!(align_of::<", stringify!($name), ">(), 1);")]
    /// ```
    #[expect(non_camel_case_types, reason = "named like the primitive numbers they stand for")]
    #[derive(Copy, Clone, PartialEq, Eq, Hash, Default)]
    #[repr(transparent)]
    pub struct $name([u8; $size]);

    impl $name {
      /// Creates `value`.
      ///
      /// # Examples
      ///
      /// ```
      #[doc = concat!("use puns::num::", stringify!($name), ";")]
      ///
      #[doc = concat!("let x = ", stringify!($name), "::new(", stringify!($value), ");")]
      #[doc = concat!(
        "assert_eq!(x.to_bytes(), (", stringify!($value), stringify!($raw), ").",
        endian!($endian, "to_le_bytes", "to_be_bytes"), "());"
      )]
      #[doc = concat!("assert_eq!(x.get(), ", stringify!($value), ");")]
      /// ```
      #[inline]
      pub const fn new(value: $raw) -> Self {
        Self(endian!($endian, value.to_le_bytes(), value.to_be_bytes()))
      }

      /// Returns the value.
      ///
      /// # Examples
      ///
      /// ```
      #[doc = concat!("use puns::num::", stringify!($name), ";")]
      ///
      #[doc = concat!(
        "let bytes = (", stringify!($value), stringify!($raw), ").",
        endian!($endian, "to_le_bytes", "to_be_bytes"), "();"
      )]
      #[doc = concat!(
        "assert_eq!(", stringify!($name), "::from_bytes(bytes).get(), ", stringify!($value), ");"
      )]
      /// ```
      #[inline]
      pub const fn get(self) -> $raw {
        endian!($endian, $raw::from_le_bytes(self.0), $raw::from_be_bytes(self.0))
      }

      /// Returns its bytes.
      ///
      /// # Examples
      ///
      /// ```
      #[doc = concat!("use puns::num::", stringify!($name), ";")]
      ///
      #[doc = concat!(
        "assert_eq!(", stringify!($name), "::new(", stringify!($value), ").to_bytes(), (",
        stringify!($value), stringify!($raw), ").", endian!($endian, "to_le_bytes", "to_be_bytes"),
        "());"
      )]
      /// ```
      #[inline]
      pub const fn to_bytes(self) -> [u8; $size] {
        self.0
      }

      /// Creates a value from its bytes.
      ///
      /// # Examples
      ///
      /// ```
      #[doc = concat!("use puns::num::", stringify!($name), ";")]
      ///
      #[doc = concat!(
        "let bytes = (", stringify!($value), stringify!($raw), ").",
        endian!($endian, "to_le_bytes", "to_be_bytes"), "();"
      )]
      #[doc = concat!(
        "assert_eq!(", stringify!($name), "::from_bytes(bytes), ", stringify!($name), "::new(",
        stringify!($value), "));"
      )]
      /// ```
      #[inline]
      pub const fn from_bytes(bytes: [u8; $size]) -> Self {
        Self(bytes)
      }
    }

    // SAFETY: `$name` is `repr(transparent)` over `[u8; $size]`, so it has its layout [1], and
    // `SPEC` has its size. The items of the `# Safety` of `Scalar` hold:
    // - Every value of `$name` is initialized bytes, all of which `Spec::any` accepts.
    // - Every initialized `[u8; $size]` is a valid `$name`, whose one field is all that it requires
    //   to be valid. The Rust Reference requires the field to be valid [2]. `puns` relies on this
    //   being all, which no documentation states (`docs/rust.md`, "A value whose fields are
    //   valid"). No code of `$name` relies on what its bytes are: `from_bytes` makes one of any
    //   bytes.
    // - `$name` holds no `UnsafeCell`.
    //
    // [1] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.transparent.layout-abi:
    //
    //     Structs and enums with this representation have the same layout and ABI as the only
    //     non-size 0 non-alignment 1 field, if present, or unit otherwise.
    //
    // [2] Per https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.struct:
    //
    //     A struct, tuple, and array requires all fields/elements to be valid at their respective
    //     type.
    unsafe impl Scalar for $name {
      const SPEC: Spec = reading!($raw)(Spec::any::<Self>().with_endian(Endian::$endian));
      const ID: Id = Id::new(stringify!($name), &[]);
    }

    impl From<$name> for $raw {
      #[inline]
      fn from(value: $name) -> Self {
        value.get()
      }
    }

    impl From<$raw> for $name {
      #[inline]
      fn from(value: $raw) -> Self {
        Self::new(value)
      }
    }

    impl fmt::Debug for $name {
      fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.get(), f)
      }
    }

    impl fmt::Display for $name {
      fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.get(), f)
      }
    }
  )+};
}

// With the value of their examples.
numbers! {
  i16le: i16[2] Little = -2;
  i16be: i16[2] Big = -2;
  i32le: i32[4] Little = -2;
  i32be: i32[4] Big = -2;
  i64le: i64[8] Little = -2;
  i64be: i64[8] Big = -2;
  i128le: i128[16] Little = -2;
  i128be: i128[16] Big = -2;
  f32le: f32[4] Little = -1.5;
  f32be: f32[4] Big = -1.5;
  f64le: f64[8] Little = -1.5;
  f64be: f64[8] Big = -1.5;
}

/// Implements `PartialOrd` and `Ord` by value, whatever the bytes, for each signed integer type.
///
/// A crate of self-relative pointers (`nearest`) needs its offsets ordered so, among other uses.
macro_rules! ordered {
  ($($name:ident)+) => {$(
    impl PartialOrd for $name {
      #[inline]
      fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
      }
    }

    impl Ord for $name {
      #[inline]
      fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.get().cmp(&other.get())
      }
    }
  )+};
}

ordered!(i16le i16be i32le i32be i64le i64be i128le i128be);

/// An integer of this module that has a form that is not zero, a [`NonZero`]: the signed and
/// unsigned integers of 2, 4, 8 and 16 bytes, of either byte order. Sealed.
///
/// Each is a byte array of its size, valid for any initialized bytes, and its `Stored` is the
/// `core::num::NonZero` of the same size, valid for any bytes that are not all zero: what
/// [`NonZero::new`] and [`NonZero::get`] rely on, which only this module implements.
///
/// # Examples
///
/// ```
/// use puns::num::{NonZero, NonZeroable, u32le};
///
/// /// The first value that is not zero.
/// fn first<T: NonZeroable>(values: &[T]) -> Option<NonZero<T>> {
///   values.iter().find_map(|value| NonZero::new(*value))
/// }
///
/// let values = [u32le::new(0), u32le::new(5)];
/// assert_eq!(first(&values).map(NonZero::get), Some(u32le::new(5)));
/// ```
pub trait NonZeroable: Copy + seal::NonZeroable {
  /// The `core::num::NonZero` of the size of `Self` that holds the bytes of a `NonZero<Self>`,
  /// whose niche is that of `Option<NonZero<Self>>`.
  #[doc(hidden)]
  type Stored: Copy;
}

mod seal {
  /// The seal of [`NonZeroable`](super::NonZeroable).
  pub trait NonZeroable {}
}

/// A `T` of this module that is not zero: an integer of a fixed byte order without the value 0,
/// as [`core::num::NonZero`] is an integer of the byte order of the target without it.
///
/// Its bytes are those of the `T`, which are never all zero, and its alignment is 1, as the one of
/// `T`. It holds a `core::num::NonZero` of its size, packed, whose niche `rustc` keeps: so
/// `Option<NonZero<T>>` has the size of `T`, `None` being zeros. `core` does not document the niche
/// of a packed struct, so `puns` assumes it where it lays out such an `Option`, and checks it when
/// compiling ([`Spec::stored_as`]). It is compared and ordered as its value, and formatted as it.
///
/// # Examples
///
/// ```
/// use puns::num::{NonZero, i32le};
///
/// let two = NonZero::new(i32le::new(-2)).unwrap();
/// assert_eq!(two.get(), i32le::new(-2));
/// assert_eq!(NonZero::new(i32le::new(0)), None);
/// assert_eq!((align_of::<NonZero<i32le>>(), size_of::<Option<NonZero<i32le>>>()), (1, 4));
/// // Zeros are `None`, and any other bytes are a value.
/// let some = (-2i32).to_le_bytes();
/// assert_eq!(puns::try_pun::<[u8; 4], Option<NonZero<i32le>>>([0; 4]), Ok(None));
/// assert_eq!(puns::try_pun::<[u8; 4], Option<NonZero<i32le>>>(some), Ok(Some(two)));
/// assert!(puns::from_bytes::<NonZero<i32le>>(&[0; 4]).is_err());
/// ```
#[repr(C, packed)]
pub struct NonZero<T: NonZeroable>(T::Stored, PhantomData<T>);

impl<T: NonZeroable> NonZero<T> {
  /// Returns the `NonZero` of `value`, or `None` if it is zero.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::num::{NonZero, u16be};
  ///
  /// assert!(NonZero::new(u16be::new(1)).is_some());
  /// assert_eq!(NonZero::new(u16be::new(0)), None);
  /// ```
  #[inline]
  #[must_use]
  pub const fn new(value: T) -> Option<Self> {
    const { assert!(size_of::<T>() == size_of::<T::Stored>(), "a `NonZero` has the size of `T`") };
    let bytes = (&raw const value).cast::<u8>();
    let mut i = 0;
    while i < size_of::<T>() {
      // SAFETY: `T` is a byte array of its size (`NonZeroable`), whose bytes are initialized, so
      // `bytes.add(i)` is within it, and is a `u8`, aligned at any address.
      if unsafe { *bytes.add(i) } != 0 {
        // SAFETY: The bytes of `value` are initialized, of the size of `T::Stored` (asserted
        // above), the `core::num::NonZero` of that size (`NonZeroable`), and not all zero: a
        // valid `T::Stored`, read without alignment.
        return Some(Self(unsafe { bytes.cast::<T::Stored>().read_unaligned() }, PhantomData));
      }
      i += 1;
    }
    None
  }

  /// Returns the `T`, which is not zero.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::num::{NonZero, i64be};
  ///
  /// let one = i64be::new(1);
  /// assert_eq!(NonZero::new(one).unwrap().get(), one);
  /// ```
  #[inline]
  #[must_use]
  pub const fn get(self) -> T {
    const { assert!(size_of::<T>() == size_of::<T::Stored>(), "a `NonZero` has the size of `T`") };
    // SAFETY: The bytes of `self.0`, a `T::Stored`, are initialized, of the size of `T`, and `T` is
    // a byte array that any initialized bytes are (`NonZeroable`), read without alignment, since
    // the field is packed.
    unsafe { (&raw const self.0).cast::<T>().read_unaligned() }
  }
}

impl<T: NonZeroable> Clone for NonZero<T> {
  #[inline]
  fn clone(&self) -> Self {
    *self
  }
}

impl<T: NonZeroable> Copy for NonZero<T> {}

impl<T: NonZeroable + PartialEq> PartialEq for NonZero<T> {
  #[inline]
  fn eq(&self, other: &Self) -> bool {
    self.get() == other.get()
  }
}

impl<T: NonZeroable + Eq> Eq for NonZero<T> {}

impl<T: NonZeroable + core::hash::Hash> core::hash::Hash for NonZero<T> {
  #[inline]
  fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
    self.get().hash(state);
  }
}

impl<T: NonZeroable + PartialOrd> PartialOrd for NonZero<T> {
  #[inline]
  fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
    self.get().partial_cmp(&other.get())
  }
}

impl<T: NonZeroable + Ord> Ord for NonZero<T> {
  #[inline]
  fn cmp(&self, other: &Self) -> core::cmp::Ordering {
    self.get().cmp(&other.get())
  }
}

impl<T: NonZeroable + fmt::Debug> fmt::Debug for NonZero<T> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Debug::fmt(&self.get(), f)
  }
}

impl<T: NonZeroable + fmt::Display> fmt::Display for NonZero<T> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Display::fmt(&self.get(), f)
  }
}

/// Defines, for each `$name: $raw $bits $endian`, the [`NonZero`] of the integer `$name`, of the
/// byte order `$endian`, read as a `$raw`, whose bit patterns are those of a `$bits`, signed with
/// `signed`.
macro_rules! non_zero {
  ($($name:ident: $raw:ident $bits:ident $endian:ident $($signed:ident)?;)+) => {$(
    impl seal::NonZeroable for $name {}

    // `$name` is `repr(transparent)` over a byte array of the size of `$raw` (`ints!`,
    // `numbers!`), and `core::num::NonZero<$raw>` is valid for any bytes of that size that are not
    // all zero, as the documentation of `NonZeroable` states.
    impl NonZeroable for $name {
      type Stored = core::num::NonZero<$raw>;
    }

    impl From<NonZero<$name>> for $name {
      #[inline]
      fn from(value: NonZero<$name>) -> Self {
        value.get()
      }
    }

    // SAFETY: `NonZero<$name>` is `repr(C)`, so its first field, a `core::num::NonZero<$raw>`, is
    // at offset 0 [1], and its other field, a `PhantomData`, has size 0. `SPEC` has the size of
    // `NonZero<$name>` (`Spec::ranges::<Self>`), which `stored_as` asserts is the size of a
    // `core::num::NonZero<$raw>`, so every byte of it is a byte of its first field: it has no
    // padding. The items of the `# Safety` of `Scalar` hold:
    // - The bytes of every value are a valid `core::num::NonZero<$raw>`: initialized, and not all
    //   zero [2]. Read as a `$bits` in any byte order, they are a value from 1 to `$bits::MAX`,
    //   which `SPEC` accepts.
    // - Every value that `SPEC` accepts is bytes that are not all zero, so a valid
    //   `core::num::NonZero<$raw>`, so a valid `NonZero<$name>` [3], whose code relies on nothing
    //   else: `get` reads any such bytes.
    // - It holds no `UnsafeCell`.
    // `signed` changes no value: rules read the bits under a cast, as for `$name`. `stored_as`
    // declares the type that the bytes are, which is not part of that contract.
    //
    // [1] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.c.struct.size-field-offset:
    //
    //     Start with a current offset of 0 bytes.
    //
    //     For each field in declaration order in the struct, first determine the size and
    //     alignment of the field. If the current offset is not a multiple of the field's
    //     alignment, then add padding bytes to the current offset until it is a multiple of the
    //     field's alignment. The offset for the field is what the current offset is now. [...]
    //
    // [2] Per https://doc.rust-lang.org/1.99.0/core/num/struct.NonZero.html:
    //
    //     `NonZero<T>` is guaranteed to have the same layout and bit validity as `T` with the
    //     exception that the all-zero bit pattern is invalid.
    //
    // [3] Per https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.struct:
    //
    //     A struct, tuple, and array requires all fields/elements to be valid at their respective
    //     type.
    unsafe impl Scalar for NonZero<$name> {
      const SPEC: Spec = Spec::ranges::<Self>(Endian::$endian, &[(1, $bits::MAX as u128)])
        $(.$signed())?
        .stored_as::<core::num::NonZero<$raw>>();
      const ID: Id = Id::new(concat!("num::NonZero<", stringify!($name), ">"), &[]);
    }
  )+};
}

non_zero! {
  u16le: u16 u16 Little;
  u16be: u16 u16 Big;
  u32le: u32 u32 Little;
  u32be: u32 u32 Big;
  u64le: u64 u64 Little;
  u64be: u64 u64 Big;
  u128le: u128 u128 Little;
  u128be: u128 u128 Big;
  i16le: i16 u16 Little signed;
  i16be: i16 u16 Big signed;
  i32le: i32 u32 Little signed;
  i32be: i32 u32 Big signed;
  i64le: i64 u64 Little signed;
  i64be: i64 u64 Big signed;
  i128le: i128 u128 Little signed;
  i128be: i128 u128 Big signed;
}

/// A `u8` whose only valid value is `V`.
///
/// # Examples
///
/// ```
/// use puns::num::u8c;
///
/// // Version 4, and no other.
/// type Version = u8c<4>;
///
/// assert!(puns::from_bytes::<Version>(&[4]).is_ok());
/// assert_eq!(puns::from_bytes::<Version>(&[6]), Err(puns::Error::Validity));
/// // Proven: nothing is checked.
/// assert_eq!(puns::pun::<Version, u8>(Version::new()), 4);
/// ```
#[expect(non_camel_case_types, reason = "named like the primitive integers they stand for")]
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct u8c<const V: u8>(u8);

impl<const V: u8> u8c<V> {
  /// Creates `V`.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::num::u8c;
  ///
  /// assert_eq!(puns::as_bytes(&u8c::<4>::new()), [4]);
  /// ```
  #[inline]
  #[must_use]
  pub const fn new() -> Self {
    Self(V)
  }

  /// Returns `V`.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::num::u8c;
  ///
  /// assert_eq!(u8c::<4>::default().get(), 4);
  /// ```
  #[inline]
  #[must_use]
  pub const fn get(self) -> u8 {
    V
  }
}

// SAFETY: `u8c<V>` is `repr(transparent)` over `u8`, so it has its layout [1], and `SPEC` has its
// size. The items of the `# Safety` of `Scalar` hold:
// - Its field is private, and `new` is the only code that makes one, of the byte `V`, which
//   `SPEC` accepts.
// - `V` is a valid `u8`. Code of `u8c<V>` relies on its value being `V`, which is the only value
//   `SPEC` accepts.
// - `u8c<V>` holds no `UnsafeCell`.
//
// [1] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.transparent.layout-abi:
//
//     Structs and enums with this representation have the same layout and ABI as the only
//     non-size 0 non-alignment 1 field, if present, or unit otherwise.
unsafe impl<const V: u8> Scalar for u8c<V> {
  const SPEC: Spec = Spec::ranges::<Self>(Endian::NATIVE, const { &[(V as u128, V as u128)] });
  const ID: Id = Id::sized("u8c", &[], V as usize);
}

impl<const V: u8> Default for u8c<V> {
  #[inline]
  fn default() -> Self {
    Self::new()
  }
}

impl<const V: u8> fmt::Debug for u8c<V> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Debug::fmt(&V, f)
  }
}

/// A `u8` from `MIN` to `MAX`, inclusive.
///
/// # Compile-time errors
///
/// A pun of a type that holds it [compiles](crate::reference::errors) only if `MIN` is not greater
/// than `MAX`:
///
/// ```compile_fail,E0080
/// use puns::num::u8in;
///
/// // No byte is from 5 to 3.
/// let _ = puns::from_bytes::<u8in<5, 3>>(&[4]);
/// ```
///
/// # Examples
///
/// ```
/// use puns::num::u8in;
///
/// type Ascii = u8in<0, 0x7f>;
///
/// // Checked: bytes below `0x80`.
/// let ascii: &[Ascii] = puns::from_bytes(b"hello").unwrap();
/// assert!(puns::from_bytes::<[Ascii]>("h\u{e9}llo".as_bytes()).is_err());
/// // Proven: ASCII is UTF-8, nothing is checked.
/// let text: &str = puns::pun_ref(ascii);
/// assert_eq!(text, "hello");
/// ```
#[expect(non_camel_case_types, reason = "named like the primitive integers they stand for")]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct u8in<const MIN: u8, const MAX: u8>(u8);

impl<const MIN: u8, const MAX: u8> u8in<MIN, MAX> {
  /// Returns `value` as a `u8in`, or `None` if it is not from `MIN` to `MAX`.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::num::u8in;
  ///
  /// type Digit = u8in<b'0', b'9'>;
  ///
  /// assert_eq!(Digit::new(b'7').map(Digit::get), Some(b'7'));
  /// assert_eq!(Digit::new(b'a'), None);
  /// ```
  #[inline]
  #[must_use]
  pub const fn new(value: u8) -> Option<Self> {
    if MIN <= value && value <= MAX { Some(Self(value)) } else { None }
  }

  /// Returns the value, which is from `MIN` to `MAX`.
  ///
  /// # Examples
  ///
  /// ```
  /// use puns::num::u8in;
  ///
  /// let digit: &u8in<b'0', b'9'> = puns::from_bytes(b"7").unwrap();
  /// assert_eq!(digit.get(), b'7');
  /// ```
  #[inline]
  #[must_use]
  pub const fn get(self) -> u8 {
    self.0
  }
}

// SAFETY: `u8in<MIN, MAX>` is `repr(transparent)` over `u8`, so it has its layout [1], and `SPEC`
// has its size. The items of the `# Safety` of `Scalar` hold:
// - Its field is private, and `new` is the only code that makes one. `new` makes one only of a
//   value from `MIN` to `MAX`, which `SPEC` accepts. If `MIN > MAX`, `new` makes none, and the
//   evaluation of `SPEC` panics (`Spec::with_ranges` refuses an empty range).
// - Every value from `MIN` to `MAX` is a valid `u8`. Code of `u8in<MIN, MAX>` relies on its value
//   being in that range, which is what `SPEC` accepts.
// - `u8in<MIN, MAX>` holds no `UnsafeCell`.
//
// [1] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.transparent.layout-abi:
//
//     Structs and enums with this representation have the same layout and ABI as the only
//     non-size 0 non-alignment 1 field, if present, or unit otherwise.
unsafe impl<const MIN: u8, const MAX: u8> Scalar for u8in<MIN, MAX> {
  const SPEC: Spec = Spec::ranges::<Self>(Endian::NATIVE, const { &[(MIN as u128, MAX as u128)] });
  const ID: Id = Id::sized("u8in", &[], MIN as usize * 256 + MAX as usize);
}

impl<const MIN: u8, const MAX: u8> fmt::Debug for u8in<MIN, MAX> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Debug::fmt(&self.0, f)
  }
}
