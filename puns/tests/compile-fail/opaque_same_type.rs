//! Nothing proves an opaque scalar, not even the same type, with or without the
//! `unstable` feature: here a struct of the `Rust` representation, which is otherwise proven by
//! itself.
#[path = "../support/opaque.rs"]
#[allow(dead_code)]
mod opaque;

#[derive(puns::Pun)]
struct Loose {
  next: opaque::Off,
  byte: u8,
}

fn main() {
  let loose = Loose { next: opaque::Off(core::num::NonZero::new(1).unwrap()), byte: 0 };
  let _: &Loose = puns::pun_ref(&loose);
}
