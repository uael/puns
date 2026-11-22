//! `#[pun(crate = ..)]` cannot lead to aliases that list other rules than those of the struct
//! (here, the first one alone, so that the second would not be checked), or another operation than
//! a walk where the expression has one.

/// `T`, whatever `Self` is: what an alias of two types makes of the one that it drops.
pub trait Ignore<T> {
  type Kept;
}

impl<X, T> Ignore<T> for X {
  type Kept = T;
}

mod fewer {
  pub use puns::*;
  pub type RuleAt<const R: usize, E, N> = <N as super::Ignore<puns::RuleAt<R, E, ()>>>::Kept;
}

mod no_walk {
  pub use puns::*;
  pub mod steps {
    pub use puns::steps::*;
    pub type Walk<const I: usize, C, N> = <(C, N) as super::super::Ignore<One<I>>>::Kept;
  }
}

#[derive(puns::Pun, Clone, Copy)]
#[pun(crate = fewer)]
#[pun(assert(a < 10), assert(b < 10))]
#[repr(C)]
struct Fewer {
  a: u8,
  b: u8,
}

#[derive(puns::Pun, Clone, Copy)]
#[pun(crate = no_walk)]
#[pun(assert(walk(a, |at| at < 4, |at| at + 1) == 4))]
#[repr(C)]
struct NoWalk {
  a: u8,
}

fn main() {
  let _ = puns::try_pun::<[u8; 2], Fewer>([0, 7]);
  let _ = puns::try_pun::<u8, NoWalk>(0);
}
