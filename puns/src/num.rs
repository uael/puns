//! Integers of a given byte order, and integers of a given value.
//!
//! - `u16le`, `u32be`...: integers stored in a given byte order. They are byte arrays, hence have
//!   no alignment requirement.
//! - `i16le`, `i32be`..., `f32le`, `f64be`...: signed and floating-point numbers, the same way.
//! - `u8c<V>`, `u16lec<V>`, `u32bec<V>`...: integers whose only valid value is `V`: magic numbers,
//!   type tags and the like.

use core::fmt;

use crate::{
  Scalar,
  layout::{Endian, Id, Spec},
};

macro_rules! endian {
  (Little, $le:expr, $be:expr) => {
    $le
  };
  (Big, $le:expr, $be:expr) => {
    $be
  };
}

/// What the examples of an integer of `$size` bytes write: the value whose bytes, as they are in
/// memory, are 1, 2, 3... in the byte order `$endian`, and those bytes (`bytes`). The size is
/// matched as a token (`ints!` takes it as a `tt`: a `literal` fragment matches no literal).
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

macro_rules! ints {
  ($($name:ident $literal:ident: $raw:ident[$size:tt] $endian:ident;)+) => {$(
    #[doc = concat!("A `", stringify!($raw), "` of ", stringify!($size), " bytes, most significant ")]
    #[doc = endian!($endian, "last (little-endian).", "first (big-endian).")]
    ///
    #[doc = concat!("Any ", stringify!($size), " bytes are one, at any address: its alignment is 1.")]
    /// It is compared, ordered and formatted as its value.
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
      pub const MAX: Self = Self([0xff; $size]);

      #[doc = concat!("The integer `value`, in ", stringify!($size), " bytes of this byte order.")]
      ///
      /// ```
      #[doc = concat!("use puns::num::", stringify!($name), ";")]
      ///
      #[doc = concat!("let x = ", stringify!($name), "::new(", sample!($endian $size), ");")]
      #[doc = concat!("assert_eq!(x.to_bytes(), ", sample!(bytes $size), ");")]
      #[doc = concat!("assert_eq!(x.get(), ", sample!($endian $size), ");")]
      /// ```
      ///
      /// # Panics
      /// If `value` does not fit.
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

      #[doc = concat!("The value, a `", stringify!($raw), "`.")]
      ///
      /// ```
      #[doc = concat!("use puns::num::", stringify!($name), ";")]
      ///
      #[doc = concat!(
        "assert_eq!(", stringify!($name), "::from_bytes(", sample!(bytes $size), ").get(), ",
        sample!($endian $size), ");"
      )]
      /// // The greatest value: as many bits as the bytes have.
      #[doc = concat!(
        "assert_eq!(", stringify!($name), "::MAX.get(), ", stringify!($raw), "::MAX >> (",
        stringify!($raw), "::BITS - 8 * ", stringify!($size), "));"
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

      /// The bytes, as they are in memory.
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

      /// The integer whose bytes, as they are in memory, are `bytes`: any bytes are one.
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

    // SAFETY: a byte array.
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

    #[doc = concat!("A [`", stringify!($name), "`] that is `V`, and nothing else.")]
    ///
    /// Bytes are one only if they are `V` in this byte order: a magic number, a version, a type
    #[doc = concat!(
      "tag. It is always a valid [`", stringify!($name), "`], which it proves where a pun reads ",
      "it as one."
    )]
    #[doc = concat!(
      "A `V` that does not fit in ", stringify!($size), " bytes is refused where the type is ",
      "read: it does not compile."
    )]
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
      /// The value, `V`, which is the only one.
      ///
      /// ```
      #[doc = concat!("use puns::num::", stringify!($literal), ";")]
      ///
      #[doc = concat!(
        "let magic = ", stringify!($literal), "::<", sample!($endian $size), ">::new();"
      )]
      #[doc = concat!("assert_eq!(puns::as_bytes(&magic), ", sample!(bytes $size), ");")]
      /// ```
      ///
      /// # Panics
      #[doc = concat!("If `V` does not fit in ", stringify!($size), " bytes.")]
      #[inline]
      pub const fn new() -> Self {
        Self($name::new(V))
      }

      /// `V`.
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

    // SAFETY: the only value that `new` makes.
    unsafe impl<const V: $raw> Scalar for $literal<V> {
      const SPEC: Spec = {
        let _ = Self::new();
        Spec::ranges::<Self>(Endian::$endian, const { &[(V as u128, V as u128)] })
      };
      // `V` may not fit a `usize`: its bytes are in the layout, which identical types share.
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

impl From<u16> for u16le {
  #[inline]
  fn from(value: u16) -> Self {
    Self::new(value)
  }
}

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
  u16be: u16; u32le: u32; u32be: u32; u64le: u64; u64be: u64; u128le: u128; u128be: u128;
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

macro_rules! numbers {
  ($($name:ident: $raw:ident[$size:literal] $endian:ident = $value:literal;)+) => {$(
    #[doc = concat!("A `", stringify!($raw), "` of ", stringify!($size), " bytes, most significant ")]
    #[doc = endian!($endian, "last (little-endian).", "first (big-endian).")]
    ///
    /// Its bits are not its value as an unsigned integer: the rules of `#[derive(Pun)]` do not read
    /// it. Two are equal if their bytes are.
    ///
    #[doc = concat!("Any ", stringify!($size), " bytes are one, at any address: its alignment is 1.")]
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
      #[doc = concat!("The number `value`, in ", stringify!($size), " bytes of this byte order.")]
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

      #[doc = concat!("The value, a `", stringify!($raw), "`.")]
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

      /// The bytes, as they are in memory.
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

      /// The number whose bytes, as they are in memory, are `bytes`: any bytes are one.
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

    // SAFETY: a byte array.
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

/// Signed integers are ordered by their value, whatever their bytes: what the offsets of a
/// crate of self-relative pointers must be (`nearest`), among other uses.
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

/// A `u8` that is `V`, and nothing else.
///
/// A byte is one only if it is `V`: a version, a type tag. It is always a valid `u8`, which it
/// proves where a pun reads it as one.
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
  /// The value, `V`, which is the only one.
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

  /// `V`.
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

// SAFETY: the only value that `new` makes.
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

/// A `u8` from `MIN` to `MAX`: `u8in<0, 0x7f>` is an ASCII character.
///
/// Unlike a field restricted by `#[pun(valid(..))]`, which safe code can write anything to, this
/// is a guarantee: it can prove what a target requires.
///
/// A `MIN` greater than `MAX` is refused where the type is read: it does not compile.
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
  /// `value`, if it is from `MIN` to `MAX`.
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

  /// The value, from `MIN` to `MAX`.
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

// SAFETY: the only values that `new` makes.
unsafe impl<const MIN: u8, const MAX: u8> Scalar for u8in<MIN, MAX> {
  const SPEC: Spec = Spec::ranges::<Self>(Endian::NATIVE, const { &[(MIN as u128, MAX as u128)] });
  const ID: Id = Id::sized("u8in", &[], MIN as usize * 256 + MAX as usize);
}

impl<const MIN: u8, const MAX: u8> fmt::Debug for u8in<MIN, MAX> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Debug::fmt(&self.0, f)
  }
}
