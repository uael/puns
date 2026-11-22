//! A string type whose automaton does not accept the empty string is not proven by a source that
//! has no byte for it: the string of `[u8; 0]` is always empty.
#[path = "../support/non_empty.rs"]
mod non_empty;

fn main() {
  let _ = puns::pun_ref::<[u8; 0], non_empty::NonEmpty>(&[]);
}
