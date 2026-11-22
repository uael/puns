//! A variant that holds a tuple with a scalar that a pun never makes, its `bool` declared first.
//! No value of the variant is valid, but the enum is checked as a whole, and the check of the
//! variant reads the fields of the tuple where `rustc` put them, which would read a byte that may
//! be uninitialized in the source.
#[path = "../support/never.rs"]
#[allow(dead_code)]
mod never;
use never::Off;
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Byte {
  any: u8,
  none: (),
}

#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Target {
  A((bool, Off)),
  B,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Source {
  tag: u8,
  rest: [Byte; 11],
}

fn main() {
  const { assert!(size_of::<Source>() == size_of::<Target>()) };
  let source = Source { tag: 0, rest: [Byte { none: () }; 11] };
  let _ = puns::try_pun::<Source, Target>(source);
}
