//! The layout of a type whose derive failed is an error where a constant reads it, which says so,
//! next to the error of the derive.
#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(a += 1))]
struct Failed {
  a: u8,
}

const _: puns::layout::Layout = <Failed as puns::Pun>::LAYOUT;

fn main() {}
