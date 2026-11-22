//! An integer of 3 bytes holds no value of 25 bits.
use puns::num::u24le;

const _: u24le = u24le::new(1 << 24);

fn main() {}
