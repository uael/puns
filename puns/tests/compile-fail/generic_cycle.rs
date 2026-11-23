//! Two generic types that hold each other: the bounds that the derive finds for each (the type
//! of each field that depends on a parameter is `Pun`) go round in a circle. They are told with
//! `#[pun(bound())]` on both (the ones that the types declare are enough), which this lacks.
#[path = "../support/opaque.rs"]
#[allow(dead_code)]
mod opaque;
use opaque::Ref;
use puns::Pun;

#[derive(Pun)]
#[repr(C)]
struct Ping<T> {
  value: T,
  pong: Ref<Pong<T>>,
}

#[derive(Pun)]
#[repr(C)]
struct Pong<T> {
  value: T,
  ping: Ref<Ping<T>>,
}

fn main() {
  let ping = Ping { value: 0u32, pong: Ref::new(4) };
  let _ = puns::as_bytes(&ping);
}
