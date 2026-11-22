//! An option that `#[pun(..)]` does not have is refused, not ignored.

#[derive(puns::Pun)]
#[repr(C)]
#[pun(sized = 4)]
struct Misspelled {
  a: u32,
}

fn main() {}
