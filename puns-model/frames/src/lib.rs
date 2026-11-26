//! The checks in `const`s of the frames test of `puns-model` (`tests/frames.rs`).
//!
//! Each constant checks data of one shape, of a depth, after as many frames of evaluation of its
//! own as `PUNS_FRAMES_PADS` gives it: the test finds the most that leave the check within the 128
//! frames that Rust allows by default, so how many the check takes. Without the variable, the
//! constants check nothing, so that the types build where the test reads them.
//!
//! The types are of alignment 1, the only one of which a check in a `const` takes pointees. Each
//! value points at a list of values of its type, `off` bytes from its start, of `n` values: the
//! list of the value below it, or an empty list in the last one, which then has the same deepest
//! calls as the others. `Chain` is a chain of links instead, whose last value is deeper than the
//! others.

use core::marker::PhantomData;

use puns::{InPlace, Pun, Thin};

/// The pads of the constants, in their order, separated by commas: 0 for those that it does not
/// give.
const PADS: &str = match option_env!("PUNS_FRAMES_PADS") {
  Some(pads) => pads,
  None => "",
};

/// Returns the pad of the constant `case` in `PADS`.
const fn pad(case: usize) -> usize {
  let bytes = PADS.as_bytes();
  let (mut i, mut at, mut value) = (0, 0, 0);
  while i < bytes.len() {
    if bytes[i] == b',' {
      if at == case {
        return value;
      }
      (at, value) = (at + 1, 0);
    } else {
      value = value * 10 + (bytes[i] - b'0') as usize;
    }
    i += 1;
  }
  if at == case { value } else { 0 }
}

/// The bytes of the data of a constant, which is shorter.
type Bytes = [u8; 256];

/// Defines `$check(case, depth)`, which checks the data of `$bytes(depth)` as a `$ty` after the
/// frames of `pad(case)` calls of `$padded`.
macro_rules! shape {
  ($check:ident, $padded:ident, $ty:ty, $bytes:ident) => {
    #[doc = concat!("Checks `", stringify!($bytes), "(depth)` as a `", stringify!($ty), "`.")]
    const fn $check(case: usize, depth: usize) {
      if !PADS.is_empty() {
        $padded(pad(case), &$bytes(depth));
      }
    }

    /// Takes a frame for each of `pad`, then checks `bytes`.
    const fn $padded(pad: usize, bytes: &Bytes) {
      if pad > 0 {
        return $padded(pad - 1, bytes);
      }
      assert!(puns::from_head::<$ty>(bytes).is_ok(), "not valid");
    }
  };
}

/// The list of `n` values of `T`, `off` bytes from the start of this struct.
#[derive(Pun)]
#[pun(bound(T: Pun))]
#[repr(C)]
pub struct Kids<T> {
  #[pun(pointer([T] = off as u128, len = n as u128))]
  pub off: InPlace<u8>,
  pub n: u8,
  pub _t: PhantomData<T>,
}

/// A value whose aim is its own rule, at its first field.
#[derive(Pun)]
#[repr(C)]
pub struct Nest {
  #[pun(pointer([Nest] = off as u128, len = n as u128))]
  pub off: InPlace<u8>,
  pub n: u8,
}

/// Values of 2 bytes: each points 2 bytes ahead, at 1 value, the last at none.
const fn nest_bytes(depth: usize) -> Bytes {
  let mut bytes = [0; 256];
  let mut i = 0;
  while i <= depth {
    (bytes[2 * i], bytes[2 * i + 1]) = (2, (i < depth) as u8);
    i += 1;
  }
  bytes
}

shape!(nest, nest_padded, Nest, nest_bytes);

/// A value whose list is its third field, a struct.
#[derive(Pun)]
#[repr(C)]
pub struct Fields {
  pub a: u8,
  pub b: u8,
  pub kids: Kids<Fields>,
}

/// Values of 4 bytes, whose lists are 2 bytes after the start of `Kids`.
const fn fields_bytes(depth: usize) -> Bytes {
  let mut bytes = [0; 256];
  let mut i = 0;
  while i <= depth {
    (bytes[4 * i + 2], bytes[4 * i + 3]) = (2, (i < depth) as u8);
    i += 1;
  }
  bytes
}

shape!(fields, fields_padded, Fields, fields_bytes);

/// A value whose list is in its first variant of four.
#[derive(Pun)]
#[repr(u8)]
pub enum Variants {
  Kids(Kids<Variants>),
  A(u8),
  B(u8, u8),
  C,
}

/// Values of 3 bytes: the tag of `Kids`, then its list 2 bytes after it.
const fn variants_bytes(depth: usize) -> Bytes {
  let mut bytes = [0; 256];
  let mut i = 0;
  while i <= depth {
    (bytes[3 * i + 1], bytes[3 * i + 2]) = (2, (i < depth) as u8);
    i += 1;
  }
  bytes
}

shape!(variants, variants_padded, Variants, variants_bytes);

/// A value whose lists are the elements of an array: the first holds the value below, the second
/// is empty, at the end of the data.
#[derive(Pun)]
#[repr(C)]
pub struct Array {
  pub kids: [Kids<Array>; 2],
}

/// Values of 4 bytes: the first list 4 bytes ahead, the second at the end of the data.
const fn array_bytes(depth: usize) -> Bytes {
  let mut bytes = [0; 256];
  let end = 4 * (depth + 1);
  let mut i = 0;
  while i <= depth {
    (bytes[4 * i], bytes[4 * i + 1]) = (4, (i < depth) as u8);
    (bytes[4 * i + 2], bytes[4 * i + 3]) = ((end - 4 * i - 2) as u8, 0);
    i += 1;
  }
  bytes
}

shape!(array, array_padded, Array, array_bytes);

/// A value whose list stores its length before it, in a `Thin`.
#[derive(Pun)]
#[repr(C)]
pub struct Prefixed {
  #[pun(pointer(Thin<[Prefixed], u8> = off as u128))]
  pub off: InPlace<u8>,
}

/// Values of 1 byte, each followed by the length of the list after it.
const fn prefixed_bytes(depth: usize) -> Bytes {
  let mut bytes = [0; 256];
  let mut i = 0;
  while i <= depth {
    (bytes[2 * i], bytes[2 * i + 1]) = (1, (i < depth) as u8);
    i += 1;
  }
  bytes
}

shape!(prefixed, prefixed_padded, Prefixed, prefixed_bytes);

/// A value whose aim is its third rule, after two assertions.
#[derive(Pun)]
#[pun(assert(a != 0), assert(b != 0))]
#[repr(C)]
pub struct Ruled {
  pub a: u8,
  pub b: u8,
  #[pun(pointer([Ruled] = off as u128, len = n as u128))]
  pub off: InPlace<u8>,
  pub n: u8,
}

/// Values of 4 bytes, whose lists are 4 bytes ahead.
const fn ruled_bytes(depth: usize) -> Bytes {
  let mut bytes = [0; 256];
  let mut i = 0;
  while i <= depth {
    bytes[4 * i] = 1;
    bytes[4 * i + 1] = 1;
    (bytes[4 * i + 2], bytes[4 * i + 3]) = (4, (i < depth) as u8);
    i += 1;
  }
  bytes
}

shape!(ruled, ruled_padded, Ruled, ruled_bytes);

/// A link of a chain, or its end, whose check is deeper than that of a link.
#[derive(Pun)]
#[repr(u8)]
pub enum Chain {
  Link(Next),
  End(Deep),
}

/// The next value of a `Chain`, `off` bytes from this struct.
#[derive(Pun)]
#[repr(transparent)]
pub struct Next {
  #[pun(pointer(Chain))]
  pub off: InPlace<u8>,
}

/// A struct around a `T`.
#[derive(Pun)]
#[pun(bound(T: Pun))]
#[repr(C)]
pub struct Around<T> {
  pub around: T,
}

/// 12 structs around a byte.
type Deep = Around<Around<Around<Around<Around<Around<Deeper>>>>>>;

/// The 6 innermost of them.
type Deeper = Around<Around<Around<Around<Around<Around<u8>>>>>>;

/// Links of 2 bytes, each pointing at the next, then the end.
const fn chain_bytes(depth: usize) -> Bytes {
  let mut bytes = [0; 256];
  let mut i = 0;
  while i < depth {
    bytes[2 * i + 1] = 1;
    i += 1;
  }
  bytes[2 * depth] = 1;
  bytes
}

shape!(chain, chain_padded, Chain, chain_bytes);

/// A value with a region after its list.
#[derive(Pun)]
#[repr(C)]
pub struct Region {
  #[pun(pointer([Region] = off as u128, len = n as u128))]
  pub off: InPlace<u8>,
  pub n: u8,
  #[pun(regions(eager inner: Plain))]
  pub tail: InPlace<[u8; 2]>,
}

/// Two bytes, the first less than the second.
#[derive(Pun)]
#[pun(assert(x < y))]
#[repr(C)]
pub struct Plain {
  pub x: u8,
  pub y: u8,
}

/// Values of 4 bytes, whose lists are 4 bytes ahead.
const fn region_bytes(depth: usize) -> Bytes {
  let mut bytes = [0; 256];
  let mut i = 0;
  while i <= depth {
    (bytes[4 * i], bytes[4 * i + 1]) = (4, (i < depth) as u8);
    (bytes[4 * i + 2], bytes[4 * i + 3]) = (1, 2);
    i += 1;
  }
  bytes
}

shape!(region, region_padded, Region, region_bytes);

/// A `Nest` whose aim is the deepest expression: 4 walks, one in another, each of whose
/// expressions has 256 operations, the most that a rule may have. Its value is `off`.
#[derive(Pun)]
#[repr(C)]
pub struct Worst {
  #[pun(pointer([Worst] =
    walk(off as u128, |a| walk(a, |b| walk(b, |c| walk(c, |d| !(d + 1 + 1 + 1 + 1 + 1 + 1 + 1 +
    1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 +
    1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 +
    1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 +
    1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 +
    1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 +
    1 + 1 + 1 + 1 != 7777), |d| d) + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 +
    1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 +
    1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 +
    1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 +
    1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 +
    1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 == 7777, |c| c) +
    1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 +
    1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 +
    1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 +
    1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 +
    1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 +
    1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 == 7777, |b| b) + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 +
    1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 +
    1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 +
    1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 +
    1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 +
    1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 +
    1 + 1 + 1 == 7777, |a| a) + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 +
    0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 +
    0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 +
    0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 +
    0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 +
    0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0 + 0,
    len = n as u128))]
  pub off: InPlace<u8>,
  pub n: u8,
}

shape!(worst, worst_padded, Worst, nest_bytes);

pub const NEST_0: () = nest(0, 0);
pub const NEST_1: () = nest(1, 1);
pub const NEST_2: () = nest(2, 2);
pub const NEST_3: () = nest(3, 3);
pub const NEST_4: () = nest(4, 4);
pub const NEST_5: () = nest(5, 5);
pub const NEST_6: () = nest(6, 6);
pub const NEST_7: () = nest(7, 7);
pub const NEST_8: () = nest(8, 8);
pub const NEST_9: () = nest(9, 9);
pub const NEST_10: () = nest(10, 10);
pub const NEST_11: () = nest(11, 11);
pub const NEST_12: () = nest(12, 12);
pub const NEST_40: () = nest(13, 40);
pub const FIELDS_0: () = fields(14, 0);
pub const FIELDS_1: () = fields(15, 1);
pub const FIELDS_2: () = fields(16, 2);
pub const FIELDS_3: () = fields(17, 3);
pub const FIELDS_4: () = fields(18, 4);
pub const FIELDS_5: () = fields(19, 5);
pub const FIELDS_6: () = fields(20, 6);
pub const FIELDS_7: () = fields(21, 7);
pub const FIELDS_8: () = fields(22, 8);
pub const FIELDS_9: () = fields(23, 9);
pub const FIELDS_10: () = fields(24, 10);
pub const FIELDS_11: () = fields(25, 11);
pub const FIELDS_12: () = fields(26, 12);
pub const FIELDS_40: () = fields(27, 40);
pub const VARIANTS_0: () = variants(28, 0);
pub const VARIANTS_1: () = variants(29, 1);
pub const VARIANTS_2: () = variants(30, 2);
pub const VARIANTS_3: () = variants(31, 3);
pub const VARIANTS_4: () = variants(32, 4);
pub const VARIANTS_5: () = variants(33, 5);
pub const VARIANTS_6: () = variants(34, 6);
pub const VARIANTS_7: () = variants(35, 7);
pub const VARIANTS_8: () = variants(36, 8);
pub const VARIANTS_9: () = variants(37, 9);
pub const VARIANTS_10: () = variants(38, 10);
pub const VARIANTS_11: () = variants(39, 11);
pub const VARIANTS_12: () = variants(40, 12);
pub const VARIANTS_40: () = variants(41, 40);
pub const ARRAY_0: () = array(42, 0);
pub const ARRAY_1: () = array(43, 1);
pub const ARRAY_2: () = array(44, 2);
pub const ARRAY_3: () = array(45, 3);
pub const ARRAY_4: () = array(46, 4);
pub const ARRAY_5: () = array(47, 5);
pub const ARRAY_6: () = array(48, 6);
pub const ARRAY_7: () = array(49, 7);
pub const ARRAY_8: () = array(50, 8);
pub const ARRAY_9: () = array(51, 9);
pub const ARRAY_10: () = array(52, 10);
pub const ARRAY_11: () = array(53, 11);
pub const ARRAY_12: () = array(54, 12);
pub const ARRAY_40: () = array(55, 40);
pub const PREFIXED_0: () = prefixed(56, 0);
pub const PREFIXED_1: () = prefixed(57, 1);
pub const PREFIXED_2: () = prefixed(58, 2);
pub const PREFIXED_3: () = prefixed(59, 3);
pub const PREFIXED_4: () = prefixed(60, 4);
pub const PREFIXED_5: () = prefixed(61, 5);
pub const PREFIXED_6: () = prefixed(62, 6);
pub const PREFIXED_7: () = prefixed(63, 7);
pub const PREFIXED_8: () = prefixed(64, 8);
pub const PREFIXED_9: () = prefixed(65, 9);
pub const PREFIXED_10: () = prefixed(66, 10);
pub const PREFIXED_11: () = prefixed(67, 11);
pub const PREFIXED_12: () = prefixed(68, 12);
pub const PREFIXED_40: () = prefixed(69, 40);
pub const RULED_0: () = ruled(70, 0);
pub const RULED_1: () = ruled(71, 1);
pub const RULED_2: () = ruled(72, 2);
pub const RULED_3: () = ruled(73, 3);
pub const RULED_4: () = ruled(74, 4);
pub const RULED_5: () = ruled(75, 5);
pub const RULED_6: () = ruled(76, 6);
pub const RULED_7: () = ruled(77, 7);
pub const RULED_8: () = ruled(78, 8);
pub const RULED_9: () = ruled(79, 9);
pub const RULED_10: () = ruled(80, 10);
pub const RULED_11: () = ruled(81, 11);
pub const RULED_12: () = ruled(82, 12);
pub const RULED_40: () = ruled(83, 40);
pub const CHAIN_0: () = chain(84, 0);
pub const CHAIN_1: () = chain(85, 1);
pub const CHAIN_2: () = chain(86, 2);
pub const CHAIN_3: () = chain(87, 3);
pub const CHAIN_4: () = chain(88, 4);
pub const CHAIN_5: () = chain(89, 5);
pub const CHAIN_6: () = chain(90, 6);
pub const CHAIN_7: () = chain(91, 7);
pub const CHAIN_8: () = chain(92, 8);
pub const CHAIN_9: () = chain(93, 9);
pub const CHAIN_10: () = chain(94, 10);
pub const CHAIN_11: () = chain(95, 11);
pub const CHAIN_12: () = chain(96, 12);
pub const CHAIN_40: () = chain(97, 40);
pub const REGION_0: () = region(98, 0);
pub const REGION_1: () = region(99, 1);
pub const REGION_2: () = region(100, 2);
pub const REGION_3: () = region(101, 3);
pub const REGION_4: () = region(102, 4);
pub const REGION_5: () = region(103, 5);
pub const REGION_6: () = region(104, 6);
pub const REGION_7: () = region(105, 7);
pub const REGION_8: () = region(106, 8);
pub const REGION_9: () = region(107, 9);
pub const REGION_10: () = region(108, 10);
pub const REGION_11: () = region(109, 11);
pub const REGION_12: () = region(110, 12);
pub const REGION_40: () = region(111, 40);
pub const WORST_0: () = worst(112, 0);
pub const WORST_1: () = worst(113, 1);
pub const WORST_2: () = worst(114, 2);
pub const WORST_3: () = worst(115, 3);
pub const WORST_4: () = worst(116, 4);
pub const WORST_5: () = worst(117, 5);
pub const WORST_6: () = worst(118, 6);
pub const WORST_7: () = worst(119, 7);
pub const WORST_8: () = worst(120, 8);
pub const WORST_9: () = worst(121, 9);
pub const WORST_10: () = worst(122, 10);
pub const WORST_11: () = worst(123, 11);
pub const WORST_12: () = worst(124, 12);
pub const WORST_40: () = worst(125, 40);
