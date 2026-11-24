//! The field of an eager aim is written `InPlace<T>`, which no code makes, so that no safe code
//! makes a value of the struct: a field of another type is refused.

#[derive(puns::Pun)]
#[repr(transparent)]
struct Near {
  #[pun(pointer(u32 = off as u128))]
  off: i32,
}

fn main() {}
