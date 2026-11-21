//! `#[pun(crate = ..)]` cannot lead to other types than the ones of the crate: here a module
//! `Op` whose `load` would read the padding byte of the struct, where its field is.
#[expect(unused_imports, reason = "what the derive used to name through the path: it no longer does")]
mod fake {
  pub use puns::{At, Fields, Path, Pun, RuleAt, __site, steps};
  pub mod layout {
    pub use puns::layout::{
      Endian, Enum, Field, Id, Layout, Rule, Spec, Variant, merge_ranges, range, tail_offset,
    };
    #[expect(non_snake_case, reason = "a module that stands for the type `Op`")]
    pub mod Op {
      pub use puns::layout::Op::*;
      pub const fn load<T>(_: usize) -> puns::layout::Op {
        puns::layout::Op::Load { offset: 1, size: 1, big: false }
      }
    }
  }
}

#[derive(puns::Pun)]
#[pun(crate = fake)]
#[repr(C)]
#[pun(assert(a == 0))]
struct Ruled {
  a: u8,
  b: u16,
}

fn main() {
  let _ = puns::try_pun_ref::<[u16; 2], Ruled>(&[0, 0]);
}
