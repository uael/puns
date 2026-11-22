//! A rule that reads the padding of its struct, which the same struct proves, before a `str` of
//! the source that the target takes as `bool`s. Nothing is proven under the `str`, so every part of
//! the target is checked, the rule too.
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(be(0, 4) != 0))]
struct Padded {
  a: u8,
  b: u16,
}

#[derive(Pun)]
#[repr(C)]
struct Tagged<T: ?Sized> {
  head: Padded,
  rest: T,
}

fn main() {
  let source: Tagged<[u8; 2]> = Tagged { head: Padded { a: 1, b: 2 }, rest: [1, 1] };
  let text: &Tagged<str> = puns::try_pun_ref(&source).unwrap();
  let _ = puns::try_pun_ref::<Tagged<str>, Tagged<[bool]>>(text);
}
