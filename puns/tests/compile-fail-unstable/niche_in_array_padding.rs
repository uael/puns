// The niche of the `Option` is in the first `bool` of the array. In the variant `A` of the source,
// the byte of the second `bool` is padding. What the tag of `A` proves of the first `bool` says
// nothing of that byte.
#[derive(puns::Pun, Clone, Copy)]
#[repr(u8)]
enum Source {
  A = 0,
  B(bool) = 1,
}

fn main() {
  let _: Option<[bool; 2]> = puns::pun(Source::A);
}
