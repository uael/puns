// The sample of a type is written from its layout, part by part, and a sized type holds no slice
// and no `str`. Here a sized type is said to have the layout of a slice. If this ever compiles, a
// sample would be written with a length that nothing gives.
use core::mem::MaybeUninit;

use puns::{Pun, layout::Layout};

struct Wrong(u8);

// SAFETY: Not sound, on purpose: `Wrong` is sized, and its layout is said to be the one of `[u8]`.
// The sample of it must be refused, and fail the build.
unsafe impl Pun for Wrong {
  type Elem = u8;
  type Fields = ();
  type Rules = ();
  type Variants = ();
  const LAYOUT: Layout = <[u8] as Pun>::LAYOUT;
  const PROBE: *const Self = core::ptr::null();
}

const _: MaybeUninit<Wrong> = Layout::sample::<Wrong>();

fn main() {}
