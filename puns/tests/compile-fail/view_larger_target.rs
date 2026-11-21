//! What a pointer aims at is reinterpreted as something that is not larger.
#[path = "../support/relative.rs"]
#[allow(dead_code)]
mod relative;
use relative::One;

fn view(source: &One<u16>) -> &One<[u8; 4]> {
  puns::view_ref(source)
}

fn main() {
  let _: fn(&One<u16>) -> &One<[u8; 4]> = view;
}
