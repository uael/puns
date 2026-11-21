//! A `MaybeUninit<T>` is `Pun` only if `T` is. A `T` that holds an `UnsafeCell` is not, so a
//! shared reference cannot be reinterpreted as a `&MaybeUninit<T>`, through which `T` could write.

use core::{cell::Cell, mem::MaybeUninit};

fn main() {
  let byte = 5u8;
  let _: &MaybeUninit<Cell<u8>> = puns::pun_ref(&byte);
}
