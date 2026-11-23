//! Zero bytes are a valid `Level`, but its `valid(..)` range does not accept them.
#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
struct Level {
  #[pun(valid(1..=5))]
  x: u8,
}

fn main() {
  let _ = puns::zeroed::<Level>();
}
