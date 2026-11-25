//! An eager aim is proven by the same aim at the same place of the source, of a pointee of the same
//! type: a crate path, which may name anything, does not give a type the identity of another. The
//! derive used to name the identity through the path, with `__site!`; were it to, `Lax` and
//! `Strict` would have one, and the `Strict` that `pun_ref` returned would hold 2, a value it does
//! not allow.
#[allow(dead_code)]
#[path = "../support/aims.rs"]
mod aims;

mod one_site {
  pub use puns::*;
  #[allow(unused_macros)]
  macro_rules! one_site {
    ($name:ident, $n:literal) => {
      "one site"
    };
  }
  #[allow(unused_imports)]
  pub(crate) use one_site as __site;
}

#[derive(puns::Pun)]
#[pun(crate = one_site)]
#[repr(C)]
struct Lax {
  value: u32,
}

#[derive(puns::Pun)]
#[pun(crate = one_site)]
#[repr(C)]
struct Strict {
  #[pun(valid(0..=1))]
  value: u32,
}

#[derive(puns::Pun)]
#[repr(C)]
struct HoldsLax {
  p: aims::Near<Lax>,
}

#[derive(puns::Pun)]
#[repr(C)]
struct HoldsStrict {
  p: aims::Near<Strict>,
}

fn main() {
  let words = [4u32, 2];
  let (lax, _) = puns::from_head::<HoldsLax>(puns::as_bytes(&words[..])).unwrap();
  let _: &HoldsStrict = puns::pun_ref(lax);
}
