//! A string type whose automaton does not accept the empty string is not proven by a slice of
//! bytes, which may be empty.
#[path = "../support/non_empty.rs"]
mod non_empty;

fn main() {
  let _ = puns::pun_ref::<[u8], non_empty::NonEmpty>(&[7]);
}
