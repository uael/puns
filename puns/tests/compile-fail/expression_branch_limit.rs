//! A branch of an expression that may not be computed, the right side of `||` here, counts
//! toward the 256 operations of the expression like the rest. The derive reports it with the
//! numbers, however long the branch (here 65,536 operations in it).
use puns::Pun;

macro_rules! item {
  ($($sum:tt)*) => {
    #[derive(Pun)]
    #[repr(C)]
    #[pun(assert(a == 0 || $($sum)* == 0))]
    struct Long {
      a: u8,
    }
  };
}

// A sum of 2^15 `a`s, balanced by parentheses: 32,768 reads and 32,767 additions.
macro_rules! double {
  ((); $($sum:tt)*) => { item!($($sum)*); };
  (($x:tt $($rest:tt)*); $($sum:tt)*) => { double!(($($rest)*); ($($sum)*) + ($($sum)*)); };
}

double!((x x x x x x x x x x x x x x x); a);

fn main() {}
