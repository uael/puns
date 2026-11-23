//! Zero bytes are a `Light` only by the tag that `rustc` gives `Off`, which the `Rust`
//! representation leaves to the compiler.
#[derive(puns::Pun, Clone, Copy)]
enum Light {
  Off,
  On,
}

fn main() {
  let _ = puns::zeroed::<Light>();
}
