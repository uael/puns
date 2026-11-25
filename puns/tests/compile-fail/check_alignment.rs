//! `check` checks bytes as they will be at an address of the alignment that it is given, a power of
//! two, which the target needs.

fn main() {
  let _ = puns::check::<u32, 2, _>(&[0; 4], &mut []);
  let _ = puns::check::<u8, 3, _>(&[0; 4], &mut []);
}
