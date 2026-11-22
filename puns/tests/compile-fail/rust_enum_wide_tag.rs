// The direct tag of an enum of the `Rust` representation is all the bytes before the first field
// of a variant, which must be 1, 2, 4, 8 or 16: `rustc` makes no tag of another size. Here the
// fields are aligned to 32, so 32 bytes are before them, and the enum is refused.
#[derive(puns::Pun, Clone, Copy)]
#[repr(C, align(32))]
struct Wide(u8);

#[derive(puns::Pun, Clone, Copy)]
enum Shape {
  A(Wide),
  B(Wide),
}

fn main() {
  let _ = <Shape as puns::Pun>::LAYOUT;
}
