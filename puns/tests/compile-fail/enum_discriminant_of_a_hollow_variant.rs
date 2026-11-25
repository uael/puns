//! A variant that holds a union none of whose fields has values has no sample, but a value of it
//! may exist in a source, so its tag must be known. A discriminant that a macro writes may mean
//! something else where the derive evaluates it again, so its tag is not known, and the enum is
//! refused, though this macro writes a literal. So is an attribute, which may be a macro on an item
//! of a block, though this one is `allow`. With a literal, an expression without a macro or a
//! constant, it compiles (`a_hollow_variant_has_a_tag_from_its_declaration` in `tests/uninit.rs`).
use core::convert::Infallible;

#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
union Hollow {
  a: Infallible,
}

macro_rules! two {
  () => {
    2
  };
}

#[derive(puns::Pun, Clone, Copy)]
#[repr(u8)]
#[allow(dead_code)]
enum E {
  A(Hollow) = two!(),
  B(u8),
}

#[derive(puns::Pun, Clone, Copy)]
#[repr(u8)]
#[allow(dead_code)]
enum F {
  A(Hollow) = {
    #[allow(unused_parens)]
    let tag = (2);
    tag
  },
  B(u8),
}

fn main() {
  let _ = puns::try_pun::<[u8; 2], E>([3, 7]);
  let _ = puns::try_pun::<[u8; 2], F>([3, 7]);
}
