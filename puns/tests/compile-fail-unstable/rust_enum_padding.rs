// An enum of the `Rust` representation: its tag is one byte, and the byte after it is padding in
// `B`.
#[derive(puns::Pun, Clone, Copy)]
enum Direct {
  A(u8),
  B(u16),
}

fn main() {
  let _: [u8; 4] = puns::pun(Direct::B(1));
}
