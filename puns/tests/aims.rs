//! Eager aims: a self-relative pointer declared through the public API (`support/aims.rs`), whose
//! pointees `puns` checks with the value, and `Thin`.
//!
//! The tests that read pointees through a reference to the value are in `reads`, which Miri runs
//! with tree borrows only.

use core::marker::PhantomData;

#[path = "support/aims.rs"]
mod aims;

use aims::{Expr, Near, Pair, Tree};
use puns::{
  Error, InPlace, Pointee, Pun, Thin,
  num::{i32be, i32le, u8c},
};

fn row(name: &str, values: &[u32]) -> (String, Vec<u32>) {
  (name.to_string(), values.to_vec())
}

/// An image of `a, [1, 2, 3] -> bc, [4]`, and where each pair is.
fn two() -> (Image, Vec<usize>) {
  Image::pairs(&[("a", &[1, 2, 3]), ("bc", &[4])])
}

fn check(image: &Image) -> Result<(), Error> {
  puns::from_head::<Pair>(image.aligned().bytes()).map(|_| ())
}

#[test]
fn each_corruption_is_refused() {
  let (image, at) = two();
  assert_eq!(check(&image), Ok(()));
  let set = |field: usize, value: i32| {
    let mut image = two().0;
    image.put_i32(field, value);
    check(&image)
  };
  // Each fails where the check finds it: the scalar of the offset, or the struct of the aim whose
  // pointee does not fit, or the text that is not UTF-8.
  let fails = |at: usize| Err(Error::Validity { at });
  // A zero offset is no `Near`, and a zero `Option<Near>` is `None`.
  assert_eq!(set(at[0], 0), fails(at[0]));
  assert_eq!(set(at[0] + 8, 0), Ok(()));
  // Past the end, before the start, and the start of the pointee past the end by one.
  assert_eq!(set(at[0], 10_000), fails(at[0]));
  assert_eq!(set(at[1] + 4, -i32::try_from(at[1] + 8).unwrap()), fails(at[1] + 4));
  assert_eq!(set(at[0], i32::try_from(image.bytes.len()).unwrap() + 1), fails(at[0]));
  // The next pair one byte later: not aligned.
  assert_eq!(set(at[0] + 8, image.get_i32(at[0] + 8) + 1), fails(at[0] + 8));
  // A length past the end, and a negative length.
  let name = at[0] + usize::try_from(image.get_i32(at[0])).unwrap();
  assert_eq!(set(name, 1000), fails(at[0]));
  assert_eq!(set(name, -1), fails(at[0]));
  // The values of the next pair past the end.
  let values = at[1] + 4 + usize::try_from(image.get_i32(at[1] + 4)).unwrap();
  assert_eq!(set(values, 1000), fails(at[1] + 4));
  // Invalid UTF-8 in the name of the next pair: the text, after its length.
  let mut bad = two().0;
  let name = at[1] + usize::try_from(image.get_i32(at[1])).unwrap();
  bad.bytes[name + 4] = 0xff;
  assert_eq!(check(&bad), fails(name + 4));
  // One byte short: the last values do not fit.
  let mut short = two().0;
  short.bytes.pop();
  assert_eq!(check(&short), fails(at[1] + 4));
}

#[test]
fn the_pointees_form_a_tree_in_the_order_of_a_copy() {
  let (image, at) = two();
  let name = |pair: usize| pair + usize::try_from(image.get_i32(pair)).unwrap();
  let aim = |field: usize, to: usize| {
    let mut image = two().0;
    image.aim(field, to);
    check(&image)
  };
  // Each fails at the struct of the aim whose pointee starts before the end of the claims.
  let fails = |at: usize| Err(Error::Validity { at });
  // The second pair has the name of the first: one pointee, two aims.
  assert_eq!(aim(at[1], name(at[0])), fails(at[1]));
  // Its name is the first pair, or within the root: a pointee overlaps a value.
  assert_eq!(aim(at[0] + 8, 0), fails(at[0] + 8));
  assert_eq!(aim(at[0], 4), fails(at[0]));
  // The next pair is the first one: a cycle of one.
  assert_eq!(aim(at[1] + 8, at[1]), fails(at[1] + 8));
  // Bytes that no aim reaches, before each pointee: allowed.
  let mut gaps = Image::new();
  let root = gaps.reserve(size_of::<Pair>(), 4);
  gaps.reserve(8, 4);
  let text = gaps.text("a");
  gaps.aim(root, text);
  gaps.reserve(12, 4);
  let words = gaps.words(&[1]);
  gaps.aim(root + 4, words);
  assert_eq!(check(&gaps), Ok(()));
  // The values before the name, out of the order of the fields.
  let mut swapped = Image::new();
  let root = swapped.reserve(size_of::<Pair>(), 4);
  let words = swapped.words(&[1]);
  let text = swapped.text("a");
  swapped.aim(root, text);
  swapped.aim(root + 4, words);
  assert_eq!(check(&swapped), Err(Error::Validity { at: root + 4 }));
}

/// Where each node of a balanced expression of `levels` levels is, as `balanced` writes it: the
/// root at 0.
fn nodes_of(levels: usize) -> (Image, Vec<usize>) {
  let mut image = Image::new();
  balanced(&mut image, levels);
  let size = image.bytes.len();
  let nodes = (0..size).step_by(size_of::<Expr>()).collect();
  (image, nodes)
}

/// Whether the expression at 0 of `image` reaches each node once at most, from a walk with a set of
/// the places visited: whether it is a tree.
fn walks_a_tree(image: &Image) -> bool {
  let mut seen = std::collections::HashSet::new();
  let mut todo = vec![0];
  while let Some(at) = todo.pop() {
    if !seen.insert(at) {
      return false;
    }
    if image.bytes[at] == 1 {
      for field in [at + 4, at + 8] {
        todo
          .push(field.checked_add_signed(isize::try_from(image.get_i32(field)).unwrap()).unwrap());
      }
    }
  }
  true
}

#[test]
fn the_windows_accept_exactly_the_trees() {
  // Each pointer of a balanced expression aimed at each other node: past the levels that the check
  // takes in place, the operands wait for the loop of `Expr`, and are checked in their windows.
  // A redirection is a tree where the node it leaves had the new pointee below it. Under Miri,
  // which is there for the unsafe paths, the pointers of the deepest `Add` aimed at every 21st
  // node, past the levels in place.
  let levels = if cfg!(miri) { 5 } else { 6 };
  let (image, nodes) = nodes_of(levels);
  let (mut trees, mut refused) = (0, 0);
  let adds: Vec<usize> = nodes.iter().copied().filter(|&at| image.bytes[at] == 1).collect();
  let adds = if cfg!(miri) { &adds[adds.len() - 1..] } else { &adds[..] };
  for &from in adds {
    for field in [from + 4, from + 8] {
      for &to in nodes.iter().step_by(if cfg!(miri) { 21 } else { 1 }) {
        let mut redirected = Image { bytes: image.bytes.clone() };
        redirected.aim(field, to);
        let tree = walks_a_tree(&redirected);
        assert_eq!(head::<Expr>(&redirected).is_ok(), tree, "{field} to {to}");
        if tree { trees += 1 } else { refused += 1 }
      }
    }
  }
  assert!(cfg!(miri) || trees > nodes.len() && refused > trees, "{trees} trees, {refused} refused");
}

/// A value of the first of two types that point at each other, at themselves and at words.
#[derive(Pun)]
#[repr(C)]
struct Odd {
  even: Option<Near<Even>>,
  word: Option<Near<u32>>,
  same: Option<Near<Odd>>,
}

/// A value of the second of these types.
#[derive(Pun)]
#[repr(C)]
struct Even {
  odd: Option<Near<Odd>>,
  same: Option<Near<Even>>,
  word: Option<Near<u32>>,
  last: Option<Near<Odd>>,
}

/// What a pointer of an [`Odd`] or an [`Even`] points at.
#[derive(Clone, Copy)]
enum Aimed {
  Odd,
  Even,
  Word,
}

impl Aimed {
  const fn size(self) -> usize {
    match self {
      Self::Odd => size_of::<Odd>(),
      Self::Even => size_of::<Even>(),
      Self::Word => 4,
    }
  }

  /// What each pointer of a value points at, in the order of its fields.
  const fn pointers(self) -> &'static [Self] {
    match self {
      Self::Odd => &[Self::Even, Self::Word, Self::Odd],
      Self::Even => &[Self::Odd, Self::Even, Self::Word, Self::Odd],
      Self::Word => &[],
    }
  }
}

/// The largest copy that `the_check_accepts_exactly_the_order_of_a_copy` takes, in bytes.
const LARGEST: usize = 500;

/// Numbers that look random, the same on every run.
struct Numbers(u64);

impl Numbers {
  const fn next(&mut self) -> u64 {
    self.0 ^= self.0 << 13;
    self.0 ^= self.0 >> 7;
    self.0 ^= self.0 << 17;
    self.0
  }
}

/// Writes the pointees of the value of `kind` at `at`, each after the end of `image`, in the order
/// of a copy, `depth` levels deep at most, and adds where each of its pointers is to `pointers`. It
/// writes no more pointees once the image is larger than `LARGEST`.
fn copy_of(
  image: &mut Image,
  at: usize,
  kind: Aimed,
  depth: usize,
  numbers: &mut Numbers,
  pointers: &mut Vec<usize>,
) {
  for (i, &pointee) in kind.pointers().iter().enumerate() {
    pointers.push(at + 4 * i);
    let some = match pointee {
      Aimed::Word => numbers.next().is_multiple_of(2),
      _ => depth > 0 && numbers.next() % 3 < 2,
    };
    if some && image.bytes.len() <= LARGEST {
      let to = image.reserve(pointee.size(), 4);
      image.aim(at + 4 * i, to);
      copy_of(image, to, pointee, depth.saturating_sub(1), numbers, pointers);
    }
  }
}

/// Whether the pointees of the value of `kind` at `at` are in the bytes, aligned, and in the order
/// of a copy from `cursor`, the end of the last pointee: the rule of the check, by a walk that
/// follows each pointer where it finds it.
fn in_the_order_of_a_copy(image: &Image, at: usize, kind: Aimed, cursor: &mut usize) -> bool {
  kind.pointers().iter().enumerate().all(|(i, &pointee)| {
    let pointer = at + 4 * i;
    let offset = image.get_i32(pointer);
    if offset == 0 {
      return true;
    }
    let Some(to) = pointer.checked_add_signed(isize::try_from(offset).unwrap()) else {
      return false;
    };
    let fits = to.is_multiple_of(4)
      && to.checked_add(pointee.size()).is_some_and(|end| end <= image.bytes.len());
    if !fits || to < *cursor {
      return false;
    }
    *cursor = to + pointee.size();
    in_the_order_of_a_copy(image, to, pointee, cursor)
  })
}

#[test]
fn the_check_accepts_exactly_the_order_of_a_copy() {
  // Copies of values of two types that point at each other and at themselves, deep enough for the
  // loops of both, with each pointer aimed at each aligned place of the bytes. A pointee of one
  // type found in a loop of the other waits for the loop below, which checks it after the claims
  // that follow, so its window must end at the first of them, though that claim is made after the
  // loop above returns. The check agrees with a walk that claims each pointee where it finds it.
  // Under Miri, which is there for the unsafe paths, one copy, and a pointer in 37 at a place in
  // 37: the copy takes the loops of both types.
  let (copies, pointer_step, place_step) = if cfg!(miri) { (1, 37, 148) } else { (10, 1, 4) };
  // The first seed gives a copy of a size that the test takes, so that Miri generates no other.
  let (mut seed, mut done, mut in_order, mut refused) = (0xe599_11a9_cf44_7029_u64, 0, 0, 0);
  while done < copies {
    seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
    let mut numbers = Numbers(seed | 1);
    let depth = 6 + usize::try_from(numbers.next() % 14).unwrap();
    let mut image = Image::new();
    let (root, mut pointers) = (image.reserve(size_of::<Odd>(), 4), vec![]);
    copy_of(&mut image, root, Aimed::Odd, depth, &mut numbers, &mut pointers);
    if !(200..=LARGEST).contains(&image.bytes.len()) {
      continue;
    }
    done += 1;
    assert_eq!(head::<Odd>(&image), Ok(()), "a copy of {} bytes", image.bytes.len());
    for &pointer in pointers.iter().step_by(pointer_step) {
      for to in (0..image.bytes.len()).step_by(place_step).filter(|&to| to != pointer) {
        let mut aimed = Image { bytes: image.bytes.clone() };
        aimed.aim(pointer, to);
        let ordered = in_the_order_of_a_copy(&aimed, root, Aimed::Odd, &mut size_of::<Odd>());
        let checked = head::<Odd>(&aimed);
        assert_ne!(checked, Err(Error::Limit), "{seed}: {pointer} to {to}");
        assert_eq!(checked.is_ok(), ordered, "{seed}: {pointer} to {to}");
        if ordered { in_order += 1 } else { refused += 1 }
      }
    }
  }
  assert!(cfg!(miri) || in_order > 1000 && refused > in_order, "{in_order} and {refused}");
}

/// A value of the type of the loop below two others: a pointer to a [`Mid`], then one to a word,
/// then one to a value of its own type.
#[derive(Pun)]
#[repr(C)]
struct Low {
  mid: Option<Near<Mid>>,
  word: Option<Near<u32>>,
  same: Option<Near<Self>>,
}

/// A value that points at a [`Top`], or at a [`Low`].
#[derive(Pun)]
#[repr(C)]
struct Mid {
  top: Option<Near<Top>>,
  low: Option<Near<Low>>,
}

/// A value that points at a [`Mid`].
#[derive(Pun)]
#[repr(C)]
struct Top {
  mid: Option<Near<Mid>>,
}

#[test]
fn a_pointee_handed_down_by_two_loops_ends_at_the_next_claim() {
  // A chain of `Low`, for its loop to run, whose last value points at a `Mid` then at a word. The
  // `Mid` points at a `Top`, whose loop finds a second `Mid` and hands it to the loop of `Mid`,
  // which had no next pointee and takes it at once, in the same sequence. That `Mid` points at a
  // `Low`, handed to the loop of `Low` below, which still checks the value that points at the
  // word: the window of that last `Low` ends where the word starts, so its own word may not be
  // that one.
  for chain in [0, 3, 12, 40] {
    let mut image = Image::new();
    let mut last = image.reserve(size_of::<Low>(), 4);
    for _ in 0..chain {
      let next = image.reserve(size_of::<Low>(), 4);
      image.aim(last + 8, next);
      last = next;
    }
    let mid = image.reserve(size_of::<Mid>(), 4);
    image.aim(last, mid);
    let top = image.reserve(size_of::<Top>(), 4);
    image.aim(mid, top);
    let second = image.reserve(size_of::<Mid>(), 4);
    image.aim(top, second);
    let low = image.reserve(size_of::<Low>(), 4);
    image.aim(second + 4, low);
    let (own, word) = (image.reserve(4, 4), image.reserve(4, 4));
    image.aim(low + 4, own);
    image.aim(last + 4, word);
    assert_eq!(head::<Low>(&image), Ok(()), "a chain of {chain}");
    image.aim(low + 4, word);
    // Where no loop runs, the word is refused where it is claimed again, at the pointer of the
    // value that points at it. In a loop, the sequence of the last `Low` ends past its window: it
    // is not valid.
    let at = if chain == 0 { last + 4 } else { low };
    assert_eq!(head::<Low>(&image), Err(Error::Validity { at }), "a chain of {chain}");
  }
}

/// A value that points at two values of its own type, then at an [`Other`].
#[derive(Pun)]
#[repr(C)]
struct Twice {
  first: Option<Near<Self>>,
  second: Option<Near<Self>>,
  other: Option<Near<Other>>,
}

/// A value of another type, which holds a pointer, so that it has a loop of its own.
#[derive(Pun)]
#[repr(C)]
struct Other {
  same: Option<Near<Self>>,
}

#[test]
fn the_place_of_a_window_is_the_pointee_whose_pointees_end_past_it() {
  // A chain of `Twice`, for its loop to run, whose last value points at two more. The first of
  // them waits, and its window ends where the second starts. Its `Other` is before the second,
  // as a copy writes it, or after it.
  let image = |copied: bool| {
    let mut image = Image::new();
    let mut last = image.reserve(size_of::<Twice>(), 4);
    for _ in 0..30 {
      let next = image.reserve(size_of::<Twice>(), 4);
      image.aim(last, next);
      last = next;
    }
    let first = image.reserve(size_of::<Twice>(), 4);
    let (other, second) = if copied {
      let other = image.reserve(size_of::<Other>(), 4);
      (other, image.reserve(size_of::<Twice>(), 4))
    } else {
      let second = image.reserve(size_of::<Twice>(), 4);
      (image.reserve(size_of::<Other>(), 4), second)
    };
    image.aim(last, first);
    image.aim(last + 4, second);
    image.aim(first + 8, other);
    (image, first)
  };
  assert_eq!(head::<Twice>(&image(true).0), Ok(()));
  // The loop of `Other` starts in the sequence of the first, which ends past its window. The
  // place is the first, whose own pointee is past its window, not the `Other`, which has none.
  let (image, first) = image(false);
  assert_eq!(head::<Twice>(&image), Err(Error::Validity { at: first }));
}

#[test]
fn a_cycle_is_refused() {
  let (mut image, at) = two();
  // The second pair points back at the first, which is before the cursor.
  image.aim(at[1] + 8, at[0]);
  assert_eq!(check(&image), Err(Error::Validity { at: at[1] + 8 }));
}

/// A pair of pointers to texts that other pointers may share, and a pointer to the next pair,
/// which is not shared.
#[derive(Pun)]
#[repr(C)]
struct Shared {
  #[pun(pointer(shared first: Thin<str, i32> = first as u128))]
  first: InPlace<i32>,
  #[pun(pointer(shared second: Thin<str, i32> = second as u128 + 4))]
  second: InPlace<i32>,
  next: Option<Near<Shared>>,
}

#[test]
fn a_shared_pointee_is_anywhere() {
  // The root, its next, and a text after them, which the four shared pointers share.
  let mut image = Image::new();
  let root = image.reserve(size_of::<Shared>(), 4);
  let next = image.reserve(size_of::<Shared>(), 4);
  image.aim(root + 8, next);
  let text = image.text("ab");
  for field in [root, root + 4, next, next + 4] {
    image.aim(field, text);
  }
  let read = |image: &Image| puns::from_head::<Shared>(image.aligned().bytes()).map(|_| ());
  assert_eq!(read(&image), Ok(()));
  // A shared pointer may lead into a value that is claimed: the text of the root is the bytes of
  // the next pair from its second pointer, whose value is the length.
  let mut within = Image { bytes: image.bytes.clone() };
  within.aim(root, next + 4);
  assert_eq!(read(&within), Ok(()));
  // The next pair is not shared: it may not be the root.
  let mut cycle = Image { bytes: image.bytes.clone() };
  cycle.aim(next + 8, root);
  assert_eq!(read(&cycle), Err(Error::Validity { at: next + 8 }));
}

/// A pointer to a pair that other pointers may share, and a pointer to a pair that is not
/// shared. The pointers of a pair are not shared.
#[derive(Pun)]
#[repr(C)]
struct Lends {
  #[pun(pointer(shared lent: Pair = lent as u128))]
  lent: InPlace<i32>,
  own: Near<Pair>,
}

#[test]
fn what_a_shared_pointee_reaches_is_not_claimed() {
  // The root, then one pair with its name and its numbers, which both pointers of the root lead
  // to.
  let mut image = Image::new();
  let root = image.reserve(size_of::<Lends>(), 4);
  let pair = image.reserve(size_of::<Pair>(), 4);
  let text = image.text("a");
  image.aim(pair, text);
  let words = image.words(&[1]);
  image.aim(pair + 4, words);
  image.aim(root, pair);
  image.aim(root + 4, pair);
  // The name and the numbers are claimed once, for the pair of the pointer that is not shared.
  // The pointers of the pair, which are not shared, reach them again through the shared pointer,
  // where nothing is claimed.
  let read = |image: &Image| puns::from_head::<Lends>(image.aligned().bytes()).map(|_| ());
  assert_eq!(read(&image), Ok(()));
  // Two pairs that are not shared may not have one name.
  let (mut two, at) = two();
  let name = at[0].wrapping_add_signed(isize::try_from(two.get_i32(at[0])).unwrap());
  two.aim(at[1], name);
  assert_eq!(check(&two), Err(Error::Validity { at: at[1] }));
}

/// A pointer to a `u32`, a distance in bytes from itself, which no other pointer shares.
#[derive(Pun)]
#[repr(transparent)]
struct Word {
  #[pun(pointer(word: u32 = off as u128))]
  off: InPlace<i32>,
}

/// An aim of the struct itself, on its first field, then a field that holds an aim.
#[derive(Pun)]
#[repr(C)]
struct Mixed {
  #[pun(pointer(first: u32 = off as u128))]
  off: InPlace<i32>,
  second: Word,
}

#[test]
fn the_aims_within_the_fields_of_a_struct_come_before_its_own() {
  let check = |words: [i32; 4]| puns::from_head::<Mixed>(puns::as_bytes(&words)).map(|_| ());
  // The struct at 0, then two words. The fields are checked before the rules of the struct: the
  // pointee of the pointer of the second field is at 8, and the one of the aim of the struct,
  // which its first field declares, after it, at 12.
  assert_eq!(check([12, 4, 1, 2]), Ok(()));
  // In the order of the two fields, the second pointee claimed is before the first: the struct,
  // whose aim leads to it, is where the bytes are not valid.
  assert_eq!(check([8, 8, 1, 2]), Err(Error::Validity { at: 0 }));
}

/// What a list stores before its elements: a pointer, then its length.
#[derive(Pun)]
#[repr(C)]
struct Noted {
  note: Word,
  len: i32,
}

/// A pointer to a list behind that prefix, at its length.
#[derive(Pun)]
#[repr(C)]
struct ToNoted {
  #[pun(pointer(list: Thin<[u32], Noted> = off as u128))]
  off: InPlace<i32>,
  pad: u32,
}

#[test]
fn the_pointee_of_an_aim_of_a_prefix_comes_before_the_pointee_that_stores_it() {
  let check = |words: [i32; 6]| puns::from_head::<ToNoted>(puns::as_bytes(&words)).map(|_| ());
  // What a pointee stores before its value is checked before the pointee is claimed. The pointee
  // of the pointer of the prefix is at 8, then the prefix at 12 and 16, and an element at 20.
  assert_eq!(check([16, 0, 5, -4, 1, 77]), Ok(()));
  // The prefix at 8 and 12, an element at 16, then the pointee of the pointer of the prefix at
  // 20: the list is claimed after it, and starts before it.
  assert_eq!(check([12, 0, 12, 1, 77, 5]), Err(Error::Validity { at: 0 }));
}

/// The rest of the tail of the struct, from a signed distance.
#[derive(Pun)]
#[repr(C)]
struct Tail {
  #[pun(pointer(rest: [u8] = from as u128, size = ..))]
  from: InPlace<i8>,
  x: u8,
}

/// A pointer to a `Tail`, after its offset, a byte.
#[derive(Pun)]
#[repr(C)]
struct ToTail {
  #[pun(pointer(tail: Tail = off as u128))]
  off: InPlace<u8>,
  a: u8,
}

#[test]
fn a_pointee_of_the_rest_of_the_tail_starts_within_its_struct() {
  let check = |from: i8| puns::from_head::<ToTail>(&[2, 9, from.cast_unsigned(), 7]).map(|_| ());
  // The `Tail` is the bytes 2 and 3. Its rest is within it: both bytes, the last one, or none.
  assert_eq!((check(0), check(1), check(2)), (Ok(()), Ok(()), Ok(())));
  // Past its end, there is no rest: the `Tail`, whose aim leads to none, is not valid.
  let refused = Err(Error::Validity { at: 2 });
  assert_eq!(check(3), refused);
  // Before it, the rest would hold bytes of the root, and the `Tail`, which the root claims: a
  // pointee within its struct is not claimed, so it may not start before the struct.
  assert_eq!((check(-1), check(-2)), (refused, refused));
}

#[test]
fn the_pointees_of_a_value_read_from_the_tail_end_before_it() {
  // A word, then a pointer that no other shares, read from the tail of the bytes.
  let read = |off: i32| puns::from_tail::<Word>(puns::as_bytes(&[7, off])).map(|_| ());
  assert_eq!(read(-4), Ok(()));
  // Its pointee may not be the pointer itself: the claims end where the value starts.
  assert_eq!(read(0), Err(Error::Validity { at: 4 }));
}

/// A struct whose tail is a region of one pointer that no other shares, then what remains.
#[derive(Pun)]
#[repr(C)]
struct Parcel {
  kind: u32,
  #[pun(regions(label: Word = 4, rest))]
  tail: [u8],
}

#[test]
fn a_pointer_of_a_lazy_region_leads_after_the_region() {
  let label = |off: i32| {
    let words = [1, off, 7, 8];
    let parcel = puns::from_bytes::<Parcel>(puns::as_bytes(&words)).unwrap();
    parcel.label().map(|_| ())
  };
  // The region is the bytes 4 to 8. Its read claims the pointee of its pointer after it, within
  // the struct.
  assert_eq!((label(4), label(8)), (Ok(()), Ok(())));
  // Not the region itself, nor a byte before it, nor past the struct: the pointer, at 4 in the
  // struct, is where its bytes are not valid.
  let refused = Err(Error::Validity { at: 4 });
  assert_eq!((label(0), label(-4), label(12)), (refused, refused, refused));
}

/// Two shared pointers to one list, whose prefix holds a pointer that is not shared.
#[derive(Pun)]
#[repr(C)]
struct TwoNoted {
  #[pun(pointer(shared first: Thin<[u32], Noted> = first as u128))]
  first: InPlace<i32>,
  #[pun(pointer(shared second: Thin<[u32], Noted> = second as u128))]
  second: InPlace<i32>,
}

#[test]
fn what_a_shared_pointee_stores_before_its_value_is_not_claimed() {
  // The struct at 0, whose two pointers count from its start; the prefix at 8 (its pointer, to
  // the word at 20) and 12 (the length, 1), an element at 16. Both shared pointers lead to the
  // list, so its prefix is checked twice, and the
  // pointee of the pointer of the prefix is reached twice: nothing within a shared pointee is
  // claimed, what it stores before its value included.
  let words = [12i32, 12, 12, 1, 77, 5];
  assert_eq!(puns::from_head::<TwoNoted>(puns::as_bytes(&words)).map(|_| ()), Ok(()));
}

/// A chain of `n` pairs, each pointing at the next, with an empty name and an empty list, in the
/// order of a copy.
fn chain(n: usize) -> Image {
  let mut image = Image::new();
  let mut at = image.reserve(size_of::<Pair>(), 4);
  for i in 0..n {
    let text = image.text("");
    image.aim(at, text);
    let words = image.words(&[]);
    image.aim(at + 4, words);
    if i + 1 < n {
      let next = image.reserve(size_of::<Pair>(), 4);
      image.aim(at + 8, next);
      at = next;
    }
  }
  image
}

/// The stack of the thread of a check: what a check takes at most (`AIM_PARTS`), and room for the
/// test.
const SMALL_STACK: usize = 64 << 10;

/// Runs `f` on a thread of `SMALL_STACK` bytes of stack, in a build without optimizations too.
fn on_a_small_stack<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
  std::thread::Builder::new().stack_size(SMALL_STACK).spawn(f).unwrap().join().unwrap()
}

/// How deep the chains of the tests of depth are: far past `AIM_PARTS` natively, and past it under
/// Miri, which runs each unsafe path, not each value, so that a check in place of a type without an
/// identity stops at the limit.
const LONG: usize = if cfg!(miri) { 40 } else { 100_000 };

/// A deep struct: each field is a struct of the next, down to a pointer, so that the check of one
/// level of the chain nests a call for each.
#[derive(Pun)]
#[repr(C)]
struct Deep {
  next: Deep1,
}

#[derive(Pun)]
#[repr(C)]
struct Deep1 {
  next: Deep2,
}

#[derive(Pun)]
#[repr(C)]
struct Deep2 {
  next: Deep3,
}

#[derive(Pun)]
#[repr(C)]
struct Deep3 {
  value: u32,
  next: Option<Near<Deep>>,
}

/// A wide struct: many fields before its pointer, which the check of one level walks one by one.
#[derive(Pun)]
#[repr(C)]
struct Wide {
  fields: [Option<Near<u32>>; 2],
  a: u32,
  b: u32,
  c: u32,
  d: u32,
  e: u32,
  f: u32,
  g: u32,
  h: u32,
  next: Option<Near<Wide>>,
}

#[test]
fn the_stack_of_a_check_is_bounded_whatever_the_types() {
  // A chain of each, as deep as a check in place would follow it (a `Deep` has 14 parts, and a
  // `Wide` 28), then deeper, which the loop of each follows, on a small stack.
  let results = on_a_small_stack(|| {
    let deep = |n: usize| {
      let mut image = Image::new();
      let at = (0..n).map(|_| image.reserve(size_of::<Deep>(), 4)).collect::<Vec<_>>();
      for i in 0..n - 1 {
        image.aim(at[i] + 4, at[i + 1]);
      }
      let image = image.aligned();
      puns::from_head::<Deep>(image.bytes()).map(|_| ())
    };
    let wide = |n: usize| {
      let mut image = Image::new();
      let at = (0..n).map(|_| image.reserve(size_of::<Wide>(), 4)).collect::<Vec<_>>();
      for i in 0..n - 1 {
        image.aim(at[i] + 40, at[i + 1]);
      }
      let image = image.aligned();
      puns::from_head::<Wide>(image.bytes()).map(|_| ())
    };
    [deep(5), deep(6), deep(LONG), wide(3), wide(4), wide(LONG)]
  });
  assert_eq!(results, [Ok(()); 6]);
}

/// A chain of `n` trees, each the one child of the one before.
fn trees(n: usize) -> Image {
  let mut image = Image::new();
  let mut at = image.reserve(8, 4);
  for _ in 1..n {
    let list = image.list(1);
    image.aim(at + 4, list);
    at = list + 4;
  }
  let empty = image.list(0);
  image.aim(at + 4, empty);
  image
}

#[test]
fn the_place_of_a_list_is_that_of_its_first_child_that_is_not_valid() {
  // A root with `n` children, each with an empty list. The pointer of one child is aimed past
  // the bytes: the check is not valid there, whatever the children before it, which were claimed
  // and are valid.
  for n in [1, 7, 8, 9, 16, 20] {
    let mut image = Image::new();
    let root = image.reserve(8, 4);
    let list = image.list(n);
    image.aim(root + 4, list);
    for child in 0..n {
      let empty = image.list(0);
      image.aim(list + 4 + 8 * child + 4, empty);
    }
    assert_eq!(head::<Tree>(&image), Ok(()), "{n} children");
    for child in 0..n {
      let pointer = list + 4 + 8 * child + 4;
      let mut past = Image { bytes: image.bytes.clone() };
      past.aim(pointer, image.bytes.len() + 64);
      assert_eq!(head::<Tree>(&past), Err(Error::Validity { at: pointer }), "child {child} of {n}");
    }
  }
}

/// A balanced tree of `levels` levels of `Tree`s below the root, two children each.
fn balanced_trees(levels: usize) -> Image {
  fn kids(image: &mut Image, at: usize, levels: usize) {
    let list = image.list(if levels > 0 { 2 } else { 0 });
    image.aim(at + 4, list);
    if levels > 0 {
      kids(image, list + 4, levels - 1);
      kids(image, list + 12, levels - 1);
    }
  }
  let mut image = Image::new();
  let root = image.reserve(8, 4);
  kids(&mut image, root, levels);
  image
}

/// Reserves an expression and returns where it is: a `Lit` of 1, until `add` makes it an `Add`.
fn expr(image: &mut Image) -> usize {
  let at = image.reserve(size_of::<Expr>(), align_of::<Expr>());
  image.put(at + align_of::<i64>(), &1i64.to_ne_bytes());
  at
}

/// Makes the expression at `at` the `Add` of those at `left` and `right`. `repr(u8)` lays the
/// variant out as a `repr(C)` struct of the tag and the fields: the pointers at 4 and 8.
fn add(image: &mut Image, at: usize, left: usize, right: usize) {
  image.bytes[at] = 1;
  image.aim(at + 4, left);
  image.aim(at + 8, right);
}

/// Writes a balanced expression of `levels` levels of `Add`s, each before its operands, as a copy
/// writes it, and returns where it is.
fn balanced(image: &mut Image, levels: usize) -> usize {
  let at = expr(image);
  if levels > 0 {
    let (left, right) = (balanced(image, levels - 1), balanced(image, levels - 1));
    add(image, at, left, right);
  }
  at
}

/// An expression of `n` `Add`s, each of the next and a `Lit`, deep on the left or on the right,
/// the first at 0, in the order of a copy: on the left, the `Add`s, then the `Lit`s from the
/// deepest.
fn sided(n: usize, left: bool) -> Image {
  let mut image = Image::new();
  if left {
    let at = (0..=n).map(|_| expr(&mut image)).collect::<Vec<_>>();
    for i in (0..n).rev() {
      let one = expr(&mut image);
      add(&mut image, at[i], at[i + 1], one);
    }
  } else {
    let mut at = expr(&mut image);
    for _ in 0..n {
      let (one, next) = (expr(&mut image), expr(&mut image));
      add(&mut image, at, one, next);
      at = next;
    }
  }
  image
}

/// A sum of `n` terms built to the left, the first at 0: each `Add` holds the next on its left and
/// a term on its right, a balanced expression of `levels` levels, which waits while the check goes
/// down the left.
fn comb(n: usize, levels: usize) -> Image {
  let mut image = Image::new();
  let at = (0..=n).map(|_| expr(&mut image)).collect::<Vec<_>>();
  for i in (0..n).rev() {
    let term = balanced(&mut image, levels);
    add(&mut image, at[i], at[i + 1], term);
  }
  image
}

/// Values like those of JSON: null, an array of values, or an object of fields, each a key and a
/// value.
#[derive(Pun)]
#[repr(u8)]
enum Json {
  Null,
  Array(Near<[Json]>),
  Object(Near<[Field]>),
}

#[derive(Pun)]
#[repr(C)]
struct Field {
  key: Near<str>,
  value: Json,
}

/// Makes the value at `at` an array (`1`) or an object (`2`) of `n` elements, and returns where
/// the first is.
fn json_list(image: &mut Image, at: usize, kind: u8, n: usize) -> usize {
  let size = if kind == 1 { size_of::<Json>() } else { size_of::<Field>() };
  let list = image.reserve(4 + n * size, 4);
  image.put_i32(list, i32::try_from(n).unwrap());
  image.bytes[at] = kind;
  image.aim(at + 4, list);
  list + 4
}

/// `[{"k": {"k": null}, "k": {"k": null}, "k": [..]}]`, `n` levels of arrays and objects in turn,
/// the first at 0. Past the levels that the check takes in place, an object waits for the loop of
/// objects, below the second object of the object before it, and an array for the loop of arrays.
fn json(n: usize) -> Image {
  let mut image = Image::new();
  let mut at = image.reserve(size_of::<Json>(), 4);
  for level in 0..n {
    if level % 2 == 0 {
      at = json_list(&mut image, at, 1, 1);
      continue;
    }
    let fields = json_list(&mut image, at, 2, 3);
    for i in 0..3 {
      let field = fields + i * size_of::<Field>();
      let key = image.text("k");
      image.aim(field, key);
      if i < 2 {
        let inner = json_list(&mut image, field + 4, 2, 1);
        let key = image.text("k");
        image.aim(inner, key);
      }
    }
    at = fields + 2 * size_of::<Field>() + 4;
  }
  image
}

/// Three types that point at each other in turn.
#[derive(Pun)]
#[repr(C)]
struct One {
  value: u32,
  next: Option<Near<Two>>,
}

#[derive(Pun)]
#[repr(C)]
struct Two {
  value: u32,
  next: Option<Near<Three>>,
}

#[derive(Pun)]
#[repr(C)]
struct Three {
  value: u32,
  next: Option<Near<One>>,
}

/// A chain of `n` values of `One`, `Two` and `Three` in turn, or of any type of 8 bytes whose
/// pointer is its second word, the first at 0.
fn in_turn(n: usize) -> Image {
  let mut image = Image::new();
  let at = (0..n).map(|_| image.reserve(8, 4)).collect::<Vec<_>>();
  for i in 0..n - 1 {
    image.aim(at[i] + 4, at[i + 1]);
  }
  image
}

/// `from_head` of a `T` at the start of the bytes of `image`, without the value.
fn head<T: Pun>(image: &Image) -> Result<(), Error> {
  puns::from_head::<T>(image.aligned().bytes()).map(|_| ())
}

#[test]
fn deep_data_is_checked_on_a_small_stack() {
  // A loop for each type on the path: deep on either side, balanced, of types that point at each
  // other, the stack of the check does not grow with the data.
  // Under Miri, 5 levels of `Expr` pass the 4 that a check takes in place (`AIM_PARTS`).
  let (levels, json_levels) = if cfg!(miri) { (5, 20) } else { (20, 10_000) };
  let results = on_a_small_stack(move || {
    let mut image = Image::new();
    balanced(&mut image, levels);
    [
      head::<Expr>(&image),
      head::<Tree>(&balanced_trees(levels)),
      head::<Expr>(&sided(LONG, true)),
      head::<Expr>(&sided(LONG, false)),
      head::<Tree>(&trees(LONG)),
      head::<Json>(&json(json_levels)),
      head::<One>(&in_turn(LONG)),
    ]
  });
  assert_eq!(results, [Ok(()); 7]);
}

/// A node of many parts: four texts and its children.
#[derive(Pun)]
#[repr(C)]
struct Node {
  texts: [Near<str>; 4],
  kids: Near<[Node]>,
}

/// A chain of `levels` `Node`s below the root, each the one child of the one before, the last
/// with an empty list of children, in the order of a copy.
fn nodes(levels: usize) -> Image {
  fn fill(image: &mut Image, at: usize, levels: usize) {
    for i in 0..4 {
      let text = image.text("x");
      image.aim(at + 4 * i, text);
    }
    let n = usize::from(levels > 0);
    let list = image.reserve(4 + size_of::<Node>() * n, 4);
    image.put_i32(list, i32::try_from(n).unwrap());
    image.aim(at + 16, list);
    if levels > 0 {
      fill(image, list + 4, levels - 1);
    }
  }
  let mut image = Image::new();
  let root = image.reserve(size_of::<Node>(), 4);
  fill(&mut image, root, levels);
  image
}

/// A link of a chain that has a name, so that each value leads to two pointees, in an array, so that
/// the parts of the values leave too few for the name of the last one that fits in them.
#[derive(Pun)]
#[repr(C)]
struct Named {
  names: [Near<str>; 1],
  next: Option<Near<Named>>,
}

/// A chain of `n` values of `Named`, each with a name, in the order of a copy.
fn named(n: usize) -> Image {
  let mut image = Image::new();
  let mut at = image.reserve(size_of::<Named>(), 4);
  for i in 0..n {
    let name = image.text("n");
    image.aim(at, name);
    if i + 1 < n {
      let next = image.reserve(size_of::<Named>(), 4);
      image.aim(at + 4, next);
      at = next;
    }
  }
  image
}

#[test]
fn a_pointee_without_pointees_takes_no_parts() {
  // A text, or an empty list, nests the check of no other pointee, wherever the parts of the values
  // above it leave it: the name of the last value of a chain that fits in the parts, or the empty
  // list of children that ends a branch of a tree. Under Miri, the sizes that stopped.
  let (chains, trees) = if cfg!(miri) { (5..6, 4..5) } else { (1..12, 0..8) };
  let n = chains.len() + trees.len();
  let results: Vec<_> = on_a_small_stack(|| {
    let chains = chains.map(|n| head::<Named>(&named(n)));
    chains.chain(trees.map(|levels| head::<Node>(&nodes(levels)))).collect()
  });
  assert_eq!(results, vec![Ok(()); n]);
}

/// A struct of many parts, its aims first, then 24 bytes.
macro_rules! many_parts {
  ($name:ident { $($aim:ident: $pointee:ty),* }) => {
    #[derive(Pun)]
    #[repr(C)]
    struct $name {
      $($aim: Option<Near<$pointee>>,)*
      bytes: Bytes24,
    }
  };
}

/// 24 bytes, each a part.
#[derive(Pun)]
#[repr(C)]
struct Bytes24 {
  b0: u8,
  b1: u8,
  b2: u8,
  b3: u8,
  b4: u8,
  b5: u8,
  b6: u8,
  b7: u8,
  b8: u8,
  b9: u8,
  b10: u8,
  b11: u8,
  b12: u8,
  b13: u8,
  b14: u8,
  b15: u8,
  b16: u8,
  b17: u8,
  b18: u8,
  b19: u8,
  b20: u8,
  b21: u8,
  b22: u8,
  b23: u8,
}

many_parts!(Outer { middle: Middle, next: Outer });
many_parts!(Middle { inner: Inner });
many_parts!(Inner { next: Inner });

/// A chain of `n` values of `Outer`, each with a `Middle` that has an `Inner`, in the order of a
/// copy.
fn outers(n: usize) -> Image {
  let mut image = Image::new();
  let mut at = image.reserve(size_of::<Outer>(), 4);
  for i in 0..n {
    let middle = image.reserve(size_of::<Middle>(), 4);
    image.aim(at, middle);
    let inner = image.reserve(size_of::<Inner>(), 4);
    image.aim(middle, inner);
    if i + 1 < n {
      let next = image.reserve(size_of::<Outer>(), 4);
      image.aim(at + 4, next);
      at = next;
    }
  }
  image
}

#[test]
fn the_loop_of_each_type_takes_parts_of_its_own() {
  // Past the parts of the check in place, the outer values wait for a loop, and the middle and
  // inner ones start a loop each, within it: three types whose parts add up to more than
  // `AIM_PARTS`.
  let results: Vec<_> = on_a_small_stack(|| (1..8).map(|n| head::<Outer>(&outers(n))).collect());
  assert_eq!(results, vec![Ok(()); 7]);
}

#[test]
fn more_deep_values_than_the_loops_hold_at_once_stop_at_the_limit() {
  // The terms of a sum built to the left wait while the check goes down the left, 64 at most. Past
  // them, each is checked in place, nested in the check of the value that holds it, which the parts
  // of the loops bound: a term of 2 levels fits, one of 3 does not. Under Miri, which is there for
  // the unsafe paths, the entries fill up, and two small terms are past them.
  if cfg!(miri) {
    assert_eq!(on_a_small_stack(|| head::<Expr>(&comb(66, 1))), Ok(()));
    return;
  }
  let results = on_a_small_stack(|| [head::<Expr>(&comb(100, 2)), head::<Expr>(&comb(70, 3))]);
  assert_eq!(results, [Ok(()), Err(Error::Limit)]);
}

/// A chain of a type without an identity: its parameter is not bounded by `Pun`.
#[derive(Pun)]
#[repr(C)]
struct Anonymous<T> {
  value: u32,
  next: Option<Near<Anonymous<T>>>,
  _p: PhantomData<T>,
}

#[test]
fn a_pointee_without_an_identity_is_checked_in_place() {
  // No loop can tell that a pointee is of its type without an identity, so each is checked in
  // place, nested in the check of the one before, as deep as the parts allow.
  let results = on_a_small_stack(|| {
    [head::<Anonymous<u8>>(&in_turn(3)), head::<Anonymous<u8>>(&in_turn(LONG))]
  });
  assert_eq!(results, [Ok(()), Err(Error::Limit)]);
}

/// A list of bytes, of alignment 1: a check evaluated when compiling knows no address, so only a
/// pointee of alignment 1 is aligned there (`puns::ptr::is_aligned`).
#[derive(Pun)]
#[repr(u8)]
enum Chain {
  End,
  Link(u8, Next),
}

/// The next value of a `Chain`, at the distance that a little-endian `i32` gives, from its own
/// address.
#[derive(Pun)]
#[repr(transparent)]
struct Next {
  #[pun(pointer(next: Chain))]
  off: InPlace<i32le>,
}

/// 1,000 links of 6 bytes, each a tag, a byte, and the distance to the next from the pointer, then
/// the end.
const LINKS: [u8; 6_006] = {
  let mut bytes = [0; 6_006];
  let mut i = 0;
  while i < 1_000 {
    bytes[6 * i] = 1;
    bytes[6 * i + 1] = i as u8;
    let next = 4i32.to_le_bytes();
    let mut j = 0;
    while j < 4 {
      bytes[6 * i + 2 + j] = next[j];
      j += 1;
    }
    i += 1;
  }
  bytes
};

/// Returns what `from_head` finds of a `Chain` at the start of `bytes`: how many bytes follow it.
const fn chain_rest(bytes: &[u8]) -> Result<usize, Error> {
  match puns::from_head::<Chain>(bytes) {
    Ok((_, rest)) => Ok(rest.len()),
    Err(error) => Err(error),
  }
}

/// The check of the list when compiling: `from_head` is a `const fn`, and past the links that it
/// checks in place (`IN_PLACE`), its loop of links nests no frame for each link, so the evaluation
/// stays within the frames that Rust allows by default.
const LIST: Result<usize, Error> = chain_rest(&LINKS);

/// The same list, whose last link points back at the first: a cycle.
const CYCLE: Result<usize, Error> = {
  let mut bytes = LINKS;
  let back = (-6_000i32 + 6 - 2).to_le_bytes();
  let mut j = 0;
  while j < 4 {
    bytes[6 * 999 + 2 + j] = back[j];
    j += 1;
  }
  chain_rest(&bytes)
};

#[test]
fn a_deep_list_is_checked_when_compiling() {
  assert_eq!(LIST, Ok(6_006 - size_of::<Chain>()));
  assert!(matches!(CYCLE, Err(Error::Validity { .. })));
  // The same check when running, which hands a link of the chain over to its loop from the check
  // in place of a link. Under Miri, which is there for that path, 40 links reach it: the 41st is the
  // end.
  let mut links = LINKS;
  let n = if cfg!(miri) { 40 } else { 1_000 };
  if n < 1_000 {
    links[6 * n] = 0;
  }
  assert_eq!(chain_rest(core::hint::black_box(&links)), LIST);
}

/// The value of an expression.
fn sum(e: &Expr) -> i64 {
  match e {
    Expr::Lit(v) => *v,
    Expr::Add(a, b) => sum(a.pointee()) + sum(b.pointee()),
  }
}

/// A pointer to a `u32`, then a number.
#[derive(Pun)]
#[repr(C)]
struct Counted {
  #[pun(pointer(u32 = off as u128))]
  off: InPlace<i32>,
  count: u32,
}

/// The same, with a rule over the number, before the aim or after it.
#[derive(Pun)]
#[repr(C)]
#[pun(assert(count < 5))]
struct CountedBelow {
  #[pun(pointer(pointee: u32 = off as u128))]
  off: InPlace<i32>,
  count: u32,
}

#[derive(Pun)]
#[repr(C)]
struct CountedAbove {
  #[pun(pointer(u32 = off as u128))]
  off: InPlace<i32>,
  #[pun(valid(1..))]
  count: u32,
}

/// Regions, then the same regions with a rule over their sizes.
#[derive(Pun)]
#[repr(C)]
struct Sized2 {
  size: u8,
  #[pun(regions(first: u8 = size as u128, rest))]
  tail: [u8],
}

#[derive(Pun)]
#[repr(C)]
#[pun(assert(size == 1))]
struct SizedOne {
  size: u8,
  #[pun(regions(first: u8 = size as u128, rest))]
  tail: [u8],
}

/// A list of values of a generic type, whose parameter is bounded by `Pun`.
#[derive(Pun)]
#[repr(C)]
struct List<T: Pun> {
  value: T,
  next: Option<Near<List<T>>>,
}

/// A struct that ends with a slice, behind its stored length.
#[derive(Pun)]
#[repr(C)]
struct Record {
  kind: u16,
  data: [u16],
}

/// A pointer to a `P`, after its offset, a byte.
#[derive(Pun)]
#[pun(bound(P: puns::Pointee))]
#[repr(C)]
struct To<P: ?Sized> {
  #[pun(pointer(pointee: P = at as u128))]
  at: InPlace<u8>,
  _p: PhantomData<P>,
}

/// A pointer to a `u16` before it or after it, stored as the distance from its own address, which
/// other pointers may share.
#[derive(Pun)]
#[repr(transparent)]
struct Back {
  #[pun(pointer(shared back: u16 = off as u128))]
  off: InPlace<i16>,
}

/// A pointer whose distance is big-endian on every target, to a pair of words that starts 4
/// bytes before the place where it aims, its second word.
#[derive(Pun)]
#[repr(transparent)]
struct Second {
  #[pun(pointer(pair: [u32; 2] = off as u128 - 4))]
  off: InPlace<i32be>,
}

/// A message that points at a word before or after it, within it too, and whose tail is regions: a
/// header, then a body of pointers, each to a word of the body.
#[derive(Pun)]
#[repr(C)]
struct Message {
  #[pun(pointer(shared word: u16 = off as u128))]
  off: InPlace<i16>,
  body_len: u16,
  #[pun(regions(header = 2, body: [Back] = body_len))]
  rest: [u8],
}

/// A text that stores its length before its bytes, as a block of `nearest` does.
#[derive(Pun)]
#[repr(C)]
struct Text {
  len: u16,
  #[pun(len = len)]
  data: str,
}

/// A pointer to a `Text`.
#[derive(Pun)]
#[repr(transparent)]
struct ToText {
  #[pun(pointer(text: Text = off as u128))]
  off: InPlace<i16>,
}

/// An image under construction: values written at positions aligned for them, as a writer of
/// pointers does, and the offsets between them.
struct Image {
  bytes: Vec<u8>,
}

impl Image {
  const fn new() -> Self {
    Self { bytes: Vec::new() }
  }

  /// Reserves `size` bytes aligned to `align`, and returns where they are.
  fn reserve(&mut self, size: usize, align: usize) -> usize {
    let at = self.bytes.len().next_multiple_of(align);
    self.bytes.resize(at + size, 0);
    at
  }

  fn put(&mut self, at: usize, bytes: &[u8]) {
    self.bytes[at..at + bytes.len()].copy_from_slice(bytes);
  }

  fn put_i32(&mut self, at: usize, value: i32) {
    self.put(at, &value.to_ne_bytes());
  }

  fn get_i32(&self, at: usize) -> i32 {
    i32::from_ne_bytes(self.bytes[at..at + 4].try_into().unwrap())
  }

  /// Points the `Near` at `at` to `to`.
  fn aim(&mut self, at: usize, to: usize) {
    let off = i32::try_from(to.cast_signed() - at.cast_signed()).unwrap();
    self.put_i32(at, off);
  }

  /// Writes a `Thin<str, i32>` and returns where it is.
  fn text(&mut self, text: &str) -> usize {
    let at = self.reserve(4 + text.len(), 4);
    self.put_i32(at, i32::try_from(text.len()).unwrap());
    self.put(at + 4, text.as_bytes());
    at
  }

  /// Writes a list of `n` elements of 8 bytes with nothing in them, after its length, and returns
  /// where its length is, where a `Near` points.
  fn list(&mut self, n: usize) -> usize {
    let at = self.reserve(4 + 8 * n, 4);
    self.put_i32(at, i32::try_from(n).unwrap());
    at
  }

  /// Writes a `Thin<[u32], i32>`, the length then the words, and returns where its length is, where
  /// a `Near` points.
  fn words(&mut self, words: &[u32]) -> usize {
    let at = self.reserve(4 + 4 * words.len(), 4);
    self.put_i32(at, i32::try_from(words.len()).unwrap());
    for (i, word) in words.iter().enumerate() {
      self.put(at + 4 + 4 * i, &word.to_ne_bytes());
    }
    at
  }

  /// Writes a chain of pairs, the first at 0, and returns where each is.
  fn pairs(rows: &[(&str, &[u32])]) -> (Self, Vec<usize>) {
    let mut image = Self::new();
    let mut at = vec![image.reserve(size_of::<Pair>(), 4)];
    for (i, (name, values)) in rows.iter().enumerate() {
      let text = image.text(name);
      image.aim(at[i], text);
      let words = image.words(values);
      image.aim(at[i] + 4, words);
      if i + 1 < rows.len() {
        at.push(image.reserve(size_of::<Pair>(), 4));
        image.aim(at[i] + 8, at[i + 1]);
      }
    }
    (image, at)
  }

  /// A copy of the bytes at an address aligned to 8.
  fn aligned(&self) -> Aligned {
    let words = self
      .bytes
      .chunks(8)
      .map(|chunk| {
        let mut word = [0; 8];
        word[..chunk.len()].copy_from_slice(chunk);
        u64::from_ne_bytes(word)
      })
      .collect();
    Aligned(words, self.bytes.len())
  }
}

/// Bytes at an address aligned to 8: words, and how many of their bytes.
struct Aligned(Vec<u64>, usize);

impl Aligned {
  fn bytes(&self) -> &[u8] {
    &puns::as_bytes(&self.0[..])[..self.1]
  }
}

/// The rows of a chain of pairs.
fn rows(root: &Pair) -> Vec<(String, Vec<u32>)> {
  let mut out = vec![];
  let mut at = Some(root);
  while let Some(pair) = at {
    out.push((pair.name.pointee().to_string(), pair.values.pointee().to_vec()));
    at = pair.next.as_ref().map(Near::pointee);
  }
  out
}

/// Rows at a stored place, of a given size in bytes, and a text of a given length from another.
#[derive(Pun)]
#[repr(C)]
struct Table {
  #[pun(pointer(rows: [u16] = at as u128, size = len as u128))]
  at: InPlace<u8>,
  len: u8,
  #[pun(pointer(text: str = from as u128, len = count as u128))]
  from: InPlace<u8>,
  count: u8,
}

/// Words at a stored place, of a given size in bytes, and the rest of the tail of the struct from
/// another place in it, both checked with the struct.
#[derive(Pun)]
#[repr(C)]
struct Spans {
  #[pun(pointer(words: [u16] = off as u128, size = len as u128))]
  off: InPlace<i8>,
  len: u8,
  #[pun(pointer(rest: [u8] = from as u128, size = ..))]
  from: InPlace<u8>,
  count: u8,
  #[pun(len = count)]
  tail: [u8],
}

/// A header, then regions: a text of its own size, checked when read, then a version and words
/// checked with the struct, which holds its bytes in place.
#[derive(Pun)]
#[repr(C)]
struct Packet {
  count: u8,
  #[pun(regions(text: Text, eager version: u8c<2> = 1, eager words: [u16] = count as u128, rest))]
  tail: InPlace<[u8]>,
}

#[test]
fn regions_of_every_kind() {
  // The count, then the text, 2-aligned as its length is: a byte of padding, its length, 2, and
  // its bytes. Then the version, a byte of padding, and the words at an even offset of the packet,
  // which leave nothing for the rest.
  let mut words = [0u16; 6];
  let bytes =
    puns::from_bytes_mut::<[u8]>(puns::pun_mut::<[u16; 6], [u8; 12]>(&mut words)).unwrap();
  bytes[0] = 4;
  bytes[2..4].copy_from_slice(&2u16.to_ne_bytes());
  bytes[4..7].copy_from_slice(b"ab\x02");
  bytes[8..12].copy_from_slice(puns::as_bytes(&[7u16, 8]));
  let bytes: &[u8] = bytes;
  let packet = puns::from_bytes::<Packet>(bytes).unwrap();
  assert_eq!(&packet.text().unwrap().data, "ab");
  // The eager regions are within the packet, which a reference to it covers.
  assert_eq!((packet.version().get(), packet.words(), packet.rest()), (2, &[7, 8][..], &[][..]));
  // An eager region is checked with the struct: another version is not one.
  let mut other = bytes.to_vec();
  other[6] = 3;
  let other = Image { bytes: other }.aligned();
  assert!(matches!(puns::from_bytes::<Packet>(other.bytes()), Err(Error::Validity { .. })));
  // Where the words start depends on their offset in the packet, not on its address: from an odd
  // address, they are at the same offset, which is not aligned, and the packet is not valid.
  let shifted = Image { bytes: [&[0][..], bytes].concat() }.aligned();
  assert!(matches!(puns::from_bytes::<Packet>(&shifted.bytes()[1..]), Err(Error::Validity { .. })));
}

/// A length, then the text, at the cursor: the pointee of a region that stores its length.
#[derive(Pun)]
#[repr(C)]
struct Strings {
  count: u8,
  #[pun(regions(first: Thin<str, u8>, eager second: Thin<str, u8>, rest))]
  tail: InPlace<[u8]>,
}

#[test]
fn a_region_stores_its_length() {
  let bytes = [2, 2, b'a', b'b', 1, b'c', 9];
  let strings = puns::from_bytes::<Strings>(&bytes).unwrap();
  assert_eq!((strings.first(), strings.second(), strings.rest()), (Ok("ab"), "c", &[9][..]));
  // A length past the tail does not fit: the struct is not valid. And the second, eager, is
  // checked with the struct.
  assert!(matches!(puns::from_bytes::<Strings>(&[2, 9, b'a']), Err(Error::Validity { .. })));
  assert!(matches!(puns::from_bytes::<Strings>(&[2, 0, 1, 0xff]), Err(Error::Validity { .. })));
  // The first, lazy, when it is read.
  let strings = puns::from_bytes::<Strings>(&[2, 1, 0xff, 0]).unwrap();
  assert!(matches!(strings.first(), Err(Error::Validity { .. })));
  assert_eq!(strings.second(), "");
}

/// A pointee without an identity: its parameter is not bounded by `Pun`.
#[derive(Pun)]
#[repr(C)]
struct Unnamed<T> {
  a: u64,
  _p: PhantomData<T>,
}

/// Regions whose pointee has no identity, or stores its length before its value.
#[derive(Pun)]
#[repr(C)]
struct Unnamed2 {
  #[pun(regions(unnamed: Unnamed<u8>, words: Thin<[u64], u64>, rest))]
  tail: [u8],
}

#[test]
fn a_region_is_read_as_the_pointee_of_its_struct() {
  // A read names the region by where its rule is in the list of the struct, so it reads the
  // pointee that the struct declares, where the check finds it, whatever its identity: what the
  // `Thin` stores is in the tail, before its value, which Miri checks.
  let words = [5u64, 1, 7, 9];
  let regions = puns::from_bytes::<Unnamed2>(puns::as_bytes(&words)).unwrap();
  assert_eq!(regions.unnamed().map(|unnamed| unnamed.a), Ok(5));
  assert_eq!(regions.words(), Ok(&[7][..]));
  assert_eq!(regions.rest(), puns::as_bytes(&[9u64]));
}

/// What a list stores before its elements, its prefix: its capacity then its length, which a rule
/// requires to be equal.
#[derive(Pun)]
#[pun(assert(cap as u128 == len as u128))]
#[repr(C)]
struct ListPrefix {
  cap: i32,
  len: i32,
}

/// A pointer to a list behind its prefix, at its length.
#[derive(Pun)]
#[repr(transparent)]
struct ToList {
  #[pun(pointer(list: Thin<[u32], ListPrefix> = off as u128))]
  off: InPlace<i32>,
}

#[test]
fn an_offset_near_the_end_of_an_i128_is_refused() {
  // A pointer to a text whose offset is the largest `i128`: the place of the value after the
  // length overflows, and the check refuses it, in every profile.
  let mut bytes = [0u8; 20];
  bytes[..16].copy_from_slice(&i128::MAX.to_le_bytes());
  assert!(matches!(puns::from_head::<Far>(&bytes), Err(Error::Validity { .. })));
}

/// A pointer to a text behind a `u8` length, at an offset of 16 bytes.
#[derive(Pun)]
#[repr(transparent)]
struct Far {
  #[pun(pointer(Thin<str, u8> = le(0, 16)))]
  off: InPlace<[u8; 16]>,
}

/// The `InPlace` of an offset, by another name.
type Offset = puns::InPlace<i8>;

/// A pointer to the next byte, whose offset is an `InPlace` by an alias, and which is `Debug`.
#[derive(Pun, Debug)]
#[repr(transparent)]
struct Aliased {
  #[pun(pointer(next: u8 = off as u128))]
  off: Offset,
}

/// A pointer to a `Thin` of a type parameter, without `#[pun(bound(..))]`: the derive bounds the
/// pointee by `Pointee`, which a `Thin<T, u8>` is when `T` is `Pun`.
#[derive(Pun)]
#[repr(C)]
struct Generic<T: ?Sized> {
  #[pun(pointer(value: Thin<T, u8> = off as u128))]
  off: InPlace<u8>,
  _t: PhantomData<T>,
}

/// The tests that read the pointees of eager aims through a reference to the value: `name(&self)`,
/// which the derive writes, and is safe, and `puns::aim_mut` from a pointer made from a `&mut`.
///
/// A reference to a value gives the provenance of the bytes of a pointee outside it under tree
/// borrows, the model that `puns` follows for the reads of pointees, but not under stacked borrows.
/// So Miri runs this module with tree borrows only: the jobs with stacked borrows skip it
/// (`--skip reads::`), and run it with tree borrows apart (`.github/workflows/ci.yml`).
mod reads {
  use core::num::NonZero;

  use super::*;

  #[test]
  fn a_chain_is_followed_at_any_depth_on_a_small_stack() {
    // Past the pairs that `from_head` checks in place, its loop of pairs takes the next pair of
    // each, so its stack does not grow with the chain.
    let results = on_a_small_stack(|| {
      let read = |n: usize| {
        let image = chain(n).aligned();
        puns::from_head::<Pair>(image.bytes()).map(|(root, _)| rows(root).len())
      };
      (read(4), read(5), read(LONG))
    });
    assert_eq!(results, (Ok(4), Ok(5), Ok(LONG)));
  }

  #[test]
  fn a_chain_of_pairs() {
    let (image, _) = Image::pairs(&[("a", &[1, 2, 3]), ("bcd", &[4]), ("", &[])]);
    let image = image.aligned();
    let (root, rest) = puns::from_head::<Pair>(image.bytes()).unwrap();
    assert_eq!(rows(root), [row("a", &[1, 2, 3]), row("bcd", &[4]), row("", &[])]);
    // The pointees are after the root, in what `from_head` returns as the rest.
    assert_eq!(rest.len(), image.1 - size_of::<Pair>());
    // The same through `from_bytes` fails on the size: the image is larger than a `Pair`.
    assert_eq!(puns::from_bytes::<Pair>(image.bytes()).err(), Some(Error::Size));
  }

  #[test]
  fn a_generic_type_bounded_by_pun_proves_itself() {
    // Two `List<u32>`: 1, then 2.
    let mut image = Image::new();
    let at = (0..2).map(|_| image.reserve(size_of::<List<u32>>(), 4)).collect::<Vec<_>>();
    for (i, value) in [(0, 1u32), (1, 2)] {
      image.put(at[i], &value.to_ne_bytes());
    }
    image.aim(at[0] + 4, at[1]);
    let image = image.aligned();
    let list = puns::from_head::<List<u32>>(image.bytes()).unwrap().0;
    // `List<u32>` has an identity, made of that of `u32`, and so has the pointee of its aim: the same
    // aim at the same place proves it, and nothing is checked.
    let same: &List<u32> = puns::pun_ref(list);
    let next = same.next.as_ref().map(|next| next.pointee().value);
    assert_eq!((same.value, next), (1, Some(2)));
  }

  #[test]
  fn a_pointee_before_the_pointer() {
    // The pointee, 7, then a pointer 2 bytes after it, which points back at it.
    let words = [7, (-2i16).cast_unsigned(), 0];
    let bytes = puns::as_bytes(&words);
    let (_, back) = puns::from_tail::<Back>(&bytes[..4]).unwrap();
    assert_eq!(back.off.get(), -2);
    assert_eq!(back.back(), &7);
    // The pointee is not in the bytes that are checked.
    assert!(matches!(puns::from_bytes::<Back>(&bytes[2..4]), Err(Error::Validity { .. })));
    // One byte before the pointee is not aligned for a `u16`.
    let words = [7, (-3i16).cast_unsigned(), 0];
    assert!(matches!(
      puns::from_tail::<Back>(&puns::as_bytes(&words)[..4]),
      Err(Error::Validity { .. })
    ));
  }

  #[test]
  fn a_thin_ends_its_prefix_with_the_length() {
    // The pointer at 0, to the length at 8, after the capacity at 4: the words at 12.
    let image = |cap: i32, len: i32| [4 + 4, cap.cast_unsigned(), len.cast_unsigned(), 7, 9];
    let check = |words: &[u32; 5]| puns::from_head::<ToList>(puns::as_bytes(words)).err();
    assert_eq!(check(&image(2, 2)), None);
    // A capacity that is not the length, a negative length, and one past the end.
    assert_eq!(check(&image(3, 2)), Some(Error::Validity { at: 4 }));
    assert_eq!(check(&image(-1, -1)), Some(Error::Validity { at: 0 }));
    assert_eq!(check(&image(3, 3)), Some(Error::Validity { at: 0 }));
    let words = image(2, 2);
    let list = puns::from_head::<ToList>(puns::as_bytes(&words)).unwrap().0;
    assert_eq!(list.list(), &[7, 9][..]);
  }

  #[test]
  fn a_typed_source_is_checked_within_its_bytes() {
    // The first word is an offset of 7, to an address that is not aligned for a `u32`.
    let words = [7u32, 0];
    assert!(matches!(puns::try_pun_ref::<_, [Near<u32>; 2]>(&words), Err(Error::Validity { .. })));
    // Each word points at the other, within the value that holds it.
    let words = [4u32, (-4i32).cast_unsigned()];
    assert!(matches!(puns::try_pun_ref::<_, [Near<u32>; 2]>(&words), Err(Error::Validity { .. })));
    // Past the bytes of the source, though within the array that holds it.
    let words = [4u32, 9];
    let first = puns::head_ref::<[u32; 2], [u32; 1]>(&words);
    assert!(matches!(puns::try_pun_ref::<[u32; 1], Near<u32>>(first), Err(Error::Validity { .. })));
  }

  #[test]
  fn an_unsized_pointee_is_read_with_its_length() {
    // A pointer at 0 to the text at 2, of 3 bytes.
    let mut words = [2, 3, u16::from_ne_bytes(*b"ab"), u16::from_ne_bytes([b'c', 0])];
    let bytes = puns::as_bytes(&words);
    let to = puns::from_head::<ToText>(bytes).unwrap().0;
    let text = to.text();
    assert_eq!((text.len, &text.data), (3, "abc"));
    // To write, through a pointer to the bytes that `puns` checked, with their provenance.
    let to = words.as_mut_ptr().cast::<ToText>();
    // SAFETY: `puns` checked the `ToText` at `to` with its pointee, and neither changed. The bytes
    // are accessed only through `text` while it lives, and an uppercase ASCII text is still one.
    let text = unsafe { puns::aim_mut(to, ToText::AIMS.text) };
    text.data.make_ascii_uppercase();
    assert_eq!(&puns::as_bytes(&words)[4..7], b"ABC");
  }

  #[test]
  fn each_rule_is_proven_or_checked_on_its_own() {
    // A `Counted` at 0, whose pointee, 7, is at 8, outside its 8 bytes.
    let words = [8u32, 1, 7];
    let bytes = puns::as_bytes(&words);
    let counted = puns::from_head::<Counted>(bytes).unwrap().0;
    // The aim of the target is the same aim at the same place of the source: proven, and not looked
    // for again within the bytes of the source, which do not hold its pointee. Only the assertion
    // is checked.
    let below = puns::try_pun_ref::<Counted, CountedBelow>(counted).unwrap();
    assert_eq!((below.count, *below.pointee()), (1, 7));
    let words = [8u32, 5, 7];
    let counted = puns::from_head::<Counted>(puns::as_bytes(&words)).unwrap().0;
    assert!(matches!(
      puns::try_pun_ref::<Counted, CountedBelow>(counted),
      Err(Error::Validity { .. })
    ));
    // A field that is checked next to a proven aim.
    let words = [8u32, 0, 7];
    let counted = puns::from_head::<Counted>(puns::as_bytes(&words)).unwrap().0;
    assert!(matches!(
      puns::try_pun_ref::<Counted, CountedAbove>(counted),
      Err(Error::Validity { .. })
    ));
    // A region at the cursor, proven, and the assertion after it, checked.
    let bytes = [1, 9, 3];
    let regions = puns::from_bytes::<Sized2>(&bytes).unwrap();
    let one = puns::try_pun_ref::<Sized2, SizedOne>(regions).unwrap();
    assert_eq!((one.first(), one.rest()), (Ok(&9), &[3][..]));
    let regions = puns::from_bytes::<Sized2>(&[2, 9, 3]).unwrap();
    assert!(matches!(puns::try_pun_ref::<Sized2, SizedOne>(regions), Err(Error::Validity { .. })));
  }

  #[test]
  fn eager_pointers_of_a_given_size_and_of_the_rest() {
    // The words after the struct, and the rest from its offset 5, in its tail of 3 bytes.
    let mut words = [0u16; 5];
    let bytes =
      puns::from_bytes_mut::<[u8]>(puns::pun_mut::<[u16; 5], [u8; 10]>(&mut words)).unwrap();
    bytes[..7].copy_from_slice(&[8, 2, 5, 3, 1, 2, 3]);
    bytes[8..].copy_from_slice(puns::as_bytes(&[5u16]));
    let bytes: &[u8] = bytes;
    let spans = puns::from_head::<Spans>(bytes).unwrap().0;
    assert_eq!(spans.tail, [1, 2, 3]);
    // The rest is in the struct, which a reference to it covers.
    assert_eq!(spans.rest(), &[2, 3][..]);
    assert_eq!(spans.words(), &[5][..]);
    // A size of 3 bytes is no size of a `[u16]`, and an eager pointee is checked with the struct.
    let mut odd = words;
    puns::pun_mut::<[u16; 5], [u8; 10]>(&mut odd)[1] = 3;
    assert!(matches!(puns::from_head::<Spans>(puns::as_bytes(&odd)), Err(Error::Validity { .. })));
    // The rest of the tail starts in it, or at its end: past it, the struct is not valid.
    let mut past = words;
    puns::pun_mut::<[u16; 5], [u8; 10]>(&mut past)[2] = 7;
    assert_eq!(puns::from_head::<Spans>(puns::as_bytes(&past)).map(|(s, _)| s.rest().len()), Ok(0));
    puns::pun_mut::<[u16; 5], [u8; 10]>(&mut past)[2] = 8;
    assert!(matches!(puns::from_head::<Spans>(puns::as_bytes(&past)), Err(Error::Validity { .. })));
  }

  #[test]
  fn regions_and_eager_aims_compose() {
    // The message at 0, its offset to the header at 4, the header, and a body of two pointers that
    // point at each other.
    let message = |back: i16| [4, 4, 0x1234, 2, back.cast_unsigned()];
    let words = message(-2);
    let message_at =
      |words: &[u16; 5]| puns::from_bytes::<Message>(puns::as_bytes(words)).map(|_| ());
    let bytes = puns::as_bytes(&words);
    let read = puns::from_bytes::<Message>(bytes).unwrap();
    assert_eq!(read.header(), &0x1234u16.to_ne_bytes()[..]);
    let body = read.body().unwrap();
    assert_eq!((body.len(), body[0].off.get(), body[1].off.get()), (2, 2, -2));
    // The two doors read through one place: the word that the message points at is the header,
    // within the message, and the pointees of the pointers of the body are their words.
    let (first, second) = (body[0].back(), body[1].back());
    assert_eq!((*read.word(), *first, *second), (0x1234, (-2i16).cast_unsigned(), 2));
    // The eager aim is checked with the message, and the pointers of the body when it is read,
    // within the bytes of the message: one may point at the header, another region of the message,
    // but not before the message.
    let words = message(-4);
    assert_eq!(message_at(&words), Ok(()));
    let read = puns::from_bytes::<Message>(puns::as_bytes(&words)).unwrap();
    let body = read.body().unwrap();
    assert_eq!(*body[1].back(), 0x1234);
    let words = message(-10);
    let read = puns::from_bytes::<Message>(puns::as_bytes(&words)).unwrap();
    assert!(matches!(read.body(), Err(Error::Validity { .. })));
    let mut words = message(-2);
    words[0] = 100;
    assert!(matches!(message_at(&words), Err(Error::Validity { .. })));
  }

  #[test]
  fn the_field_of_an_eager_aim_is_an_in_place_of_any_spelling() {
    let bytes = [1, 7];
    let aliased = puns::from_head::<Aliased>(&bytes).unwrap().0;
    assert_eq!(format!("{aliased:?}"), "Aliased { off: InPlace(1) }");
    assert_eq!(aliased.next(), &7);
  }

  #[test]
  fn the_same_aim_at_the_same_place_proves_it() {
    let (image, _) = two();
    let image = image.aligned();
    let root = puns::from_head::<Pair>(image.bytes()).unwrap().0;
    // The same type, and a part: proven, nothing is checked.
    let same: &Pair = puns::pun_ref(root);
    let name: &Near<str> = puns::head_ref(root);
    assert_eq!((same.name.pointee(), name.pointee()), ("a", "a"));
    // Plain bytes of a `Pair`, which a shared reference may read.
    assert_eq!(puns::as_bytes(root).len(), size_of::<Pair>());
  }

  #[test]
  fn trees_and_enums() {
    // A tree: 1 with the children 2 and 3, and 3 with the child 4.
    let mut image = Image::new();
    let root = image.reserve(8, 4);
    let kids = image.list(2);
    let (two, grandkids) = (image.list(0), image.list(1));
    let four = image.list(0);
    for (at, id, list) in
      [(root, 1, kids), (kids + 4, 2, two), (kids + 12, 3, grandkids), (grandkids + 4, 4, four)]
    {
      image.put(at, &u32::to_ne_bytes(id));
      image.aim(at + 4, list);
    }
    let image = image.aligned();
    let bytes = image.bytes();
    let tree = puns::from_head::<Tree>(bytes).unwrap().0;
    let ids = |tree: &Tree| tree.kids.pointee().iter().map(|kid| kid.id).collect::<Vec<_>>();
    assert_eq!((tree.id, ids(tree), ids(&tree.kids.pointee()[1])), (1, vec![2, 3], vec![4]));
    // An expression: `1 + 2`. `repr(u8)` lays each variant out as a `repr(C)` struct of the tag and
    // the fields: the `i64` of `Lit` after the tag, at its alignment, and the pointers of `Add` at 4
    // and 8.
    let mut image = Image::new();
    let (size, align) = (size_of::<Expr>(), align_of::<Expr>());
    let at = (0..3).map(|_| image.reserve(size, align)).collect::<Vec<_>>();
    image.bytes[at[0]] = 1;
    image.aim(at[0] + 4, at[1]);
    image.aim(at[0] + 8, at[2]);
    for (i, value) in [(1, 1i64), (2, 2)] {
      image.put(at[i] + align_of::<i64>(), &value.to_ne_bytes());
    }
    let good = image.aligned();
    assert_eq!(puns::from_head::<Expr>(good.bytes()).map(|(e, _)| sum(e)), Ok(3));
    // A wrong tag deep in the tree.
    image.bytes[at[2]] = 7;
    assert!(matches!(
      puns::from_head::<Expr>(image.aligned().bytes()),
      Err(Error::Validity { .. })
    ));
  }

  #[test]
  fn where_a_pointee_starts_is_a_rule() {
    let at = |off: i32| [u32::from_ne_bytes(off.to_be_bytes()), 1, 2];
    // At 8, the second word of the pair at 4.
    let words = at(8);
    let bytes = puns::as_bytes(&words);
    let second = puns::from_head::<Second>(bytes).unwrap().0;
    assert_eq!(second.off.get().get(), 8);
    assert_eq!(second.pair(), &[1, 2]);
    // At 4: the pair at 0, which holds the pointer, which the pointee of an aim that is not shared
    // may not overlap.
    assert!(matches!(
      puns::from_head::<Second>(puns::as_bytes(&at(4))),
      Err(Error::Validity { .. })
    ));
    // At 12: the pair at 8 ends past the bytes.
    assert!(matches!(
      puns::from_head::<Second>(puns::as_bytes(&at(12))),
      Err(Error::Validity { .. })
    ));
    // At 0: the pair would start 4 bytes before the pointer, and `0 - 4` fails.
    assert!(matches!(
      puns::from_head::<Second>(puns::as_bytes(&at(0))),
      Err(Error::Validity { .. })
    ));
  }

  /// A pointer to a `u16` before it or after it, stored as the distance from its own address.
  #[derive(Pun)]
  #[repr(transparent)]
  struct Back {
    #[pun(pointer(back: u16 = off as u128))]
    off: InPlace<i16>,
  }

  #[test]
  fn a_pointee_is_read_through_a_reference_to_its_value() {
    // The pointee, 7, then a pointer 2 bytes after it, which points back at it.
    let words = [7, (-2i16).cast_unsigned()];
    let (_, back) = puns::from_tail::<Back>(puns::as_bytes(&words)).unwrap();
    assert_eq!(*back.back(), 7);
    // Through a proof: the same aim at the same place.
    let same: &Back = puns::pun_ref(back);
    assert_eq!(*same.back(), 7);
  }

  /// Two pairs, in words, in the order of a copy: the first at 0, its name at 12, "ab" behind its
  /// length, its values at 20, one word, and the next pair at 28, with the same name and values
  /// after it.
  const fn pairs() -> [u32; 14] {
    let ab = u32::from_ne_bytes(*b"ab\0\0");
    [12, 16, 20, 2, ab, 1, 9, 12, 16, 0, 2, ab, 1, 9]
  }

  #[test]
  fn a_chain_is_read_through_references() {
    let words = pairs();
    let pair = puns::from_head::<Pair>(puns::as_bytes(&words)).unwrap().0;
    let next = pair.next.as_ref().unwrap().pointee();
    assert_eq!(
      (pair.name.pointee(), pair.values.pointee(), next.name.pointee()),
      ("ab", &[9][..], "ab")
    );
    assert!(next.next.is_none());
  }

  #[test]
  fn a_tree_is_read_through_references() {
    // A root of id 1 at 0, whose two children follow their length at 8: ids 2 and 3, with the
    // empty lists at 28 and 32 as their children.
    let words = [1, 4, 2, 2, 12, 3, 8, 0, 0];
    let tree = puns::from_head::<Tree>(puns::as_bytes(&words)).unwrap().0;
    let ids = tree.kids.pointee().iter().map(|kid| (kid.id, kid.kids.pointee().len()));
    assert_eq!(ids.collect::<Vec<_>>(), [(2, 0), (3, 0)]);
  }

  #[test]
  fn an_expression_is_read_through_references() {
    fn sum(e: &Expr) -> i64 {
      match e {
        Expr::Lit(value) => *value,
        Expr::Add(a, b) => sum(a.pointee()) + sum(b.pointee()),
      }
    }
    // `1 + 2`: the tag of `Add` and its pointers at 4 and 8, then two `Lit`s. An `Expr` is 12 bytes
    // where an `i64` is aligned to 4, as on 32-bit x86.
    let (size, align) = (size_of::<Expr>(), align_of::<Expr>());
    let mut words = vec![0u64; (3 * size).div_ceil(8)];
    let bytes = puns::from_bytes_mut::<[u8]>(puns::pun_mut::<[u64], [u8]>(&mut words[..])).unwrap();
    bytes[0] = 1;
    for (at, to) in [(4, size), (8, 2 * size)] {
      let off = i32::try_from(to - at).unwrap();
      bytes[at..at + 4].copy_from_slice(&off.to_ne_bytes());
    }
    for (i, value) in [(1, 1i64), (2, 2)] {
      bytes[i * size + align..i * size + align + 8].copy_from_slice(&value.to_ne_bytes());
    }
    let bytes: &[u8] = bytes;
    assert_eq!(puns::from_head::<Expr>(bytes).map(|(e, _)| sum(e)), Ok(3));
  }

  /// A pointer to a text, whose pointee is read to write too.
  #[derive(Pun)]
  #[repr(transparent)]
  struct Text {
    #[pun(pointer(text: Thin<str, i16> = off as u128))]
    off: InPlace<NonZero<i16>>,
  }

  #[test]
  fn a_pointee_is_written_through_a_mutable_reference_to_its_value() {
    // The pointer at 0, the length at 2, then the text.
    let mut words = [2i16, 2, i16::from_ne_bytes(*b"hi")];
    let at = words.as_mut_ptr().cast::<Text>();
    // SAFETY: `puns` checks the `Text` at the start of `words`, with its pointee, below. The bytes
    // of the pointee are accessed only through `text` while it lives, and an uppercase ASCII text is
    // still one.
    let text = unsafe {
      assert!(puns::from_head::<Text>(puns::as_bytes(&*at.cast::<[i16; 3]>())).is_ok());
      puns::aim_mut(core::ptr::from_mut(&mut *at), Text::AIMS.text)
    };
    text.make_ascii_uppercase();
    assert_eq!(&puns::as_bytes(&words)[4..], b"HI");
    assert!(matches!(
      puns::from_head::<Text>(puns::as_bytes(&[2i16, -1, 0])),
      Err(Error::Validity { .. })
    ));
  }

  /// The path of `puns`, renamed: what the derive names through `#[pun(crate = ..)]`.
  mod renamed {
    pub use puns::*;
  }

  /// A pointer to the byte after it, derived through another path of `puns`.
  #[derive(Pun)]
  #[pun(crate = renamed)]
  #[repr(transparent)]
  struct Renamed {
    #[pun(pointer(next: u8 = off as u128))]
    off: renamed::InPlace<u8>,
  }

  #[test]
  fn a_crate_path_that_names_puns_reads_the_same_pointee() {
    let bytes = [1, 9];
    let renamed = puns::from_head::<Renamed>(&bytes).unwrap().0;
    assert_eq!(*renamed.next(), 9);
    assert!(matches!(puns::from_head::<Renamed>(&[2, 9]), Err(Error::Validity { .. })));
  }

  /// The pointee `P` of the `To<P>` at the start of `bytes`.
  fn read<'a, P: ?Sized + Pointee + 'a>(bytes: &'a [u8]) -> Result<&'a P::Value, Error> {
    puns::from_head::<To<P>>(bytes).map(|(to, _)| to.pointee())
  }

  #[test]
  fn a_thin_of_a_struct_with_a_tail() {
    // An offset of 4, to an `i32` length of 2, then a `Record` of 2 elements: the kind, then the
    // data.
    let mut words = [0u32; 4];
    let bytes =
      puns::from_bytes_mut::<[u8]>(puns::pun_mut::<[u32; 4], [u8; 16]>(&mut words)).unwrap();
    bytes[0] = 4;
    bytes[4..8].copy_from_slice(&2i32.to_ne_bytes());
    bytes[8..10].copy_from_slice(&7u16.to_ne_bytes());
    bytes[10..14].copy_from_slice(&[1, 0, 2, 0]);
    let bytes: &[u8] = bytes;
    let record = read::<Thin<Record, i32>>(bytes).unwrap();
    assert_eq!((record.kind, record.data.len()), (7, 2));
    // A length that does not fit: the struct is not valid.
    assert!(matches!(read::<Thin<Record, i32>>(&bytes[..12]), Err(Error::Validity { .. })));
  }

  #[test]
  fn the_length_of_a_thin_has_the_sign_of_its_type() {
    // An offset of 1, to an unsigned length of 200, then 200 bytes.
    let mut bytes = [0u8; 202];
    bytes[..2].copy_from_slice(&[1, 200]);
    assert_eq!(read::<Thin<[u8], u8>>(&bytes).map(<[u8]>::len), Ok(200));
    // The same byte as a signed length is negative, a size that no bytes have.
    assert!(matches!(read::<Thin<[u8], i8>>(&bytes), Err(Error::Validity { .. })));
    // An offset of 2 in the first byte, to a `u16` length of 40,000, in the byte order of the
    // target.
    let mut words = vec![0u16; 20_002];
    words[..2].copy_from_slice(&[u16::from_ne_bytes([2, 0]), 40_000]);
    assert_eq!(read::<Thin<[u8], u16>>(puns::as_bytes(&words[..])).map(<[u8]>::len), Ok(40_000));
  }

  #[test]
  fn the_value_of_a_thin_follows_its_length() {
    // The words of a `Thin<[u32], i16>` are aligned to 4, and its 2-byte length is right before
    // them, so an offset of 2 leads to one word at 4.
    let mut words = [0u32; 2];
    let bytes =
      puns::from_bytes_mut::<[u8]>(puns::pun_mut::<[u32; 2], [u8; 8]>(&mut words)).unwrap();
    bytes[0] = 2;
    bytes[2..4].copy_from_slice(&1i16.to_ne_bytes());
    bytes[4..8].copy_from_slice(&9u32.to_ne_bytes());
    assert_eq!(read::<Thin<[u32], i16>>(bytes), Ok(&[9][..]));
    // An offset of 0 leads to a length whose words would be at 2, which is not aligned for them:
    // the struct is not valid.
    bytes[0] = 0;
    assert!(matches!(read::<Thin<[u32], i16>>(bytes), Err(Error::Validity { .. })));
  }

  #[test]
  fn pointers_of_a_given_size_and_of_a_given_length() {
    // The table, then two rows, then a text of two bytes.
    let mut words = [0u16; 5];
    let bytes =
      puns::from_bytes_mut::<[u8]>(puns::pun_mut::<[u16; 5], [u8; 10]>(&mut words)).unwrap();
    bytes[..4].copy_from_slice(&[4, 4, 8, 2]);
    bytes[4..8].copy_from_slice(puns::as_bytes(&[7u16, 9]));
    bytes[8..].copy_from_slice(b"ok");
    let bytes: &[u8] = bytes;
    let table = puns::from_head::<Table>(bytes).unwrap().0;
    assert_eq!((table.rows(), table.text()), (&[7, 9][..], "ok"));
    // An odd size of rows, and rows past the bytes: the table is not valid.
    for len in [3, 8] {
      let mut odd = words;
      puns::pun_mut::<[u16; 5], [u8; 10]>(&mut odd)[1] = len;
      assert!(matches!(
        puns::from_head::<Table>(puns::as_bytes(&odd)),
        Err(Error::Validity { .. })
      ));
    }
    // A text that is not UTF-8.
    let mut bad = words;
    puns::pun_mut::<[u16; 5], [u8; 10]>(&mut bad)[9] = 0xff;
    assert!(matches!(puns::from_head::<Table>(puns::as_bytes(&bad)), Err(Error::Validity { .. })));
  }

  #[test]
  fn a_generic_pointee_is_bounded_by_pointee() {
    let bytes = [1, 2, b'h', b'i'];
    let generic = puns::from_head::<Generic<str>>(&bytes).unwrap().0;
    assert_eq!(generic.value(), "hi");
    let generic = puns::from_head::<Generic<u8>>(&bytes).unwrap().0;
    assert_eq!(generic.value(), &2);
  }
}
