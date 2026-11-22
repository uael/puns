//! A union that the same union proves, before a `str` of the source that the target takes as
//! `bool`s. Nothing is proven under the `str`, so every part of the target is checked, the union
//! too, of which nothing is known.
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Flags {
  both: [bool; 2],
  first: bool,
}

#[derive(Pun)]
#[repr(C)]
struct Tagged<T: ?Sized> {
  flags: Flags,
  rest: T,
}

fn main() {
  let source: Tagged<[u8; 2]> = Tagged { flags: Flags { first: true }, rest: [1, 1] };
  let text: &Tagged<str> = puns::try_pun_ref(&source).unwrap();
  let _ = puns::try_pun_ref::<Tagged<str>, Tagged<[bool]>>(text);
}
