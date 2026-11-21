//! The valid counterparts of the programs in `compile-fail/`.
use puns::{Pun, num::u8c};

#[derive(Pun, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
enum Shape {
  Circle { radius: u16 },
  Rect(u8, bool),
}

#[derive(Pun, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
enum Drawing {
  Circle { radius: u16 },
  Rect(u8, u8),
  Nothing,
}

#[derive(Pun, Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
enum Unspecified {
  A(u8),
  B(u16),
}

fn main() {
  let _: &[u8; 4] = puns::pun_ref(&(0u8, 0u8, [0u8; 2]));
  let _: &str = puns::try_pun_ref(&[b'a'; 4][..]).unwrap();
  let _ = puns::try_pun::<puns::num::u8c<1>, bool>(u8c::new());
  let _: bool = puns::try_pun(1u8).unwrap();
  let _: &mut u8 = puns::pun_mut(&mut 0u8);
  let text = String::from("text");
  let _: &[u8] = puns::pun_ref(text.as_str());
  let _: &[u8; 4] = puns::pun_ref(&0u32);
  let _ = puns::zeroed::<Option<core::num::NonZero<u8>>>();
  let _: Option<Shape> = puns::try_pun([1u8, 3, 1, 0]).ok();
  let _: Drawing = puns::pun(Shape::Rect(3, true));
  let mut drawing = Drawing::Nothing;
  let _: Result<&mut Shape, _> = puns::try_pun_mut(&mut drawing);
  let _ = Unspecified::A(1);
}
