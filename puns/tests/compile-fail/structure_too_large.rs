// What the layout of an unsized struct rests on (`Layout::structure`, `# Safety`): the size of a
// value whose tail has 8 elements must fit in an `isize`. The derive does not check it, and relies
// on const evaluation to report the size of a slice that is larger than an object can be. Here, one
// element is under that bound, and 8 of them are past `isize::MAX`. If this ever compiles, the
// layout of the struct would rest on a size that overflowed.
#[derive(puns::Pun)]
#[repr(C)]
struct Huge {
  tail: [[u8; (1 << 61) - 1]],
}

fn main() {
  let _ = <Huge as puns::Pun>::LAYOUT;
}
