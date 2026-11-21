//! Each pun depends on where `rustc` put something that Rust lets it put elsewhere.
use core::num::NonZero;

use puns::Pun;

/// The `Rust` representation: its `bool` is the first byte, or the second.
#[derive(Pun, Clone, Copy)]
struct Mixed {
  a: u8,
  b: bool,
}

/// The `Rust` representation, with a rule: which byte is `a`.
#[derive(Pun, Clone, Copy)]
#[pun(assert(a < b))]
struct Ordered {
  a: u8,
  b: u8,
}

/// No `repr`: nothing tells where its tag is, nor its values.
#[derive(Pun, Clone, Copy)]
enum Ticket {
  Free,
  Paid(NonZero<u32>),
}

fn main() {
  // Which byte must be 0 or 1.
  let _ = puns::from_bytes::<Mixed>(&[1, 1]);
  let _ = puns::try_pun::<[u8; 2], (bool, u8)>([1, 1]);
  // Which bytes a rule holds of.
  let _ = puns::from_bytes::<Ordered>(&[1, 2]);
  // Where the padding is: the second byte, or the last.
  let _ = puns::head::<(u8, u16), [u8; 2]>((1, 2));
  // The size: 4 or 6 bytes.
  let _ = puns::as_bytes(&(1u8, 2u16, 3u8));
  // The values of a tag.
  let _ = puns::pun::<Ticket, u32>(Ticket::Free);
  let _ = puns::try_pun::<u8, Option<bool>>(2);
  // What a `&mut` lets be written.
  let _ = puns::pun_mut::<(bool, u8), [u8; 2]>(&mut (true, 1));
  // What `validate_in` would check.
  let _ = puns::validate_in::<Mixed, _>(&[1, 1], 0, &mut NoPointers);
}

/// Refuses every pointer: for values that hold none.
struct NoPointers;

impl puns::Visit for NoPointers {
  type Error = puns::Invalid;
  fn pointer<P: ?Sized + Pun>(&mut self, at: usize) -> Result<(), puns::Invalid> {
    Err(puns::Invalid::at(at))
  }
}
