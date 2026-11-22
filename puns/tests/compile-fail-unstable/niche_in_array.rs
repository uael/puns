// The niche of the `Option` is in the first element of the array. In `Some`, that element is known
// not to be 0, since 0 is the tag of `None`. Nothing is known of the next elements, which may be 0,
// and no `NonZero` is 0.
use core::num::NonZero;

fn main() {
  let _: Option<[NonZero<u8>; 4]> = puns::pun([1u8, 0, 0, 0]);
}
