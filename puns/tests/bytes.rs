//! Tests reading values from bytes: from all of them (`from_bytes`), from their front
//! (`from_head`) and from their back (`from_tail`), by shared and by mutable reference.

use puns::{
  Error, Pun, from_bytes, from_bytes_mut, from_head, from_head_mut, from_tail, from_tail_mut,
  num::{u16be, u16bec},
  try_pun_mut, try_pun_ref,
};

#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
struct Position {
  x: u8,
  visible: bool,
}

/// A struct whose size is in one of its fields (`size = length`).
#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
#[pun(size = length)]
struct Chunk {
  kind: u8,
  length: u16be,
  data: [u8],
}

/// A struct with the alignment of `u32`.
#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
struct Word {
  value: u32,
}

/// A struct that starts with a magic number. Most values of its first two bytes are not valid.
#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
struct Magic {
  magic: u16bec<0xcafe>,
  flag: bool,
}

#[repr(align(4))]
struct Aligned<T>(T);

#[test]
fn whole() {
  // The same as `try_pun_ref::<[u8], _>`, without naming the source.
  assert_eq!(from_bytes(&[7, 1]), Ok(&Position { x: 7, visible: true }));
  assert_eq!(from_bytes::<Position>(&[7, 1]), try_pun_ref::<[u8], Position>(&[7, 1]));
  assert_eq!(from_bytes::<Position>(&[7, 2]), Err(Error::Validity));
  assert_eq!(from_bytes::<Position>(&[7]), Err(Error::Size));
  assert_eq!(from_bytes::<str>(b"caf\xc3\xa9"), Ok("caf\u{e9}"));
  assert_eq!(from_bytes::<str>(b"caf\xc3"), Err(Error::Validity));
  let chunk: &Chunk = from_bytes(&[1, 0, 5, b'h', b'i']).unwrap();
  assert_eq!((chunk.kind, &chunk.data), (1, &b"hi"[..]));
  // The whole: the rule says 5 bytes, not 6.
  assert_eq!(from_bytes::<Chunk>(&[1, 0, 5, b'h', b'i', 0]).err(), Some(Error::Validity));
  // Alignment.
  let words = Aligned([0u8, 0, 0, 1, 0, 0, 0, 2]);
  assert_eq!(
    from_bytes::<Word>(&words.0[..4]).map(|w| w.value),
    Ok(u32::from_ne_bytes([0, 0, 0, 1]))
  );
  assert_eq!(from_bytes::<Word>(&words.0[1..5]).err(), Some(Error::Alignment));
  assert_eq!(from_bytes::<[Word]>(&words.0[..]).map(<[Word]>::len), Ok(2));
  assert_eq!(from_bytes::<Magic>(&[0xca, 0xfe, 1]).map(|m| m.flag), Ok(true));
  assert_eq!(from_bytes::<Magic>(&[0xca, 0xff, 1]).err(), Some(Error::Validity));
}

#[test]
fn whole_mut() {
  let mut bytes = [7u8, 1, 0];
  let position = from_bytes_mut::<Position>(&mut bytes[..2]).unwrap();
  position.visible = false;
  position.x = 9;
  assert_eq!(bytes, [9, 0, 0]);
  assert_eq!(from_bytes_mut::<Position>(&mut [7, 2]).err(), Some(Error::Validity));
  assert_eq!(from_bytes_mut::<Position>(&mut [7, 1, 0]).err(), Some(Error::Size));
  let mut same = [7u8, 1];
  assert!(from_bytes_mut::<Position>(&mut same).is_ok());
  assert!(try_pun_mut::<[u8], Position>(&mut same).is_ok());
  let mut text = *b"abc";
  from_bytes_mut::<str>(&mut text).unwrap().make_ascii_uppercase();
  assert_eq!(&text, b"ABC");
}

#[test]
fn in_const() {
  const POSITION: Result<&Position, Error> = from_bytes(&[3, 0]);
  const BACK: Result<(&[u8], &Position), Error> = from_tail(&[1, 1]);
  assert_eq!(POSITION.map(|p| (p.x, p.visible)), Ok((3, false)));
  assert_eq!(BACK.map(|(rest, p)| (rest.len(), p.x)), Ok((0, 1)));
}

#[test]
fn back() {
  let (rest, position) = from_tail::<Position>(&[1, 2, 3, 7, 1]).unwrap();
  assert_eq!((rest, position), (&[1, 2, 3][..], &Position { x: 7, visible: true }));
  let (rest, position) = from_tail::<Position>(&[7, 0]).unwrap();
  assert_eq!((rest.len(), position.visible), (0, false));
  assert_eq!(from_tail::<Position>(&[1, 7, 2]).err(), Some(Error::Validity));
  assert_eq!(from_tail::<Position>(&[7]).err(), Some(Error::Size));
  assert_eq!(from_tail::<[u8; 0]>(&[1, 2]), Ok((&[1, 2][..], &[])));
  // The address of the last bytes.
  let words = Aligned([0u8, 0, 0, 1, 0, 0, 0, 2]);
  let (rest, word) = from_tail::<Word>(&words.0).unwrap();
  assert_eq!((rest.len(), word.value), (4, u32::from_ne_bytes([0, 0, 0, 2])));
  assert_eq!(from_tail::<Word>(&words.0[..7]).err(), Some(Error::Alignment));
  assert_eq!(from_tail::<Magic>(&[9, 0xca, 0xfe, 0]).map(|(rest, _)| rest), Ok(&[9][..]));
  assert_eq!(from_tail::<Magic>(&[0xca, 0xfe, 0, 9]).err(), Some(Error::Validity));
  // Front and back of the same bytes.
  let bytes = [7u8, 0, 5, b'h', b'i', 0xca, 0xfe, 1];
  let (rest, magic) = from_tail::<Magic>(&bytes).unwrap();
  let (chunk, rest) = from_head::<Chunk>(rest).unwrap();
  assert_eq!((&chunk.data, rest.len(), magic.flag), (&b"hi"[..], 0, true));
}

#[test]
fn front_and_back_mut() {
  let mut bytes = [7u8, 0, 5, b'h', b'i', 0xca, 0xfe, 1];
  let (chunk, rest) = from_head_mut::<Chunk>(&mut bytes).unwrap();
  chunk.data.make_ascii_uppercase();
  chunk.kind = 8;
  rest[2] = 0;
  let (rest, position) = from_tail_mut::<Position>(&mut bytes).unwrap();
  position.x = 0xff;
  rest[0] = 9;
  assert_eq!(bytes, [9, 0, 5, b'H', b'I', 0xca, 0xff, 0]);
  // The same checks as by shared reference.
  assert_eq!(from_head_mut::<Chunk>(&mut [1, 0, 9, 0]).err(), Some(Error::Size));
  assert_eq!(from_head_mut::<Position>(&mut [1, 2]).err(), Some(Error::Validity));
  assert_eq!(from_tail_mut::<Position>(&mut [1, 2]).err(), Some(Error::Validity));
  assert_eq!(from_tail_mut::<Position>(&mut [1]).err(), Some(Error::Size));
  let mut words = Aligned([0u8, 0, 0, 1, 0, 0, 0, 2]);
  assert_eq!(from_tail_mut::<Word>(&mut words.0[..7]).err(), Some(Error::Alignment));
  assert_eq!(from_head_mut::<Word>(&mut words.0[1..]).err(), Some(Error::Alignment));
  let (rest, word) = from_tail_mut::<Word>(&mut words.0).unwrap();
  word.value = 0;
  let (word, _) = from_head_mut::<Word>(rest).unwrap();
  word.value = u32::MAX;
  assert_eq!(words.0, [0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0]);
  // A slice takes what there is, a string checks it all.
  let mut text = *b"abc";
  let (all, rest) = from_head_mut::<str>(&mut text).unwrap();
  all.make_ascii_uppercase();
  assert_eq!(rest.len(), 0);
  assert_eq!(&text, b"ABC");
  assert!(from_head_mut::<str>(&mut [0xff]).is_err());
}

#[test]
fn mut_in_const() {
  const BYTES: [u8; 4] = {
    let mut bytes = [1u8, 0, 0, 1];
    if let Ok((flags, _)) = from_head_mut::<[bool; 1]>(&mut bytes) {
      flags[0] = false;
    }
    if let Ok((_, flags)) = from_tail_mut::<[bool; 1]>(&mut bytes) {
      flags[0] = false;
    }
    bytes
  };
  assert_eq!(BYTES, [0, 0, 0, 0]);
}

/// An enum whose valid values form two ranges, 1 to 3 and 7.
#[derive(Pun, Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
enum Few {
  A = 1,
  B = 2,
  C = 3,
  D = 7,
}

/// A struct with a rule over both of its fields.
#[derive(Pun, Debug, PartialEq)]
#[repr(C)]
#[pun(assert(low <= high && low + high != 9))]
struct Ordered {
  low: u8,
  high: u8,
}

/// Checks that an invalid element is found at any position of a slice whose elements are checked
/// a block at a time, inside the blocks and after them.
#[test]
fn an_invalid_element_anywhere() {
  // Under Miri, lengths around the blocks (of about 64 bytes) and places around their ends;
  // natively, every length up to 150 and every place.
  let lengths: Vec<usize> = if cfg!(miri) { vec![1, 65, 129] } else { (0..150).collect() };
  for len in lengths {
    let valid = vec![1u8; 2 * len];
    assert!(from_bytes::<[bool]>(&valid[..len]).is_ok());
    assert!(from_bytes::<[Few]>(&valid[..len]).is_ok());
    assert!(from_bytes::<[Ordered]>(&valid).is_ok());
    let places: Vec<usize> = if cfg!(miri) {
      [0, 63, 64, len.saturating_sub(1)].into_iter().filter(|&at| at < len).collect()
    } else {
      (0..len).collect()
    };
    for at in places {
      let mut bytes = valid.clone();
      bytes[at] = 2;
      assert!(from_bytes::<[bool]>(&bytes[..len]).is_err(), "{len} {at}");
      bytes[at] = 4;
      assert!(from_bytes::<[Few]>(&bytes[..len]).is_err(), "{len} {at}");
      let mut bytes = valid.clone();
      bytes[2 * at] = 2;
      assert!(from_bytes::<[Ordered]>(&bytes).is_err(), "{len} {at}");
      (bytes[2 * at], bytes[2 * at + 1]) = (4, 5);
      assert!(from_bytes::<[Ordered]>(&bytes).is_err(), "{len} {at}");
    }
  }
}

/// Checks that a slice of elements without values is made only empty, and that what proves it
/// stays proven.
///
/// A slice has a value whatever its element, the empty one. `Void1` has a byte, but no value.
#[test]
fn a_slice_of_elements_without_values_is_empty() {
  use core::convert::Infallible;

  use puns::{from_bytes, pun_ref};

  #[derive(Pun, Clone, Copy)]
  #[repr(C)]
  struct Void1 {
    n: Infallible,
    x: u8,
  }
  #[derive(Pun)]
  #[repr(C)]
  struct Tail {
    h: u8,
    items: [Void1],
  }
  #[derive(Pun, Clone, Copy, Debug, PartialEq, Eq)]
  #[repr(C)]
  struct Some1 {
    x: u8,
  }

  // No element: the source has no byte for one.
  let empty: &[Void1] = pun_ref::<[u8; 0], [Void1]>(&[]);
  assert!(empty.is_empty());
  let tail = pun_ref::<[u8; 1], Tail>(&[7]);
  assert_eq!((tail.h, tail.items.len()), (7, 0));
  // Each element of the target is an element of the source.
  assert!(pun_ref::<[Void1], [Void1]>(empty).is_empty());
  assert_eq!(pun_ref::<Tail, Tail>(tail).h, 7);
  // The target has values.
  assert_eq!(pun_ref::<[Void1], [u8]>(empty), &[] as &[u8]);
  assert_eq!(from_bytes::<[Some1]>(&[1, 2]), Ok(&[Some1 { x: 1 }, Some1 { x: 2 }][..]));
}
