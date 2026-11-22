//! A discriminant that a macro writes may name, where the derive evaluates it again, an item that
//! the derive declares there, though the derive sees no such name in the enum. Here `__PunTag` is
//! `NotCopy` in the module, so `A` is 2 in the enum, and the tag that the derive declares where it
//! is `Copy`, so 1 there. The tag of a sample of each variant must be the one that the derive
//! declares.
use puns::Pun;

struct NotCopy;

trait Num {
  const N: u8;
}

impl<T: Copy> Num for T {
  const N: u8 = 1;
}

impl Num for NotCopy {
  const N: u8 = 2;
}

#[allow(non_camel_case_types)]
type __PunTag = NotCopy;

macro_rules! two {
  () => {
    <__PunTag as Num>::N
  };
}

#[derive(Pun)]
#[repr(u8)]
#[allow(dead_code)]
enum F {
  A(bool) = two!(),
  B(u8),
}

fn main() {
  let _ = puns::try_pun_ref::<[u8; 2], F>(&[2, 7]);
}
