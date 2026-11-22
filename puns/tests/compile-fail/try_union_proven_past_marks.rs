//! A union that the same union proves, among the parts past the first 1,024 of the target, which
//! are checked all together or not at all. The `bool` after the union is one of them and is
//! checked, so the union would be checked too, and nothing is known of its bytes.
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
struct Source {
  head: Flags1024,
  either: Either,
  byte: u8,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Target {
  head: Flags1024,
  either: Either,
  flag: bool,
}

fn main() {
  let _: fn(&Source) -> Result<&Target, puns::Error> = puns::try_pun_ref;
}
