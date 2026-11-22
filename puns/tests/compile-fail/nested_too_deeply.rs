//! The analysis follows a type 32 levels deep, and refuses a deeper one.
#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
struct In<T>(bool, T);

macro_rules! nest {
  ($t:ty;) => { $t };
  ($t:ty; $first:tt $($rest:tt)*) => { nest!(In<$t>; $($rest)*) };
}

/// A `bool` in 33 structs.
type Deep = nest!(bool; 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2);

fn main() {
  let _ = puns::try_pun::<[u8; 34], Deep>([0; 34]);
}
