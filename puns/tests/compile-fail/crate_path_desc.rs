//! `#[pun(crate = ..)]` cannot lead to another description of an enum of the `Rust` representation
//! than the one that the experiments find: here a `layout::Desc` that says that the tags of `E` are
//! 5 and 7, where `rustc` gives it 0 and 1, so that the bytes `[5, 9]` would be an `E`.

mod fake {
  pub use puns::*;
  pub mod layout {
    pub use puns::layout::*;

    /// A description that no experiment made.
    pub struct Desc<const V: usize>;

    impl<const V: usize> Desc<V> {
      pub const fn variant_ranges(&'static self, variant: usize) -> &'static [(u128, u128)] {
        if variant == 0 { &[(5, 5)] } else { &[(7, 7)] }
      }
      pub const fn holds_tag(&self, _: usize) -> bool {
        false
      }
      pub const fn inhabited(&self, _: usize) -> bool {
        true
      }
      pub const fn tag(&'static self) -> Layout {
        Layout::scalar::<u8>(Spec::ranges::<u8>(Endian::NATIVE, &[(5, 5), (7, 7)]))
      }
      pub const fn tag_offset(&self) -> usize {
        0
      }
    }

    /// What the derive implemented to find the description, before it called `Layout::discover`.
    pub trait Discover<const V: usize>: Sized {
      const SAMPLES: [core::mem::MaybeUninit<Self>; V];
      const FIELDS: [&'static [Field]; V];
      const DESC: Desc<V> = Desc;
    }
  }
}

#[derive(puns::Pun, Clone, Copy)]
#[pun(crate = fake)]
enum E {
  A(u8),
  B(u8),
}

fn main() {
  let _ = puns::try_pun::<[u8; 2], E>([5, 9]);
}
