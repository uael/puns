//! Eager aims: a self-relative pointer declared through the public API (`support/aims.rs`), whose
//! targets `puns` checks with the value, and `Thin`.

use core::marker::PhantomData;

#[path = "support/aims.rs"]
mod aims;

use aims::{Aimed, Expr, Near, Pair, Tree};
use puns::{
  Error, Failure, InPlace, Pending, Pointee, Pun, Thin, Worklist,
  num::{i32be, u8c},
};

/// The target of `near`, in `image`, the bytes in which `puns` checked it.
fn to<'a, T: ?Sized + Aimed>(near: &'a Near<T>, image: &'a [u8]) -> &'a T {
  // SAFETY: Every caller passes the bytes that `puns` checked and that hold `near`.
  unsafe { near.get(image) }
}

fn row(name: &str, values: &[u32]) -> (String, Vec<u32>) {
  (name.to_string(), values.to_vec())
}

#[test]
fn a_chain_of_pairs() {
  let (image, _) = Image::pairs(&[("a", &[1, 2, 3]), ("bcd", &[4]), ("", &[])]);
  let image = image.aligned();
  let (root, rest) = puns::from_head::<Pair>(image.bytes()).unwrap();
  assert_eq!(rows(root, image.bytes()), [row("a", &[1, 2, 3]), row("bcd", &[4]), row("", &[])]);
  // The targets are after the root, in what `from_head` returns as the rest.
  assert_eq!(rest.len(), image.1 - size_of::<Pair>());
  // The same through `from_bytes` fails on the size: the image is larger than a `Pair`.
  assert_eq!(puns::from_bytes::<Pair>(image.bytes()).err(), Some(Error::Size));
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
  // A zero offset is no `Near`, and a zero `Option<Near>` is `None`.
  assert_eq!(set(at[0], 0), Err(Error::Validity));
  assert_eq!(set(at[0] + 8, 0), Ok(()));
  // Past the end, before the start, and the start of the target past the end by one.
  assert_eq!(set(at[0], 10_000), Err(Error::Validity));
  assert_eq!(set(at[1] + 4, -i32::try_from(at[1] + 8).unwrap()), Err(Error::Validity));
  assert_eq!(set(at[0], i32::try_from(image.bytes.len()).unwrap() + 1), Err(Error::Validity));
  // The next pair one byte later: not aligned.
  assert_eq!(set(at[0] + 8, image.get_i32(at[0] + 8) + 1), Err(Error::Validity));
  // A length past the end, and a negative length.
  let name = at[0] + usize::try_from(image.get_i32(at[0])).unwrap();
  assert_eq!(set(name, 1000), Err(Error::Validity));
  assert_eq!(set(name, -1), Err(Error::Validity));
  // The values of the next pair: past the end, with a capacity past the end too, and a capacity
  // that is not the length, which the rule of `ListLen` refuses.
  let values = at[1] + 4 + usize::try_from(image.get_i32(at[1] + 4)).unwrap();
  let mut long = two().0;
  long.put_i32(values - 4, 1000);
  long.put_i32(values, 1000);
  assert_eq!(check(&long), Err(Error::Validity));
  assert_eq!(set(values - 4, 2), Err(Error::Validity));
  // Invalid UTF-8 in the name of the next pair.
  let mut bad = two().0;
  let name = at[1] + usize::try_from(image.get_i32(at[1])).unwrap();
  bad.bytes[name + 4] = 0xff;
  assert_eq!(check(&bad), Err(Error::Validity));
  // One byte short.
  let mut short = two().0;
  short.bytes.pop();
  assert_eq!(check(&short), Err(Error::Validity));
}

#[test]
fn a_cycle_stops_at_the_limit() {
  let (mut image, at) = two();
  // The second pair points back at the first.
  image.aim(at[1] + 8, at[0]);
  assert_eq!(check(&image), Err(Error::Limit));
}

#[test]
fn a_chain_is_followed_to_its_depth() {
  let chain = |n: usize| {
    let mut image = Image::new();
    let at = (0..n).map(|_| image.reserve(size_of::<Pair>(), 4)).collect::<Vec<_>>();
    // One name and one empty list, which every pair shares.
    let (text, words) = (image.text(""), image.words(&[]));
    for i in 0..n {
      image.aim(at[i], text);
      image.aim(at[i] + 4, words);
      if i + 1 < n {
        image.aim(at[i] + 8, at[i + 1]);
      }
    }
    let image = image.aligned();
    puns::from_head::<Pair>(image.bytes()).map(|(root, _)| rows(root, image.bytes()).len())
  };
  // The pair `k` is `k` levels deep, and its name and its values one more: `AIM_DEPTH` levels
  // reach the names of `AIM_DEPTH` pairs. The check recurses once for each level, and the deepest
  // one fits in 1 MiB of stack, in a build without optimizations too (`AIM_DEPTH`).
  let depth = puns::AIM_DEPTH;
  let small = std::thread::Builder::new().stack_size(1 << 20);
  let (ok, limit) = small
    .spawn(move || if cfg!(miri) { (chain(4), Ok(4)) } else { (chain(depth), chain(depth + 1)) })
    .unwrap()
    .join()
    .unwrap();
  assert_eq!(ok, Ok(if cfg!(miri) { 4 } else { depth }));
  assert_eq!(limit, if cfg!(miri) { Ok(4) } else { Err(Error::Limit) });
}

/// The value of an expression, in `image`.
fn sum(e: &Expr, image: &[u8]) -> i64 {
  match e {
    Expr::Lit(v) => *v,
    Expr::Add(a, b) => sum(to(a, image), image) + sum(to(b, image), image),
  }
}

#[test]
fn trees_and_enums() {
  // A tree: 1 with the children 2 and 3, and 3 with the child 4.
  let mut image = Image::new();
  let root = image.reserve(8, 4);
  let (kids, grandkids, empty) = (image.list(2), image.list(1), image.list(0));
  for (at, id, list) in
    [(root, 1, kids), (kids + 4, 2, empty), (kids + 12, 3, grandkids), (grandkids + 4, 4, empty)]
  {
    image.put(at, &u32::to_ne_bytes(id));
    image.aim(at + 4, list);
  }
  let image = image.aligned();
  let bytes = image.bytes();
  let tree = puns::from_head::<Tree>(bytes).unwrap().0;
  let ids = |tree: &Tree| to(&tree.kids, bytes).iter().map(|kid| kid.id).collect::<Vec<_>>();
  assert_eq!((tree.id, ids(tree), ids(&to(&tree.kids, bytes)[1])), (1, vec![2, 3], vec![4]));
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
  assert_eq!(puns::from_head::<Expr>(good.bytes()).map(|(e, _)| sum(e, good.bytes())), Ok(3));
  // A wrong tag deep in the tree.
  image.bytes[at[2]] = 7;
  assert_eq!(puns::from_head::<Expr>(image.aligned().bytes()).err(), Some(Error::Validity));
}

#[test]
fn a_typed_source_is_checked_within_its_bytes() {
  // The first word is an offset of 7, to an address that is not aligned for a `u32`.
  let words = [7u32, 0];
  assert_eq!(puns::try_pun_ref::<_, [Near<u32>; 2]>(&words).err(), Some(Error::Validity));
  // Each word points at the other.
  let words = [4u32, (-4i32).cast_unsigned()];
  let near = puns::try_pun_ref::<_, [Near<u32>; 2]>(&words).unwrap();
  let bytes = puns::as_bytes(&words);
  assert_eq!((*to(&near[0], bytes), *to(&near[1], bytes)), ((-4i32).cast_unsigned(), 4));
  // Past the bytes of the source, though within the array that holds it.
  let words = [4u32, 9];
  let first = puns::head_ref::<[u32; 2], [u32; 1]>(&words);
  assert_eq!(puns::try_pun_ref::<[u32; 1], Near<u32>>(first).err(), Some(Error::Validity));
}

#[test]
fn the_same_aim_at_the_same_place_proves_it() {
  let (image, _) = two();
  let image = image.aligned();
  let root = puns::from_head::<Pair>(image.bytes()).unwrap().0;
  // The same type, and a part: proven, nothing is checked.
  let same: &Pair = puns::pun_ref(root);
  let name: &Near<str> = puns::head_ref(root);
  assert_eq!((to(&same.name, image.bytes()), to(name, image.bytes())), ("a", "a"));
  // Plain bytes of a `Pair`, which a shared reference may read.
  assert_eq!(puns::as_bytes(root).len(), size_of::<Pair>());
}

/// A list of values of a generic type, whose parameter is bounded by `Pun`.
#[derive(Pun)]
#[repr(C)]
struct List<T: Pun> {
  value: T,
  next: Option<Near<List<T>>>,
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
  // `List<u32>` has an identity, made of that of `u32`, and so has the target of its aim: the same
  // aim at the same place proves it, and nothing is checked.
  let same: &List<u32> = puns::pun_ref(list);
  let next = same.next.as_ref().map(|next| to(next, image.bytes()).value);
  assert_eq!((same.value, next), (1, Some(2)));
}

/// A struct that ends with a slice, behind its stored length.
#[derive(Pun)]
#[repr(C)]
struct Record {
  kind: u16,
  data: [u16],
}

/// A pointer to a `P`, after its offset, a byte, checked when it is read.
#[derive(Pun)]
#[pun(bound(P: puns::Pointee))]
#[repr(C)]
struct To<P: ?Sized> {
  #[pun(pointer(lazy pointee: P = at as u128))]
  at: u8,
  _p: PhantomData<P>,
}

/// The pointee `P` of the `To<P>` at the start of `bytes`, read in `bytes`.
fn read<P: ?Sized + Pointee>(bytes: &[u8]) -> Result<&P::Value, Error> {
  puns::from_head::<To<P>>(bytes)?.0.pointee(bytes)
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
  // A length that does not fit: the check of a pointer requires that its pointee fits.
  assert_eq!(read::<Thin<Record, i32>>(&bytes[..12]).err(), Some(Error::Validity));
}

#[test]
fn the_length_of_a_thin_has_the_sign_of_its_type() {
  // An offset of 1, to an unsigned length of 200, then 200 bytes.
  let mut bytes = [0u8; 202];
  bytes[..2].copy_from_slice(&[1, 200]);
  assert_eq!(read::<Thin<[u8], u8>>(&bytes).map(<[u8]>::len), Ok(200));
  // The same byte as a signed length is negative, a size that no bytes have, so the pointee does
  // not fit.
  assert_eq!(read::<Thin<[u8], i8>>(&bytes).err(), Some(Error::Validity));
  // An offset of 2, to a `u16` length of 40,000.
  let mut words = vec![0u16; 20_002];
  words[..2].copy_from_slice(&[2, 40_000]);
  assert_eq!(read::<Thin<[u8], u16>>(puns::as_bytes(&words[..])).map(<[u8]>::len), Ok(40_000));
}

#[test]
fn the_value_of_a_thin_follows_its_length() {
  // The words of a `Thin<[u32], i16>` are aligned to 4, and its 2-byte length is right before
  // them, so an offset of 2 leads to one word at 4.
  let mut words = [0u32; 2];
  let bytes = puns::from_bytes_mut::<[u8]>(puns::pun_mut::<[u32; 2], [u8; 8]>(&mut words)).unwrap();
  bytes[0] = 2;
  bytes[2..4].copy_from_slice(&1i16.to_ne_bytes());
  bytes[4..8].copy_from_slice(&9u32.to_ne_bytes());
  assert_eq!(read::<Thin<[u32], i16>>(bytes), Ok(&[9][..]));
  // An offset of 0 leads to a length whose words would be at 2, which is not aligned for them. The
  // pointee of a lazy pointer fits there, and its read refuses it, as `from_bytes` would.
  bytes[0] = 0;
  assert_eq!(read::<Thin<[u32], i16>>(bytes).err(), Some(Error::Alignment));
}

/// A pointer to a `u16` before it or after it, stored as the distance from its own address.
#[derive(Pun)]
#[repr(transparent)]
struct Back {
  #[pun(pointer(back: u16 = off as u128))]
  off: InPlace<i16>,
}

/// `value`, which `puns` checked in `bytes`, at its address in them with their provenance: what the
/// read of a target needs under the stacked borrows model of Miri.
fn within<T>(value: &T, bytes: &[u8]) -> *const T {
  bytes.as_ptr().with_addr(core::ptr::from_ref(value).addr()).cast()
}

#[test]
fn a_target_before_the_pointer() {
  // The target, 7, then a pointer 2 bytes after it, which points back at it.
  let words = [7, (-2i16).cast_unsigned(), 0];
  let bytes = puns::as_bytes(&words);
  let (_, back) = puns::from_tail::<Back>(&bytes[..4]).unwrap();
  assert_eq!(back.off.get(), -2);
  // SAFETY: `puns` checked `back` with its target, in `bytes`, which are not written.
  assert_eq!(unsafe { Back::back(within(back, bytes)) }, &7);
  // The target is not in the bytes that are checked.
  assert_eq!(puns::from_bytes::<Back>(&bytes[2..4]).err(), Some(Error::Validity));
  // One byte before the target is not aligned for a `u16`.
  let words = [7, (-3i16).cast_unsigned(), 0];
  assert_eq!(puns::from_tail::<Back>(&puns::as_bytes(&words)[..4]).err(), Some(Error::Validity));
}

/// A pointer whose distance is big-endian on every target, to a pair of words that starts 4
/// bytes before the place where it aims, its second word.
#[derive(Pun)]
#[repr(transparent)]
struct Second {
  #[pun(pointer(pair: [u32; 2] = off as u128 - 4))]
  off: InPlace<i32be>,
}

#[test]
fn where_a_target_starts_is_a_rule() {
  let at = |off: i32| [u32::from_ne_bytes(off.to_be_bytes()), 1, 2];
  // At 8, the second word of the pair at 4.
  let words = at(8);
  let bytes = puns::as_bytes(&words);
  let second = puns::from_head::<Second>(bytes).unwrap().0;
  assert_eq!(second.off.get().get(), 8);
  // SAFETY: `puns` checked `second` with its target, in `bytes`, which are not written.
  assert_eq!(unsafe { Second::pair(within(second, bytes)) }, &[1, 2]);
  // At 4: the pair at 0, which holds the pointer, as a target may.
  assert!(puns::from_head::<Second>(puns::as_bytes(&at(4))).is_ok());
  // At 12: the pair at 8 ends past the bytes.
  assert_eq!(puns::from_head::<Second>(puns::as_bytes(&at(12))).err(), Some(Error::Validity));
  // At 0: the pair would start 4 bytes before the pointer, and `0 - 4` fails.
  assert_eq!(puns::from_head::<Second>(puns::as_bytes(&at(0))).err(), Some(Error::Validity));
}

/// A message that points at a word before or after it, and whose tail is regions: a header, then a
/// body of pointers, each to a word of the body.
#[derive(Pun)]
#[repr(C)]
struct Message {
  #[pun(pointer(word: u16 = off as u128))]
  off: InPlace<i16>,
  body_len: u16,
  #[pun(regions(header = 2, body: [Back] = body_len))]
  rest: [u8],
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
  // The two doors read through one place: the word that the message points at is the header, and
  // the targets of the pointers of the body are their words, read with the provenance of the
  // bytes, which hold them all. The message is at the start of the bytes, and unsized: a pointer to
  // its slice, cast to a pointer to it, has the same metadata.
  #[expect(clippy::cast_ptr_alignment, reason = "the address of `read`, which is aligned")]
  let holder = core::ptr::slice_from_raw_parts(bytes.as_ptr(), read.rest.len()) as *const Message;
  // SAFETY: `puns` checked `read` with its target, and the region `body` with the targets of its
  // pointers, in `bytes`, which are not written.
  let (word, first, second) = unsafe {
    (
      Message::word(holder),
      Back::back(within(&body[0], bytes)),
      Back::back(within(&body[1], bytes)),
    )
  };
  assert_eq!((*word, *first, *second), (0x1234, (-2i16).cast_unsigned(), 2));
  // The eager aim is checked with the message, and the pointers of the body when it is read,
  // within the bytes of the message: one may point at the header, another region of the message,
  // but not before the message.
  let words = message(-4);
  assert_eq!(message_at(&words), Ok(()));
  let bytes = puns::as_bytes(&words);
  let read = puns::from_bytes::<Message>(bytes).unwrap();
  let body = read.body().unwrap();
  // SAFETY: `puns` checked the region `body` with the targets of its pointers, in the bytes of the
  // message, which `bytes` holds and which are not written.
  assert_eq!(unsafe { *Back::back(within(&body[1], bytes)) }, 0x1234);
  let words = message(-10);
  let read = puns::from_bytes::<Message>(puns::as_bytes(&words)).unwrap();
  assert_eq!(read.body().err(), Some(Error::Validity));
  let mut words = message(-2);
  words[0] = 100;
  assert_eq!(message_at(&words), Err(Error::Validity));
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
  #[pun(pointer(mut text: Text = off as u128))]
  off: InPlace<i16>,
}

#[test]
fn an_unsized_target_is_read_with_its_length() {
  // A pointer at 0 to the text at 2, of 3 bytes.
  let mut words = [2, 3, u16::from_ne_bytes(*b"ab"), u16::from_ne_bytes([b'c', 0])];
  let bytes = puns::as_bytes(&words);
  let to = puns::from_head::<ToText>(bytes).unwrap().0;
  // SAFETY: `puns` checked `to` with its target, in `bytes`, which are not written.
  let text = unsafe { ToText::text(within(to, bytes)) };
  assert_eq!((text.len, &text.data), (3, "abc"));
  // To write, through a pointer to the bytes that `puns` checked, with their provenance.
  let to = words.as_mut_ptr().cast::<ToText>();
  // SAFETY: `puns` checked the `ToText` at `to` with its target, and neither changed. The bytes
  // are accessed only through `text` while it lives, and an uppercase ASCII text is still one.
  let text = unsafe { ToText::text_mut(to) };
  text.data.make_ascii_uppercase();
  assert_eq!(&puns::as_bytes(&words)[4..7], b"ABC");
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

  /// Writes a list of `n` elements of 8 bytes with nothing in them, and returns where its length
  /// is, where a `Near` points.
  fn list(&mut self, n: usize) -> usize {
    let n32 = i32::try_from(n).unwrap();
    let at = self.reserve(8 + 8 * n, 4);
    self.put_i32(at, n32);
    self.put_i32(at + 4, n32);
    at + 4
  }

  /// Writes a `Thin<[u32], ListLen>`, the capacity, the length, then the words, and returns where
  /// its length is, where a `Near` points.
  fn words(&mut self, words: &[u32]) -> usize {
    let n = i32::try_from(words.len()).unwrap();
    let at = self.reserve(8 + 4 * words.len(), 4);
    self.put_i32(at, n);
    self.put_i32(at + 4, n);
    for (i, word) in words.iter().enumerate() {
      self.put(at + 8 + 4 * i, &word.to_ne_bytes());
    }
    at + 4
  }

  /// Writes a chain of pairs, the first at 0, and returns where each is.
  fn pairs(rows: &[(&str, &[u32])]) -> (Self, Vec<usize>) {
    let mut image = Self::new();
    let at = (0..rows.len()).map(|_| image.reserve(size_of::<Pair>(), 4)).collect::<Vec<_>>();
    for (i, (name, values)) in rows.iter().enumerate() {
      let text = image.text(name);
      image.aim(at[i], text);
      let words = image.words(values);
      image.aim(at[i] + 4, words);
      if i + 1 < rows.len() {
        image.aim(at[i] + 8, at[i + 1]);
      }
    }
    (image, at)
  }

  /// Writes a chain of pairs as a writer that copies the first one depth first does: each pair,
  /// then its name, its values, and the next pair, each at the next place aligned for it.
  fn compact(rows: &[(&str, &[u32])]) -> Self {
    let mut image = Self::new();
    let mut next = None;
    for (name, values) in rows {
      let at = image.reserve(size_of::<Pair>(), 4);
      if let Some(next) = next {
        image.aim(next, at);
      }
      let text = image.text(name);
      image.aim(at, text);
      let words = image.words(values);
      image.aim(at + 4, words);
      next = Some(at + 8);
    }
    image
  }

  /// A copy of the bytes, at an address aligned to 8, of any length, after `skip` zeros.
  fn words8(&self, skip: usize) -> Vec<u64> {
    let mut bytes = vec![0; skip];
    bytes.extend_from_slice(&self.bytes);
    bytes
      .chunks(8)
      .map(|chunk| {
        let mut word = [0; 8];
        word[..chunk.len()].copy_from_slice(chunk);
        u64::from_ne_bytes(word)
      })
      .collect()
  }

  /// A copy of the bytes at an address aligned to 8.
  fn aligned(&self) -> Aligned {
    let mut copy = Aligned([0; 4096], self.bytes.len());
    copy.0[..self.bytes.len()].copy_from_slice(&self.bytes);
    copy
  }
}

/// Bytes at an address aligned to 8.
#[repr(C, align(8))]
struct Aligned([u8; 4096], usize);

impl Aligned {
  fn bytes(&self) -> &[u8] {
    &self.0[..self.1]
  }
}

/// The rows of a chain of pairs.
fn rows(root: &Pair, image: &[u8]) -> Vec<(String, Vec<u32>)> {
  let mut out = vec![];
  let mut at = Some(root);
  while let Some(pair) = at {
    out.push((to(&pair.name, image).to_string(), to(&pair.values, image).to_vec()));
    at = pair.next.as_ref().map(|next| to(next, image));
  }
  out
}

/// Rows at a stored place, of a given size, and the rest of the bytes from another, both checked
/// when they are read, from the bytes that hold the table.
#[derive(Pun)]
#[repr(C)]
struct Table {
  #[pun(pointer(lazy rows: [u16] = at as u128, size = len as u128))]
  at: u8,
  len: u8,
  #[pun(pointer(lazy rest: str = from as u128, size = ..))]
  from: u8,
  version: u8,
}

#[test]
fn lazy_pointers_are_checked_when_read() {
  // The table, then two rows, then a text that runs to the end.
  let mut words = [0u16; 5];
  let bytes =
    puns::from_bytes_mut::<[u8]>(puns::pun_mut::<[u16; 5], [u8; 10]>(&mut words)).unwrap();
  bytes[..4].copy_from_slice(&[4, 4, 8, 1]);
  bytes[4..8].copy_from_slice(puns::as_bytes(&[7u16, 9]));
  bytes[8..].copy_from_slice(b"ok");
  let bytes: &[u8] = bytes;
  let table = puns::from_head::<Table>(bytes).unwrap().0;
  assert_eq!((table.rows(bytes), table.rest(bytes)), (Ok(&[7, 9][..]), Ok("ok")));
  // The table, read from bytes that do not hold it, has no rows.
  assert_eq!(table.rows(&bytes[1..]).err(), Some(Error::Validity));
  // The check of the table requires only that its pointees fit: an odd size of rows is a size
  // that no rows have, refused when they are read.
  let mut odd = words;
  puns::pun_mut::<[u16; 5], [u8; 10]>(&mut odd)[1] = 3;
  let odd = puns::as_bytes(&odd);
  let table = puns::from_head::<Table>(odd).unwrap().0;
  assert_eq!(table.rows(odd).err(), Some(Error::Size));
  // Rows past the bytes do not fit.
  let mut long = words;
  puns::pun_mut::<[u16; 5], [u8; 10]>(&mut long)[1] = 8;
  assert_eq!(puns::from_head::<Table>(puns::as_bytes(&long)).err(), Some(Error::Validity));
}

/// Words at a stored place, of a given size, and the rest of the bytes from another place, both
/// checked with the struct.
#[derive(Pun)]
#[repr(C)]
struct Spans {
  #[pun(pointer(mut words: [u16] = off as u128, size = len as u128))]
  off: InPlace<i8>,
  len: u8,
  #[pun(pointer(rest: [u8] = from as u128, size = ..))]
  from: InPlace<u8>,
  _pad: u8,
}

#[test]
fn eager_pointers_of_a_given_size_and_of_the_rest() {
  let mut words = [0u16; 4];
  let bytes = puns::from_bytes_mut::<[u8]>(puns::pun_mut::<[u16; 4], [u8; 8]>(&mut words)).unwrap();
  bytes[..4].copy_from_slice(&[4, 4, 6, 0]);
  bytes[4..].copy_from_slice(puns::as_bytes(&[5u16, 6]));
  let bytes: &[u8] = bytes;
  let spans = puns::from_head::<Spans>(bytes).unwrap().0;
  let end = bytes.as_ptr_range().end;
  // SAFETY: `puns` checked `spans` with its pointees, in `bytes`, which end at `end` and are not
  // written.
  let (all, rest) =
    unsafe { (Spans::words(within(spans, bytes)), Spans::rest(within(spans, bytes), end)) };
  assert_eq!((all, rest), (&[5, 6][..], &bytes[6..]));
  // A size of 3 bytes is no size of a `[u16]`, and an eager pointee is checked with the struct.
  let mut odd = words;
  puns::pun_mut::<[u16; 4], [u8; 8]>(&mut odd)[1] = 3;
  assert_eq!(puns::from_head::<Spans>(puns::as_bytes(&odd)).err(), Some(Error::Validity));
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
  // SAFETY: `puns` checked `packet` with its eager regions, in `bytes`, which are not written.
  let (version, all) = unsafe { (Packet::version(packet), Packet::words(packet)) };
  assert_eq!((version.get(), all, packet.rest()), (2, &[7, 8][..], &[][..]));
  // An eager region is checked with the struct: another version is not one.
  let mut other = bytes.to_vec();
  other[6] = 3;
  let other = Image { bytes: other }.aligned();
  assert_eq!(puns::from_bytes::<Packet>(other.bytes()).err(), Some(Error::Validity));
  // Where the words start depends on their offset in the packet, not on its address: from an odd
  // address, they are at the same offset, which is not aligned, and the packet is not valid.
  let mut shifted = [0u8; 13];
  shifted[1..].copy_from_slice(bytes);
  assert_eq!(puns::from_bytes::<Packet>(&shifted[1..]).err(), Some(Error::Validity));
}

/// Entries on the heap, as many as a check needs.
struct Grows(Vec<Pending>);

impl Worklist for Grows {
  fn items(&mut self) -> &mut [Pending] {
    &mut self.0
  }

  fn grow(&mut self) -> bool {
    self.0.resize(self.0.len().max(4) * 2, Pending::default());
    true
  }
}

/// `puns::check` of a `Pair` at the start of `bytes`, with a worklist that grows.
fn image_of(bytes: &[u8], align: usize, compact: bool) -> Result<(), Failure> {
  puns::check::<Pair, _>(bytes, align, compact, &mut Grows(Vec::new()))
}

#[test]
fn a_worklist_goes_deeper_than_the_recursion() {
  // A chain of pairs ten times deeper than the recursion goes, each with a name and values.
  let names = (0..10 * puns::AIM_DEPTH).map(|i| format!("{i}")).collect::<Vec<_>>();
  let rows = names.iter().map(|name| (name.as_str(), &[1, 2][..])).collect::<Vec<_>>();
  let image = Image::compact(&rows);
  let words = image.words8(0);
  let bytes = &puns::as_bytes(&words[..])[..image.bytes.len()];
  assert_eq!(puns::from_head::<Pair>(bytes).err(), Some(Error::Limit));
  assert_eq!(image_of(bytes, 8, true), Ok(()));
  assert_eq!(image_of(bytes, 8, false), Ok(()));
  // A worklist that cannot grow, too small for the two pointees that wait at each pair.
  let mut work = [Pending::default(); 1];
  let full = Failure { error: Error::Limit, at: 0 };
  assert_eq!(puns::check::<Pair, _>(bytes, 8, true, &mut work), Err(full));
  let mut work = [Pending::default(); 3];
  assert_eq!(puns::check::<Pair, _>(bytes, 8, true, &mut work), Ok(()));
}

#[test]
fn a_compact_image_is_in_the_order_of_a_copy() {
  let written: &[(&str, &[u32])] = &[("a", &[1, 2, 3]), ("bcd", &[4]), ("", &[])];
  let image = Image::compact(written);
  let words = image.words8(0);
  let bytes = &puns::as_bytes(&words[..])[..image.bytes.len()];
  assert_eq!(image_of(bytes, 8, true), Ok(()));
  let root = puns::from_head::<Pair>(bytes).unwrap().0;
  assert_eq!(rows(root, bytes), [row("a", &[1, 2, 3]), row("bcd", &[4]), row("", &[])]);
  // The pairs first, then the names and values: valid, but not compact. The first pointee in the
  // order of a copy is the name of the first pair, which is not right after it.
  let (spread, _) = Image::pairs(written);
  let spread_words = spread.words8(0);
  let spread_bytes = &puns::as_bytes(&spread_words[..])[..spread.bytes.len()];
  assert_eq!(image_of(spread_bytes, 8, false), Ok(()));
  assert_eq!(image_of(spread_bytes, 8, true), Err(Failure { error: Error::Validity, at: 0 }));
  // A byte more: a compact image ends where its last pointee does.
  let mut long = image.bytes.clone();
  long.push(0);
  let long = Image { bytes: long };
  let long_words = long.words8(0);
  let long_bytes = &puns::as_bytes(&long_words[..])[..long.bytes.len()];
  let end = Failure { error: Error::Validity, at: image.bytes.len() };
  assert_eq!(image_of(long_bytes, 8, true), Err(end));
  assert_eq!(image_of(long_bytes, 8, false), Ok(()));
}

#[test]
fn a_check_says_where_the_bytes_stop_being_a_value() {
  let image = Image::compact(&[("ab", &[1, 2]), ("c", &[3])]);
  let at = |near: usize| near + usize::try_from(image.get_i32(near)).unwrap();
  let failed = |place: usize, value: i32| {
    let mut bad = Image { bytes: image.bytes.clone() };
    bad.put_i32(place, value);
    let words = bad.words8(0);
    image_of(&puns::as_bytes(&words[..])[..bad.bytes.len()], 8, true).map_err(|f| f.at)
  };
  let invalid = |place: usize, byte: u8| {
    let mut bad = image.bytes.clone();
    bad[place] = byte;
    let bad = Image { bytes: bad };
    let words = bad.words8(0);
    image_of(&puns::as_bytes(&words[..])[..bad.bytes.len()], 8, true).map_err(|f| f.at)
  };
  let (name, values) = (at(0), at(4));
  // A pointer that is zero, where it is, and one that leads elsewhere, where it is.
  assert_eq!(failed(4, 0), Err(4));
  assert_eq!(failed(4, image.get_i32(4) + 4), Err(4));
  // A name that is not UTF-8, and a length past the end: where the name starts, at its length.
  assert_eq!(invalid(name + 4, 0xff), Err(name));
  assert_eq!(failed(name, 1000), Err(name));
  // A capacity that is not the length: where the list starts, at its capacity.
  assert_eq!(failed(values - 4, 3), Err(values - 4));
  // The same in the second pair, behind the first.
  let second = at(8);
  assert_eq!(invalid(at(second) + 4, 0xff), Err(at(second)));
  assert_eq!(failed(second + 8, 0), Ok(()));
  assert_eq!(failed(second, 0), Err(second));
}

#[test]
fn a_check_takes_bytes_at_any_address() {
  let image = Image::compact(&[("ab", &[1, 2])]);
  // The bytes one byte after an address aligned to 8, checked as they will be once copied to an
  // address aligned to 8.
  let words = image.words8(1);
  let bytes = &puns::as_bytes(&words[..])[1..=image.bytes.len()];
  assert_eq!(puns::from_head::<Pair>(bytes).err(), Some(Error::Alignment));
  assert_eq!(image_of(bytes, 8, true), Ok(()));
  // An alignment of 2 is less than a `Pair` needs.
  assert_eq!(image_of(bytes, 2, true), Err(Failure { error: Error::Alignment, at: 0 }));
}
