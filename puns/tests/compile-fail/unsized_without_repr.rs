// A struct of the `Rust` representation does not say where its fields are. `#[derive(Pun)]` finds
// them on a value, which an unsized struct cannot be.
#[derive(puns::Pun)]
struct Packet {
  len: u8,
  data: [u8],
}

#[derive(puns::Pun)]
struct Text(u16, str);

fn main() {}
