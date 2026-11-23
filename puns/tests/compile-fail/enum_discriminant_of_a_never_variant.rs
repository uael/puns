//! A variant that holds a scalar that a pun never makes has no sample, so its tag is not read from
//! the enum. A discriminant that a macro writes may mean something else where the derive
//! evaluates it again, so its tag is not known, and the enum is refused, though this macro writes
//! a literal. So is an attribute, which may be a macro on an item of a block, though this one is
//! `allow`.
#[path = "../support/never.rs"]
#[allow(dead_code)]
mod never;

macro_rules! two {
  () => {
    2
  };
}

#[derive(puns::Pun, Clone, Copy)]
#[repr(u8)]
#[allow(dead_code)]
enum E {
  A(never::Off) = two!(),
  B(u8),
}

#[derive(puns::Pun, Clone, Copy)]
#[repr(u8)]
#[allow(dead_code)]
enum F {
  A(never::Off) = {
    #[allow(unused_parens)]
    let tag = (2);
    tag
  },
  B(u8),
}

fn main() {
  let _ = puns::try_pun::<[u8; 8], E>([3, 0, 0, 0, 7, 0, 0, 0]);
  let _ = puns::try_pun::<[u8; 8], F>([3, 0, 0, 0, 7, 0, 0, 0]);
}
