//! A self-relative pointer with an eager aim, declared through the public API of `puns` as another
//! crate (`nearest`) declares one: an `i32` offset from its own address to a `Thin<T, i32>`.

use core::{
  marker::{PhantomData, PhantomPinned},
  num::NonZero,
  ops::Deref,
  ptr,
};

use puns::{
  At, Fields, Path, Pun, RuleAt, Thin,
  layout::{Aim, Check, Field, Id, Layout, Metadata, Op, Rule, Start},
  steps::{One, Two},
};

/// A pointer to a `T`, stored as the distance from its own address, an `i32` of the byte order of
/// the target that is never zero. A slice or a `str` is behind its length, an `i32`.
#[repr(transparent)]
pub struct Near<T: ?Sized> {
  off: NonZero<i32>,
  _t: PhantomData<T>,
  _pin: PhantomPinned,
}

/// The offset of a `Near`, sign-extended to an `i128` in two's complement:
/// `off | (off >> 31) * (u128::MAX << 32)`.
const AT: [Op; 7] = {
  let load = Op::Load { offset: 0, size: 4, big: cfg!(target_endian = "big") };
  [load, load, Op::Lit(31), Op::Shr, Op::Lit(u128::MAX << 32), Op::Mul, Op::Or]
};

/// The operations of `AT`, as a type.
type AtSteps = Two<Two<One<0>, Two<One<1>, One<2>>>, Two<Two<One<3>, One<4>>, Two<One<5>, One<6>>>>;

/// The field `off` of a `Near`, its only field with bytes.
pub struct NearOff<T: ?Sized>(PhantomData<T>);

// SAFETY: `FIELD` is the one field of the layout of `Near<T>`, `off`, of type `NonZero<i32>`, and
// the list ends after it. The other fields have no bytes and no invariant, and are not in the
// layout.
unsafe impl<T: ?Sized + Pun> Fields for NearOff<T> {
  type Type = NonZero<i32>;
  type Next = ();
  type Last = Self;
  type Path<P: Path> = At<P, Self>;
  const FIELD: Option<Field> = Some(*<Near<T> as Pun>::LAYOUT.field(0));
}

// SAFETY: `Near<T>` is `repr(transparent)` over its `NonZero<i32>`, whose layout is the one field,
// at 0. `PhantomData` and `PhantomPinned` have no bytes and no invariant. The rule is an eager aim
// at a `Thin<T, i32>`, at the address of the `Near` plus its offset, which `Rules` names. A `Near`
// has no constructor and is neither `Clone` nor `Copy`, so one exists only where `puns` checked its
// target, which `Deref` reads. `identified` changes neither its size, its alignment nor its
// values. `core` documents `Option<Near<T>>` as the `Near`, `None` as zero.
unsafe impl<T: ?Sized + Pun> Pun for Near<T> {
  type Elem = ();
  type Fields = NearOff<T>;
  type Variants = ();
  type Rules = RuleAt<0, AtSteps, (), Thin<T, i32>>;
  type Metadata = ();
  const PROBE: *const Self = ptr::null();
  // SAFETY: `Near<T>` is `Sized`.
  const LAYOUT: Layout = unsafe {
    Layout::structure::<Self>(
      ptr::null(),
      const { &Layout::sort_fields([Field::new(0, &<NonZero<i32> as Pun>::LAYOUT)]) },
      const {
        &[Rule::Aim(Aim::new::<Thin<T, i32>>(Start::Stored(&AT), Metadata::Thin, Check::Eager))]
      },
    )
    .identified(Self::ID)
  };
  // Where `Near` is declared, and the identity of `T`, as the derive names a generic type whose
  // parameter is bounded by `Pun`.
  const ID: Id = Id::new(puns::__site!(Near, 0), const { &[T::ID] });
  const NULL_OPTIMIZED: bool = true;
}

impl<T: ?Sized + Pun> Deref for Near<T> {
  type Target = T;

  fn deref(&self) -> &T {
    let at = ptr::from_ref(self).cast::<u8>().wrapping_offset(self.off.get() as isize);
    // SAFETY: A `Near` exists only where `puns` checked that its target is a valid `Thin<T, i32>`,
    // aligned, in the bytes that the reference to it borrows.
    unsafe { Thin::<T, i32>::get(at) }
  }
}

/// A pair of a name, numbers and the next pair.
#[derive(Pun)]
#[repr(C)]
pub struct Pair {
  pub name: Near<str>,
  pub values: Near<[u32]>,
  pub next: Option<Near<Pair>>,
}

/// A node whose children are a list.
#[derive(Pun)]
#[repr(C)]
pub struct Tree {
  pub id: u32,
  pub kids: Near<[Tree]>,
}

/// An expression whose operands are pointers.
#[derive(Pun)]
#[repr(u8)]
pub enum Expr {
  Lit(i64),
  Add(Near<Expr>, Near<Expr>),
}
