//! Two generic types that hold each other: the bounds that the derive finds for each (the type
//! of each field that depends on a parameter is `Pun`) go round in a circle. They are told with
//! `#[pun(bound())]` on both (the ones that the types declare are enough), which this lacks.
#[allow(dead_code)]
#[path = "../support/aims.rs"]
mod aims;
use aims::Near;
use puns::Pun;

#[derive(Pun)]
#[repr(C)]
struct Ping<T> {
  value: T,
  pong: Near<Pong<T>>,
}

#[derive(Pun)]
#[repr(C)]
struct Pong<T> {
  value: T,
  ping: Near<Ping<T>>,
}

fn bytes(ping: &Ping<u32>) -> &[u8] {
  puns::as_bytes(ping)
}

fn main() {
  let _ = bytes;
}
