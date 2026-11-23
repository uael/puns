//! Opaque scalars: offsets, as the self-relative pointers of another crate hold, which `puns` reads
//! by their values and never makes, and their proof by `Validate`.

use core::{marker::PhantomData, num::NonZero};

use puns::{
  Pun, Scalar, Validate,
  layout::{Endian, Spec},
};

/// The values of a `NonZero<u32>`: every `u32` but zero.
const NON_ZERO: &[(u128, u128)] = &[(1, u32::MAX as u128)];

/// An offset that is never zero. Its invariant, beyond its values: it is a multiple of 4. It does
/// not depend on where the offset is, so a copy meets it as the original does. Nothing in these
/// tests relies on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct Off(pub NonZero<u32>);

// SAFETY: `Off` is `repr(transparent)` over a `NonZero<u32>`, so it has its size and its values,
// every `u32` but zero in the byte order of the target, each a valid `Off` in Rust's sense. Its
// invariant is not in its bytes: `SPEC` is opaque. It holds no `UnsafeCell`.
unsafe impl Scalar for Off {
  const SPEC: Spec = Spec::ranges::<Off>(Endian::NATIVE, NON_ZERO).opaque();
}

#[derive(Pun, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Node {
  pub tag: u32,
  pub next: Off,
}

/// What a `Node` is proven against: its offset must also be less than `len`, which these tests
/// require beyond the invariant of `Off`.
pub struct Context {
  pub len: usize,
}

impl Default for Context {
  fn default() -> Self {
    Self { len: 16 }
  }
}

// SAFETY: `validate` returns `Ok` only if `next` is a multiple of 4, the invariant of `Off`, which
// does not depend on where the node is. Nothing relies on it, so no write through a `&mut Node`
// breaks what the code of `Node` relies on.
unsafe impl Validate for Node {
  type Context<'a> = Context;
  type Error = u32;

  fn validate(&self, cx: &mut Context) -> Result<(), u32> {
    let next = self.next.0.get();
    if next.is_multiple_of(4) && (next as usize) < cx.len { Ok(()) } else { Err(next) }
  }
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
/// packed struct, and `rustc` keeps the one of its field. Its invariant is that of `Off`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C, packed)]
pub struct Packed(pub NonZero<u32>);

// SAFETY: `Packed` is a packed struct of one `NonZero<u32>`, so it has its size, without padding,
// and its values, every `u32` but zero, each a valid `Packed`. `SPEC` is opaque, as for `Off`. It
// holds no `UnsafeCell`.
unsafe impl Scalar for Packed {
  const SPEC: Spec = Spec::ranges::<Packed>(Endian::NATIVE, NON_ZERO).opaque();
}

/// A node whose offsets may be null, `None` as zero.
#[derive(Pun, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct Nullable {
  pub tag: u32,
  pub next: Option<Off>,
  pub packed: Option<Packed>,
}

/// An offset to a `T`, generic as the pointers of another crate are. Its invariant, beyond its
/// values: it aims at a `T` of the bytes that hold it. Nothing in these tests relies on it.
#[repr(transparent)]
pub struct Ref<T>(NonZero<u32>, PhantomData<T>);

impl<T> Ref<T> {
  pub const fn new(offset: u32) -> Self {
    Self(NonZero::new(offset).unwrap(), PhantomData)
  }
}

// SAFETY: As for `Off`: `repr(transparent)` over a `NonZero<u32>` (a `PhantomData` has size 0),
// every `u32` but zero, opaque, no `UnsafeCell`.
unsafe impl<T: Pun> Scalar for Ref<T> {
  const SPEC: Spec = Spec::ranges::<Ref<T>>(Endian::NATIVE, NON_ZERO).opaque();
}
