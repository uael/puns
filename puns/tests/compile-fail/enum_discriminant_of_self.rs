//! A discriminant that names `Self` is the enum in the enum, and another type where the derive
//! evaluates it again, to declare the tag. Here `A` is 2 in the enum, and 1 where `Self` is
//! `Copy`, so the layout would read `[2, 7]` as `B(7)` while the enum holds `A` with a `bool` of 7.
//! The tag of a sample of each variant must be the one that the derive declares.
use puns::Pun;

trait Num {
  const N: u8;
}

impl<T: Copy> Num for T {
  const N: u8 = 1;
}

impl Num for E {
  const N: u8 = 2;
}

#[derive(Pun)]
#[repr(u8)]
#[allow(dead_code)]
enum E {
  A(bool) = <Self as Num>::N,
  B(u8),
}

fn main() {
  let _ = puns::try_pun_ref::<[u8; 2], E>(&[2, 7]);
}
