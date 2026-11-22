// The niche of the `Option` is in a field of the first element of the array, after another field.
// What is known of that field is not known of the same field of the next elements.
use core::num::NonZero;

#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
struct Pair(u8, NonZero<u8>);

fn main() {
  let bytes = [[0u8, 1], [0, 0], [0, 0]];
  let _: &Option<[Pair; 3]> = puns::pun_ref(&bytes);
}
