// What the layout of an enum of the `Rust` representation rests on (`docs/rust.md`, "Errors of
// const evaluation" and "The padding of a new value"). The padding of a value made in const
// evaluation is uninitialized, and reading it is an error. Here, bytes 1 to 3 of `E::B`, between
// its tag and its `u32`. If this ever compiles, padding would pass for a tag.
use core::mem::MaybeUninit;

#[expect(dead_code, reason = "only its layout matters")]
enum E {
  A(u8),
  B(u32),
}

const SAMPLE: MaybeUninit<E> = MaybeUninit::new(E::B(7));

// SAFETY: Not sound, on purpose: byte 2 of `E::B` is padding. Const evaluation must report it.
const PADDING: u8 = unsafe { *SAMPLE.as_ptr().cast::<u8>().add(2) };

fn main() {
  let _ = PADDING;
}
