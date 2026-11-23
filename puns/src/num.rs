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
//! [guarantee]: crate::guide::concepts#what-is-a-valid-value
//! [requirement]: crate::guide::concepts#requirements

use core::fmt;

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
    //   to be valid. The Reference requires the field to be valid [2]. `puns` relies on this being
    //   all, which no documentation states (`docs/rust.md`, "A value whose fields are valid"). No
    //   code of `$name` relies on what its bytes are: `from_bytes` makes one of any bytes.
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
      "# Compile-time errors\n\nA pun of a type that holds it [compiles](crate::guide::errors) ",
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
    //   to be valid. The Reference requires the field to be valid [2]. `puns` relies on this being
    //   all, which no documentation states (`docs/rust.md`, "A value whose fields are valid"). No
    //   code of `$name` relies on what its bytes are: `from_bytes` makes one of any bytes.
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
      const SPEC: Spec = Spec::any::<Self>().with_endian(Endian::$endian).not_a_number();
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
/// A pun of a type that holds it [compiles](crate::guide::errors) only if `MIN` is not greater
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
