//! An eager aim is proven only by the same aim at a pointee of a known identity. A generic type
//! whose parameter is not bounded by `Pun` where the derive sees it has none, so a list of it does
//! not prove itself, and the refusal says why. With `T: Pun`, it would (`tests/aims.rs`).
#[allow(dead_code)]
#[path = "../support/aims.rs"]
mod aims;

#[derive(puns::Pun)]
#[repr(C)]
struct List<T> {
  value: u32,
  next: Option<aims::Near<List<T>>>,
  kind: core::marker::PhantomData<T>,
}

fn same(list: &List<u8>) -> &List<u8> {
  puns::pun_ref(list)
}

fn main() {
  core::hint::black_box(same as fn(&_) -> &_);
}
