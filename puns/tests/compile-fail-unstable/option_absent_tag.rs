// `Shape::A` has no value, so `rustc` gives its tag, 0, to the `None` of an `Option<Shape>`. For
// `puns`, 0 is a value of the tag of `Shape`, and the tag of `None` must be no value of the scalar
// that holds it, whose other values are those of `Some`. This layout is refused as unsupported.
use core::convert::Infallible;

#[derive(puns::Pun, Clone, Copy)]
#[repr(u8)]
enum Shape {
  A(u8, Infallible),
  B(bool),
}

fn main() {
  let _ = <Option<Shape> as puns::Pun>::LAYOUT;
}
