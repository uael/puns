//! The reads of the pointees of eager aims through a reference to the value: `name(&self)`, which
//! the derive writes, and is safe, and `puns::aim_mut` from a pointer made from a `&mut`.
//!
//! Miri runs this file with tree borrows only (`tree_borrows_only` in `.github/tests.sh`). A
//! reference to a value gives the provenance of the bytes of a pointee outside it under tree
//! borrows, which these reads rely on, but not under stacked borrows, under which `aims.rs` reads
//! the same pointees from pointers to the bytes that hold them.

use core::num::NonZero;

#[path = "support/aims.rs"]
mod aims;

use aims::{Expr, Pair, Tree};
use puns::{Error, InPlace, Pun, Thin};

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

/// Two pairs, in words: the first at 0, its name at 12, "ab" behind its length, its values at 20,
/// one word, and the next pair at 28, whose name and values are those of the first.
fn pairs() -> [u32; 10] {
  let back = |off: i32| off.cast_unsigned();
  [12, 16, 20, 2, u32::from_ne_bytes(*b"ab\0\0"), 1, 9, back(-16), back(-12), 0]
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
  // The same as from a pointer with the provenance of the bytes.
  // SAFETY: The words hold the pair and its name.
  assert_eq!(unsafe { pair.name.get(puns::as_bytes(&words)) }, pair.name.pointee());
}

#[test]
fn a_tree_is_read_through_references() {
  // A root of id 1 at 0, whose two children follow their length at 8: ids 2 and 3, each with the
  // empty list at 28 as its children.
  let empty = |at: i32| (28 - at).cast_unsigned();
  let words = [1, 4, 2, 2, empty(16), 3, empty(24), 0];
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
  assert_eq!(puns::from_head::<Text>(puns::as_bytes(&[2i16, -1, 0])).err(), Some(Error::Validity));
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
  // SAFETY: `puns` checked `renamed` with its pointee, in `bytes`, which are not written, and the
  // reference has the provenance of both under tree borrows.
  assert_eq!(unsafe { *puns::aim(renamed, Renamed::AIMS.next) }, 9);
  assert_eq!(puns::from_head::<Renamed>(&[2, 9]).err(), Some(Error::Validity));
}
