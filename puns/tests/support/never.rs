//! A scalar that a pun reads but never makes: an offset, as a self-relative pointer of another
//! crate would hold.

use core::{marker::PhantomData, num::NonZero};

use puns::{Pun, Scalar, layout::Spec};

/// An offset that is never zero, whose validity is not in its bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct Off(pub NonZero<u32>);

// SAFETY: `Off` is `repr(transparent)` over a `NonZero<u32>`, so it has its size, and every value
// is 4 initialized bytes, which is what `Spec::never` says is known of it. It accepts no value, so
// no pun makes one. It holds no `UnsafeCell`.
unsafe impl Scalar for Off {
  const SPEC: Spec = Spec::never::<Off>();
}

#[derive(Pun, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Node {
  pub tag: u32,
  pub next: Off,
}

#[derive(Pun, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C, u32)]
pub enum Link {
  End = 0,
  Next(Off) = 1,
  Value(u32) = 2,
}

#[derive(Pun, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
#[pun(assert(count < 10))]
pub struct Ruled {
  pub count: u32,
  pub next: Off,
}

/// An offset of alignment 1, as an offset of a fixed byte order is: `core` documents no niche of a
/// packed struct, and `rustc` keeps the one of its field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C, packed)]
pub struct Packed(pub NonZero<u32>);

// SAFETY: `Packed` is a packed struct of one `NonZero<u32>`, so it has its size, without padding,
// and every value is 4 initialized bytes, as `Spec::never` says. It accepts no value, so no pun
// makes one. It holds no `UnsafeCell`.
unsafe impl Scalar for Packed {
  const SPEC: Spec = Spec::never::<Packed>();
}

/// A node whose offsets may be null, `None` as zero.
#[derive(Pun, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Nullable {
  pub tag: u32,
  pub next: Option<Off>,
  pub packed: Option<Packed>,
}

/// An offset to a `T`, generic as the pointers of another crate are.
#[repr(transparent)]
pub struct Ref<T>(NonZero<u32>, PhantomData<T>);

impl<T> Ref<T> {
  pub const fn new(offset: u32) -> Self {
    Self(NonZero::new(offset).unwrap(), PhantomData)
  }
}

// SAFETY: As for `Off`: `repr(transparent)` over a `NonZero<u32>` (a `PhantomData` has size 0),
// 4 initialized bytes, no value accepted, no `UnsafeCell`.
unsafe impl<T: Pun> Scalar for Ref<T> {
  const SPEC: Spec = Spec::never::<Ref<T>>();
}
