//! The model check of the order of a copy: every tree of a few values of three types that point at
//! each other and at themselves, under a chain of values of the type of its root, with each of its
//! pointers aimed at each aligned place of the bytes. A check of `puns` must accept exactly the
//! images whose pointees are in the order of a copy, as the oracle tells: a walk that follows each
//! pointer where it finds it, and claims its pointee at or after the end of the one before.
//!
//! The check of `puns` does not follow a pointer where it finds it once its loops run: a pointee
//! waits for the loop of its type, which may be a loop below the one that found it, and its own
//! pointees are claimed later, within a window. Which loop runs where depends on what the check
//! took in place before, so the trees are under chains of several lengths. `cargo test` takes the
//! trees of up to 3 values; the `full` feature, those of up to 6: a pointee that a loop hands to
//! the loop below, which hands one of its own to the loop below it, takes 5.
//!
//! The types are of two families: one of alignment 4, and one of alignment 1, whose values a check
//! takes in place only while their loops would still fit in the frames of evaluation of a `const`
//! (`puns::hook::frames`), which starts their loops at another depth. The chains of the second
//! family go from none to past that depth one value at a time.
//!
//! This file is not built under Miri: it checks verdicts over every image, whose unsafe paths the
//! tests of `puns` on aims (`puns/tests/aims.rs`) reach under Miri.
#![cfg(not(miri))]
// Not for Miri: verdicts over every image, whose unsafe paths `puns/tests/aims.rs` reaches.

use core::marker::{PhantomData, PhantomPinned};

use puns::{InPlace, Pun, Thin};

/// Declares, in the module `$family`, a pointer whose offset is a `$offset` and the types `Red`,
/// `Green` and `Blue`, whose words are `$word`s.
macro_rules! family {
  ($family:ident, $offset:ty, $word:ty) => {
    mod $family {
      use super::{InPlace, PhantomData, PhantomPinned, Pun, Thin};

      /// A pointer to a `T`, stored as the distance from its own address, never zero.
      #[derive(Pun)]
      #[pun(bound(T: Pun))]
      #[repr(transparent)]
      pub struct Near<T: ?Sized> {
        #[pun(pointer(pointee: Thin<T, i32>))]
        off: InPlace<$offset>,
        _t: PhantomData<T>,
        _pin: PhantomPinned,
      }

      #[derive(Pun)]
      #[repr(C)]
      pub struct Red {
        green: Option<Near<Green>>,
        word: Option<Near<$word>>,
        same: Option<Near<Red>>,
      }

      #[derive(Pun)]
      #[repr(C)]
      pub struct Green {
        blue: Option<Near<Blue>>,
        same: Option<Near<Green>>,
        word: Option<Near<$word>>,
        red: Option<Near<Red>>,
      }

      #[derive(Pun)]
      #[repr(C)]
      pub struct Blue {
        red: Option<Near<Red>>,
        green: Option<Near<Green>>,
        same: Option<Near<Blue>>,
      }
    }
  };
}

family!(wide, core::num::NonZero<i32>, u32);
family!(narrow, puns::num::NonZero<puns::num::i32le>, puns::num::u32le);

/// What a pointer of these types points at.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
  Red,
  Green,
  Blue,
  Word,
}

impl Kind {
  const fn size(self) -> usize {
    match self {
      Self::Red => size_of::<wide::Red>(),
      Self::Green => size_of::<wide::Green>(),
      Self::Blue => size_of::<wide::Blue>(),
      Self::Word => 4,
    }
  }

  /// What each pointer of a value points at, in the order of its fields.
  const fn pointers(self) -> &'static [Self] {
    match self {
      Self::Red => &[Self::Green, Self::Word, Self::Red],
      Self::Green => &[Self::Blue, Self::Green, Self::Word, Self::Red],
      Self::Blue => &[Self::Red, Self::Green, Self::Blue],
      Self::Word => &[],
    }
  }

  /// Where the pointer of a value to a value of its own type is, in bytes from its start.
  const fn same(self) -> usize {
    match self {
      Self::Green => 4,
      _ => 8,
    }
  }
}

/// A value, and what each of its pointers points at.
#[derive(Clone)]
struct Tree(Kind, Vec<Option<Self>>);

/// Every tree of `kind` with exactly `values` values that are not words.
fn trees(kind: Kind, values: usize) -> Vec<Tree> {
  match (kind, values) {
    (Kind::Word, 0) => vec![Tree(kind, vec![])],
    (Kind::Word, _) | (_, 0) => vec![],
    _ => pointees(kind.pointers(), values - 1).into_iter().map(|of| Tree(kind, of)).collect(),
  }
}

/// Every way to give `values` values that are not words to the pointees of pointers at `kinds`.
fn pointees(kinds: &[Kind], values: usize) -> Vec<Vec<Option<Tree>>> {
  let Some((&first, rest)) = kinds.split_first() else {
    return if values == 0 { vec![vec![]] } else { vec![] };
  };
  let mut all = vec![];
  let mut with = |pointee: Option<Tree>, tail: &[Option<Tree>]| {
    all.push(core::iter::once(pointee).chain(tail.iter().cloned()).collect());
  };
  for tail in pointees(rest, values) {
    with(None, &tail);
    if first == Kind::Word {
      with(Some(Tree(first, vec![])), &tail);
    }
  }
  for here in 1..=values {
    for tree in trees(first, here) {
      for tail in pointees(rest, values - here) {
        with(Some(tree.clone()), &tail);
      }
    }
  }
  all
}

fn offset(image: &[u8], pointer: usize) -> i32 {
  i32::from_ne_bytes(image[pointer..pointer + 4].try_into().unwrap())
}

/// Points the pointer at `pointer` to `to`.
fn aim(image: &mut [u8], pointer: usize, to: usize) {
  let off = i32::try_from(to.cast_signed() - pointer.cast_signed()).unwrap();
  image[pointer..pointer + 4].copy_from_slice(&off.to_ne_bytes());
}

/// Writes the pointees of `tree`, whose value is at `at`, each after the end of `image`, in the
/// order of a copy, and adds where each of its pointers is to `pointers`.
fn copy(image: &mut Vec<u8>, at: usize, tree: &Tree, pointers: &mut Vec<usize>) {
  for (i, pointee) in tree.1.iter().enumerate() {
    pointers.push(at + 4 * i);
    if let Some(pointee) = pointee {
      let to = image.len();
      image.resize(to + pointee.0.size(), 0);
      aim(image, at + 4 * i, to);
      copy(image, to, pointee, pointers);
    }
  }
}

/// The oracle: whether the pointees of the value of `kind` at `at` are in the bytes, aligned to
/// `align`, and in the order of a copy from `cursor`, the end of the last pointee.
fn in_order(image: &[u8], at: usize, kind: Kind, align: usize, cursor: &mut usize) -> bool {
  kind.pointers().iter().enumerate().all(|(i, &pointee)| {
    let pointer = at + 4 * i;
    let off = offset(image, pointer);
    if off == 0 {
      return true;
    }
    let Some(to) = pointer.checked_add_signed(isize::try_from(off).unwrap()) else {
      return false;
    };
    let fits = to.is_multiple_of(align)
      && to.checked_add(pointee.size()).is_some_and(|end| end <= image.len());
    if !fits || to < *cursor {
      return false;
    }
    *cursor = to + pointee.size();
    in_order(image, to, pointee, align, cursor)
  })
}

/// What `puns` says of `image`, a value of `kind` of the family of alignment `align`, at an address
/// aligned to 8 in `words`.
fn check(kind: Kind, align: usize, image: &[u8], words: &mut Vec<u64>) -> Result<(), puns::Error> {
  words.clear();
  words.resize(image.len().div_ceil(8), 0);
  puns::pun_mut::<[u64], [u8]>(&mut words[..])[..image.len()].copy_from_slice(image);
  let bytes = &puns::as_bytes(&words[..])[..image.len()];
  match (kind, align) {
    (Kind::Red, 4) => puns::from_head::<wide::Red>(bytes).map(|_| ()),
    (Kind::Green, 4) => puns::from_head::<wide::Green>(bytes).map(|_| ()),
    (_, 4) => puns::from_head::<wide::Blue>(bytes).map(|_| ()),
    (Kind::Red, _) => puns::from_head::<narrow::Red>(bytes).map(|_| ()),
    (Kind::Green, _) => puns::from_head::<narrow::Green>(bytes).map(|_| ()),
    _ => puns::from_head::<narrow::Blue>(bytes).map(|_| ()),
  }
}

/// Compares the check with the oracle on the copy of `tree` under a chain of `chain` values of its
/// kind, of the family of alignment `align`, and on that copy with each pointer of the tree aimed
/// at each aligned place. Returns how many of these images are in order.
fn compare(tree: &Tree, chain: usize, align: usize, words: &mut Vec<u64>) -> usize {
  let kind = tree.0;
  let mut image = vec![0; kind.size()];
  let mut at = 0;
  for _ in 0..chain {
    let next = image.len();
    image.resize(next + kind.size(), 0);
    aim(&mut image, at + kind.same(), next);
    at = next;
  }
  let mut pointers = vec![];
  copy(&mut image, at, tree, &mut pointers);
  assert!(in_order(&image, 0, kind, align, &mut kind.size()));
  assert_eq!(check(kind, align, &image, words), Ok(()), "the copy of a tree under {chain}");
  let mut ordered = 0;
  for pointer in pointers {
    let before = offset(&image, pointer);
    for to in (0..image.len()).step_by(align).filter(|&to| to != pointer) {
      aim(&mut image, pointer, to);
      let expected = in_order(&image, 0, kind, align, &mut kind.size());
      let checked = check(kind, align, &image, words);
      assert_ne!(checked, Err(puns::Error::Limit), "{pointer} to {to} under {chain}");
      assert_eq!(checked.is_ok(), expected, "{pointer} to {to} under {chain}: {image:?}");
      ordered += usize::from(expected);
    }
    image[pointer..pointer + 4].copy_from_slice(&before.to_ne_bytes());
  }
  ordered
}

#[test]
fn every_small_tree_is_checked_as_the_oracle_walks_it() {
  // No chain: the check takes the tree in place. Then chains past what it takes in place, so
  // that the loop of the kind of the root runs where it reaches the tree.
  let (values, chains): (usize, &[usize]) =
    if cfg!(feature = "full") { (6, &[0, 1, 2, 3, 4, 6, 12, 40]) } else { (3, &[0, 3, 12]) };
  let (mut words, mut ordered) = (vec![], 0);
  for kind in [Kind::Red, Kind::Green, Kind::Blue] {
    for tree in (1..=values).flat_map(|values| trees(kind, values)) {
      for &chain in chains {
        ordered += compare(&tree, chain, 4, &mut words);
      }
    }
  }
  assert!(ordered > 1000, "{ordered} images in order");
}

#[test]
fn every_small_tree_of_alignment_1_is_checked_as_the_oracle_walks_it() {
  // Chains of each length from none to past the depth where a check takes its first loop.
  let values = if cfg!(feature = "full") { 6 } else { 3 };
  let (mut words, mut ordered) = (vec![], 0);
  for kind in [Kind::Red, Kind::Green, Kind::Blue] {
    for tree in (1..=values).flat_map(|values| trees(kind, values)) {
      for chain in 0..=6 {
        ordered += compare(&tree, chain, 1, &mut words);
      }
    }
  }
  assert!(ordered > 1000, "{ordered} images in order");
}
