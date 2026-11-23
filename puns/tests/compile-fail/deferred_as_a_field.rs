//! A `Deferred<T>` is a target that takes its opaque scalars by their values only where it is the
//! target itself. As a field of another type, it is checked as `T`, and its opaque scalars are
//! refused.
#[path = "../support/opaque.rs"]
#[allow(dead_code)]
mod opaque;

#[derive(puns::Pun)]
#[repr(C)]
struct Holder {
  node: puns::Deferred<opaque::Node>,
}

fn main() {
  let _ = puns::from_bytes::<Holder>(&[0; 8]);
}
