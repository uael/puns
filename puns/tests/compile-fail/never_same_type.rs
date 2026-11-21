//! Nothing proves a scalar that a pun never makes, not even the same type, with or without the
//! `unstable` feature: here a struct of the `Rust` representation, which is otherwise proven by
//! itself.
#[path = "../support/never.rs"]
#[allow(dead_code)]
mod never;

#[derive(puns::Pun)]
struct Loose {
  next: never::Off,
  byte: u8,
}

fn main() {
  let loose = Loose { next: never::Off(core::num::NonZero::new(1).unwrap()), byte: 0 };
  let _: &Loose = puns::pun_ref(&loose);
}
