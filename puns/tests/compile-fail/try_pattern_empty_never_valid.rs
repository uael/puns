//! A string type whose automaton does not accept the empty string is never made from a source
//! that has no byte for it: no check accepts the string of `[u8; 0]`, which is always empty.
#[path = "../support/non_empty.rs"]
mod non_empty;

fn main() {
  let _ = puns::try_pun_ref::<[u8; 0], non_empty::NonEmpty>(&[]);
}
