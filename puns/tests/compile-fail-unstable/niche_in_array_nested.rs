// The niche of the `Option` is in the first element of the first array. What is known of it is
// known of no other element, of that array or of the next one.
use core::num::NonZero;

fn main() {
  let _: Option<[[NonZero<u16>; 2]; 2]> = puns::pun([[1u16, 0], [0, 0]]);
}
