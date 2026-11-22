//! As `try_union_proven_past_marks`, with two unions in a tuple, whose layout Rust leaves to the
//! compiler. A struct of the same unions proves the tuple wherever its fields are. The tuple is
//! among the parts past the first 1,024, which are checked all together, and nothing is known of
//! the bytes of the unions.
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Flags4(bool, bool, bool, bool);

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Flags16(Flags4, Flags4, Flags4, Flags4);

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Flags64(Flags16, Flags16, Flags16, Flags16);

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Flags256(Flags64, Flags64, Flags64, Flags64);

/// 1,024 parts to check, each a `bool`.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Flags1024(Flags256, Flags256, Flags256, Flags256);

#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Either {
  both: [bool; 2],
  first: bool,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Both(Either, Either);

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Source {
  head: Flags1024,
  both: Both,
  byte: u8,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Target {
  head: Flags1024,
  both: (Either, Either),
  flag: bool,
}

fn main() {
  let _: fn(&Source) -> Result<&Target, puns::Error> = puns::try_pun_ref;
}
