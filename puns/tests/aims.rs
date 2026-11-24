//! Eager aims: a self-relative pointer declared through the public API (`support/aims.rs`), whose
//! targets `puns` checks with the value, and `Thin`.

#[path = "support/aims.rs"]
mod aims;

use aims::{Expr, Near, Pair, Tree};
use puns::{Error, InPlace, Pun, Thin, layout::Endian};

/// The target of `near`, in `image`, the bytes in which `puns` checked it.
fn to<'a, T: ?Sized + Pun>(near: &'a Near<T>, image: &'a [u8]) -> &'a T {
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
  // The values of the next pair: past the end.
  let values = at[1] + 4 + usize::try_from(image.get_i32(at[1] + 4)).unwrap();
  assert_eq!(set(values, 1000), Err(Error::Validity));
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
  let kids = image.reserve(4 + 2 * 8, 4);
  image.put_i32(kids, 2);
  let grandkids = image.reserve(4 + 8, 4);
  image.put_i32(grandkids, 1);
  let empty = image.reserve(4, 4);
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

#[test]
fn thin_over_a_struct_with_a_tail() {
  // A length of 2, then a `Record` of 2 elements: the kind, then the data.
  let mut words = [0u32; 3];
  let bytes =
    puns::from_bytes_mut::<[u8]>(puns::pun_mut::<[u32; 3], [u8; 12]>(&mut words)).unwrap();
  bytes[..4].copy_from_slice(&2i32.to_ne_bytes());
  bytes[4..6].copy_from_slice(&7u16.to_ne_bytes());
  bytes[6..10].copy_from_slice(&[1, 0, 2, 0]);
  let bytes: &[u8] = bytes;
  let thin = puns::from_head::<Thin<Record, i32>>(bytes).unwrap().0;
  assert_eq!((thin.value().kind, thin.value().data.len()), (7, 2));
  // SAFETY: `thin` is a valid `Thin<Record, i32>`, aligned, in `bytes`, which are not written.
  let record = unsafe { Thin::<Record, i32>::get(core::ptr::from_ref(thin).cast()) };
  assert_eq!(record.data.len(), 2);
  // A length that does not fit.
  let mut long = words;
  puns::pun_mut::<[u32; 3], [u8; 12]>(&mut long)[..4].copy_from_slice(&3i32.to_ne_bytes());
  let long = &puns::as_bytes(&long)[..10];
  assert_eq!(puns::from_head::<Thin<Record, i32>>(long).err(), Some(Error::Size));
  // `Thin` of a sized type is the type.
  assert_eq!(size_of::<Thin<u64, i16>>(), 8);
  // The data of `Thin<[u32], i16>` is after 2 bytes of padding.
  assert_eq!(<Thin<[u32], i16> as Pun>::LAYOUT.size(), 4);
}

#[test]
fn the_length_of_a_thin_has_the_sign_of_its_type() {
  // An unsigned length takes every value of its type.
  let mut bytes = [0u8; 201];
  bytes[0] = 200;
  let thin = puns::from_bytes::<Thin<[u8], u8>>(&bytes).unwrap();
  assert_eq!(thin.value().len(), 200);
  let mut words = vec![0u16; 20_001];
  words[0] = 40_000;
  let thin = puns::from_bytes::<Thin<[u8], u16>>(puns::as_bytes(&words[..])).unwrap();
  assert_eq!(thin.value().len(), 40_000);
  // The same byte as a signed length is negative, a size that no bytes have.
  assert_eq!(puns::from_head::<Thin<[u8], i8>>(&bytes).err(), Some(Error::Size));
}

/// A pointer to a `u16` before it or after it, stored as the distance from its own address.
#[derive(Pun)]
#[repr(transparent)]
struct Back {
  #[pun(aim(back: u16 = off as u128))]
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
  #[pun(aim(pair: [u32; 2] = signed(0, 4, Endian::Big) - 4))]
  off: InPlace<[u8; 4]>,
}

#[test]
fn where_a_target_starts_is_a_rule() {
  let at = |off: i32| [u32::from_ne_bytes(off.to_be_bytes()), 1, 2];
  // At 8, the second word of the pair at 4.
  let words = at(8);
  let bytes = puns::as_bytes(&words);
  let second = puns::from_head::<Second>(bytes).unwrap().0;
  assert_eq!(second.off.get(), 8i32.to_be_bytes());
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
  #[pun(aim(word: u16 = off as u128))]
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
  // within its bytes: one that points at the header is not.
  let words = message(-4);
  assert_eq!(message_at(&words), Ok(()));
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
  #[pun(aim(text: Text = off as u128))]
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

  /// Writes a `Thin<[u32], i32>` and returns where it is.
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
