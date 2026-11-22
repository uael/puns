//! A view holds the bytes it was made from: the references of its variants have one lifetime.

#[derive(puns::Pun)]
enum Message<'a, 'b> {
  Bytes(&'a [u8]),
  Word(&'b u32),
}

fn main() {}
