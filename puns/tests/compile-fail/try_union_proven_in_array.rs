//! A union that the same union proves, in an array whose other element is checked. The elements
//! share their check, which would read the first union too, of which nothing is known: its second
//! byte is not written here.
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Flags {
  both: [bool; 2],
  first: bool,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Source {
  flags: Flags,
  bytes: [u8; 2],
}

fn main() {
  let source = Source { flags: Flags { first: true }, bytes: [1, 1] };
  let _ = puns::try_pun_ref::<Source, [Flags; 2]>(&source);
}
