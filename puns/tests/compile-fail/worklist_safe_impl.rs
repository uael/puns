//! The check keeps its entries in the memory of a `Worklist` and reads them back, which a safe
//! implementation could not promise: the trait is `unsafe`.

use puns::{Pending, Worklist};

struct Shrinks(Vec<Pending>);

impl Worklist for Shrinks {
  fn items(&mut self) -> &mut [Pending] {
    &mut self.0
  }

  fn grow(&mut self) -> bool {
    self.0 = Vec::new();
    true
  }
}

fn main() {
  let _ = puns::check::<u8, 1, _>(&[0], &mut Shrinks(Vec::new()));
}
