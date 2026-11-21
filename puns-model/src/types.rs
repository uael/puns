//! The types of the model, each with its oracle.
//!
//! Each oracle is written from the declaration of its type, never from what the library makes of
//! it.

use core::{mem::offset_of, num::NonZero};

use puns::{
  Pun,
  num::{
    u8c, u8in, u16be, u16bec, u16le, u16lec, u24be, u24bec, u24le, u24lec, u32be, u32bec, u32le,
    u32lec,
  },
};

use crate::{Bytes, Oracle, Uninit, Valid, all, int, require, require_whole};

/// Declares a struct whose oracle combines the oracles of its fields, at the offsets that the
/// compiler gives.
macro_rules! model_struct {
  ($(#[$attr:meta])* struct $name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
    #[derive(Pun, Clone, Copy)]
    $(#[$attr])*
    pub struct $name { $(pub $field: $ty),* }

    impl Oracle for $name {
      fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
        all(&[$(<$ty as Oracle>::check(b, off + offset_of!($name, $field), reads, target)),*])
      }
    }
  };
}

/// Declares an enum without fields whose valid values are the bytes of its variants, read from a
/// value of each variant.
macro_rules! model_fieldless {
  ($(#[$attr:meta])* enum $name:ident { $($variant:ident $(= $value:expr)?),+ $(,)? }) => {
    #[derive(Pun, Clone, Copy, PartialEq, Eq, Debug)]
    $(#[$attr])*
    pub enum $name { $($variant $(= $value)?),+ }

    impl Oracle for $name {
      fn check(b: &Bytes, off: usize, reads: &mut [bool], _: bool) -> Valid {
        let size = size_of::<$name>();
        let v = int(b, off, size, crate::NATIVE_LITTLE, reads)?;
        Ok([$($name::$variant),+].iter().any(|variant| {
          let mut bits = 0u128;
          let bytes: &[u8] =
            // SAFETY: `variant` is a reference to a value of `$name`, valid for reads of its `size`
            // bytes, of alignment 1 as `u8`s. A value of an enum without fields is its
            // discriminant. With `repr(C)` or a primitive representation, it has the size of that
            // integer [1], so every byte is initialized. `Erust`, of the `Rust` representation,
            // rests on the layout that `rustc` gives it, a one-byte tag. These are the requirements
            // of `slice::from_raw_parts`.
            //
            // [1] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.primitive.enum:
            //
            //     For field-less enums, primitive representations set the size and alignment to
            //     be the same as the primitive type of the same name.
            unsafe { core::slice::from_raw_parts(core::ptr::from_ref::<$name>(variant).cast(), size) };
          for i in 0..size {
            let byte = if crate::NATIVE_LITTLE { bytes[size - 1 - i] } else { bytes[i] };
            bits = (bits << 8) | u128::from(byte);
          }
          bits == v
        }))
      }
    }
  };
}

// --- Size 1 --------------------------------------------------------------------------------------

model_fieldless!(
  #[repr(u8)]
  enum E01 {
    A,
    B,
  }
);
model_fieldless!(
  #[repr(u8)]
  enum E37 {
    A = 3,
    B = 7,
    C = 200,
  }
);
model_fieldless!(
  #[repr(u8)]
  enum Eff {
    A = 255,
  }
);
model_fieldless!(
  #[repr(i8)]
  enum Eneg {
    A = -1,
    B = -128,
    C = 0,
  }
);
model_fieldless!(
  enum Erust {
    A,
    B,
    C,
  }
);

model_struct!(
  #[repr(C)]
  struct Flag {
    on: bool,
  }
);
model_struct!(
  struct Byte {
    v: u8,
    nothing: (),
  }
);
model_struct!(
  #[repr(transparent)]
  struct Ascii {
    c: u8in<0, 0x7f>,
  }
);

/// A struct whose field has a requirement, `valid(0..=1)`.
///
/// A target requires it, and a source does not guarantee it.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
pub struct SoftBit {
  #[pun(valid(0..=1))]
  pub v: u8,
}

impl Oracle for SoftBit {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    let v = int(b, off, 1, true, reads)?;
    if target { require(off, "SoftBit", Ok(v <= 1)) } else { Ok(true) }
  }
}

/// A struct with a requirement on a field whose type already restricts its values.
///
/// The `bool` is required to be `true`.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
pub struct SoftTrue {
  #[pun(valid(1))]
  pub v: bool,
}

impl Oracle for SoftTrue {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    let v = int(b, off, 1, true, reads)?;
    if v > 1 {
      return Ok(false);
    }
    if target { require(off, "SoftTrue", Ok(v == 1)) } else { Ok(true) }
  }
}

/// A struct with a rule over its field, which requires `v` to be a multiple of 3.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(v % 3 == 0))]
pub struct Third {
  pub v: u8,
}

impl Oracle for Third {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    let v = int(b, off, 1, true, reads)?;
    if target { require(off, "v % 3 == 0", Ok(v % 3 == 0)) } else { Ok(true) }
  }
}

/// A tuple of types of 1 byte of the model.
pub type Size1 = (
  u8,
  i8,
  bool,
  NonZero<u8>,
  Option<NonZero<u8>>,
  NonZero<i8>,
  u8c<0>,
  u8c<7>,
  u8c<255>,
  u8in<0, 0x7f>,
  u8in<1, 1>,
  u8in<0x80, 0xff>,
  u8in<2, 5>,
  E01,
  E37,
  Eff,
  Eneg,
  Erust,
  [u8; 1],
  [bool; 1],
  (bool,),
  Flag,
  Byte,
  Ascii,
  SoftBit,
  SoftTrue,
  Third,
  core::mem::ManuallyDrop<bool>,
);

// --- Size 2 --------------------------------------------------------------------------------------

model_fieldless!(
  #[repr(u16)]
  enum E16 {
    A = 0x0102,
    B = 0x0201,
    C = 0xff00,
  }
);

model_struct!(
  #[repr(C)]
  struct ByteNz {
    a: u8,
    b: NonZero<u8>,
  }
);
model_struct!(
  #[repr(C)]
  struct NzByte {
    a: NonZero<u8>,
    b: u8,
  }
);
model_struct!(
  #[repr(C, align(2))]
  struct Aligned {
    a: u8,
  }
);
model_struct!(
  #[repr(C, align(2))]
  struct AlignedFlag {
    a: bool,
  }
);
model_struct!(
  #[repr(C, packed)]
  struct PackedU16 {
    a: u16,
  }
);
model_struct!(
  struct RustPair {
    a: bool,
    b: u8in<0, 3>,
  }
);

/// A struct whose rule reads its two fields as one big-endian integer, which must not be `0x0101`.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(be(0, 2) != 0x0101))]
pub struct NotOneOne {
  pub a: u8,
  pub b: u8,
}

impl Oracle for NotOneOne {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    let v = int(b, off, 2, false, reads)?;
    if target { require(off, "be(0, 2) != 0x0101", Ok(v != 0x0101)) } else { Ok(true) }
  }
}

/// Declares an enum with fields, whose oracle reads the tag, then the fields of the variant of that
/// tag.
///
/// The offsets of the fields are read from a value of the variant, made from the `$sample` of each
/// field.
macro_rules! model_tagged {
  // `repr(C)` alone: the discriminants are implicit (0, 1...), and must be given in that order.
  (
    $(#[$attr:meta])* enum $name:ident: tag $tag_size:literal, implicit {
      $($variant:ident($($ty:ty = $sample:expr),*) = $tag:literal),+ $(,)?
    }
  ) => {
    #[derive(Pun, Clone, Copy)]
    $(#[$attr])*
    pub enum $name { $($variant($($ty),*)),+ }
    model_tagged!(@oracle $name, $tag_size, $($variant($($ty = $sample),*) = $tag),+);
  };
  (
    $(#[$attr:meta])* enum $name:ident: tag $tag_size:literal {
      $($variant:ident($($ty:ty = $sample:expr),*) = $tag:literal),+ $(,)?
    }
  ) => {
    #[derive(Pun, Clone, Copy)]
    $(#[$attr])*
    pub enum $name { $($variant($($ty),*) = $tag),+ }
    model_tagged!(@oracle $name, $tag_size, $($variant($($ty = $sample),*) = $tag),+);
  };
  (@oracle $name:ident, $tag_size:literal, $($variant:ident($($ty:ty = $sample:expr),*) = $tag:literal),+) => {

    impl Oracle for $name {
      fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
        let tag = int(b, off, $tag_size, crate::NATIVE_LITTLE, reads)?;
        $(
          if tag == $tag {
            // The offsets of the fields, from a value of the variant.
            let sample = $name::$variant($($sample),*);
            let offsets: Vec<usize> = model_tagged!(@offsets sample, $name::$variant, $($ty),*);
            let checks: Vec<fn(&Bytes, usize, &mut [bool], bool) -> Valid> =
              vec![$(<$ty as Oracle>::check),*];
            let results: Vec<Valid> =
              offsets.iter().zip(&checks).map(|(o, check)| check(b, off + o, reads, target)).collect();
            return all(&results);
          }
        )+
        Ok(false)
      }
    }
  };
  (@offsets $sample:ident, $path:path, ) => {{
    let _ = &$sample;
    vec![]
  }};
  (@offsets $sample:ident, $path:path, $a:ty) => {{
    let $path(x) = &$sample else { unreachable!() };
    vec![core::ptr::from_ref::<$a>(x).addr() - core::ptr::from_ref(&$sample).addr()]
  }};
  (@offsets $sample:ident, $path:path, $a:ty, $b:ty) => {{
    let $path(x, y) = &$sample else { unreachable!() };
    let base = core::ptr::from_ref(&$sample).addr();
    vec![core::ptr::from_ref::<$a>(x).addr() - base, core::ptr::from_ref::<$b>(y).addr() - base]
  }};
}

model_tagged!(#[repr(u8)] enum Opt: tag 1 { None() = 0, Some(u8 = 0) = 1 });
model_tagged!(#[repr(u8)] enum OptFlag: tag 1 { None() = 0, Some(bool = false) = 1 });
model_tagged!(#[repr(u8)] enum Either: tag 1 { L(NonZero<u8> = NonZero::new(1).unwrap()) = 0, R(u8in<0, 3> = u8in::new(0).unwrap()) = 1 });
model_tagged!(#[repr(C, u8)] enum EitherC: tag 1 { L(NonZero<u8> = NonZero::new(1).unwrap()) = 0, R(u8in<0, 3> = u8in::new(0).unwrap()) = 1 });
model_tagged!(#[repr(u8)] enum Skewed: tag 1 { A(u8 = 0) = 2, B(bool = false) = 9, C() = 4 });

/// An enum like `Opt`, whose field in `Some` has a requirement that the type system does not
/// guarantee.
#[derive(Pun, Clone, Copy)]
#[repr(u8)]
pub enum OptSoftBit {
  None,
  Some(#[pun(valid(0..=1))] u8),
}

impl Oracle for OptSoftBit {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    match int(b, off, 1, true, reads)? {
      0 => Ok(true),
      1 => {
        let v = int(b, off + 1, 1, true, reads)?;
        if target { require(off + 1, "OptSoftBit::Some", Ok(v <= 1)) } else { Ok(true) }
      }
      _ => Ok(false),
    }
  }
}

/// A tuple of types of 2 bytes of the model.
pub type Size2 = (
  u16,
  i16,
  NonZero<u16>,
  Option<NonZero<u16>>,
  u16le,
  u16be,
  u16lec<0x1234>,
  u16bec<0x1234>,
  u16lec<1>,
  u16bec<0x100>,
  u16bec<0x3412>,
  [u8; 2],
  [bool; 2],
  (u8, bool),
  (bool, u8),
  [u8in<0, 0x7f>; 2],
  E16,
  ByteNz,
  NzByte,
  Aligned,
  AlignedFlag,
  PackedU16,
  RustPair,
  NotOneOne,
  Opt,
  OptFlag,
  Either,
  EitherC,
  Skewed,
  OptSoftBit,
  [E01; 2],
  (u8c<1>, u8),
);

// --- Size 3 --------------------------------------------------------------------------------------

model_struct!(
  #[repr(C, packed)]
  struct PackedU16Bool {
    a: u16,
    b: bool,
  }
);
model_struct!(
  #[repr(C, packed)]
  struct PackedBoolNz {
    a: bool,
    b: NonZero<u16>,
  }
);
model_struct!(
  #[repr(C)]
  struct Triple {
    a: u8,
    b: bool,
    c: u8in<1, 0xfe>,
  }
);
model_tagged!(#[repr(u8)] enum Wide: tag 1 { A(u16le = u16le::new(0)) = 0, B(bool = false, bool = false) = 1 });
model_tagged!(#[repr(u8)] enum WideBe: tag 1 { A(u16be = u16be::new(0)) = 0, B(u8 = 0, u8 = 0) = 1 });
model_tagged!(#[repr(u8)] enum Lit: tag 1 { A(u16lec<0x1234> = u16lec::new()) = 0, B(u16bec<0x1234> = u16bec::new()) = 1 });

/// A tuple of types of 3 bytes of the model.
pub type Size3 = (
  [u8; 3],
  [bool; 3],
  (u8, u8, bool),
  u24le,
  u24be,
  u24lec<0x10>,
  u24bec<0x10>,
  PackedU16Bool,
  PackedBoolNz,
  Triple,
  Wide,
  WideBe,
  Lit,
  [E01; 3],
  (u8, [u8in<0x80, 0xff>; 2]),
);

// --- Size 4 --------------------------------------------------------------------------------------

model_struct!(
  #[repr(C)]
  struct PadMid {
    a: u8,
    b: u16,
  }
);
model_struct!(
  #[repr(C)]
  struct PadEnd {
    a: u16,
    b: u8,
  }
);
model_struct!(
  #[repr(C)]
  struct PadEndFlag {
    a: u16,
    b: bool,
  }
);
model_fieldless!(
  #[repr(C)]
  enum CEnum {
    A,
    B = 0x10_ffff,
    C = 0xd800,
  }
);
model_tagged!(#[repr(u8)] enum Shape: tag 1 { Circle(u16 = 0) = 0, Rect(u8 = 0, bool = false) = 1 });
model_tagged!(#[repr(u8)] enum Drawing: tag 1 { Circle(u16 = 0) = 0, Rect(u8 = 0, u8 = 0) = 1, Nothing() = 2 });
model_tagged!(#[repr(C)] enum CTagged: tag 4, implicit { A(u8 = 0) = 0, B(bool = false) = 1 });
model_tagged!(#[repr(C, u8)] enum CU8: tag 1 { A(u16 = 0) = 0, B(u8 = 0) = 1 });
model_tagged!(#[repr(u16)] enum Tag16: tag 2 { A(u8 = 0) = 0x100, B(u16 = 0) = 3 });

/// A struct whose rule reads its byte of padding.
///
/// With `target`, its oracle marks that byte as read, so a check of the rule reads padding.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(le(1, 1) == 0))]
pub struct ReadsPad {
  pub a: u8,
  pub b: u16,
}

impl Oracle for ReadsPad {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    let fields = all(&[u8::check(b, off, reads, target), u16::check(b, off + 2, reads, target)]);
    if !target {
      return fields;
    }
    // What the rule reads.
    let rule = require(off, "le(1, 1) == 0", int(b, off + 1, 1, true, reads).map(|pad| pad == 0));
    match (fields, rule) {
      (Err(Uninit), _) | (_, Err(Uninit)) => Err(Uninit),
      (Ok(valid), Ok(rule)) => Ok(valid && rule),
    }
  }
}

/// A tuple of types of 4 bytes of the model.
pub type Size4 = (
  u32,
  i32,
  f32,
  char,
  NonZero<u32>,
  Option<NonZero<u32>>,
  u32le,
  u32be,
  u32lec<0x10_ffff>,
  u32bec<0xd800>,
  [u8; 4],
  [bool; 4],
  [u16be; 2],
  (u8, u16),
  PadMid,
  PadEnd,
  PadEndFlag,
  CEnum,
  Shape,
  Drawing,
  CU8,
  Tag16,
  ReadsPad,
  [u8in<0, 0x7f>; 4],
  (char,),
  [NonZero<u16>; 2],
);

// --- Size 8 --------------------------------------------------------------------------------------

model_tagged!(#[repr(C)] enum CTaggedB: tag 4, implicit { A(bool = false) = 0, B(u8 = 0) = 1, C(u8in<0, 1> = u8in::new(0).unwrap()) = 2 });

/// A tuple of types of 8 bytes of the model.
pub type Size8 = (
  u64,
  [u8; 8],
  [bool; 8],
  [u32; 2],
  (u32, u32),
  CTagged,
  CTaggedB,
  [Shape; 2],
  [Drawing; 2],
  (u16, u8, bool, u32),
);

// --- Uninitialized bytes -------------------------------------------------------------------------

/// A byte that a target requires to be 0, and of which a source guarantees nothing.
///
/// A source does not even guarantee that the byte is initialized, because the type holds a
/// `MaybeUninit<u8>`. `Spec::maybe_uninit` describes such a scalar.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Zero(pub core::mem::MaybeUninit<u8>);

// SAFETY: `Zero` is `repr(transparent)` over `MaybeUninit<u8>`, so `SPEC`, of a `u8`, has its
// size. The items of the `# Safety` of `Scalar` hold:
// - `SPEC` is made with `Spec::maybe_uninit`, so a value of `Zero` may hold any byte,
//   uninitialized included.
// - `SPEC` accepts 0, which is a valid `MaybeUninit<u8>`. `Zero` has no other invariant.
// - `Zero` holds no `UnsafeCell`.
unsafe impl puns::Scalar for Zero {
  const SPEC: puns::layout::Spec =
    puns::layout::Spec::ranges::<u8>(puns::layout::Endian::Little, &[(0, 0)]).maybe_uninit();
}

impl Oracle for Zero {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    // As a source, anything; what a target requires, a requirement of the library.
    if !target {
      return Ok(true);
    }
    require(off, "Zero", int(b, off, 1, true, reads).map(|v| v == 0))
  }
}

// --- Scalars that a pun never makes -------------------------------------------------------------

/// A byte whose validity is not in its bytes, as the offset of a self-relative pointer of another
/// crate: a pun reads it as an initialized byte, and never makes it.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Opaque(pub u8);

// SAFETY: `Opaque` is `repr(transparent)` over a `u8`, so `SPEC`, of a `u8`, has its size. The
// items of the `# Safety` of `Scalar` hold:
// - `SPEC` is made with `Spec::never`, and every byte of every value of `Opaque` is initialized.
// - `SPEC` accepts no value.
// - `Opaque` holds no `UnsafeCell`.
unsafe impl puns::Scalar for Opaque {
  const SPEC: puns::layout::Spec = puns::layout::Spec::never::<u8>();
}

impl Oracle for Opaque {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    // As a target, a requirement that no byte meets, which reads none: only the crate of the
    // scalar makes one. As a source, an initialized byte.
    if target {
      return require(off, "Opaque", Ok(false));
    }
    int(b, off, 1, true, reads)?;
    Ok(true)
  }
}

/// A byte that a pun never makes, as `Opaque`, with a niche at zero, as an offset that is never zero.
/// Its `Option` is one too, `None` as zero.
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct OpaqueNz(pub NonZero<u8>);

// SAFETY: `OpaqueNz` is `repr(transparent)` over a `NonZero<u8>`, so `SPEC`, of a `u8`, has its
// size. The items of the `# Safety` of `Scalar` hold:
// - `SPEC` is made with `Spec::never`, and every byte of every value of `OpaqueNz` is initialized.
// - `SPEC` accepts no value.
// - `OpaqueNz` holds no `UnsafeCell`.
unsafe impl puns::Scalar for OpaqueNz {
  const SPEC: puns::layout::Spec = puns::layout::Spec::never::<u8>();
}

impl Oracle for OpaqueNz {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    // As `Opaque`; as a source, a byte that is not zero.
    if target {
      return require(off, "OpaqueNz", Ok(false));
    }
    Ok(int(b, off, 1, true, reads)? != 0)
  }
}

impl Oracle for Option<OpaqueNz> {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    // As `Opaque`: `None` is zero and `Some` any other byte, so as a source, any initialized byte.
    const { assert!(size_of::<Option<OpaqueNz>>() == 1, "`None` is the niche of `OpaqueNz`") };
    if target {
      return require(off, "Option<OpaqueNz>", Ok(false));
    }
    int(b, off, 1, true, reads)?;
    Ok(true)
  }
}

model_struct!(
  /// A struct of a `bool`, then a byte that a pun never makes, or zero.
  #[repr(C)]
  struct HoldsOptOpaque {
    b: bool,
    o: Option<OpaqueNz>,
  }
);

model_struct!(
  /// A struct of a `bool`, then a byte that a pun never makes.
  #[repr(C)]
  struct HoldsOpaque {
    b: bool,
    o: Opaque,
  }
);

/// An enum whose second variant holds a byte that a pun never makes.
///
/// A check accepts `Empty` only.
#[derive(Pun, Clone, Copy)]
#[repr(u8)]
pub enum MaybeOpaque {
  Empty,
  Full(Opaque),
}

impl Oracle for MaybeOpaque {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    // The tag, then the byte of `Full`; the second byte of `Empty` is padding.
    match int(b, off, 1, true, reads)? {
      0 => Ok(true),
      1 => Opaque::check(b, off + 1, reads, target),
      _ => Ok(false),
    }
  }
}

impl<T: Pun + Copy + 'static> Oracle for core::mem::MaybeUninit<T> {
  fn check(_: &Bytes, _: usize, _: &mut [bool], _: bool) -> Valid {
    Ok(true)
  }
}

model_struct!(
  /// A struct of a byte, then a byte that may be uninitialized.
  #[repr(C)]
  struct ByteZero {
    a: u8,
    z: Zero,
  }
);

// --- Types without values ------------------------------------------------------------------------

/// An enum with a variant that holds a type without values.
///
/// No value is that variant.
#[derive(Pun, Clone, Copy)]
#[repr(u8)]
pub enum Never {
  A(u8),
  B(core::convert::Infallible),
}

impl Oracle for Never {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], _: bool) -> Valid {
    // `A(u8)`: the tag, then the byte; the tag of `B` is no value.
    let tag = int(b, off, 1, true, reads)?;
    if tag != 0 {
      return Ok(false);
    }
    int(b, off + 1, 1, true, reads)?;
    Ok(true)
  }
}

// --- Enums of the `Rust` representation ----------------------------------------------------------
//
// Their oracles are written from how `rustc` lays enums out today (a direct tag in the bytes before
// the fields of every variant, or a niche: the values that follow the valid ones of a scalar of a
// field). The offsets of the fields are taken from values, never from the library. The values of
// the tags are written here: if `rustc` chooses others, the model check reports a disagreement.

/// Returns the offset of `field` in `value`.
///
/// # Panics
///
/// Panics if `field` starts before `value` and overflow checks are enabled.
fn offset_in<T, F>(value: &T, field: &F) -> usize {
  core::ptr::from_ref(field).addr() - core::ptr::from_ref(value).addr()
}

/// An enum of the `Rust` representation with a niche in a `bool`, where `Nothing` is 2.
#[derive(Pun, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaybeFlag {
  Nothing,
  Flag(bool),
}

impl Oracle for MaybeFlag {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], _: bool) -> Valid {
    Ok(int(b, off, 1, true, reads)? <= 2)
  }
}

/// An enum of the `Rust` representation with a niche in a `NonZero`, where `No` is 0.
///
/// Every byte is a value.
#[derive(Pun, Clone, Copy, Debug, PartialEq, Eq)]
pub enum NzOpt {
  No,
  Yes(NonZero<u8>),
}

impl Oracle for NzOpt {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], _: bool) -> Valid {
    int(b, off, 1, true, reads)?;
    Ok(true)
  }
}

/// An enum of the `Rust` representation with a direct tag of one byte, and a byte in each variant.
#[derive(Pun, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pair {
  A(u8),
  B(u8),
}

impl Oracle for Pair {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], _: bool) -> Valid {
    let (a, b_) = (Pair::A(0), Pair::B(0));
    let (Pair::A(x), Pair::B(y)) = (&a, &b_) else { unreachable!() };
    match int(b, off, 1, true, reads)? {
      0 => int(b, off + offset_in(&a, x), 1, true, reads).map(|_| true),
      1 => int(b, off + offset_in(&b_, y), 1, true, reads).map(|_| true),
      _ => Ok(false),
    }
  }
}

/// An enum of the `Rust` representation with a niche in the tag of another enum of that
/// representation.
///
/// `Other` is 2, the first value that is not a tag of `Pair`.
#[derive(Pun, Clone, Copy, Debug, PartialEq, Eq)]
pub enum NestPair {
  Inner(Pair),
  Other,
}

impl Oracle for NestPair {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    let (inner, other) = (NestPair::Inner(Pair::A(0)), NestPair::Other);
    let _ = other;
    let NestPair::Inner(pair) = &inner else { unreachable!() };
    let at = off + offset_in(&inner, pair);
    match int(b, at, 1, true, reads)? {
      0 | 1 => Pair::check(b, at, reads, target),
      2 => Ok(true),
      _ => Ok(false),
    }
  }
}

/// An enum of the `Rust` representation with a niche in the `bool` of `Both`.
///
/// In that byte, `Byte` is 2 and `Nothing` is 3. The field of `Byte` is in another byte.
#[derive(Pun, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mixed {
  Both(u8, bool),
  Byte(u8),
  Nothing,
}

impl Oracle for Mixed {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], _: bool) -> Valid {
    let (both, byte) = (Mixed::Both(0, false), Mixed::Byte(0));
    let (Mixed::Both(x, flag), Mixed::Byte(y)) = (&both, &byte) else { unreachable!() };
    match int(b, off + offset_in(&both, flag), 1, true, reads)? {
      0 | 1 => int(b, off + offset_in(&both, x), 1, true, reads).map(|_| true),
      2 => int(b, off + offset_in(&byte, y), 1, true, reads).map(|_| true),
      3 => Ok(true),
      _ => Ok(false),
    }
  }
}

/// An enum of the `Rust` representation with one variant, laid out as a struct whose fields `rustc`
/// may reorder.
#[derive(Pun, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Single {
  Only(u8, bool),
}

impl Oracle for Single {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], _: bool) -> Valid {
    let only = Single::Only(0, false);
    let Single::Only(x, flag) = &only;
    let (x, flag) = (offset_in(&only, x), offset_in(&only, flag));
    all(&[
      int(b, off + x, 1, true, reads).map(|_| true),
      int(b, off + flag, 1, true, reads).map(|v| v <= 1),
    ])
  }
}

/// An enum of the `Rust` representation with a variant of two `bool`s.
///
/// The first `bool` holds the niche of `B`, which is 2.
#[derive(Pun, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flags2 {
  A(bool, bool),
  B,
}

impl Oracle for Flags2 {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], _: bool) -> Valid {
    let a = Flags2::A(false, false);
    let Flags2::A(first, second) = &a else { unreachable!() };
    match int(b, off + offset_in(&a, first), 1, true, reads)? {
      0 | 1 => int(b, off + offset_in(&a, second), 1, true, reads).map(|v| v <= 1),
      2 => Ok(true),
      _ => Ok(false),
    }
  }
}

/// An enum of the `Rust` representation with a niche in a `char`.
///
/// `End` and `Error` are the two values after the last `char`.
#[derive(Pun, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Letter {
  Char(char),
  End,
  Error,
}

impl Oracle for Letter {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], _: bool) -> Valid {
    let v = int(b, off, 4, crate::NATIVE_LITTLE, reads)?;
    Ok(v <= 0xd7ff || (0xe000..=0x11_0001).contains(&v))
  }
}

/// An enum of the `Rust` representation with a direct tag that `rustc` widens to two bytes, the
/// alignment of the fields that follow it.
#[derive(Pun, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wid {
  A(u16),
  B(u16),
}

impl Oracle for Wid {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], _: bool) -> Valid {
    let a = Wid::A(0);
    let Wid::A(x) = &a else { unreachable!() };
    match int(b, off, 2, crate::NATIVE_LITTLE, reads)? {
      0 | 1 => int(b, off + offset_in(&a, x), 2, true, reads).map(|_| true),
      _ => Ok(false),
    }
  }
}

/// An enum of the `Rust` representation with a variant that has values and a variant that has none.
///
/// The second variant holds a field with a size, so `rustc` gives the enum a tag.
#[derive(Pun, Clone, Copy, Debug, PartialEq, Eq)]
pub enum OneOfTwo {
  A(u8),
  B(u32, core::convert::Infallible),
}

impl Oracle for OneOfTwo {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], _: bool) -> Valid {
    let a = OneOfTwo::A(0);
    let OneOfTwo::A(x) = &a else { unreachable!() };
    match int(b, off, 1, true, reads)? {
      0 => int(b, off + offset_in(&a, x), 1, true, reads).map(|_| true),
      _ => Ok(false),
    }
  }
}

// --- Unions --------------------------------------------------------------------------------------
//
// The validity of unions is not settled by the language: the oracle takes the rule that every
// proposal allows, valid if valid for one field. What the library knows of a union as a source is
// nothing, which is less than that.

/// The oracle of a field of a union.
type Check = fn(&Bytes, usize, &mut [bool], bool) -> Valid;

/// Returns whether one of the fields of a union is valid, trying the oracles of `checks` in order.
///
/// The bytes read are those of the fields tried, up to the first valid one. The bytes of the
/// fields after it are not read, because they may be padding of that value. A field decided by an
/// uninitialized byte decides the union. The fields of each union of the model start with the same
/// bytes, so an uninitialized byte decides the same for every field.
///
/// With `target`, the result is recorded as one requirement of the union ([`require_whole`]).
///
/// # Errors
///
/// Returns [`Uninit`] if a field is decided by an uninitialized byte before a field is valid.
pub fn any(b: &Bytes, off: usize, reads: &mut [bool], target: bool, checks: &[Check]) -> Valid {
  let mut one = || {
    for check in checks {
      match check(b, off, reads, target) {
        Ok(true) => return Ok(true),
        Ok(false) => {}
        Err(Uninit) => return Err(Uninit),
      }
    }
    Ok(false)
  };
  // What a union requires is that one of its fields is valid, requirements included: one
  // requirement, not those of each field, which a value that is another field need not meet.
  if target { require_whole(off, "one of the fields of a union", one) } else { one() }
}

/// A union of a `bool` and of a number up to 3.
///
/// Its valid values are the bytes up to 3.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
pub union ChoiceU {
  pub flag: bool,
  pub small: u8in<0, 3>,
}

impl Oracle for ChoiceU {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    any(b, off, reads, target, &[bool::check, <u8in<0, 3>>::check])
  }
}

/// A union of two `bool`s and of a `NonZero<u16>`.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
pub union FlagsU {
  pub pair: [bool; 2],
  pub nz: NonZero<u16>,
}

impl Oracle for FlagsU {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    any(b, off, reads, target, &[<[bool; 2]>::check, <NonZero<u16>>::check])
  }
}

/// A union of a byte and of a pair of a byte and a `u16`.
///
/// A value of the first field has three bytes of padding.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
pub union PaddedU {
  pub byte: u8,
  pub pair: (u8, u16),
}

impl Oracle for PaddedU {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    any(b, off, reads, target, &[u8::check, <(u8, u16)>::check])
  }
}

// --- Rules proven by the same rules --------------------------------------------------------------
//
// A rule of the target is proven by the same rule of the source in a struct of the same extent at
// the same place, whatever the fields: these are such structs, and ones that are not (another
// rule, another extent), in each kind of type that can hold one. Their oracles say what each
// requires, one requirement at a time (`require`): what the source implies, the oracle finds.

/// The rule of `Third`, over a field with a requirement of its own, `valid(..=200)`.
///
/// A requirement is no niche for `rustc`, so an enum that holds a `ThirdSoft` has the layout of one
/// that holds a `Third`.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(v % 3 == 0))]
pub struct ThirdSoft {
  #[pun(valid(..=200))]
  pub v: u8,
}

impl Oracle for ThirdSoft {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    let v = int(b, off, 1, true, reads)?;
    if !target {
      return Ok(true);
    }
    all(&[
      require(off, "ThirdSoft: v <= 200", Ok(v <= 200)),
      require(off, "ThirdSoft: v % 3", Ok(v % 3 == 0)),
    ])
  }
}

/// A struct with another rule over one byte, which requires `v` to be a multiple of 5.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(v % 5 == 0))]
pub struct Fifth {
  pub v: u8,
}

impl Oracle for Fifth {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    let v = int(b, off, 1, true, reads)?;
    if target { require(off, "Fifth: v % 5", Ok(v % 5 == 0)) } else { Ok(true) }
  }
}

/// A struct with a rule that reads the size of the value, and requires its last byte not to be
/// `0xff`.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(be(size_of_val(self) - 1, 1) != 0xff))]
pub struct LastNotFf1 {
  pub v: u8,
}

impl Oracle for LastNotFf1 {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    let v = int(b, off, 1, true, reads)?;
    if target { require(off, "LastNotFf1: last byte", Ok(v != 0xff)) } else { Ok(true) }
  }
}

/// The rule of `LastNotFf1`, over 2 bytes.
///
/// Over 2 bytes, the same rule is another function of the bytes. So the rule of a part of 1 byte is
/// not this rule.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(be(size_of_val(self) - 1, 1) != 0xff))]
pub struct LastNotFf2 {
  pub a: u8,
  pub b: u8,
}

impl Oracle for LastNotFf2 {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    int(b, off, 1, true, reads)?;
    let last = int(b, off + 1, 1, true, reads)?;
    if target { require(off, "LastNotFf2: last byte", Ok(last != 0xff)) } else { Ok(true) }
  }
}

/// The rule of `NotOneOne`, over a first field that is a `NonZero<u8>`.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(be(0, 2) != 0x0101))]
pub struct NotOneOneNz {
  pub a: NonZero<u8>,
  pub b: u8,
}

impl Oracle for NotOneOneNz {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    let v = int(b, off, 2, false, reads)?;
    if v >> 8 == 0 {
      return Ok(false);
    }
    if target { require(off, "NotOneOneNz: not 0x0101", Ok(v != 0x0101)) } else { Ok(true) }
  }
}

model_tagged!(#[repr(u8)] enum ThirdIn: tag 1 { A(Third = Third { v: 0 }) = 0, B(u8 = 0) = 1 });
model_tagged!(#[repr(u8)] enum ThirdSoftIn: tag 1 { A(ThirdSoft = ThirdSoft { v: 0 }) = 0, B(u8 = 0) = 1 });

/// Returns the index of the variant of the enum `E` at `off`, and the offset of its field.
///
/// `E` is of the `Rust` representation, and two variants of one byte each. `samples` holds a value
/// of each variant and the offset of its field. `E` must have a direct tag of one byte at offset 0,
/// whose value is the index of the variant, as the bytes of the samples show. Returns `None` if the
/// tag is not the tag of a variant.
///
/// # Errors
///
/// Returns [`Uninit`] if the tag is uninitialized.
///
/// # Panics
///
/// Panics if `E` is not 2 bytes, or if the tags of the samples are not 0 and 1.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - Both bytes of each sample are initialized.
unsafe fn rust_variant<E: Copy>(
  samples: [(E, usize); 2],
  b: &Bytes,
  off: usize,
  reads: &mut [bool],
) -> Result<Option<(usize, usize)>, Uninit> {
  assert_eq!(size_of::<E>(), 2, "not the layout of the oracle");
  for (i, (sample, _)) in samples.iter().enumerate() {
    // SAFETY: `E` is 2 bytes, which is asserted above, so `transmute_copy` reads the 2 bytes of
    // `sample` as a `[u8; 2]`, which needs no alignment. Both are initialized, by contract on the
    // caller, so they are a valid `[u8; 2]`.
    let bytes: [u8; 2] = unsafe { core::mem::transmute_copy(sample) };
    assert_eq!(usize::from(bytes[0]), i, "not the layout of the oracle");
  }
  let tag = int(b, off, 1, true, reads)? as usize;
  Ok(samples.get(tag).map(|(_, field)| (tag, *field)))
}

/// An enum of the `Rust` representation whose first variant holds a struct with a rule.
///
/// `rustc` gives it a direct tag.
#[derive(Pun, Clone, Copy)]
pub enum RustThird {
  A(Third),
  B(u8),
}

impl Oracle for RustThird {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    let (a, b_) = (RustThird::A(Third { v: 0 }), RustThird::B(0));
    let (RustThird::A(x), RustThird::B(y)) = (&a, &b_) else { unreachable!() };
    let samples = [(a, offset_in(&a, x)), (b_, offset_in(&b_, y))];
    // SAFETY: Each sample is a tag and a field of 1 byte, a `Third` or a `u8`, in 2 bytes. So both
    // of its bytes are initialized, as `rust_variant` requires. If `rustc` gave the enum another
    // layout, a byte could be padding, which Miri reports.
    match unsafe { rust_variant(samples, b, off, reads) }? {
      Some((0, at)) => Third::check(b, off + at, reads, target),
      Some((_, at)) => int(b, off + at, 1, true, reads).map(|_| true),
      None => Ok(false),
    }
  }
}

/// An enum like `RustThird`, whose first variant holds a `ThirdSoft`.
#[derive(Pun, Clone, Copy)]
pub enum RustThirdSoft {
  A(ThirdSoft),
  B(u8),
}

impl Oracle for RustThirdSoft {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    let (a, b_) = (RustThirdSoft::A(ThirdSoft { v: 0 }), RustThirdSoft::B(0));
    let (RustThirdSoft::A(x), RustThirdSoft::B(y)) = (&a, &b_) else { unreachable!() };
    let samples = [(a, offset_in(&a, x)), (b_, offset_in(&b_, y))];
    // SAFETY: Each sample is a tag and a field of 1 byte, a `ThirdSoft` or a `u8`, in 2 bytes. So
    // both of its bytes are initialized, as `rust_variant` requires. If `rustc` gave the enum
    // another layout, a byte could be padding, which Miri reports.
    match unsafe { rust_variant(samples, b, off, reads) }? {
      Some((0, at)) => ThirdSoft::check(b, off + at, reads, target),
      Some((_, at)) => int(b, off + at, 1, true, reads).map(|_| true),
      None => Ok(false),
    }
  }
}

/// A struct with the rule of `Third`, and a byte that may be uninitialized.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(v % 3 == 0))]
pub struct ThirdUninit {
  pub v: u8,
  pub z: core::mem::MaybeUninit<u8>,
}

impl Oracle for ThirdUninit {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    let v = int(b, off, 1, true, reads)?;
    if target { require(off, "ThirdUninit: v % 3", Ok(v % 3 == 0)) } else { Ok(true) }
  }
}

/// A struct with the rule and the field of `ThirdSoft`, and a byte that may be uninitialized.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(v % 3 == 0))]
pub struct ThirdSoftUninit {
  #[pun(valid(..=200))]
  pub v: u8,
  pub z: core::mem::MaybeUninit<u8>,
}

impl Oracle for ThirdSoftUninit {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    let v = int(b, off, 1, true, reads)?;
    if !target {
      return Ok(true);
    }
    all(&[
      require(off, "ThirdSoftUninit: v <= 200", Ok(v <= 200)),
      require(off, "ThirdSoftUninit: v % 3", Ok(v % 3 == 0)),
    ])
  }
}

/// A union of a `Third` and of a `bool`.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
pub union ThirdU {
  pub third: Third,
  pub flag: bool,
}

impl Oracle for ThirdU {
  fn check(b: &Bytes, off: usize, reads: &mut [bool], target: bool) -> Valid {
    any(b, off, reads, target, &[Third::check, bool::check])
  }
}
