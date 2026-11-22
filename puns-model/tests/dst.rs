//! Checks slices, `str` and structs that end with a slice against oracles written from what the
//! types are.
//!
//! The functions checked are `try_pun_ref`, `pun_ref` and `from_head` from bytes, and the
//! infallible conversions back to bytes. The types include parts without values: a slice of
//! elements without values, which only the empty one is, and a struct that ends with a struct
//! without values, at two depths. They include a string type whose automaton refuses the empty
//! string, alone and after a field. The bytes are every pattern of every length up to 6, over an
//! alphabet that reaches every decision of the types below. Each pattern is placed at an aligned
//! and at a misaligned address. Under Miri, the lengths go up to 3, over a smaller alphabet.

use core::num::NonZero;

use puns::{
  Error, Pun, hook,
  layout::{Dfa, DfaState, Layout},
  num::u8in,
};

#[derive(Pun, Debug)]
#[repr(C)]
struct TailU16 {
  a: u8,
  t: [u16],
}

#[derive(Pun, Debug)]
#[repr(C)]
struct TailU8 {
  a: u16,
  t: [u8],
}

#[derive(Pun, Debug)]
#[repr(C)]
struct TailBool {
  a: bool,
  t: [bool],
}

#[derive(Pun, Debug)]
#[repr(C)]
struct Counted {
  n: u8,
  #[pun(len = n)]
  t: [NonZero<u8>],
}

#[derive(Pun, Debug)]
#[repr(C)]
#[pun(size = size)]
struct Sized_ {
  size: u8,
  t: [u8],
}

#[derive(Pun, Debug)]
#[repr(C, align(4))]
struct ZeroTail {
  a: u8,
  t: [()],
}

/// An element of 1 byte without values.
#[derive(Pun, Debug, Clone, Copy)]
#[repr(C)]
struct Void1 {
  n: core::convert::Infallible,
  x: u8,
}

/// A byte, then elements without values: the byte alone is the only value.
#[derive(Pun, Debug)]
#[repr(C)]
struct VoidTail {
  h: u8,
  items: [Void1],
}

/// A struct without values that ends with a slice.
#[derive(Pun, Debug)]
#[repr(C)]
struct VoidInner {
  v: core::convert::Infallible,
  tail: [u8],
}

/// A byte, then a `VoidInner`: no value.
#[derive(Pun, Debug)]
#[repr(C)]
struct VoidOuter {
  h: u8,
  inner: VoidInner,
}

/// A byte, then a `VoidOuter`: no value.
#[derive(Pun, Debug)]
#[repr(C)]
struct VoidDeep {
  g: u8,
  outer: VoidOuter,
}

/// At least one byte, of any value.
const NON_EMPTY: &Dfa = &Dfa {
  id: u32::from_be_bytes(*b"nonE"),
  states: &[
    DfaState { accept: false, edges: &[(0, 255, 1)] },
    DfaState { accept: true, edges: &[(0, 255, 1)] },
  ],
};

/// A string type whose automaton refuses the empty string: bytes, at least one.
#[derive(Debug)]
#[repr(transparent)]
struct NonEmpty([u8]);

// SAFETY: `Pun` is not to be implemented by hand, and the model does so to check a string type
// whose automaton does not accept the empty string, which only such an implementation makes.
// `NonEmpty` is `repr(transparent)` over `[u8]`, and the model makes a `NonEmpty` only through
// `puns`. So the layout of `Layout::pattern` with `NON_EMPTY`, a tail of bytes at offset 0 that the
// automaton accepts, is its layout. The lists are `()`, and it holds no `UnsafeCell`.
unsafe impl Pun for NonEmpty {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Rules = ();
  const PROBE: *const Self = <[u8] as Pun>::PROBE as *const Self;
  // SAFETY: `NonEmpty` is a struct whose last field is a slice. `Self::PROBE` is
  // `<[u8] as Pun>::PROBE` cast with an `as` cast, and its 3 bytes fit in an `isize`. These are the
  // requirements of `Layout::pattern`.
  const LAYOUT: Layout = unsafe { Layout::pattern::<Self>(Self::PROBE, NON_EMPTY) };
}

/// A kind, then a body of at least one byte.
#[derive(Pun, Debug)]
#[repr(C)]
struct Msg {
  kind: u8,
  body: NonEmpty,
}

/// An oracle of a target that ends with a slice, written from what the type is.
struct Oracle {
  /// The name of the type, by which [`ORACLES`] is searched.
  name: &'static str,
  /// The size of the part before the slice.
  head: usize,
  /// The size of an element of the slice.
  stride: usize,
  /// The alignment of the type.
  align: usize,
  /// Returns whether bytes of a length that fits are a valid value, the slice taking every byte
  /// after the head.
  valid: fn(&[u8]) -> bool,
  /// Returns the size of the value that starts the bytes, for `from_head`, or `None` if the bytes
  /// do not tell it. A type with a rule for its size tells it from that rule.
  prefix: fn(&[u8]) -> Option<usize>,
  /// Whether `puns` may refuse the type from bytes. A refusal is sound, and for these types it is
  /// documented: a slice of elements without values from bytes that could be empty, and a type
  /// without values. A refusal of another type is reported.
  refusable: bool,
}

/// Returns whether `b` is valid UTF-8.
const fn utf8(b: &[u8]) -> bool {
  core::str::from_utf8(b).is_ok()
}

/// Returns the size of a value whose fields cover `extent` bytes, with its trailing padding.
const fn padded(extent: usize, align: usize) -> usize {
  extent.div_ceil(align) * align
}

/// The oracles of the types of this file.
const ORACLES: &[Oracle] = &[
  Oracle {
    name: "[u8]",
    head: 0,
    stride: 1,
    align: 1,
    valid: |_| true,
    prefix: |b| Some(b.len()),
    refusable: false,
  },
  Oracle {
    name: "[bool]",
    head: 0,
    stride: 1,
    align: 1,
    valid: |b| b.iter().all(|&x| x <= 1),
    prefix: |b| Some(b.len()),
    refusable: false,
  },
  Oracle {
    name: "[u16]",
    head: 0,
    stride: 2,
    align: 2,
    valid: |_| true,
    prefix: |b| Some(b.len() / 2 * 2),
    refusable: false,
  },
  Oracle {
    name: "[NonZero<u8>]",
    head: 0,
    stride: 1,
    align: 1,
    valid: |b| b.iter().all(|&x| x != 0),
    prefix: |b| Some(b.len()),
    refusable: false,
  },
  Oracle {
    name: "[u8in<0, 0x7f>]",
    head: 0,
    stride: 1,
    align: 1,
    valid: |b| b.iter().all(|&x| x <= 0x7f),
    prefix: |b| Some(b.len()),
    refusable: false,
  },
  Oracle {
    name: "str",
    head: 0,
    stride: 1,
    align: 1,
    valid: utf8,
    prefix: |b| Some(b.len()),
    refusable: false,
  },
  Oracle {
    name: "TailU16",
    head: 2,
    stride: 2,
    align: 2,
    valid: |_| true,
    prefix: |b| (b.len() >= 2).then(|| 2 + (b.len() - 2) / 2 * 2),
    refusable: false,
  },
  Oracle {
    name: "TailU8",
    head: 2,
    stride: 1,
    align: 2,
    valid: |_| true,
    prefix: |b| (b.len() >= 2).then_some(b.len() & !1),
    refusable: false,
  },
  Oracle {
    name: "TailBool",
    head: 1,
    stride: 1,
    align: 1,
    valid: |b| b.iter().all(|&x| x <= 1),
    prefix: |b| (!b.is_empty()).then_some(b.len()),
    refusable: false,
  },
  Oracle {
    name: "Counted",
    head: 1,
    stride: 1,
    align: 1,
    valid: |b| b[0] as usize == b.len() - 1 && b[1..].iter().all(|&x| x != 0),
    prefix: |b| (!b.is_empty() && (b[0] as usize) < b.len()).then(|| 1 + b[0] as usize),
    refusable: false,
  },
  Oracle {
    name: "Sized_",
    head: 1,
    stride: 1,
    align: 1,
    valid: |b| b[0] as usize == b.len(),
    prefix: |b| (!b.is_empty() && b[0] >= 1 && b[0] as usize <= b.len()).then(|| b[0] as usize),
    refusable: false,
  },
  Oracle {
    name: "ZeroTail",
    head: 1,
    stride: 0,
    align: 4,
    valid: |_| true,
    prefix: |b| (b.len() >= 4).then_some(1),
    refusable: false,
  },
  Oracle {
    name: "NonEmpty",
    head: 0,
    stride: 1,
    align: 1,
    valid: |b| !b.is_empty(),
    prefix: |b| Some(b.len()),
    refusable: false,
  },
  Oracle {
    name: "Msg",
    head: 1,
    stride: 1,
    align: 1,
    valid: |b| b.len() >= 2,
    prefix: |b| (!b.is_empty()).then_some(b.len()),
    refusable: false,
  },
  Oracle {
    name: "[Void1]",
    head: 0,
    stride: 1,
    align: 1,
    valid: <[u8]>::is_empty,
    prefix: |b| Some(b.len()),
    refusable: true,
  },
  Oracle {
    name: "VoidTail",
    head: 1,
    stride: 1,
    align: 1,
    valid: |b| b.len() == 1,
    prefix: |b| (!b.is_empty()).then_some(b.len()),
    refusable: true,
  },
  Oracle {
    name: "VoidOuter",
    head: 1,
    stride: 1,
    align: 1,
    valid: |_| false,
    prefix: |_| None,
    refusable: true,
  },
  Oracle {
    name: "VoidDeep",
    head: 2,
    stride: 1,
    align: 1,
    valid: |_| false,
    prefix: |_| None,
    refusable: true,
  },
];

/// The bytes of the patterns, natively.
const ALPHABET: [u8; 8] = [0, 1, 2, 0x7f, 0x80, 0xc3, 0xa9, 0xff];

/// The bytes of the patterns under Miri, which are at most 3 bytes long.
///
/// Miri is there for undefined behavior, and the native run has every pattern. These bytes still
/// make each type accept and reject. `0` is not a `NonZero`, `1` is a `bool` and a length, and
/// `0xff` is not ASCII, not UTF-8 and not a `bool`.
const MIRI_ALPHABET: [u8; 3] = [0, 1, 0xff];

/// Checks `try_pun_ref`, `pun_ref` and `from_head` of `D` against `oracle`, and returns the number
/// of sources tried.
///
/// The sources are every pattern of up to 6 bytes, at an address that is a multiple of 4 and at
/// one that is not. Under Miri, they are the patterns of up to 3 bytes over [`MIRI_ALPHABET`].
/// Each disagreement is pushed to `errors`. Only success must agree with the oracle, because the
/// library may tell the errors apart in another order. A `pun_ref` that compiles must give a valid
/// value of every pattern. If `try_pun_ref::<[u8], D>` does not compile, which is also when
/// `from_head::<D>` does not, nothing is tried, and the refusal is an error unless the oracle says
/// that it may be refused.
fn check<D: ?Sized + Pun + core::fmt::Debug>(oracle: &Oracle, errors: &mut Vec<String>) -> usize {
  if !const { hook::verdict::<[u8], D>() }.try_pun_ref {
    if !oracle.refusable {
      errors.push(format!("try_pun_ref::<[u8], {}> does not compile", oracle.name));
    }
    return 0;
  }
  let mut count = 0;
  let alphabet: &[u8] = if cfg!(miri) { &MIRI_ALPHABET } else { &ALPHABET };
  for len in 0..=if cfg!(miri) { 3 } else { 6 } {
    let mut pattern = vec![0usize; len];
    loop {
      let bytes: Vec<u8> = pattern.iter().map(|&i| alphabet[i]).collect();
      for misalign in [0usize, 1] {
        // A buffer that is a multiple of 4 bytes, holding the bytes at `misalign`.
        let mut words = [0u32; 4];
        let start = {
          let all: &mut [u8; 16] = puns::pun_mut(&mut words);
          all[misalign..misalign + len].copy_from_slice(&bytes);
          misalign
        };
        let all: &[u8; 16] = puns::pun_ref(&words);
        let source = &all[start..start + len];
        count += 1;
        // `try_pun_ref`: the whole bytes.
        let fits = len >= oracle.head
          && if oracle.stride == 0 {
            len == oracle.head
          } else {
            (len - oracle.head).is_multiple_of(oracle.stride)
          };
        let room = padded(len, oracle.align) <= len;
        let aligned = start % oracle.align == 0;
        let want = match () {
          () if !aligned => Err(Error::Alignment),
          () if !fits || !room => Err(Error::Size),
          () if !(oracle.valid)(&bytes) => Err(Error::Validity),
          () => Ok(()),
        };
        if hook::pun_ref::<[u8], D>(source).is_some() && want.is_err() {
          errors.push(format!(
            "pun_ref::<[u8], {}> compiles, but {bytes:02x?} at +{start}: want {want:?}",
            oracle.name
          ));
        }
        let got = hook::try_pun_ref::<[u8], D>(source).expect("compiles").map(|_| ());
        // The library may tell the errors apart in another order: only `Ok` must match.
        if got.is_ok() != want.is_ok() {
          errors.push(format!(
            "try_pun_ref::<[u8], {}>({bytes:02x?} at +{start}) = {got:?}, want {want:?}",
            oracle.name
          ));
        }
        if got == Ok(()) {
          let value = hook::try_pun_ref::<[u8], D>(source).unwrap().unwrap();
          if core::mem::size_of_val(value) != padded(len, oracle.align) {
            errors.push(format!(
              "{}: size_of_val {} for {len} bytes",
              oracle.name,
              core::mem::size_of_val(value)
            ));
          }
        }
        // `from_head`: the value that starts the bytes.
        let want = match (oracle.prefix)(&bytes) {
          Some(size)
            if aligned && padded(size, oracle.align) <= len && (oracle.valid)(&bytes[..size]) =>
          {
            Some(size)
          }
          _ => None,
        };
        match hook::from_head::<D>(source).expect("compiles") {
          Ok((value, rest)) => {
            let size = core::mem::size_of_val(value);
            if want.map(|w| padded(w, oracle.align)) != Some(size) || rest.len() != len - size {
              errors.push(format!(
                "from_head::<{}>({bytes:02x?} at +{start}) took {size}, want {want:?}",
                oracle.name
              ));
            }
          }
          Err(_) if want.is_some() => errors.push(format!(
            "from_head::<{}>({bytes:02x?} at +{start}) failed, want {want:?}",
            oracle.name
          )),
          Err(_) => {}
        }
      }
      // Next pattern.
      let mut i = 0;
      while i < len {
        pattern[i] += 1;
        if pattern[i] < alphabet.len() {
          break;
        }
        pattern[i] = 0;
        i += 1;
      }
      if i == len {
        break;
      }
    }
  }
  count
}

/// Checks every type of [`ORACLES`] from bytes against its oracle.
#[test]
fn slices_and_tails_from_bytes() {
  let mut errors = vec![];
  let mut count = 0;
  let o = |name: &str| ORACLES.iter().find(|o| o.name == name).unwrap();
  // The first errors of each type, so that each type that disagrees is shown.
  let mut shown = vec![];
  macro_rules! check_each {
    ($($ty:ty: $name:literal),+ $(,)?) => {$(
      let before = errors.len();
      count += check::<$ty>(o($name), &mut errors);
      if errors.len() > before {
        shown.push(format!("{}: {} errors", $name, errors.len() - before));
        shown.extend(errors[before..].iter().take(4).cloned());
      }
    )+};
  }
  check_each!(
    [u8]: "[u8]",
    [bool]: "[bool]",
    [u16]: "[u16]",
    [NonZero<u8>]: "[NonZero<u8>]",
    [u8in<0, 0x7f>]: "[u8in<0, 0x7f>]",
    str: "str",
    TailU16: "TailU16",
    TailU8: "TailU8",
    TailBool: "TailBool",
    Counted: "Counted",
    Sized_: "Sized_",
    ZeroTail: "ZeroTail",
    NonEmpty: "NonEmpty",
    Msg: "Msg",
    [Void1]: "[Void1]",
    VoidTail: "VoidTail",
    VoidOuter: "VoidOuter",
    VoidDeep: "VoidDeep",
  );
  println!("== slices and tails: {count} sources, {} errors", errors.len());
  for line in &shown {
    println!("   {line}");
  }
  assert!(errors.is_empty(), "{} errors", errors.len());
}

/// Checks the verdicts on typed sources, and that the bytes stay the same through the puns.
#[test]
fn slices_and_tails_to_bytes() {
  let v = const { hook::verdict::<[bool], [u8]>() };
  assert!(v.pun_ref && !v.pun_mut && !v.try_pun_mut);
  assert!(const { hook::verdict::<[u16], [u8]>() }.pun_mut);
  assert!(const { hook::verdict::<str, [u8]>() }.pun_ref);
  assert!(!const { hook::verdict::<str, [u8]>() }.pun_mut);
  assert!(!const { hook::verdict::<[u8], str>() }.pun_ref);
  // `TailU8` has trailing padding when its slice has an odd length: none of it is seen as bytes.
  let words = [0x0102u16, 0x0304, 0x0506];
  let bytes: &[u8; 6] = puns::pun_ref(&words);
  for len in [2usize, 3, 4, 5] {
    let tail: &TailU8 = puns::try_pun_ref::<[u8], TailU8>(&bytes[..len]).unwrap_or_else(|e| {
      // An odd number of bytes after the head leaves no room for the padding.
      assert!(len % 2 == 1 && e == Error::Size, "{len}: {e:?}");
      puns::try_pun_ref::<[u8], TailU8>(&bytes[..len - 1]).unwrap()
    });
    let back: &[u8] = puns::as_bytes(tail);
    assert_eq!(back, &bytes[..2 + tail.t.len()]);
  }
  // A `str` seen as `[u8in<0, 0x7f>]` only if it is ASCII; and back as `str` without a check.
  let ascii = puns::try_pun_ref::<str, [u8in<0, 0x7f>]>("ab").unwrap();
  assert!(const { hook::verdict::<[u8in<0, 0x7f>], str>() }.pun_ref);
  assert_eq!(puns::pun_ref::<_, str>(ascii), "ab");
  assert!(puns::try_pun_ref::<str, [u8in<0, 0x7f>]>("é").is_err());
  // Slices of different strides: the length follows.
  let pairs: &[[u8; 2]] = puns::pun_ref(&words[..]);
  assert_eq!(pairs.len(), 3);
  let back: &[u16] = puns::try_pun_ref::<[[u8; 2]], [u16]>(pairs).unwrap();
  assert_eq!(back, &words);
}

/// Checks `try_pun_ref` and `pun_ref` from the values of a sized type of the model `S` to `D`,
/// against `oracle`, and returns the number of values tried.
///
/// Every target of these oracles requires every byte of its extent to be initialized: a pun that
/// compiles is unsound if a value of `S` leaves one of them uninitialized, since its check, or the
/// code that uses the target, reads it. Under Miri, the values are made with those bytes
/// uninitialized, and a check that reads one is reported. From a value whose bytes are all
/// initialized, a check must accept exactly what the oracle accepts.
fn check_typed<S: puns_model::Oracle, D: ?Sized + Pun>(
  oracle: &Oracle,
  errors: &mut Vec<String>,
) -> usize {
  let verdict = const { hook::verdict::<S, D>() };
  let name = core::any::type_name::<S>().rsplit("::").next().unwrap_or_default();
  if !verdict.try_pun_ref {
    return 0;
  }
  let len = size_of::<S>();
  let fits = len >= oracle.head
    && (len - oracle.head).is_multiple_of(oracle.stride.max(1))
    && padded(len, oracle.align) <= len;
  let mut count = 0;
  for p in puns_model::values::<S>() {
    count += 1;
    if p.iter().any(Option::is_none) {
      errors.push(format!(
        "try_pun_ref::<{name}, {}> compiles, but {p:02x?} has a byte that may be uninitialized",
        oracle.name
      ));
      break;
    }
    let bytes: Vec<u8> = p.iter().map(|b| b.unwrap_or_default()).collect();
    // SAFETY: `p` is a value of `values::<S>()`, of `size_of::<S>()` bytes. The model check relies
    // on the oracles being right, which Miri checks. These are the requirements of `make`.
    let value = unsafe { puns_model::make::<S>(&p) };
    let want = fits && (oracle.valid)(&bytes);
    let aligned = (&raw const value).addr().is_multiple_of(oracle.align);
    let got = hook::try_pun_ref::<S, D>(&value).expect("compiles");
    if aligned && got.is_ok() != want {
      errors.push(format!(
        "try_pun_ref::<{name}, {}>({bytes:02x?}) = {}",
        oracle.name,
        got.is_ok()
      ));
    }
    if verdict.pun_ref && !want {
      errors.push(format!("pun_ref::<{name}, {}> compiles, but {bytes:02x?}", oracle.name));
    }
  }
  count
}

/// Checks typed sources of the model to strings and to slices of bytes: a struct with trailing
/// padding, bytes of any value before a union that may be uninitialized, and controls.
#[test]
fn typed_sources_to_strings() {
  use puns_model::types::{ByteNz, ByteZero, PadEnd, PadMid, TagUnit};
  let mut errors = vec![];
  let mut count = 0;
  let o = |name: &str| ORACLES.iter().find(|o| o.name == name).unwrap();
  count += check_typed::<PadEnd, str>(o("str"), &mut errors);
  count += check_typed::<PadMid, str>(o("str"), &mut errors);
  count += check_typed::<TagUnit, str>(o("str"), &mut errors);
  count += check_typed::<ByteZero, str>(o("str"), &mut errors);
  count += check_typed::<TagUnit, Msg>(o("Msg"), &mut errors);
  count += check_typed::<PadEnd, [u8]>(o("[u8]"), &mut errors);
  count += check_typed::<TagUnit, [u8]>(o("[u8]"), &mut errors);
  count += check_typed::<[u8; 2], str>(o("str"), &mut errors);
  count += check_typed::<ByteNz, str>(o("str"), &mut errors);
  count += check_typed::<[u8; 2], Msg>(o("Msg"), &mut errors);
  println!("== typed sources to strings: {count} values, {} errors", errors.len());
  for error in errors.iter().take(20) {
    println!("   {error}");
  }
  assert!(errors.is_empty(), "{} errors", errors.len());
}
