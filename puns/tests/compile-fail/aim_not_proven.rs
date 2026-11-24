//! An eager aim of the target is proven only by the same aim at the same place of the source.
#[allow(dead_code)]
#[path = "../support/aims.rs"]
mod aims;

fn main() {
  let _: &aims::Near<u32> = puns::pun_ref(&[4u32]);
}
