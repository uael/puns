//! An automaton of a rule has 1 to 64 states, and the edges of each state are sorted, apart, and
//! lead to one of its states.
use puns::layout::{Dfa, DfaState};

const NO_STATE: &Dfa = &Dfa { id: 0, states: &[] };
const TO_NO_STATE: &Dfa = &Dfa { id: 0, states: &[DfaState { accept: true, edges: &[(0, 9, 1)] }] };
const UNSORTED: &Dfa =
  &Dfa { id: 0, states: &[DfaState { accept: true, edges: &[(5, 9, 0), (0, 2, 0)] }] };

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(matches(NO_STATE, 0, 2)))]
struct NoState {
  a: [u8; 2],
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(matches(TO_NO_STATE, 0, 2)))]
struct ToNoState {
  a: [u8; 2],
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(matches(UNSORTED, 0, 2)))]
struct Unsorted {
  a: [u8; 2],
}

fn main() {}
