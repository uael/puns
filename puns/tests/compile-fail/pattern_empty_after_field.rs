//! A string type whose automaton does not accept the empty string is not proven by a source that
//! ends where the string starts: the body of a `Msg` from `[u8; 1]` is always empty.
#[path = "../support/non_empty.rs"]
mod non_empty;

fn main() {
  let _ = puns::pun_ref::<[u8; 1], non_empty::Msg>(&[7]);
}
