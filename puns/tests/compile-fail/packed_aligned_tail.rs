//! A packed struct whose unsized field is a struct aligned to 4: the compiler pads that field to a
//! multiple of 4 inside a struct of alignment 1, which `puns` does not model.

#[derive(puns::Pun)]
#[repr(C)]
struct Inner {
  length: u32,
  data: [u8],
}

#[derive(puns::Pun)]
#[repr(C, packed)]
struct Outer {
  kind: u8,
  inner: Inner,
}

fn main() {
  let _ = puns::from_bytes::<Outer>(&[0; 9]);
}
