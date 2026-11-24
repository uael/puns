//! A type named `InPlace` that safe code can make is not the `InPlace` of `puns`: the field of an
//! eager aim must have the type of `puns`, which the derive requires of the struct as compiled.

/// A type of the same name, with a public field.
struct InPlace<T>(T);

#[derive(puns::Pun)]
#[repr(transparent)]
struct Near {
  #[pun(aim(u32 = off as u128))]
  off: InPlace<i32>,
}

fn main() {
  // Safe code could make one with any target.
  let _ = Near { off: InPlace(1000) };
}
