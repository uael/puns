//! A rule that reads the bytes of its struct, over an enum that the same enum proves. The rule is
//! checked, and reads the byte after the tag, which is padding in the variant `B`.
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Shape {
  A(u8),
  B,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Source {
  shape: Shape,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(be(0, 2) != 0xffff))]
struct Target {
  shape: Shape,
}

fn main() {
  let _ = puns::try_pun::<Source, Target>(Source { shape: Shape::B });
}
