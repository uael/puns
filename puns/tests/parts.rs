//! Tests the parts of a value, its first bytes (`head`) and its last bytes (`tail`), by value, by
//! reference, by mutable reference and in `const`.

use puns::{Pun, head, head_mut, head_ref, num::u16be, pun_ref, tail, tail_mut, tail_ref};

#[derive(Pun, Debug, Clone, Copy, PartialEq)]
#[repr(C)]
struct Record {
  kind: u8,
  flag: bool,
  value: u16be,
}

const RECORD: Record = Record { kind: 7, flag: true, value: u16be::new(0x0102) };

#[derive(Pun, Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
enum Shape {
  Circle { radius: u16 },
  Rect(u8, bool),
}

/// A struct with a rule over its two bytes.
#[derive(Pun, Debug, Clone, Copy, PartialEq)]
#[repr(C)]
#[pun(assert(a != b))]
struct Distinct {
  a: u8,
  b: u8,
}

/// A struct whose last field has a rule.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Framed {
  header: [u8; 2],
  trailer: Distinct,
}

#[test]
fn head_and_tail_by_value() {
  assert_eq!(head::<Record, [u8; 2]>(RECORD), [7, 1]);
  assert_eq!(tail::<Record, [u8; 2]>(RECORD), [1, 2]);
  assert_eq!(tail::<Record, u16be>(RECORD).get(), 0x0102);
  assert_eq!(tail::<Record, Record>(RECORD), RECORD);
  assert_eq!(tail::<[Record; 2], Record>([RECORD, Record { kind: 1, ..RECORD }]).kind, 1);
  // An enum is proven by an enum at the same place.
  let shapes = [Shape::Rect(1, true), Shape::Circle { radius: 3 }];
  assert_eq!(tail::<[Shape; 2], Shape>(shapes), Shape::Circle { radius: 3 });
  // A rule, by the same rule at the same place.
  let framed = Framed { header: [1, 2], trailer: Distinct { a: 3, b: 4 } };
  assert_eq!(tail::<Framed, Distinct>(framed), Distinct { a: 3, b: 4 });
  assert_eq!(tail::<Framed, [u8; 3]>(framed), [2, 3, 4]);
}

#[test]
fn head_and_tail_by_reference() {
  let words = [1u16, 2, 3];
  let last: &u16 = tail_ref(&words);
  assert_eq!(*last, 3);
  assert!(core::ptr::eq(last, &raw const words[2]));
  assert_eq!(tail_ref::<_, [u16; 2]>(&words), &[2, 3]);
  assert_eq!(tail_ref::<_, [u8; 3]>(&words), &pun_ref::<_, [u8; 6]>(&words)[3..]);
  assert_eq!(head_ref::<_, [u8; 3]>(&words), &pun_ref::<_, [u8; 6]>(&words)[..3]);
  assert_eq!(tail_ref::<Record, u16be>(&RECORD).get(), 0x0102);
  let shapes = [Shape::Rect(1, true), Shape::Circle { radius: 3 }];
  assert_eq!(tail_ref::<_, Shape>(&shapes), &Shape::Circle { radius: 3 });
  assert_eq!(head_ref::<_, Shape>(&shapes), &Shape::Rect(1, true));
}

#[test]
fn head_and_tail_in_const() {
  const WORDS: [u16; 3] = [1, 2, 3];
  const LAST: &u16 = tail_ref(&WORDS);
  const PAIR: [u8; 2] = tail([1u8, 2, 3]);
  const FIRST: [u8; 2] = head([1u8, 2, 3]);
  assert_eq!((*LAST, PAIR, FIRST), (3, [2, 3], [1, 2]));
}

/// A struct with a header, then a payload that nothing else depends on.
#[derive(Pun, Debug, Clone, Copy, PartialEq)]
#[repr(C)]
struct Message {
  header: Distinct,
  payload: [u8; 3],
  shape: Shape,
}

#[test]
fn mutable_parts() {
  let mut words = [1u16, 2, 3];
  *head_mut::<_, u16>(&mut words) = 7;
  *tail_mut::<_, u16>(&mut words) = 9;
  assert_eq!(words, [7, 2, 9]);
  tail_mut::<_, [u8; 4]>(&mut words).fill(0);
  assert_eq!(words, [7, 0, 0]);
  // As many elements as fit.
  let mut bytes = [0u8; 7];
  head_mut::<[u8], [[u8; 2]]>(&mut bytes[..]).fill([1, 2]);
  assert_eq!(bytes, [1, 2, 1, 2, 1, 2, 0]);
  let mut record = RECORD;
  *head_mut::<Record, u8>(&mut record) = 3;
  *tail_mut::<Record, u16be>(&mut record) = u16be::new(0x0304);
  assert_eq!(record, Record { kind: 3, flag: true, value: u16be::new(0x0304) });
  // What a rule or an enum does not depend on.
  let mut message =
    Message { header: Distinct { a: 1, b: 2 }, payload: [0; 3], shape: Shape::Rect(1, true) };
  let shape = tail_mut::<Message, Shape>(&mut message);
  *shape = Shape::Circle { radius: 9 };
  assert_eq!(message.shape, Shape::Circle { radius: 9 });
  let mut shapes = [Shape::Rect(1, true), Shape::Circle { radius: 3 }];
  *tail_mut::<_, Shape>(&mut shapes) = Shape::Rect(2, false);
  *head_mut::<_, Shape>(&mut shapes) = Shape::Circle { radius: 4 };
  assert_eq!(shapes, [Shape::Circle { radius: 4 }, Shape::Rect(2, false)]);
}

#[test]
fn mutable_parts_in_const() {
  const WORDS: [u16; 2] = {
    let mut words = [1u16, 2];
    *tail_mut::<_, u16>(&mut words) = 5;
    words
  };
  assert_eq!(WORDS, [1, 5]);
}
