//! A union holds no opaque scalar: what is known of a union is what each field
//! could hold, and its crate proves such a scalar where it makes one, which a union does not tell.
#[path = "../support/opaque.rs"]
#[allow(dead_code)]
mod opaque;
use opaque::Off;

#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
union Either {
  off: Off,
  word: u32,
}

fn main() {
  let _ = puns::try_pun::<_, Either>(1u32);
}
