//! Tests `validate_in`, `validate_elems_in`, `pointers_in`, `pointers_elems_in` and `copy_in`.
//!
//! `validate_in` and `validate_elems_in` check a value in a buffer, except what its pointers aim
//! at, and hand each pointer to a visitor. The tests check what each kind of layout answers
//! (scalars, structs and their rules, enums and niches, unions, `MaybeUninit`, `Option` and
//! `Result`, pointers) and where an error is. They also check that no input makes these functions
//! panic or read out of the bytes. CI runs these tests under Miri on each of its targets.
//!
//! On what `validate_in` accepts, `pointers_in` and `pointers_elems_in` must hand over the same
//! pointers, in the same order. `copy_in` must copy the bytes that a value holds initialized, and
//! no other, from a source whose other bytes are uninitialized. Neither `pointers_in` nor
//! `copy_in` needs the rules of the value to hold, which safe code can break, because they read
//! no rule.
//!
//! This file needs the `unstable` feature (`required-features` in `puns/Cargo.toml`), because it
//! reads `Option` and `Result` values whose layout Rust leaves to the compiler.

#[path = "support/relative.rs"]
#[expect(dead_code, reason = "what the model check uses, and these tests do not")]
mod relative;

use core::{mem::MaybeUninit, num::NonZero};

use puns::{Invalid, Pun, Visit, num::u8in};
use relative::{Builder, Fix, One, Opt, Region, Seq, Text, addr, field};

/// A visitor that records every pointer handed to it, with its position and what it is.
#[derive(Default)]
struct Record {
  pointers: Vec<Handed>,
  /// The position of a pointer that the visitor rejects, if any.
  refuse: Option<usize>,
}

#[derive(Debug, PartialEq, Eq)]
enum Stop {
  Invalid(usize),
  Refused(usize),
}

impl From<Invalid> for Stop {
  fn from(invalid: Invalid) -> Self {
    Self::Invalid(invalid.at)
  }
}

impl Visit for Record {
  type Error = Stop;

  fn pointer<P: ?Sized + Pun>(&mut self, at: usize) -> Result<(), Stop> {
    let pointer = const { P::LAYOUT.as_pointer() }.expect("only pointers are handed over");
    if self.refuse == Some(at) {
      return Err(Stop::Refused(at));
    }
    self.pointers.push((at, pointer.aim(), pointer.is_nullable()));
    Ok(())
  }
}

/// A pointer handed over, as its position, what it aims at, and whether it may be null.
type Handed = (usize, puns::layout::Aim, bool);

/// Returns the result of `validate_in::<T>` on `bytes` at `at`, and the pointers handed over.
fn check<T: Pun>(bytes: &[u8], at: usize) -> (Result<(), Stop>, Vec<Handed>) {
  let mut record = Record::default();
  let result = puns::validate_in::<T, Record>(bytes, at, &mut record);
  (result, record.pointers)
}

fn ok<T: Pun>(bytes: &[u8], at: usize) -> Result<(), Stop> {
  check::<T>(bytes, at).0
}

/// Returns what `check::<T>` returns. If `validate_in` accepts the bytes, also checks that
/// `pointers_in` hands over the same pointers.
///
/// # Panics
///
/// Panics if `pointers_in` returns an error or hands over other pointers.
fn walk<T: Pun>(bytes: &[u8], at: usize) -> (Result<(), Stop>, Vec<Handed>) {
  let (result, handed) = check::<T>(bytes, at);
  if result.is_ok() {
    let mut record = Record::default();
    // SAFETY: `validate_in` accepted the bytes as a `T` at `at`. So they are in `bytes`, valid for
    // reads, and meet the requirements of `pointers_in`, as its `# Safety` says of such bytes.
    let walked = unsafe { puns::pointers_in::<T, Record>(bytes.as_ptr(), at, &mut record) };
    assert_eq!(walked, Ok(()), "{}: at {at}", core::any::type_name::<T>());
    assert_eq!(record.pointers, handed, "{}: at {at}", core::any::type_name::<T>());
  }
  (result, handed)
}

// -------------------------------------------------------------------------------------------------
// Values without pointers

#[derive(Pun)]
#[repr(C)]
#[pun(assert(lo <= hi))]
struct Span {
  lo: u8,
  #[pun(valid(..=9))]
  hi: u8,
  on: bool,
}

#[test]
fn scalars_structs_and_rules() {
  assert_eq!(ok::<u32>(&[1, 2, 3, 4], 0), Ok(()));
  assert_eq!(ok::<bool>(&[0, 1, 2], 1), Ok(()));
  assert_eq!(ok::<bool>(&[0, 1, 2], 2), Err(Stop::Invalid(2)));
  assert_eq!(ok::<char>(&0xd800u32.to_ne_bytes(), 0), Err(Stop::Invalid(0)));
  assert_eq!(ok::<NonZero<i16>>(&[0, 0, 0xff, 0xff], 2), Ok(()));
  assert_eq!(ok::<NonZero<i16>>(&[0, 0, 0xff, 0xff], 0), Err(Stop::Invalid(0)));
  // The first thing that is not valid, in the order of the positions; the rule after the fields.
  let bytes = [7, 1, 2, 1, 3, 4, 2];
  assert_eq!(ok::<Span>(&bytes, 1), Ok(()));
  assert_eq!(ok::<Span>(&bytes, 4), Err(Stop::Invalid(6))); // `on` is 2
  assert_eq!(ok::<Span>(&[3, 2, 1], 0), Err(Stop::Invalid(0))); // `lo <= hi`
  assert_eq!(ok::<Span>(&[3, 10, 0], 0), Err(Stop::Invalid(1))); // `valid(..=9)`
  assert_eq!(ok::<[Span; 2]>(&[1, 2, 0, 1, 2, 3], 0), Err(Stop::Invalid(5)));
  // A `u8in`, which guarantees its values.
  assert_eq!(ok::<u8in<1, 3>>(&[0, 4], 1), Err(Stop::Invalid(1)));
}

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
#[expect(dead_code, reason = "made of bytes")]
enum Shape {
  Dot,
  Line(bool),
  Box(u8, NonZero<u8>),
}

/// Checks enums and niches. `Option<bool>` is an enum of the `Rust` representation with a niche, in
/// which `None` is 2.
#[test]
fn enums_and_niches() {
  assert_eq!(ok::<Shape>(&[2, 7, 1], 0), Ok(()));
  assert_eq!(ok::<Shape>(&[1, 2, 9], 0), Err(Stop::Invalid(1))); // the `bool`
  assert_eq!(ok::<Shape>(&[3, 0, 0], 0), Err(Stop::Invalid(0))); // no such tag
  assert_eq!(ok::<Shape>(&[0, 9, 9], 0), Ok(())); // a `Dot`: the rest is padding
  assert_eq!(ok::<Option<bool>>(&[2], 0), Ok(()));
  assert_eq!(ok::<Option<bool>>(&[3], 0), Err(Stop::Invalid(0)));
  assert_eq!(ok::<Option<NonZero<u16>>>(&[0, 0], 0), Ok(()));
  assert_eq!(ok::<Result<u8, bool>>(puns::as_bytes(&Ok::<u8, bool>(9)), 0), Ok(()));
  let err = puns::as_bytes(&Err::<bool, bool>(true)).to_vec();
  assert_eq!(ok::<Result<bool, bool>>(&err, 0), Ok(()));
  // The byte of the `bool` of `Err`, made 2: where it is, whatever the layout `rustc` chose.
  let mut bad = err;
  let at = bad.iter().position(|byte| *byte == 1).expect("the `true`");
  bad[at] = 2;
  assert!(ok::<Result<bool, bool>>(&bad, 0).is_err());
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Choice {
  flag: bool,
  small: u8in<0, 3>,
}

/// Checks that a union is valid if one of its fields is, and that a `MaybeUninit` accepts any
/// bytes.
#[test]
fn unions_and_maybe_uninit() {
  assert_eq!(ok::<Choice>(&[3], 0), Ok(()));
  assert_eq!(ok::<Choice>(&[4], 0), Err(Stop::Invalid(0)));
  assert_eq!(ok::<MaybeUninit<bool>>(&[9], 0), Ok(()));
  // A `MaybeUninit` of a pointer: no value is required of it, and no pointer handed over.
  assert_eq!(check::<MaybeUninit<One<u8>>>(&[0; 4], 0), (Ok(()), vec![]));
}

// -------------------------------------------------------------------------------------------------
// Values with pointers

#[derive(Pun)]
#[repr(C)]
struct Node {
  id: u8,
  name: Text,
  kids: Seq<Node>,
  next: Opt<Node>,
  flag: bool,
}

#[derive(Pun)]
#[repr(u8)]
#[expect(dead_code, reason = "made of bytes")]
enum Tree {
  Leaf(bool),
  Node(One<Tree>, One<Tree>),
}

/// A struct with a pointer whose offset cannot be zero.
#[derive(Pun)]
#[repr(C)]
struct Strict {
  to: One<u8, relative::Nz32le>,
}

/// Checks that each pointer is handed over once, in order, with what it is.
///
/// A null pointer that may be null is not handed over. An error of the visitor ends the walk.
#[test]
fn pointers_are_handed_over() {
  use puns::layout::Aim;
  // `id`, then three offsets of 4 bytes, `flag`.
  let mut bytes = vec![5, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 1];
  let all =
    vec![(1, Aim::Text, false), (5, Aim::Sequence { align: 4 }, false), (9, Aim::One, true)];
  assert_eq!(walk::<Node>(&bytes, 0), (Ok(()), all.clone()));
  // `next` null: not handed over.
  bytes[9] = 0;
  assert_eq!(walk::<Node>(&bytes, 0), (Ok(()), all[..2].to_vec()));
  // `flag` is 2: the pointers before it were handed over.
  bytes[13] = 2;
  assert_eq!(check::<Node>(&bytes, 0), (Err(Stop::Invalid(13)), all[..2].to_vec()));
  // Refused by the visitor, before `flag` is looked at.
  let mut record = Record { refuse: Some(5), ..Record::default() };
  assert_eq!(puns::validate_in::<Node, Record>(&bytes, 0, &mut record), Err(Stop::Refused(5)));
  // A pointer of offset zero: one whose offset may be zero is one, handed over (its scheme
  // says what it aims at); one whose offset is a `NonZero` is not valid there.
  let here = [1u8, 0, 0, 0, 0, 1, 0, 0, 0];
  assert_eq!(walk::<Tree>(&here, 0), (Ok(()), vec![(1, Aim::One, false), (5, Aim::One, false)]));
  assert_eq!(check::<Strict>(&[0; 4], 0), (Err(Stop::Invalid(0)), vec![]));
  assert_eq!(walk::<Strict>(&[4, 0, 0, 0], 0), (Ok(()), vec![(0, Aim::One, false)]));
  // The pointers of the variant that the tag tells, and only those.
  let node = [1u8, 4, 0, 0, 0, 8, 0, 0, 0];
  assert_eq!(walk::<Tree>(&node, 0), (Ok(()), vec![(1, Aim::One, false), (5, Aim::One, false)]));
  let leaf = [0u8, 1, 0, 0, 0, 1, 0, 0, 0];
  assert_eq!(walk::<Tree>(&leaf, 0), (Ok(()), vec![]));
}

/// Checks that `pointers_in` checks nothing and hands over what `validate_in` does.
///
/// It hands over the pointers of the variant that the tag tells, in order, except a null pointer
/// that may be null. An error of the visitor ends the walk. It reads only what `validate_in`
/// reads, so the bytes that the variant does not hold are uninitialized here.
#[test]
fn pointers_in_hands_over_the_same() {
  use puns::layout::Aim;
  let mut node = MaybeUninit::<[u8; 9]>::uninit();
  let mut leaf = MaybeUninit::<[u8; 9]>::uninit();
  // SAFETY: `node` and `leaf` are 9 bytes each, valid for writes, of alignment 1. The writes are
  // of 9 and 2 bytes from their start, as `write` requires.
  unsafe {
    node.as_mut_ptr().cast::<[u8; 9]>().write([1, 4, 0, 0, 0, 8, 0, 0, 0]);
    leaf.as_mut_ptr().cast::<[u8; 2]>().write([0, 1]);
  }
  let mut record = Record::default();
  // SAFETY: `node` holds a `Tree::Node`, whose tag and pointers are initialized, and `leaf` a
  // `Tree::Leaf`, whose tag and `bool` are. Their other bytes are not held by their variant,
  // which `pointers_in` allows to be uninitialized. Both are valid for reads of a `Tree`. These
  // are the requirements of `pointers_in`.
  unsafe {
    puns::pointers_in::<Tree, _>(node.as_ptr().cast(), 0, &mut record).expect("no refusal");
    assert_eq!(record.pointers, vec![(1, Aim::One, false), (5, Aim::One, false)]);
    puns::pointers_in::<Tree, _>(leaf.as_ptr().cast(), 0, &mut record).expect("no refusal");
    assert_eq!(record.pointers.len(), 2);
  }
  // The error of the visitor ends the walk: the second pointer is not handed over.
  let bytes = [5, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 1];
  let mut record = Record { refuse: Some(5), ..Record::default() };
  // SAFETY: `bytes` is valid for reads, and holds a `Node` that `validate_in` accepts
  // (`pointers_are_handed_over`), which meets the requirements of `pointers_in`.
  let walked = unsafe { puns::pointers_in::<Node, _>(bytes.as_ptr(), 0, &mut record) };
  assert_eq!((walked, record.pointers), (Err(Stop::Refused(5)), vec![(1, Aim::Text, false)]));
  // At any alignment: the same `Node` one byte further.
  let mut later = vec![0];
  later.extend_from_slice(&bytes);
  let (result, handed) = walk::<Node>(&later, 1);
  assert_eq!(result, Ok(()));
  assert_eq!(handed.iter().map(|p| p.0).collect::<Vec<_>>(), [2, 6, 10]);
  // An array of values with pointers, and their elements.
  let pair = [1u8, 4, 0, 0, 0, 8, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0];
  let (_, handed) = walk::<[Tree; 2]>(&pair, 0);
  assert_eq!(handed.iter().map(|p| p.0).collect::<Vec<_>>(), [1, 5]);
  let mut record = Record::default();
  // SAFETY: `pair` is valid for reads, and holds two `Tree`s, which `validate_elems_in` accepts,
  // so each meets the requirements of `pointers_in`, as `pointers_elems_in` requires.
  unsafe {
    assert_eq!(puns::validate_elems_in::<Tree, _>(&pair, 0, 2, &mut Record::default()), Ok(()));
    puns::pointers_elems_in::<Tree, _>(pair.as_ptr(), 0, 2, &mut record).expect("no refusal");
  }
  assert_eq!(record.pointers.iter().map(|p| p.0).collect::<Vec<_>>(), [1, 5]);
  // Elements without pointers are not looked at, whatever their count.
  // SAFETY: A `bool` holds no self-relative pointer, so `pointers_elems_in` reads nothing and
  // requires nothing of its arguments, as its `# Safety` says.
  unsafe {
    puns::pointers_elems_in::<bool, _>(core::ptr::null(), 0, usize::MAX, &mut record)
      .expect("nothing");
  }
}

/// Checks that the elements are checked one after the other, by blocks when they hold no pointers
/// and one by one when they do.
#[test]
fn elements() {
  let mut record = Record::default();
  let bytes = [1u8, 0, 1, 1, 0, 2, 1];
  assert_eq!(puns::validate_elems_in::<bool, _>(&bytes, 0, 5, &mut record), Ok(()));
  assert_eq!(puns::validate_elems_in::<bool, _>(&bytes, 0, 6, &mut record), Err(Stop::Invalid(5)));
  assert_eq!(puns::validate_elems_in::<bool, _>(&bytes, 3, 5, &mut record), Err(Stop::Invalid(3)));
  assert_eq!(puns::validate_elems_in::<u16, _>(&bytes, 1, 3, &mut record), Ok(()));
  assert_eq!(puns::validate_elems_in::<u16, _>(&bytes, 1, 4, &mut record), Err(Stop::Invalid(1)));
  // Many, by blocks: the first invalid one.
  let mut many = vec![1u8; 1000];
  many[777] = 9;
  assert_eq!(
    puns::validate_elems_in::<bool, _>(&many, 0, 1000, &mut record),
    Err(Stop::Invalid(777))
  );
  assert_eq!(puns::validate_elems_in::<bool, _>(&many, 0, 777, &mut record), Ok(()));
  // With pointers: each, in order.
  let ones = [4u8, 0, 0, 0, 4, 0, 0, 0];
  assert_eq!(puns::validate_elems_in::<One<u8>, _>(&ones, 0, 2, &mut record), Ok(()));
  assert_eq!(record.pointers.iter().map(|p| p.0).collect::<Vec<_>>(), [0, 4]);
  // A count whose bytes overflow, or that is past the end.
  assert_eq!(
    puns::validate_elems_in::<u16, _>(&bytes, 1, usize::MAX, &mut record),
    Err(Stop::Invalid(1))
  );
  assert_eq!(
    puns::validate_elems_in::<u8, _>(&bytes, usize::MAX, 1, &mut record),
    Err(Stop::Invalid(usize::MAX))
  );
  // No bytes, and none needed.
  assert_eq!(puns::validate_elems_in::<(), _>(&[], 0, usize::MAX, &mut record), Ok(()));
}

// -------------------------------------------------------------------------------------------------
// Any input

/// A small deterministic generator of random numbers.
struct Rng(u64);

impl Rng {
  const fn next(&mut self) -> u64 {
    self.0 ^= self.0 >> 12;
    self.0 ^= self.0 << 25;
    self.0 ^= self.0 >> 27;
    self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
  }
}

/// Checks random bytes at random positions, past the end too, with random counts, some of which
/// overflow.
///
/// Nothing panics or reads out of the bytes, which Miri would report. What `validate_elems_in`
/// accepts is what the same check of each element accepts.
#[test]
fn any_input() {
  fn all<T: Pun>(rng: &mut Rng, bytes: &[u8]) {
    let len = bytes.len();
    let at = match rng.next() % 4 {
      0 => usize::MAX - (rng.next() % 8) as usize,
      _ => (rng.next() % (len as u64 + 4)) as usize,
    };
    let count = match rng.next() % 4 {
      0 => usize::MAX / (1 + (rng.next() % 3) as usize),
      _ => (rng.next() % 8) as usize,
    };
    let one = walk::<T>(bytes, at).0;
    let mut record = Record::default();
    let many = puns::validate_elems_in::<T, _>(bytes, at, count, &mut record);
    if many.is_ok() && count > 0 {
      assert_eq!(one, Ok(()), "{}: at {at}, {count}", core::any::type_name::<T>());
      // The same pointers, from `pointers_elems_in`.
      let mut walked = Record::default();
      // SAFETY: `validate_elems_in` accepted the `count` values from `at`, which are in `bytes`,
      // valid for reads. So each meets the requirements of `pointers_in`, as
      // `pointers_elems_in` requires.
      unsafe { puns::pointers_elems_in::<T, _>(bytes.as_ptr(), at, count, &mut walked) }
        .expect("no refusal");
      assert_eq!(walked.pointers, record.pointers, "{}: at {at}", core::any::type_name::<T>());
    }
  }
  let mut rng = Rng(0x1234_5678_9abc_def1);
  let rounds = if cfg!(miri) { 40 } else { 4000 };
  for _ in 0..rounds {
    let len = (rng.next() % 24) as usize;
    let bytes: Vec<u8> = (0..len).map(|_| rng.next() as u8 & 0x3).collect();
    all::<bool>(&mut rng, &bytes);
    all::<Span>(&mut rng, &bytes);
    all::<Shape>(&mut rng, &bytes);
    all::<Choice>(&mut rng, &bytes);
    all::<Option<bool>>(&mut rng, &bytes);
    all::<Node>(&mut rng, &bytes);
    all::<Tree>(&mut rng, &bytes);
    all::<[NonZero<u16>; 3]>(&mut rng, &bytes);
    all::<()>(&mut rng, &bytes);
    // `copy_in` of what is accepted (not of a union, which it refuses).
    let at = (rng.next() % (len as u64 + 1)) as usize;
    copied::<bool>(&bytes, at);
    copied::<Span>(&bytes, at);
    copied::<Shape>(&bytes, at);
    copied::<Option<bool>>(&bytes, at);
    copied::<Node>(&bytes, at);
    copied::<Tree>(&bytes, at);
    copied::<[NonZero<u16>; 3]>(&bytes, at);
    copied::<()>(&bytes, at);
  }
}

/// Checks `copy_in` on the `T` at `at` in `bytes`, if `validate_in` accepts it.
///
/// `copy_in` into bytes that are all `0xee` must make a `T` that `validate_in` accepts too, with
/// the same pointers at the same places in the value. Every byte must be either the byte of the
/// source or `0xee`.
///
/// # Panics
///
/// Panics if one of these does not hold.
fn copied<T: Pun>(bytes: &[u8], at: usize) {
  let (result, handed) = check::<T>(bytes, at);
  if result.is_err() {
    return;
  }
  let size = size_of::<T>();
  let mut dst = vec![0xee; size];
  // SAFETY: `validate_in` accepted a `T` at `at`, so the bytes from `at` are valid for reads and
  // meet the requirements of `pointers_in`. `dst` is `size` bytes, valid for writes, in another
  // allocation. These are the requirements of `copy_in`.
  unsafe { puns::copy_in::<T>(bytes[at..].as_ptr(), dst.as_mut_ptr()) };
  let name = core::any::type_name::<T>();
  for (i, byte) in dst.iter().enumerate() {
    assert!(*byte == bytes[at + i] || *byte == 0xee, "{name}: at {at}, byte {i}");
  }
  // The bytes of the pointers handed over are left to the caller.
  for &(p, ..) in &handed {
    assert_eq!(dst[p - at], 0xee, "{name}: at {at}, the pointer at {p}");
  }
  let (again, moved) = check::<T>(&dst, 0);
  assert_eq!(again, Ok(()), "{name}: at {at}");
  let shifted: Vec<Handed> = handed.into_iter().map(|(p, aim, null)| (p - at, aim, null)).collect();
  assert_eq!(moved, shifted, "{name}: at {at}");
}

/// An enum without variants, which has no values.
#[derive(Pun)]
#[expect(clippy::empty_enums, reason = "a type without values is what is tested")]
enum Never {}

/// A struct that holds a type without values, and so has no values either.
#[derive(Pun)]
#[repr(C)]
struct Holds {
  id: u8,
  never: Never,
}

/// Checks that a type without values is never valid, whatever the bytes.
///
/// Its layout is not free of constraints (`is_free` is `leaves == 0`), so `validate_in` does not
/// return early, and what it checks fails.
#[test]
fn types_without_values() {
  use core::convert::Infallible;
  fn never<T: Pun>(bytes: &[u8]) {
    assert!(!const { T::LAYOUT.is_free() }, "{}", core::any::type_name::<T>());
    assert!(!const { T::LAYOUT.is_inhabited() }, "{}", core::any::type_name::<T>());
    for at in 0..=bytes.len() {
      assert!(ok::<T>(bytes, at).is_err(), "{} at {at}", core::any::type_name::<T>());
    }
    let mut record = Record::default();
    for count in 1..3 {
      assert!(puns::validate_elems_in::<T, _>(bytes, 0, count, &mut record).is_err());
    }
  }
  let bytes = [0u8, 1, 2, 0xff];
  never::<Never>(&bytes);
  never::<Infallible>(&bytes);
  never::<Holds>(&bytes);
  never::<[Never; 2]>(&bytes);
  never::<(u8, Infallible)>(&bytes);
  never::<Result<Infallible, Never>>(&bytes);
  // No element of a type without values: one value, of no byte.
  assert_eq!(ok::<[Never; 0]>(&bytes, 4), Ok(()));
  // `Option<Infallible>`: `None` only, whatever its bytes (it has none).
  assert_eq!(size_of::<Option<Infallible>>(), 0);
  assert_eq!(ok::<Option<Infallible>>(&bytes, 0), Ok(()));
  // `Result<u8, Infallible>`: an `Ok` only.
  assert_eq!(ok::<Result<u8, Infallible>>(&[7], 0), Ok(()));
}

/// Bytes at an address aligned for every type of this file.
#[repr(C, align(16))]
struct Aligned([u8; 32]);

/// Checks that, for types without pointers, `validate_in` agrees with `try_pun_ref`.
///
/// They agree on a value and on every copy of it with one byte changed by each mask of one bit
/// and by the mask of all bits. `validate_elems_in` of two values agrees with `try_pun_ref` to a
/// slice of two.
#[test]
fn the_verdict_is_the_one_of_try_pun_ref() {
  /// Checks the agreement on `value`, the bytes of a value of `T`, and on its changed copies.
  fn agree<T: Pun>(value: &[u8]) {
    let size = size_of::<T>();
    assert_eq!(value.len(), size);
    let mut buffer = Aligned([0; 32]);
    buffer.0[..size].copy_from_slice(value);
    buffer.0[size..2 * size].copy_from_slice(value);
    let masks: &[u8] =
      if cfg!(miri) { &[0x01, 0xff] } else { &[1, 2, 4, 8, 16, 32, 64, 128, 0xff] };
    for at in 0..2 * size {
      for &mask in masks {
        buffer.0[at] ^= mask;
        let bytes = &buffer.0[..2 * size];
        let mut record = Record::default();
        let ours = puns::validate_in::<T, _>(bytes, 0, &mut record).is_ok();
        assert_eq!(ours, puns::try_pun_ref::<[u8], T>(&bytes[..size]).is_ok(), "{at} {mask:#x}");
        let ours = puns::validate_elems_in::<T, _>(bytes, 0, 2, &mut record).is_ok();
        assert_eq!(ours, puns::try_pun_ref::<[u8], [T]>(bytes).is_ok(), "{at} {mask:#x}, two");
        buffer.0[at] ^= mask;
      }
    }
  }
  agree::<bool>(&[1]);
  agree::<char>(&('é' as u32).to_ne_bytes());
  agree::<NonZero<u16>>(&258u16.to_ne_bytes());
  agree::<Span>(&[1, 2, 1]);
  agree::<Shape>(&[2, 3, 4]);
  agree::<Shape>(&[1, 1, 0]);
  agree::<Choice>(&[1]);
  agree::<Option<bool>>(&[0]);
  agree::<[NonZero<u16>; 3]>(&[1, 1, 1, 1, 1, 1]);
  agree::<Result<u8, bool>>(puns::as_bytes(&Ok::<u8, bool>(7)));
  agree::<Result<u8, bool>>(puns::as_bytes(&Err::<u8, bool>(true)));
  agree::<u8in<1, 3>>(&[2]);
  agree::<Option<NonZero<u32>>>(&9u32.to_ne_bytes());
}

// -------------------------------------------------------------------------------------------------
// `copy_in`

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Padded {
  a: u8,
  b: u32,
}

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
#[expect(dead_code, reason = "read as bytes")]
enum Two {
  Small(u8),
  Big(u32),
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Mixed {
  two: Two,
  padded: [Padded; 2],
  maybe: Option<bool>,
  any: MaybeUninit<u16>,
}

/// Returns the bytes that `copy_in` makes of `value` in memory filled with `fill`.
///
/// The caller reads them whole, as a `&[u8]`, which Miri reports if one of them is
/// uninitialized.
fn copy_of<T: Pun>(value: &T, fill: u8) -> Vec<u8> {
  let mut dst = vec![fill; size_of::<T>()];
  // SAFETY: `value` is a valid `T`, valid for reads, which meets the requirements of `copy_in`
  // whatever its padding holds. `dst` is `size_of::<T>()` bytes, valid for writes, in another
  // allocation.
  unsafe { puns::copy_in::<T>(core::ptr::from_ref(value).cast(), dst.as_mut_ptr()) };
  dst
}

/// Checks `copy_in` on values on the stack, whose padding and other variants are uninitialized.
///
/// `copy_in` copies their scalars, tags and the fields of their variants. It leaves every other
/// byte of the destination as it was. The test reads the destination whole.
#[test]
fn copy_in_copies_the_value_and_nothing_else() {
  let b = 0x0102_0304_u32.to_ne_bytes();
  let padded = Padded { a: 9, b: 0x0102_0304 };
  assert_eq!(copy_of(&padded, 0), [&[9, 0, 0, 0][..], &b].concat());
  assert_eq!(copy_of(&padded, 0xaa), [&[9, 0xaa, 0xaa, 0xaa][..], &b].concat());
  // The other variant is uninitialized: only the tag and the field of `Small`.
  assert_eq!(copy_of(&Two::Small(7), 0), [0, 7, 0, 0, 0, 0, 0, 0]);
  assert_eq!(copy_of(&Two::Big(0x0102_0304), 0xaa), [&[1, 0xaa, 0xaa, 0xaa][..], &b].concat());
  // Nested: an enum, an array of padded structs, a niche, bytes that may be uninitialized.
  let mixed = Mixed {
    two: Two::Small(5),
    padded: [padded, Padded { a: 8, b: 0x0102_0304 }],
    maybe: None,
    any: MaybeUninit::uninit(),
  };
  let copy = copy_of(&mixed, 0xaa);
  let at = |offset: usize| copy[offset];
  assert_eq!((at(0), at(1), at(2)), (0, 5, 0xaa));
  let p = core::mem::offset_of!(Mixed, padded);
  assert_eq!(&copy[p..p + 8], [&[9, 0xaa, 0xaa, 0xaa][..], &b].concat());
  assert_eq!(&copy[p + 8..p + 16], [&[8, 0xaa, 0xaa, 0xaa][..], &b].concat());
  assert_eq!(at(core::mem::offset_of!(Mixed, maybe)), 2);
  let any = core::mem::offset_of!(Mixed, any);
  assert_eq!(&copy[any..any + 2], [0xaa, 0xaa]);
  // What is copied is the value.
  assert!(puns::validate_in::<Mixed, Record>(&copy, 0, &mut Record::default()).is_ok());
  // Pointers are left to the caller (bytes that `validate_in` accepted), but a null one, whose
  // zeros are copied.
  let mut node = [5u8, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 1];
  let a = 0xaa;
  for (next, copied) in [(3, [a, a, a, a]), (0, [0, 0, 0, 0])] {
    node[9] = next;
    let mut dst = [a; 14];
    // SAFETY: `node` holds a `Node` that `validate_in` accepts (`pointers_are_handed_over`), valid
    // for reads. `dst` is the size of a `Node`, valid for writes, in another allocation. These are
    // the requirements of `copy_in`.
    unsafe { puns::copy_in::<Node>(node.as_ptr(), dst.as_mut_ptr()) };
    assert_eq!(dst, [&[5, a, a, a, a, a, a, a, a][..], &copied, &[1]].concat()[..]);
  }
}

/// A struct with pointers, the rule `lo <= hi`, and the requirement that `level` is at most 9.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(lo <= hi))]
struct Ruled {
  lo: u8,
  hi: u8,
  #[pun(valid(..=9))]
  level: u8,
  first: One<u8>,
  next: Opt<u8>,
}

// SAFETY: `fix` calls `field` on each field that holds a pointer, which fixes it at its position,
// and changes nothing else, as the `# Safety` of `Fix` requires.
unsafe impl Fix for Ruled {
  fn fix(&mut self, at: usize) {
    let whole = addr(self);
    field(whole, at, &mut self.first);
    field(whole, at, &mut self.next);
  }
}

/// Checks that `pointers_in` and `copy_in` need a value whose scalars, tags and pointers are what
/// its type says, but not its rules.
///
/// The test makes a `Ruled` that breaks its rule and its requirement, as safe code can, in a
/// buffer, with a `next` and without. The pointers handed over are those of the value. The bytes
/// copied are its scalars, the restricted one too, and its null pointer. `validate_in` does not
/// accept the value.
#[test]
fn rules_need_not_hold_for_pointers_in_and_copy_in() {
  use puns::layout::Aim;
  let (first, next) = (core::mem::offset_of!(Ruled, first), core::mem::offset_of!(Ruled, next));
  for some in [true, false] {
    let mut b = Builder::new();
    let (one, opt) = (b.one(7u8), b.opt(some.then_some(9u8)));
    let broken = Ruled { lo: 2, hi: 1, level: 12, first: one, next: opt };
    // SAFETY: Every pointer of the root and of what `b` holds was made by `b` with what it aims
    // at, and is in one value only. `Ruled` implements `Fix` as its `# Safety` requires. These
    // are the requirements of `Region::new`.
    let region = unsafe { Region::new(b, broken) };
    assert_eq!(*region.get(&region.first), 7);
    assert_eq!(region.opt(&region.next).copied(), some.then_some(9));
    let (base, root) = (region.bytes().as_ptr().cast::<u8>(), region.root());

    let mut record = Record::default();
    // SAFETY: The buffer holds a `Ruled` at `root`, valid for reads, whose scalars and pointers
    // are valid for their types, which is what `pointers_in` requires. Its rules need not hold.
    unsafe { puns::pointers_in::<Ruled, _>(base, root, &mut record) }.expect("no refusal");
    let mut handed = vec![(root + first, Aim::One, false)];
    if some {
      handed.push((root + next, Aim::One, true));
    }
    assert_eq!(record.pointers, handed);

    let a = 0xaa;
    let mut dst = [a; size_of::<Ruled>()];
    // SAFETY: The `Ruled` at `root` meets the requirements of `copy_in`, for the reasons it meets
    // those of `pointers_in`, and `add(root)` stays in the buffer. `dst` is the size of a `Ruled`,
    // valid for writes, in another allocation.
    unsafe { puns::copy_in::<Ruled>(base.add(root), dst.as_mut_ptr()) };
    assert_eq!(dst[..3], [2, 1, 12]);
    // A pointer is left to the caller; a null one is copied.
    assert_eq!(dst[first..first + 4], [a; 4]);
    assert_eq!(dst[next..next + 4], if some { [a; 4] } else { [0; 4] });

    // What `validate_in` refuses: `level` (12), then `lo <= hi`, after the pointers.
    assert_eq!(check::<Ruled>(&dst, 0), (Err(Stop::Invalid(2)), vec![]));
    dst[2] = 3;
    assert_eq!(check::<Ruled>(&dst, 0).0, Err(Stop::Invalid(0)));
  }
}
