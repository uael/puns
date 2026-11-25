// What the layout of an enum of the `Rust` representation rests on (`docs/what_puns_relies_on.md`,
// "Errors of const evaluation"): const evaluation refuses to read the variant of a value whose tag
// is not one, here a `bool` of 2 where `rustc` put the niche of `None`. If this ever compiles, an
// experiment that goes wrong could give an answer instead of failing.
use core::mem::{self, MaybeUninit};

const fn variant_of_two() -> mem::Discriminant<Option<bool>> {
  let mut value = MaybeUninit::new(Some(true));
  // SAFETY: Not sound, on purpose: 3 is no tag of `Option<bool>` (0 and 1 are `Some`, 2 is `None`),
  // so the reference is to an invalid value. Const evaluation must report it, and fail the build.
  unsafe {
    value.as_mut_ptr().cast::<u8>().write(3);
    mem::discriminant(&*value.as_ptr())
  }
}

const VARIANT: mem::Discriminant<Option<bool>> = variant_of_two();

fn main() {
  let _ = VARIANT;
}
