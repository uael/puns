//! A union holds no scalar that a pun never makes: what is known of a union is what each field
//! could hold, and its crate proves such a scalar where it makes one, which a union does not tell.
#[path = "../support/never.rs"]
#[allow(dead_code)]
mod never;
use never::Off;

#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
union Either {
  off: Off,
  word: u32,
}

fn main() {
  let _ = puns::try_pun::<_, Either>(1u32);
}
