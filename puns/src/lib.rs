#![doc = include_str!("../README.md")]
//!
//! # Reference
//!
//! What follows is only in this documentation: the functions with their links, what deriving
//! `Pun` gives away, and two more refusals of enums.
//!
//! |               | Whole                                  | Front                     | Back                      |
//! |---------------|----------------------------------------|---------------------------|---------------------------|
//! | a typed value | [`pun`], [`pun_ref`], [`pun_mut`] (and [`try_pun`], [`try_pun_ref`], [`try_pun_mut`]) | [`head`], [`head_ref`], [`head_mut`] | [`tail`], [`tail_ref`], [`tail_mut`] |
//! | bytes         | [`from_bytes`], [`from_bytes_mut`], [`as_bytes`] | [`from_head`], [`from_head_mut`] | [`from_tail`], [`from_tail_mut`] |
//!
//! The types of a given byte order or value are in [`num`]; the options of the derive are in
//! [`macro@Pun`]; what a layout is made of, and the checksums, in [`layout`].
//!
//! # What deriving `Pun` means
//!
//! A type is known by its layout and by what the type system enforces of its bytes, nothing more.
//! Two consequences are by design.
//!
//! `PhantomData<T>` is a scalar of no byte, for any `T`: types that differ only by phantom
//! parameters, or by lifetimes, convert to each other freely. A type whose parameters are a brand
//! (a unit, an invariant lifetime that ties a value to a scope, a marker of what a value was
//! checked against) should not derive `Pun`:
//!
//! ```
//! use core::marker::PhantomData;
//!
//! #[derive(Clone, Copy)]
//! struct Meters;
//! #[derive(Clone, Copy)]
//! struct Feet;
//!
//! #[derive(puns::Pun, Clone, Copy)]
//! #[repr(C)]
//! struct Length<U> {
//!   value: u32,
//!   unit: PhantomData<U>,
//! }
//!
//! // Not what a unit is for: 3 meters are 3 feet.
//! let feet: Length<Feet> = puns::pun(Length::<Meters> { value: 3, unit: PhantomData });
//! assert_eq!(feet.value, 3);
//! ```
//!
//! Deriving `Pun` makes every field as accessible as if it were public: a value can be made from
//! anything whose bytes are valid for its fields (and the rules the derive is told of), read as
//! anything its bytes are, and written through a `&mut` to another type. A type whose private
//! fields keep an invariant that the derive is not told of should not derive `Pun`:
//!
//! ```
//! mod even {
//!   /// Only even numbers, says `new`.
//!   #[derive(puns::Pun, Clone, Copy)]
//!   #[repr(transparent)]
//!   pub struct Even(u32);
//!
//!   impl Even {
//!     pub const fn new(value: u32) -> Option<Self> {
//!       if value % 2 == 0 { Some(Self(value)) } else { None }
//!     }
//!
//!     pub const fn get(self) -> u32 {
//!       self.0
//!     }
//!   }
//! }
//!
//! assert!(even::Even::new(3).is_none());
//! // What `new` refuses, `pun` makes.
//! let odd: even::Even = puns::pun(3u32);
//! assert_eq!(odd.get(), 3);
//! ```
//!
//! # Enums: what is refused
//!
//! A `Shape` is not 4 bytes: the byte after the tag of a `Circle` is padding, and nothing tells a
//! `Circle` from a `Rect` at compile time.
//!
//! ```compile_fail
//! # use puns::Pun;
//! # #[derive(Pun, Clone, Copy)] #[repr(u8)] enum Shape { Circle { radius: u16 }, Rect(u8, bool) }
//! let _: [u8; 4] = puns::pun(Shape::Rect(3, true));
//! ```
//!
//! Neither can a `Drawing` be written where a `Shape` is:
//!
//! ```compile_fail
//! # use puns::Pun;
//! # #[derive(Pun, Clone, Copy)] #[repr(u8)] enum Shape { Circle { radius: u16 }, Rect(u8, bool) }
//! # #[derive(Pun, Clone, Copy)] #[repr(u8)] enum Drawing { Circle { radius: u16 }, Rect(u8, u8), Nothing }
//! let mut shape = Shape::Rect(3, true);
//! let _: &mut Drawing = puns::pun_mut(&mut shape);
//! ```
//!
//! # Self-relative pointers
//!
//! A crate declares its self-relative pointers (offsets from their own address to a target in the
//! same buffer, as the regions of [`nearest`](https://docs.rs/nearest) have) with
//! [`SelfRelative`], and [`view_ref`] reads a value that holds them as another type, with
//! everything they aim at proven at compile time. The crate that has the pointers checks a
//! buffer of them with [`validate_in`]: each value from its layout, its pointers handed to the
//! crate's [`Visit`], which follows them as its scheme says; and walks the pointers of a value
//! known to be valid (to copy it, or to move it in its buffer) with [`pointers_in`], which hands
//! the visitor the same pointers and checks nothing; and copies such a value, its initialized
//! bytes only, with [`copy_in`].
//!
//! Refusals are errors of the monomorphization of the functions: `cargo build` reports them,
//! `cargo check` does not.
//!
//! See `DESIGN.md` for how it works.
#![cfg_attr(not(test), no_std)]
// Every public item has documentation, as docs.rs counts it; CI checks that each has an example.
#![deny(missing_docs)]

mod analysis;
mod deep;
mod discover;
#[cfg(feature = "verdict")]
#[doc(hidden)]
pub mod hook;
pub mod layout;
mod narrow;
#[cfg(test)]
mod narrow_tests;
pub mod num;
mod ptr;
mod relative;
mod std_enums;
#[cfg(test)]
mod tests;
mod validate;
#[doc(hidden)]
pub mod view;
mod visit;

use core::{marker::PhantomData, mem::ManuallyDrop};

use analysis::{
  Analysis, Exact, MUTABLE, Marks, POINTER_MUT, TAIL_UNSIZED_SOURCE, TAIL_UNSIZED_TARGET, analyze,
  analyze_tail, mutable, mutable_part, mutable_unspecified,
};
use layout::{Dfa, Endian, Field, Layout, Spec};
pub use layout::{crc32, sum16};
pub use puns_derive::Pun;
pub use relative::SelfRelative;
#[doc(hidden)]
pub use relative::sized_probe as __sized_probe;
use validate::{Checks, validate};
pub use visit::{
  Invalid, Pointee, Visit, copy_in, pointers_elems_in, pointers_in, validate_elems_in, validate_in,
};

/// Where the type `$name` is declared, its name, and `$n`, the number of the derive in its crate:
/// its identity (`layout::Id`), for `#[derive(Pun)]`, which names `core` through this crate (a
/// crate may have another `core`). The place is the outermost macro call that declares the type,
/// which may declare two (one in each of two functions): the number tells them apart.
#[doc(hidden)]
#[macro_export]
macro_rules! __site {
  ($name:ident, $n:literal) => {
    $crate::__core::concat!(
      $crate::__core::module_path!(),
      "::",
      $crate::__core::stringify!($name),
      "@",
      $crate::__core::file!(),
      ":",
      $crate::__core::line!(),
      ":",
      $crate::__core::column!(),
      "#",
      $n
    )
  };
}

#[doc(hidden)]
pub use core as __core;

// -------------------------------------------------------------------------------------------------
// Traits

/// Leaf types: integers and the like, see [`Spec`].
///
/// This is what `#[derive(Pun)]` implements for enums without fields. Every `Scalar` is a
/// [`Pun`] type.
///
/// A type of a few bytes whose valid values are checked by its constructor implements it by hand:
///
/// ```
/// mod percent {
///   use puns::layout::{Endian, Spec};
///
///   /// A byte from 0 to 100.
///   #[derive(Clone, Copy)]
///   #[repr(transparent)]
///   pub struct Percent(u8);
///
///   impl Percent {
///     pub const fn new(value: u8) -> Option<Self> {
///       if value <= 100 { Some(Self(value)) } else { None }
///     }
///
///     pub const fn get(self) -> u8 {
///       self.0
///     }
///   }
///
///   // SAFETY: `new` makes the bytes from 0 to 100, and nothing else makes one.
///   unsafe impl puns::Scalar for Percent {
///     const SPEC: Spec = Spec::ranges::<Percent>(Endian::NATIVE, &[(0, 100)]);
///   }
/// }
///
/// use percent::Percent;
///
/// assert_eq!(puns::from_bytes::<Percent>(&[42]).unwrap().get(), 42);
/// assert_eq!(puns::from_bytes::<Percent>(&[101]).err(), Some(puns::Error::Validity));
/// // Proven: every `Percent` is a `u8`.
/// assert_eq!(puns::pun::<Percent, u8>(Percent::new(7).unwrap()), 7);
/// ```
///
/// # Safety
/// `SPEC` must describe every valid value of `Self`, and nothing more.
pub unsafe trait Scalar {
  /// The size, the byte order and the valid values of `Self`.
  const SPEC: Spec;
  /// See [`Pun::NULL_OPTIMIZED`].
  #[doc(hidden)]
  const NULL_OPTIMIZED: bool = false;
  /// The identity of the type (see [`layout::Id`]); none by default.
  #[doc(hidden)]
  const ID: layout::Id = layout::Id::NONE;
  /// Whatever Rust specifies how values of the type are stored: not of an enum without `repr`,
  /// which `#[derive(Pun)]` makes a `Scalar` (see [`layout::UNSTABLE`]).
  #[doc(hidden)]
  const SPECIFIED: bool = true;
}

/// Types made of plain data, whose layout and valid values are known at compile time.
///
/// Every function of this crate takes `Pun` types: the integer and floating-point types, `bool`,
/// `char`, `NonZero`, `()`, `PhantomData`, arrays, slices, `str`, tuples of up to 4, `Option` and
/// `Result` of them, `MaybeUninit`, `ManuallyDrop`, `Infallible`, the types of [`num`], the
/// self-relative pointers of [`self_relative!`], and what derives it. Of what Rust lets the
/// compiler lay out otherwise (a tuple, a struct or an enum without `repr`), what a pun may rely
/// on without the `unstable` feature is what every layout that Rust allows has
/// ([`layout::UNSTABLE`]).
///
/// ```
/// use puns::{Pun, num::u16le};
///
/// #[derive(Pun, Clone, Copy)]
/// #[repr(C)]
/// struct Point {
///   x: u16le,
///   y: u16le,
/// }
///
/// /// The first `T` of `bytes`, and what follows it.
/// fn first<T: Pun>(bytes: &[u8]) -> Option<(&T, &[u8])> {
///   puns::from_head(bytes).ok()
/// }
///
/// let (point, rest) = first::<Point>(&[1, 0, 2, 0, 9]).unwrap();
/// assert_eq!((point.x.get(), point.y.get(), rest), (1, 2, &[9][..]));
/// ```
///
/// # Safety
/// Not meant to be implemented by hand: use `#[derive(Pun)]`, or [`self_relative!`] for a
/// self-relative pointer. What an implementation promises is that `LAYOUT` describes the values
/// of `Self` (every value is valid for it as a source, every valid target is a value), with the
/// lists of its fields, variants and elements; and that `OPTION`, if it is `Some`, describes the
/// values of `Option<Self>` the same way, `Option<Self>` having the lists of `Option`.
#[diagnostic::on_unimplemented(
  message = "`{Self}` is not a `Pun` type: `puns` does not know its layout and its valid values",
  label = "not a `Pun` type",
  note = "a struct, an enum or a union is one with `#[derive(Pun)]`, a self-relative pointer with \
          `SelfRelative` and `self_relative!`"
)]
pub unsafe trait Pun {
  /// Type of the elements of an array or of a slice, `()` otherwise.
  #[doc(hidden)]
  type Elem: Pun;
  /// First field of a struct, `()` otherwise.
  #[doc(hidden)]
  type Fields: Fields;
  /// First variant of an enum with fields, `()` otherwise.
  #[doc(hidden)]
  type Variants: Variants;
  /// First of the rules of a struct that have an expression, `()` if none.
  #[doc(hidden)]
  type Rules: Rules;
  /// What a self-relative pointer aims at (the element, if it aims at a list), `()` if `Self`
  /// is not one or aims at text.
  ///
  /// This is how the analysis follows pointers: through types, so that a recursive type is a
  /// finite set of pairs of types rather than a layout without end (see [`view_ref`]).
  #[doc(hidden)]
  type Pointee: Pun;
  /// A null pointer to `Self`, whose slice length is [`ptr::PROBE_LEN`] if `Self` is unsized.
  /// It must be made with `as` casts only: this is what the pointer tricks are checked against.
  #[doc(hidden)]
  const PROBE: *const Self;
  #[doc(hidden)]
  const LAYOUT: Layout;
  /// The layout of `Option<Self>`, if `Self` gives it, rather than the one found of `Option` as
  /// of any enum without `repr` (`discover.rs`).
  ///
  /// This is how `Option` of a self-relative pointer is the same pointer, that may be null:
  /// [`self_relative!`] fills it with [`Layout::nullable`], which checks that `None` is the
  /// pointer of zeros and nothing else. A crate that implements `Pun` otherwise (which only
  /// `#[derive(Pun)]` and the macros of this crate do) leaves it `None`. If it is `Some`, it is
  /// the layout of `Option<Self>`, as `LAYOUT` is of `Self` (see the `# Safety` of the trait).
  #[doc(hidden)]
  const OPTION: Option<Layout> = None;
  /// Whether `core` guarantees that `Option<Self>` is `Self`, with `None` the bytes of zeros:
  /// `Self` is a `NonZero` integer, or a `repr(transparent)` struct around one ("Representation"
  /// in the documentation of `core::option`). The layout of `Option<Self>` is then the same from
  /// one version of the compiler to the next, and so is the one of `Result<Self, E>` and
  /// `Result<E, Self>` where `E` has size 0 and alignment 1 ("Representation" of
  /// `core::result`). Of any other `Self`, they need the `unstable` feature.
  #[doc(hidden)]
  const NULL_OPTIMIZED: bool = false;
}

/// A type that goes with a tag, in what carries it: a payload and the value of the field that
/// tells its type, a variant and its discriminant...
///
/// `K` tells of which tag: a type can be carried by several things. It is, by convention, the type
/// of the tag when nothing is known of what is carried: bytes go with any `K`.
///
/// This is implemented by `#[pun(tag(K = T))]`, see [`macro@Pun`]:
///
/// ```
/// use puns::num::{u8c, u16be};
/// use puns::{Pun, Tag};
///
/// /// The kind of a message, when its payload is not known.
/// #[derive(Pun, Copy, Clone)]
/// #[repr(transparent)]
/// struct Kind(u8);
///
/// #[derive(Pun)]
/// #[repr(C)]
/// struct Message<P: ?Sized + Tag<Kind> = [u8]> {
///   kind: P::Tag,
///   payload: P,
/// }
///
/// #[derive(Pun)]
/// #[repr(C)]
/// #[pun(tag(Kind = u8c<7>))]
/// struct Ping {
///   sequence: u16be,
/// }
///
/// // A message of any kind, and a message that must be a ping.
/// assert_eq!(puns::try_pun_ref::<_, Message>(&[9u8, 0, 1]).unwrap().kind.0, 9);
/// assert!(puns::try_pun_ref::<_, Message<Ping>>(&[9u8, 0, 1]).is_err());
/// assert!(puns::try_pun_ref::<_, Message<Ping>>(&[7u8, 0, 1]).is_ok());
/// ```
pub trait Tag<K: ?Sized>: Pun {
  /// What goes with `Self` where it is carried with a `K`: the type of the field that tells what
  /// is carried, which bytes of it must be for them to be a `Self`.
  type Tag: Pun + Copy;
}

/// Bytes are carried with any tag.
impl<K: Pun + Copy> Tag<K> for [u8] {
  type Tag = K;
}

/// A field of a struct, and the ones after it: a list at the type level, for validation.
///
/// # Safety
/// Implemented by `#[derive(Pun)]`.
#[doc(hidden)]
pub unsafe trait Fields {
  type Type: ?Sized + Pun;
  type Next: Fields;
  /// The last field of the list, which may be unsized (`()` but for the fields of a struct, of
  /// which only the last one may be).
  type Last: Fields;
  /// Where the field is in the target, given where the struct is.
  type Path<P: Path>: Path;
  /// `None` at the end of the list.
  const FIELD: Option<Field>;
}

/// A variant of an enum with fields, and the ones after it: a list at the type level.
///
/// # Safety
/// Implemented by `#[derive(Pun)]`.
#[doc(hidden)]
pub unsafe trait Variants {
  /// First field of the variant.
  type Fields: Fields;
  type Next: Variants;
  /// The tag of the variant, `None` at the end of the list.
  const TAG: Option<u128>;
  /// The values of the tag of the variant if they are not `TAG` alone, sorted: those of an enum
  /// whose layout `rustc` chose (see `Layout::variant_ranges`), where `TAG` is its index.
  const RANGES: Option<&'static [(u128, u128)]> = None;
}

// SAFETY: the end of every list.
unsafe impl Variants for () {
  type Fields = ();
  type Next = ();
  const TAG: Option<u128> = None;
}

/// A rule of a struct that has an expression, and the ones after it: a list at the type level,
/// in the order of the rules, by which the validation applies each operation of each expression
/// in an instance of its own (see `validate::steps`).
///
/// Sealed. What a list says is checked against the layout of the struct, where it is used: every
/// rule that has an expression is listed, once, in order, with every one of its operations.
#[doc(hidden)]
pub trait Rules: sealed::Sealed {
  /// Index of the rule in the layout of the struct, `usize::MAX` at the end of the list.
  const RULE: usize;
  /// The operations of its expression.
  type Expr: Steps;
  type Next: Rules;
}

impl Rules for () {
  const RULE: usize = usize::MAX;
  type Expr = ();
  type Next = ();
}

/// The rule `R` of a struct, whose expression is `E`, then the list `N`.
#[doc(hidden)]
pub struct RuleAt<const R: usize, E, N>(PhantomData<(E, N)>);

impl<const R: usize, E: Steps, N: Rules> Rules for RuleAt<R, E, N> {
  const RULE: usize = R;
  type Expr = E;
  type Next = N;
}

/// What the operations of an expression are, as a type: the index of each, and the expressions of
/// each walk. A tree, so that the recursions down it are as deep as the logarithm of its length.
///
/// Sealed. Its indices are checked against the expression where it is applied: every operation,
/// once, in order (see `validate::covers`).
#[doc(hidden)]
pub trait Steps: sealed::Sealed {
  /// [`steps::NONE`], [`steps::ONE`], [`steps::TWO`] or [`steps::WALK`].
  const KIND: u8;
  /// The index of the operation (`ONE`, `WALK`).
  const AT: usize;
  /// The index of the first operation, and the one after the last.
  const FIRST: usize;
  const END: usize;
  /// The two parts (`TWO`), `()` otherwise.
  type A: Steps;
  type B: Steps;
  /// The condition of a walk and what follows a step (`WALK`), `()` otherwise.
  type Cond: Steps;
  type Then: Steps;
  /// Where they are, given where the walk is: `X` itself but for a walk. The functions that
  /// apply the operations name all of these, whatever the kind of `Self` (the compiler makes
  /// what a function names, used or not): so they name nothing new where they are not used.
  type InCond<X: steps::Ops>: steps::Ops;
  type InThen<X: steps::Ops>: steps::Ops;
}

/// The operations of expressions, as types: see [`Steps`].
#[doc(hidden)]
pub mod steps {
  use core::marker::PhantomData;

  use super::{Pun, Rules, Steps, sealed::Sealed};
  use crate::layout::Op;

  /// Nothing (what the parts of an operation are).
  pub const NONE: u8 = 0;
  /// One operation, which is not a walk.
  pub const ONE: u8 = 1;
  /// Two parts, one after the other.
  pub const TWO: u8 = 2;
  /// A walk, and its expressions.
  pub const WALK: u8 = 3;

  /// The operation `I`.
  pub struct One<const I: usize>;

  /// `A`, then `B`.
  pub struct Two<A, B>(PhantomData<(A, B)>);

  /// The walk `I`, whose condition is `C` and what follows a step `N`.
  pub struct Walk<const I: usize, C, N>(PhantomData<(C, N)>);

  impl Sealed for () {}
  impl<const I: usize> Sealed for One<I> {}
  impl<A: Steps, B: Steps> Sealed for Two<A, B> {}
  impl<const I: usize, C: Steps, N: Steps> Sealed for Walk<I, C, N> {}
  impl<const R: usize, E: Steps, N: Rules> Sealed for super::RuleAt<R, E, N> {}

  impl Steps for () {
    const KIND: u8 = NONE;
    const AT: usize = 0;
    const FIRST: usize = 0;
    const END: usize = 0;
    type A = ();
    type B = ();
    type Cond = ();
    type Then = ();
    type InCond<X: Ops> = X;
    type InThen<X: Ops> = X;
  }

  impl<const I: usize> Steps for One<I> {
    const KIND: u8 = ONE;
    const AT: usize = I;
    const FIRST: usize = I;
    const END: usize = I + 1;
    type A = ();
    type B = ();
    type Cond = ();
    type Then = ();
    type InCond<X: Ops> = X;
    type InThen<X: Ops> = X;
  }

  impl<A: Steps, B: Steps> Steps for Two<A, B> {
    const KIND: u8 = TWO;
    const AT: usize = 0;
    // Contiguous, or no range at all (checked by `validate::covers`).
    const FIRST: usize =
      if A::END == B::FIRST && A::KIND != NONE && B::KIND != NONE { A::FIRST } else { usize::MAX };
    const END: usize = B::END;
    type A = A;
    type B = B;
    type Cond = ();
    type Then = ();
    type InCond<X: Ops> = X;
    type InThen<X: Ops> = X;
  }

  impl<const I: usize, C: Steps, N: Steps> Steps for Walk<I, C, N> {
    const KIND: u8 = WALK;
    const AT: usize = I;
    const FIRST: usize = I;
    const END: usize = I + 1;
    type A = ();
    type B = ();
    type Cond = C;
    type Then = N;
    type InCond<X: Ops> = InWalk<X, Self, false>;
    type InThen<X: Ops> = InWalk<X, Self, true>;
  }

  /// An expression of the layout of a struct, as a type: the one of a rule, or of a walk of one.
  pub trait Ops: Sealed {
    const OPS: &'static [Op];
    /// What its variables may be where it starts: the evaluator computes what fits 64 or 32 bits
    /// in that width (`narrow`).
    const ENTRY: Entry;
  }

  /// What the variables of an expression may be where it starts, and the number of walks that it
  /// is in (see `narrow`).
  #[derive(Copy, Clone)]
  pub struct Entry {
    pub(crate) vars: crate::narrow::Vars,
    pub(crate) walks: usize,
  }

  impl Entry {
    /// The expression of a rule: its variables are not defined yet.
    pub(crate) const RULE: Entry =
      Entry { vars: [crate::narrow::Range::ANY; crate::layout::VARIABLES], walks: 0 };
  }

  /// The expression of the rule `R::RULE` of `T`.
  pub struct InRule<T: ?Sized, R>(PhantomData<R>, PhantomData<T>);

  /// The condition (or with `NEXT`, what follows a step) of the walk `W` of the expression `X`.
  pub struct InWalk<X, W, const NEXT: bool>(PhantomData<(X, W)>);

  impl<T: ?Sized + Pun, R: Rules> Sealed for InRule<T, R> {}
  impl<X: Ops, W: Steps, const NEXT: bool> Sealed for InWalk<X, W, NEXT> {}

  impl<T: ?Sized + Pun, R: Rules> Ops for InRule<T, R> {
    const OPS: &'static [Op] = T::LAYOUT.rule_ops(R::RULE);
    const ENTRY: Entry = Entry::RULE;
  }

  impl<X: Ops, W: Steps, const NEXT: bool> Ops for InWalk<X, W, NEXT> {
    const OPS: &'static [Op] = match crate::layout::op_at(X::OPS, W::AT) {
      Some(Op::Walk { cond, next, .. }) => {
        if NEXT {
          next
        } else {
          cond
        }
      }
      _ => &[],
    };
    // Where the walk is in `X`, with its state.
    const ENTRY: Entry = crate::narrow::walk_entry(X::OPS, &X::ENTRY, W::AT);
  }
}

/// Implemented by this crate only: what implements [`Path`], [`Rules`] and [`Steps`] is trusted.
mod sealed {
  pub trait Sealed {}
}

impl sealed::Sealed for Root {}
impl<P: Path, F: ?Sized + Fields> sealed::Sealed for At<P, F> {}

/// Where something is in a target, for validation.
///
/// Sealed: the code that validates a value trusts it.
#[doc(hidden)]
pub trait Path: sealed::Sealed {
  /// Index of its first leaf.
  const LEAF: usize;
}

#[doc(hidden)]
pub struct Root;

#[doc(hidden)]
pub struct At<P, F: ?Sized>(PhantomData<P>, PhantomData<F>);

impl Path for Root {
  const LEAF: usize = 0;
}

impl<P: Path, F: ?Sized + Fields> Path for At<P, F> {
  const LEAF: usize = P::LEAF
    + match F::FIELD {
      Some(field) => field.leaf(),
      None => 0,
    };
}

// SAFETY: the end of every list.
unsafe impl Fields for () {
  type Type = ();
  type Next = ();
  type Last = ();
  type Path<P: Path> = P;
  const FIELD: Option<Field> = None;
}

// SAFETY: `SPEC` describes the type.
unsafe impl<T: Scalar> Pun for T {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Rules = ();
  type Pointee = ();
  const PROBE: *const Self = core::ptr::null();
  const LAYOUT: Layout = {
    let layout = Layout::scalar::<T>(T::SPEC).identified(T::ID);
    if T::SPECIFIED { layout } else { layout.unspecified() }
  };
  const NULL_OPTIMIZED: bool = T::NULL_OPTIMIZED;
}

// SAFETY: an array is its elements.
unsafe impl<T: Pun, const N: usize> Pun for [T; N] {
  type Elem = T;
  type Fields = ();
  type Variants = ();
  type Rules = ();
  type Pointee = ();
  const PROBE: *const Self = core::ptr::null();
  const LAYOUT: Layout =
    Layout::array(&T::LAYOUT, N).identified(layout::Id::sized("[T; N]", &[T::LAYOUT.id], N));
}

// SAFETY: a slice is its elements.
unsafe impl<T: Pun> Pun for [T] {
  type Elem = T;
  type Fields = ();
  type Variants = ();
  type Rules = ();
  type Pointee = ();
  const PROBE: *const Self = core::ptr::slice_from_raw_parts(core::ptr::null(), ptr::PROBE_LEN);
  // SAFETY: the probe of a slice.
  const LAYOUT: Layout = unsafe { Layout::slice::<T>(Self::PROBE, &T::LAYOUT) }
    .identified(layout::Id::new("[T]", &[T::LAYOUT.id]));
}

// SAFETY: a `str` is bytes that are UTF-8.
unsafe impl Pun for str {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Rules = ();
  type Pointee = ();
  const PROBE: *const Self = <[u8] as Pun>::PROBE as *const str;
  // SAFETY: the probe of a `str`, which is the one of a slice.
  const LAYOUT: Layout = unsafe { Layout::pattern::<str>(Self::PROBE, Dfa::UTF8) }
    .identified(layout::Id::new("str", &[]));
}

// SAFETY: `ManuallyDrop<T>` is a `T`.
unsafe impl<T: ?Sized + Pun> Pun for ManuallyDrop<T> {
  type Elem = T::Elem;
  type Fields = T::Fields;
  type Variants = T::Variants;
  type Rules = T::Rules;
  type Pointee = T::Pointee;
  const PROBE: *const Self = T::PROBE as *const Self;
  const LAYOUT: Layout = T::LAYOUT;
  // `ManuallyDrop<T>` "is subject to the same layout optimizations as `T`".
  const NULL_OPTIMIZED: bool = T::NULL_OPTIMIZED;
}

// SAFETY: a `MaybeUninit<T>` holds any bytes, uninitialized ones included, and nothing is known
// of them: every byte is padding, which a target accepts whatever it is, and which a source does
// not prove anything of. Its size and alignment are those of `T`.
//
// What may be written through a `&mut` to one is anything, uninitialized bytes included: a
// `&mut MaybeUninit<T>` is only made of a source of which every byte may be uninitialized (the
// reverse relation of `pun_mut`), and a `&mut T` of a `MaybeUninit<T>` never (a byte that `T`
// needs initialized would be read from padding).
//
// Its size and alignment are the ones of `T`, which are only known to be the same from one version
// of the compiler to the next if the layout of `T` is: without the `unstable` feature, `T` is a
// `Pun` type, whose layout is evaluated (and refused if it is not specified).
#[cfg(not(feature = "unstable"))]
unsafe impl<T: Pun> Pun for core::mem::MaybeUninit<T> {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Rules = ();
  type Pointee = ();
  const PROBE: *const Self = core::ptr::null();
  const LAYOUT: Layout = Layout::padding::<Self>()
    .sized_like(&T::LAYOUT)
    .identified(layout::Id::new("MaybeUninit", &[T::LAYOUT.id]));
}

// SAFETY: as above, for any `T`.
#[cfg(feature = "unstable")]
unsafe impl<T> Pun for core::mem::MaybeUninit<T> {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Rules = ();
  type Pointee = ();
  const PROBE: *const Self = core::ptr::null();
  const LAYOUT: Layout = Layout::padding::<Self>();
}

// SAFETY: no value: the layout of a type without values.
unsafe impl Pun for core::convert::Infallible {
  type Elem = ();
  type Fields = ();
  type Variants = ();
  type Rules = ();
  type Pointee = ();
  const PROBE: *const Self = core::ptr::null();
  const LAYOUT: Layout = Layout::uninhabited::<Self>().identified(layout::Id::new("!", &[]));
}

/// The `I`th field of the tuple `T`.
#[doc(hidden)]
pub struct TupleField<T, const I: usize>(PhantomData<T>);

macro_rules! tuple_field {
  ([$($all:ident)+] $index:tt $ty:ident $next:ty) => {
    // SAFETY: the field of the layout of the tuple.
    unsafe impl<$($all: Pun),+> Fields for TupleField<($($all,)+), $index> {
      type Type = $ty;
      type Next = $next;
      type Last = ();
      type Path<P: Path> = At<P, Self>;
      const FIELD: Option<Field> = Some(*<($($all,)+) as Pun>::LAYOUT.field($index));
    }
  };
}

macro_rules! tuple {
  ([$($all:ident)+] $($index:tt)+) => {
    // SAFETY: the offsets come from the compiler.
    unsafe impl<$($all: Pun),+> Pun for ($($all,)+) {
      type Elem = ();
      type Fields = TupleField<Self, 0>;
      type Variants = ();
      type Rules = ();
      type Pointee = ();
      const PROBE: *const Self = core::ptr::null();
      // SAFETY: a tuple of `Sized` things is `Sized`.
      const LAYOUT: Layout = unsafe {
        Layout::structure::<Self>(
          Self::PROBE,
          const {
            &Layout::sort_fields([
              $(Field::new(core::mem::offset_of!(Self, $index), &$all::LAYOUT)),+
            ])
          },
          &[],
        )
        // The `Rust` representation: what every order of its fields has.
        .rust(usize::MAX)
        .identified(layout::Id::new("(..)", &[$($all::LAYOUT.id),+]))
      };
    }
  };
}

tuple!([A] 0);
tuple_field!([A] 0 A ());
tuple!([A B] 0 1);
tuple_field!([A B] 0 A TupleField<(A, B), 1>);
tuple_field!([A B] 1 B ());
tuple!([A B C] 0 1 2);
tuple_field!([A B C] 0 A TupleField<(A, B, C), 1>);
tuple_field!([A B C] 1 B TupleField<(A, B, C), 2>);
tuple_field!([A B C] 2 C ());
tuple!([A B C D] 0 1 2 3);
tuple_field!([A B C D] 0 A TupleField<(A, B, C, D), 1>);
tuple_field!([A B C D] 1 B TupleField<(A, B, C, D), 2>);
tuple_field!([A B C D] 2 C TupleField<(A, B, C, D), 3>);
tuple_field!([A B C D] 3 D ());

macro_rules! impl_any {
  ($($ty:ty)+) => {$(
    // SAFETY: every bit pattern is valid.
    unsafe impl Scalar for $ty {
      const SPEC: Spec = Spec::any::<$ty>();
      const ID: layout::Id = layout::Id::new(stringify!($ty), &[]);
    }
  )+};
}

impl_any!(() u8 u16 u32 u64 u128 usize);

macro_rules! impl_bits {
  ($($ty:ty)+) => {$(
    // SAFETY: every bit pattern is valid.
    unsafe impl Scalar for $ty {
      const SPEC: Spec = Spec::any::<$ty>().not_a_number();
      const ID: layout::Id = layout::Id::new(stringify!($ty), &[]);
    }
  )+};
}

impl_bits!(i8 i16 i32 i64 i128 isize f32 f64);

// SAFETY: no byte.
unsafe impl<T: ?Sized> Scalar for PhantomData<T> {
  const SPEC: Spec = Spec::any::<Self>();
}

// SAFETY: 0 or 1.
unsafe impl Scalar for bool {
  const SPEC: Spec = Spec::ranges::<bool>(Endian::NATIVE, &[(0, 1)]).not_a_number();
  const ID: layout::Id = layout::Id::new("bool", &[]);
}

// SAFETY: a Unicode scalar value.
unsafe impl Scalar for char {
  const SPEC: Spec =
    Spec::ranges::<char>(Endian::NATIVE, &[(0, 0xD7FF), (0xE000, 0x10_FFFF)]).not_a_number();
  const ID: layout::Id = layout::Id::new("char", &[]);
}

macro_rules! impl_non_zero {
  ($($ty:ident)+) => {$(
    // SAFETY: anything but 0.
    unsafe impl Scalar for core::num::NonZero<$ty> {
      const SPEC: Spec = Spec::ranges::<$ty>(Endian::NATIVE, &[(1, $ty::MAX as u128)]);
      const NULL_OPTIMIZED: bool = true;
      const ID: layout::Id = layout::Id::new(concat!("NonZero<", stringify!($ty), ">"), &[]);
    }
  )+};
}

impl_non_zero!(u8 u16 u32 u64 u128 usize);

macro_rules! impl_signed_non_zero {
  ($($ty:ident)+) => {$(
    // SAFETY: any bits but all zeros, the only bit pattern of 0 in two's complement. Its bits
    // are not its value as an unsigned integer: rules do not read it, as for `iN`.
    unsafe impl Scalar for core::num::NonZero<$ty> {
      const SPEC: Spec = Spec::ranges::<$ty>(
        Endian::NATIVE,
        &[(1, u128::MAX >> (128 - 8 * size_of::<$ty>()))],
      )
      .not_a_number();
      const NULL_OPTIMIZED: bool = true;
      const ID: layout::Id = layout::Id::new(concat!("NonZero<", stringify!($ty), ">"), &[]);
    }
  )+};
}

impl_signed_non_zero!(i8 i16 i32 i64 i128 isize);

// -------------------------------------------------------------------------------------------------
// `Error`

/// Why a value cannot be seen as another type.
///
/// ```
/// use puns::Error;
///
/// let words = [0u32; 2];
/// // At an address aligned for a `u32`: one byte further is not.
/// let bytes: &[u8] = puns::as_bytes(&words);
///
/// assert_eq!(puns::from_bytes::<u32>(&bytes[..4]), Ok(&0));
/// assert_eq!(puns::from_bytes::<u32>(&bytes[..3]), Err(Error::Size));
/// assert_eq!(puns::from_bytes::<u32>(&bytes[1..5]), Err(Error::Alignment));
/// assert_eq!(puns::from_bytes::<bool>(&[2]), Err(Error::Validity));
/// assert_eq!(Error::Validity.to_string(), "value is not valid for the target type");
/// ```
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Error {
  /// The address of the value is not a multiple of the alignment of the target.
  Alignment,
  /// The size of the value is not a size that the target can have.
  Size,
  /// The value is not a valid target: a scalar is out of its valid values, a string is not
  /// UTF-8, a rule does not hold...
  Validity,
}

impl core::fmt::Display for Error {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    f.write_str(match self {
      Error::Alignment => "value is not aligned for the target type",
      Error::Size => "value has not the size of the target type",
      Error::Validity => "value is not valid for the target type",
    })
  }
}

impl core::error::Error for Error {}

// -------------------------------------------------------------------------------------------------
// `Plan`

/// What is known at compile time of a pun from `S` to `D`.
struct Plan<S: ?Sized, D: ?Sized>(PhantomData<S>, PhantomData<D>);

/// What a function requires of a pun.
#[derive(Copy, Clone)]
#[expect(clippy::struct_excessive_bools, reason = "each is a question of its own, asked alone")]
struct Need {
  /// The target may be a part of the source: its first bytes, or with `Plan::TAIL` its last.
  part: bool,
  /// What is not proven is checked at run time.
  fallible: bool,
  /// The target is behind a reference.
  reference: bool,
  /// The target may hold self-relative pointers: the function proves what they aim at
  /// ([`view_ref`]). No other function makes a value that holds one.
  pointers: bool,
}

impl Need {
  const VALUE: Need = Need { part: false, fallible: false, reference: false, pointers: false };
  const REF: Need = Need { reference: true, ..Need::VALUE };
  const PART: Need = Need { part: true, ..Need::VALUE };
  const PART_REF: Need = Need { part: true, ..Need::REF };
  const TRY: Need = Need { fallible: true, ..Need::VALUE };
  const TRY_REF: Need = Need { fallible: true, ..Need::REF };
  const VIEW: Need = Need { pointers: true, ..Need::REF };
  const TRY_VIEW: Need = Need { pointers: true, ..Need::TRY_REF };
}

/// A value that holds a self-relative pointer is only made by what proves, or checks, what the
/// pointer aims at.
const POINTER_TARGET: &str = "the target holds a self-relative pointer, and what a pointer aims \
                              at is not a property of its bytes.\nUse `view_ref` to read a value \
                              that holds pointers as another type (it proves what they aim at)";

/// A pun that depends on what Rust does not keep from one compilation to the next.
const UNSPECIFIED: &str = "the pun depends on a part of the source or of the target whose layout \
  Rust does not keep from one compilation to the next (a struct or a tuple of the `Rust` \
  representation, an enum without `repr`, `Option` or `Result` of most types): another compiler, \
  or `-Zrandomize-layout`, could arrange it otherwise. Only the same type at the same place \
  proves it.\nAdd `#[repr(C)]` (or `#[repr(u8)]` to an enum), or enable the `unstable` feature \
  of `puns`";

/// A `&mut` whose writes are not known to leave a valid source in every layout that Rust allows.
const UNSPECIFIED_MUT: &str = "what may be written through the `&mut` is a valid source in the \
  layout that `rustc` chose, not in every layout that Rust allows: the source has a part whose \
  layout Rust does not keep from one compilation to the next (a struct or a tuple of the `Rust` \
  representation, an enum without `repr`).\nAdd `#[repr(C)]` (or `#[repr(u8)]` to an enum), or \
  enable the `unstable` feature of `puns`";

/// Why a pun of which `analysis` is the analysis does not meet the `need`, if it does not.
const fn check(analysis: &Analysis, need: Need) -> Option<Message> {
  let Analysis { fit, relation, aligned, shift } = *analysis;
  match fit.exact {
    _ if need.part && fit.shrink => {}
    _ if need.part => {
      return Some(Message::new(
        "the source can be smaller than the target (if it has a slice, it may be empty)",
      ));
    }
    Exact::Always => {}
    Exact::Sometimes if need.fallible => {}
    Exact::Sometimes => {
      return Some(Message::new(
        "the size of the source may not be a size of the target, depending on the length of \
         its slice.\nUse `try_pun_ref` or `try_pun_mut` to check it at run time",
      ));
    }
    Exact::Never(why) if fit.shrink => {
      return Some(Message::new(why).str(
        ".\nUse `head` or `tail` (or their `_ref` and `_mut` forms) to take a part of the source",
      ));
    }
    Exact::Never(why) => return Some(Message::new(why)),
  }
  // Offsets of the target.
  if let Some(offset) = relation.unspecified {
    return Some(Message::new(UNSPECIFIED).at(offset.saturating_sub(shift)));
  }
  if let Some(offset) = relation.pointer {
    return Some(
      Message::new(
        "a pointer of the target is not, in the source, a pointer whose target it can read (the \
         same scheme and offset, the same kind of target, not larger nor more aligned): only a \
         pointer proves a pointer, and no check makes up for it",
      )
      .at(offset.saturating_sub(shift)),
    );
  }
  if let Some(offset) = relation.uninit {
    return Some(
      Message::new("a byte that the target requires to be initialized is padding in the source")
        .at(offset.saturating_sub(shift)),
    );
  }
  if let Some(offset) = relation.disjoint {
    return Some(
      Message::new("no value of the source is valid for the target")
        .at(offset.saturating_sub(shift)),
    );
  }
  if let (Some(offset), false) = (relation.partial, need.fallible) {
    return Some(
      Message::new(if need.pointers {
        "some values of the source are not valid for the target, or do not prove one of its \
         rules.\nUse `try_view_ref` to check them at run time"
      } else {
        "some values of the source are not valid for the target, or do not prove one of its \
         rules.\nUse a `try_*` function to check them at run time"
      })
      .at(offset.saturating_sub(shift)),
    );
  }
  if need.reference && !aligned && !need.fallible {
    return Some(Message::new(
      "the target has a greater alignment than the source.\nUse `try_pun_ref` or \
       `try_pun_mut` to check the address at run time",
    ));
  }
  None
}

impl<S: ?Sized + Pun, D: ?Sized + Pun> Plan<S, D> {
  const ANALYSIS: Analysis = analyze(&S::LAYOUT, &D::LAYOUT);

  /// The analysis of the last bytes of the source as the target.
  const TAIL_ANALYSIS: Analysis = analyze_tail(&S::LAYOUT, &D::LAYOUT);

  const MUTABLE: bool = mutable(&S::LAYOUT, &D::LAYOUT, &Self::ANALYSIS.fit);

  /// Fail to compile if `message` says why.
  const fn require(message: Option<Message>) {
    if let Some(message) = message {
      message.panic();
    }
  }

  /// Why the pun does not meet the `need`, if it does not.
  const fn check(need: Need) -> Option<Message> {
    if let Some(message) = Self::gate(need) {
      return Some(message);
    }
    check(&Self::ANALYSIS, need)
  }

  /// Why the function is not one that may make a `D`, if it is not: a `D` that holds a
  /// self-relative pointer is only made by what proves what the pointer aims at.
  const fn gate(need: Need) -> Option<Message> {
    if !need.pointers && D::LAYOUT.has_pointers() {
      return Some(Message::new(POINTER_TARGET));
    }
    None
  }

  /// Why a `&mut` to a `D` cannot be made from a `&mut` to an `S`, when [`Plan::MUTABLE`] says
  /// so.
  const NOT_MUTABLE: &'static str = if S::LAYOUT.has_pointers() || D::LAYOUT.has_pointers() {
    POINTER_MUT
  } else if mutable_unspecified(&S::LAYOUT, &D::LAYOUT, &Self::ANALYSIS.fit) {
    UNSPECIFIED_MUT
  } else {
    MUTABLE
  };

  /// Why the pun of the last bytes of the source does not meet the `need` (a part), if it does
  /// not.
  const fn check_tail(need: Need) -> Option<Message> {
    if let Some(message) = Self::gate(need) {
      return Some(message);
    }
    if S::LAYOUT.is_unsized() {
      return Some(Message::new(TAIL_UNSIZED_SOURCE));
    }
    if D::LAYOUT.is_unsized() {
      return Some(Message::new(TAIL_UNSIZED_TARGET));
    }
    match check(&Self::TAIL_ANALYSIS, Need { reference: false, ..need }) {
      None if need.reference && !Self::TAIL_ANALYSIS.aligned => Some(Message::new(
        "the target has a greater alignment than the source.\nUse `tail` to copy it, or \
         `from_tail` to take it from bytes, whose address is checked at run time",
      )),
      verdict => verdict,
    }
  }

  const BY_VALUE: () = Self::require(Self::check(Need::VALUE));
  const BY_REF: () = Self::require(Self::check(Need::REF));
  const BY_MUT: () = {
    let () = Self::BY_REF;
    if !Self::MUTABLE {
      Message::new(Self::NOT_MUTABLE).panic();
    }
  };
  const HEAD: () = Self::require(Self::check(Need::PART));
  const HEAD_REF: () = Self::require(Self::check(Need::PART_REF));
  const TAIL: () = Self::require(Self::check_tail(Need::PART));
  const TAIL_REF: () = Self::require(Self::check_tail(Need::PART_REF));
  const HEAD_MUT: () = Self::require(Self::check_head_mut());
  const TAIL_MUT: () = Self::require(Self::check_tail_mut());
  const TRY: () = Self::require(Self::check(Need::TRY));
  const TRY_REF: () = Self::require(Self::check(Need::TRY_REF));
  const FROM_TAIL: () = Self::require(Self::check_from_tail());
  const FROM_TAIL_MUT: () = {
    let () = Self::FROM_TAIL;
    if !Self::MUTABLE {
      Message::new(Self::NOT_MUTABLE).panic();
    }
  };

  /// Why the target cannot be taken from the end of the source (bytes) at run time, if it
  /// cannot.
  const fn check_from_tail() -> Option<Message> {
    if D::LAYOUT.is_unsized() {
      return Some(Message::new(
        "the target is unsized: where it starts cannot be found from the end of the bytes.\nUse \
         `from_head` or `from_bytes`",
      ));
    }
    Self::check(Need::TRY_REF)
  }

  /// Why a `&mut` to the first bytes of the source cannot be handed out, if it cannot: what a
  /// `&` cannot, and what [`mutable_part`] refuses.
  const fn check_head_mut() -> Option<Message> {
    if let Some(message) = Self::check(Need::PART_REF) {
      return Some(message);
    }
    match mutable_part(&S::LAYOUT, &D::LAYOUT, &Self::ANALYSIS.fit, 0) {
      Some(why) => Some(Message::new(why)),
      None => None,
    }
  }

  /// Why a `&mut` to the last bytes of the source cannot be handed out, if it cannot.
  const fn check_tail_mut() -> Option<Message> {
    if let Some(message) = Self::check_tail(Need::PART_REF) {
      return Some(message);
    }
    let Analysis { fit, shift, .. } = Self::TAIL_ANALYSIS;
    match mutable_part(&S::LAYOUT, &D::LAYOUT, &fit, shift) {
      Some(why) => Some(Message::new(why)),
      None => None,
    }
  }

  /// [`view_ref`]: by reference, with pointers, whose targets [`deep`] proves.
  const VIEW: () = Self::require(Self::check(Need::VIEW));
  /// [`try_view_ref`]: the same, with what is not proven of the value itself checked at run time.
  const TRY_VIEW: () = Self::require(Self::check(Need::TRY_VIEW));

  const TRY_MUT: () = {
    let () = Self::TRY_REF;
    if !Self::MUTABLE {
      Message::new(Self::NOT_MUTABLE).panic();
    }
  };

  /// Pointer to the target at `source`, when there is nothing to check (`BY_REF`,
  /// `HEAD_REF`).
  #[inline(always)]
  const fn cast(source: *const S) -> *const D {
    let (head, stride, exact) = const {
      // The slice length of the target is `head + len * stride`, exactly or rounded down.
      let (source, target) = (S::LAYOUT, D::LAYOUT);
      match (target.stride(), Self::ANALYSIS.fit.exact) {
        (0, _) => (0, 0, true),
        (by, Exact::Always) => ((source.head() - target.head()) / by, source.stride() / by, true),
        _ => (0, 0, false),
      }
    };
    let len = if exact {
      head + ptr::metadata(source) * stride
    } else {
      let (layout, target) = const { (S::LAYOUT, D::LAYOUT) };
      let extent = layout.head() + ptr::metadata(source) * layout.stride();
      (extent - target.head()) / target.stride()
    };
    ptr::from_raw_parts::<D>(source.cast(), len)
  }

  /// Pointer to the target at the end of `source`, when there is nothing to check (`TAIL_REF`:
  /// both are sized).
  #[inline(always)]
  const fn tail_cast(source: *const S) -> *const D {
    let shift = const { Self::TAIL_ANALYSIS.shift };
    // SAFETY: `shift` is at most the size of the source, at the end of which the target ends.
    ptr::from_raw_parts::<D>(unsafe { source.cast::<u8>().add(shift) }, 0)
  }

  /// Pointer to the target at `source`, with every check that could not be made at compile
  /// time (`TRY`, `TRY_REF`).
  #[inline(always)]
  const fn try_cast(source: *const S) -> Result<*const D, Error> {
    // SAFETY: the marks of this pun, of a source that is a valid `S`.
    unsafe { Self::try_cast_by::<Self>(source) }
  }

  /// [`Plan::try_cast`], checking what `C` marks rather than what the analysis of this pun does.
  ///
  /// # Safety
  /// `source` must be a valid `S`, or `C` must mark of the target what the bytes of `source`
  /// do not prove: [`Plan`] itself, or the marks of a source that the bytes are a valid value of,
  /// of the extent of `source`.
  #[inline(always)]
  const unsafe fn try_cast_by<C: Checks>(source: *const S) -> Result<*const D, Error> {
    let (layout, target) = const { (S::LAYOUT, D::LAYOUT) };
    let addr = source.cast::<u8>();
    if const { !Self::ANALYSIS.aligned } && !ptr::is_aligned(addr, target.align()) {
      return Err(Error::Alignment);
    }
    // The size of a value that exists: no overflow.
    let extent = layout.head() + ptr::metadata(source) * layout.stride();
    if extent < target.head() {
      return Err(Error::Size);
    }
    let rest = extent - target.head();
    let len = match target.stride() {
      0 if rest == 0 => 0,
      stride if stride != 0 && rest.is_multiple_of(stride) => rest / stride,
      _ => return Err(Error::Size),
    };
    // An unsized target of greater alignment may end with more padding than the source has room
    // for, even if its slice has no size (`[()]`).
    if const { D::LAYOUT.is_unsized() && D::LAYOUT.align() > S::LAYOUT.align() } {
      match (target.size_of_val(len), layout.size_of_val(ptr::metadata(source))) {
        (Some(needed), Some(available)) if needed <= available => {}
        _ => return Err(Error::Size),
      }
    }
    let target = ptr::from_raw_parts::<D>(addr, len);
    // SAFETY: the target is in the bounds of the source, which is a valid `S`: what the target
    // reads of it is initialized, since the analysis found no padding under it. What `C` does not
    // mark, the source proves (guaranteed by the caller).
    if !unsafe { validate::<C, Root, D>(target) } {
      return Err(Error::Validity);
    }
    Ok(target)
  }
}

impl<S: ?Sized + Pun, D: ?Sized + Pun> Checks for Plan<S, D> {
  const MARKS: Marks = Self::ANALYSIS.relation.marks;
}

/// A compile-time error message, without `format!`.
struct Message {
  bytes: [u8; 512],
  len: usize,
}

impl Message {
  const fn new(what: &str) -> Self {
    Self { bytes: [0; 512], len: 0 }.str("\ncannot pun: ").str(what)
  }

  const fn str(mut self, str: &str) -> Self {
    let (bytes, mut i) = (str.as_bytes(), 0);
    while i < bytes.len() && self.len < self.bytes.len() {
      self.bytes[self.len] = bytes[i];
      (self.len, i) = (self.len + 1, i + 1);
    }
    self
  }

  /// Add the offset of what is wrong.
  const fn at(self, offset: usize) -> Self {
    let (mut digits, mut len, mut value) = ([0; 20], 0, offset);
    loop {
      len += 1;
      digits[20 - len] = b'0' + (value % 10) as u8;
      value /= 10;
      if value == 0 {
        break;
      }
    }
    // SAFETY: ASCII digits.
    let digits = unsafe { core::str::from_utf8_unchecked(digits.split_at(20 - len).1) };
    self.str(".\nThis is at byte ").str(digits).str(" of the target")
  }

  const fn panic(self) -> ! {
    let this = self.str(".\n");
    // SAFETY: a concatenation of `str`s (only cut if too long, and messages are ASCII).
    panic!("{}", unsafe { core::str::from_utf8_unchecked(this.bytes.split_at(this.len).0) })
  }
}

// -------------------------------------------------------------------------------------------------
// By value

/// Reinterpret a value as a value of another type of the same size.
///
/// This compiles if every value of `S` is a valid `D`, and then costs nothing.
///
/// ```
/// assert_eq!(puns::pun::<[bool; 2], u16>([true, true]), 0x0101);
/// ```
///
/// ```compile_fail
/// // Not every `u8` is a `bool`.
/// let _: bool = puns::pun(2u8);
/// ```
#[inline(always)]
pub const fn pun<S: Pun + Copy, D: Pun + Copy>(value: S) -> D {
  const { Plan::<S, D>::BY_VALUE };
  // SAFETY: checked at compile time.
  unsafe { pun_unchecked(value) }
}

/// Reinterpret the first bytes of a value as a value of a type that is not larger.
///
/// This compiles if the first `size_of::<D>()` bytes of every `S` are a valid `D`, and then costs
/// nothing.
///
/// ```
/// assert_eq!(puns::head::<[u8; 4], [u8; 2]>([1, 2, 3, 4]), [1, 2]);
/// ```
///
/// ```compile_fail
/// // The second byte is padding.
/// let _: [u8; 2] = puns::head((0u8, 0u16));
/// ```
#[inline(always)]
pub const fn head<S: Pun + Copy, D: Pun + Copy>(value: S) -> D {
  const { Plan::<S, D>::HEAD };
  // SAFETY: checked at compile time.
  unsafe { pun_unchecked(value) }
}

/// Reinterpret the last bytes of a value as a value of a type that is not larger.
///
/// This compiles if the last `size_of::<D>()` bytes of every `S` are a valid `D`, and then costs
/// nothing.
///
/// ```
/// assert_eq!(puns::tail::<[u8; 4], [u8; 2]>([1, 2, 3, 4]), [3, 4]);
/// // Two `bool`s are two initialized bytes.
/// assert_eq!(puns::tail::<[bool; 3], u16>([false, true, true]), 0x0101);
/// ```
///
/// ```compile_fail
/// // Not every `u8` is a `bool`.
/// let _: bool = puns::tail::<[u8; 2], bool>([1, 7]);
/// ```
///
/// ```compile_fail
/// #[derive(puns::Pun, Clone, Copy)]
/// #[repr(C)]
/// struct Padded {
///   a: u16,
///   b: u8,
/// }
///
/// // The last byte is padding.
/// let _: u8 = puns::tail(Padded { a: 0, b: 0 });
/// ```
#[inline(always)]
pub const fn tail<S: Pun + Copy, D: Pun + Copy>(value: S) -> D {
  const { Plan::<S, D>::TAIL };
  let shift = const { Plan::<S, D>::TAIL_ANALYSIS.shift };
  // SAFETY: checked at compile time: `shift + size_of::<D>() == size_of::<S>()`, and the bytes of
  // `value` from `shift` are a valid `D` (which may not be aligned there).
  unsafe { (&raw const value).cast::<u8>().add(shift).cast::<D>().read_unaligned() }
}

/// Reinterpret a value as a value of another type of the same size, if it is a valid one.
///
/// Only what could not be proven at compile time is checked.
///
/// ```
/// assert_eq!(puns::try_pun::<u8, bool>(1), Ok(true));
/// assert_eq!(puns::try_pun::<u8, bool>(2), Err(puns::Error::Validity));
/// ```
///
/// # Errors
/// [`Error::Validity`] if `value` is not a valid `D`.
#[inline]
pub const fn try_pun<S: Pun + Copy, D: Pun + Copy>(value: S) -> Result<D, Error> {
  const { Plan::<S, D>::TRY };
  // SAFETY: `value` is a valid `S` of the size of `D`; nothing is assumed of its alignment.
  if !unsafe { validate::<Plan<S, D>, Root, D>((&raw const value).cast()) } {
    return Err(Error::Validity);
  }
  // SAFETY: just validated.
  Ok(unsafe { pun_unchecked(value) })
}

/// # Safety
/// `D` must not be larger than `S`, and the first bytes of `value` must be a valid `D`.
#[inline(always)]
const unsafe fn pun_unchecked<S: Copy, D: Copy>(value: S) -> D {
  union Overlay<S: Copy, D: Copy> {
    source: S,
    target: D,
  }
  // SAFETY: guaranteed by the caller.
  unsafe { Overlay { source: value }.target }
}

// -------------------------------------------------------------------------------------------------
// By reference

/// Reinterpret a reference as a reference to another type.
///
/// This compiles if every value of `S` is a valid `D` of the same size, whatever the length of
/// its slice if it has one, and if `D` has no greater alignment. It then costs nothing.
///
/// ```
/// let bytes: &[u8] = puns::pun_ref(&[0x0102u16, 0x0304][..]);
/// assert_eq!(bytes.len(), 4);
/// let text: &[u8] = puns::pun_ref("héllo");
/// assert_eq!(text.len(), 6);
/// ```
#[inline(always)]
pub const fn pun_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> &D {
  const { Plan::<S, D>::BY_REF };
  // SAFETY: checked at compile time.
  unsafe { &*Plan::<S, D>::cast(value) }
}

/// Reinterpret a mutable reference as a mutable reference to another type.
///
/// On top of what [`pun_ref`] requires, every value of `D` must be a valid `S`: anything
/// can be written through the result.
///
/// ```
/// let mut value = 0x0102u16;
/// puns::pun_mut::<_, [u8; 2]>(&mut value)[0] = 0x02;
/// assert!(value == 0x0202 || value == 0x0102);
/// ```
#[inline(always)]
pub const fn pun_mut<S: ?Sized + Pun, D: ?Sized + Pun>(value: &mut S) -> &mut D {
  const { Plan::<S, D>::BY_MUT };
  let source: *mut S = value;
  // SAFETY: checked at compile time.
  unsafe { &mut *Plan::<S, D>::cast(source).cast_mut() }
}

/// Reinterpret a reference to a value that holds self-relative pointers as a reference to
/// another type, when every `S`, *and everything it points to*, is a valid `D`.
///
/// A self-relative pointer ([`SelfRelative`]) is an offset to a target in the same buffer. No
/// other function makes a value that holds one: what a pointer aims at is not a property of its
/// bytes. This one proves it, at compile time: a pointer of `D` must be, in `S`, a pointer at the
/// same place of the same scheme, whose offset is the same integer (width and byte order) and
/// whose target can be read as the target of `D`'s: a value as a prefix of it that needs no more
/// alignment, a sequence as a sequence of elements of the same size that needs no more alignment
/// than the source guarantees, text as text. Those targets are related the same way, and so on: a recursive type is proven by
/// induction over the pairs of types that are met, each once. Then nothing is checked and
/// nothing is compiled: the result is the same address.
///
/// Behind a pointer nothing can be checked, so nothing may be left to check there: every value
/// must be valid for what it is read as.
///
/// With `Rel<T>`, an `i32` from its own address to a `T`, as declared in the example of
/// [`SelfRelative`]:
///
/// ```
/// # use core::marker::PhantomData;
/// # use puns::{Pun, SelfRelative, layout::{Endian, Pointer}};
/// # /// A pointer to a `T` in the same buffer: the distance from its own address, an `i32`.
/// # #[repr(transparent)]
/// # pub struct Rel<T> { offset: i32, target: PhantomData<T> }
/// # // SAFETY: as in the example of `SelfRelative`: the only ones of this example are made where
/// # // they aim at a valid `T`, and nothing writes to the buffer while it is read.
/// # unsafe impl<T: Pun> SelfRelative for Rel<T> {
/// #   type Target = T;
/// #   const POINTER: Pointer =
/// #     Pointer::one::<T>("rel, an example", size_of::<i32>(), Endian::NATIVE);
/// # }
/// # puns::self_relative!(impl[T: Pun] Rel<T>);
/// use puns::num::u8in;
///
/// /// A list of digits: each points to the next one, the last one to itself.
/// #[derive(Pun)]
/// #[repr(C)]
/// struct Digits {
///   digit: u8in<b'0', b'9'>,
///   next: Rel<Digits>,
/// }
///
/// /// A list of bytes.
/// #[derive(Pun)]
/// #[repr(C)]
/// struct Bytes {
///   byte: u8,
///   next: Rel<Bytes>,
/// }
///
/// #[repr(C, align(4))]
/// struct Buffer([u8; 8]);
///
/// // The digit 7, whose pointer, at 4, aims at the list itself, at 0.
/// let mut bytes = [b'7', 0, 0, 0, 0, 0, 0, 0];
/// bytes[4..].copy_from_slice(&(-4i32).to_ne_bytes());
/// let buffer = Buffer(bytes);
/// // SAFETY: the bytes at 0 are `Digits`, whose pointer aims at `Digits` of the same buffer.
/// let digits: &Digits = unsafe { &*core::ptr::from_ref(&buffer).cast::<Digits>() };
///
/// // Proven at compile time, through the pointers of the recursive type.
/// let bytes: &Bytes = puns::view_ref(digits);
/// assert!(core::ptr::addr_eq(bytes, digits));
/// assert_eq!(bytes.byte, b'7');
/// ```
///
/// It is not a `const fn`: what the pointers aim at is proven by instances of functions, which
/// only a build makes (`cargo check` reports nothing of it). It never gives a `&mut`: a pointer
/// is only read.
#[inline(always)]
pub fn view_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> &D {
  const { Plan::<S, D>::VIEW };
  // Everything that the pointers of `D` aim at, proven by the instances that this makes.
  deep::proof::<S, D>();
  // SAFETY: checked at compile time: the bytes of the value by the plan, what every pointer
  // reachable from it aims at by `deep`. The value is borrowed, and nothing writes to what a
  // shared reference to a pointer leads to: all of it stays what it is while the result lives.
  unsafe { &*Plan::<S, D>::cast(value) }
}

/// [`view_ref`], when only some values of `S` are a valid `D`: what could not be proven of the
/// value itself is checked at run time, as [`try_pun_ref`] does.
///
/// What is checked is the value that `value` refers to, not what it points to: a variant that
/// `D` does not have, a scalar that `D` restricts, a pointer that may be null
/// ([`Pointer::or_null`](layout::Pointer::or_null)) read as one that is not. Behind its pointers nothing is checked, so everything
/// must be proven there, as for [`view_ref`].
///
/// With `Rel<T>` as in the example of [`view_ref`]:
///
/// ```
/// # use core::marker::PhantomData;
/// # use puns::{Pun, SelfRelative, layout::{Endian, Pointer}};
/// # /// A pointer to a `T` in the same buffer: the distance from its own address, an `i32`.
/// # #[repr(transparent)]
/// # pub struct Rel<T> { offset: i32, target: PhantomData<T> }
/// # // SAFETY: as in the example of `SelfRelative`: the only ones of this example are made where
/// # // they aim at a valid `T`, and nothing writes to the buffer while it is read.
/// # unsafe impl<T: Pun> SelfRelative for Rel<T> {
/// #   type Target = T;
/// #   const POINTER: Pointer =
/// #     Pointer::one::<T>("rel, an example", size_of::<i32>(), Endian::NATIVE);
/// # }
/// # puns::self_relative!(impl[T: Pun] Rel<T>);
/// #[derive(Pun)]
/// #[repr(C)]
/// struct Loose {
///   byte: u8,
///   next: Rel<u8>,
/// }
///
/// #[derive(Pun)]
/// #[repr(C)]
/// struct Strict {
///   flag: bool,
///   next: Rel<u8>,
/// }
///
/// #[repr(C, align(4))]
/// struct Buffer([u8; 12]);
///
/// /// A `Loose` of `byte` at 0, whose pointer, at 4, aims at a byte at 8.
/// fn buffer(byte: u8) -> Buffer {
///   let mut bytes = [byte, 0, 0, 0, 0, 0, 0, 0, 9, 0, 0, 0];
///   bytes[4..8].copy_from_slice(&4i32.to_ne_bytes());
///   Buffer(bytes)
/// }
///
/// let (a, b) = (buffer(1), buffer(2));
/// // SAFETY: the bytes at 0 of each are a `Loose`, whose pointer aims at a byte of the buffer.
/// let one: &Loose = unsafe { &*core::ptr::from_ref(&a).cast::<Loose>() };
/// let two: &Loose = unsafe { &*core::ptr::from_ref(&b).cast::<Loose>() };
///
/// // The `bool` is checked; what the pointer aims at is proven.
/// assert!(puns::try_view_ref::<Loose, Strict>(one).unwrap().flag);
/// assert_eq!(puns::try_view_ref::<Loose, Strict>(two).err(), Some(puns::Error::Validity));
/// ```
///
/// What a pointer aims at is not checked: a `u8` there is never read as a `bool`.
///
/// ```compile_fail
/// # use core::marker::PhantomData;
/// # use puns::{Pun, SelfRelative, layout::{Endian, Pointer}};
/// # /// A pointer to a `T` in the same buffer: the distance from its own address, an `i32`.
/// # #[repr(transparent)]
/// # pub struct Rel<T> { offset: i32, target: PhantomData<T> }
/// # // SAFETY: as in the example of `SelfRelative`: the only ones of this example are made where
/// # // they aim at a valid `T`, and nothing writes to the buffer while it is read.
/// # unsafe impl<T: Pun> SelfRelative for Rel<T> {
/// #   type Target = T;
/// #   const POINTER: Pointer =
/// #     Pointer::one::<T>("rel, an example", size_of::<i32>(), Endian::NATIVE);
/// # }
/// # puns::self_relative!(impl[T: Pun] Rel<T>);
/// # #[derive(Pun)] #[repr(C)] struct Loose { byte: u8, next: Rel<u8> }
/// #[derive(Pun)]
/// #[repr(C)]
/// struct Behind {
///   byte: u8,
///   next: Rel<bool>,
/// }
///
/// fn check(loose: &Loose) -> Result<&Behind, puns::Error> {
///   puns::try_view_ref(loose)
/// }
/// # let _: fn(&Loose) -> Result<&Behind, puns::Error> = check;
/// ```
///
/// # Errors
/// As [`try_pun_ref`]: [`Error::Alignment`], [`Error::Size`], [`Error::Validity`].
#[inline]
pub fn try_view_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> Result<&D, Error> {
  const { Plan::<S, D>::TRY_VIEW };
  // Everything that the pointers of `D` aim at, proven by the instances that this makes.
  deep::proof::<S, D>();
  match Plan::<S, D>::try_cast(value) {
    // SAFETY: validated: the bytes of the value are a valid `D`, each of its pointers a
    // pointer of `S` that is not null, and what they aim at is proven by `deep`, as in
    // `view_ref`.
    Ok(target) => Ok(unsafe { &*target }),
    Err(error) => Err(error),
  }
}

/// Reinterpret a reference as a reference to a type that covers the start of the value.
///
/// If the target has a slice, it takes as many elements as fit. This compiles if those first
/// bytes of every `S` are a valid `D`, and if `D` has no greater alignment. It then costs nothing.
///
/// ```
/// let head: &[u8; 2] = puns::head_ref(&[1u8, 2, 3]);
/// assert_eq!(head, &[1, 2]);
/// let pairs: &[[u8; 2]] = puns::head_ref(&[1u8, 2, 3, 4, 5]);
/// assert_eq!(pairs, &[[1, 2], [3, 4]]);
/// ```
///
/// ```compile_fail
/// // An empty slice has no first byte.
/// let _: &u8 = puns::head_ref::<[u8], u8>(&[1, 2]);
/// ```
#[inline(always)]
pub const fn head_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> &D {
  const { Plan::<S, D>::HEAD_REF };
  // SAFETY: checked at compile time.
  unsafe { &*Plan::<S, D>::cast(value) }
}

/// Reinterpret a reference as a reference to a type that covers the end of the value.
///
/// The target is the last `size_of::<D>()` bytes of the source, which must be `Sized`: this
/// compiles if those bytes of every `S` are a valid `D`, and if `D` has no greater alignment (its
/// offset in `S` is then a multiple of its alignment, as sizes are multiples of alignments). It
/// then costs an addition.
///
/// ```
/// let tail: &[u8; 2] = puns::tail_ref(&[1u8, 2, 3]);
/// assert_eq!(tail, &[2, 3]);
/// let last: &u16 = puns::tail_ref(&[1u16, 2, 3]);
/// assert_eq!(*last, 3);
/// ```
///
/// Where the last bytes of an unsized value start is only known at run time: see [`from_tail`].
///
/// ```compile_fail
/// let _: &u8 = puns::tail_ref::<[u8], u8>(&[1, 2]);
/// ```
///
/// ```compile_fail
/// // A greater alignment.
/// let _: &u16 = puns::tail_ref(&[0u8; 4]);
/// ```
#[inline(always)]
pub const fn tail_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> &D {
  const { Plan::<S, D>::TAIL_REF };
  // SAFETY: checked at compile time.
  unsafe { &*Plan::<S, D>::tail_cast(value) }
}

/// Reinterpret a mutable reference as a mutable reference to a type that covers the start of the
/// value.
///
/// On top of what [`head_ref`] requires, whatever is written through the result must leave a
/// valid `S`: every value of `D` must be valid for the bytes it covers, and nothing of `S`
/// outside of them may depend on them. So a source is refused if a rule of one of its structs
/// (a size, a length, an assertion, a region) holds over those bytes, or if one of its enums is
/// only partly covered (its tag in, its fields out, or the reverse).
///
/// ```
/// let mut words = [0x0102u16, 0x0304];
/// *puns::head_mut::<_, u16>(&mut words) = 7;
/// assert_eq!(words, [7, 0x0304]);
/// let mut bytes = [0u8; 5];
/// puns::head_mut::<_, [[u8; 2]]>(&mut bytes[..]).fill([1, 2]);
/// assert_eq!(bytes, [1, 2, 1, 2, 0]);
/// ```
///
/// ```compile_fail
/// // A `u8` can be 2, which is not a `bool`.
/// let _: &mut u8 = puns::head_mut(&mut [true, false]);
/// ```
///
/// ```compile_fail
/// #[derive(puns::Pun)]
/// #[repr(u8)]
/// enum Shape {
///   Circle { radius: u16 },
///   Rect(u8, bool),
/// }
///
/// // The tag of the shape, without what it tells of.
/// let _: &mut u8 = puns::head_mut(&mut Shape::Rect(1, true));
/// ```
#[inline(always)]
pub const fn head_mut<S: ?Sized + Pun, D: ?Sized + Pun>(value: &mut S) -> &mut D {
  const { Plan::<S, D>::HEAD_MUT };
  let source: *mut S = value;
  // SAFETY: checked at compile time.
  unsafe { &mut *Plan::<S, D>::cast(source).cast_mut() }
}

/// Reinterpret a mutable reference as a mutable reference to a type that covers the end of the
/// value.
///
/// On top of what [`tail_ref`] requires, whatever is written through the result must leave a
/// valid `S`, as for [`head_mut`].
///
/// ```
/// let mut words = [0x0102u16, 0x0304, 0x0506];
/// *puns::tail_mut::<_, [u16; 2]>(&mut words) = [7, 8];
/// assert_eq!(words, [0x0102, 7, 8]);
/// ```
///
/// ```compile_fail
/// #[derive(puns::Pun)]
/// #[repr(C)]
/// #[pun(assert(a < b))]
/// struct Ordered {
///   a: u8,
///   b: u8,
/// }
///
/// // Writing `b` could break the rule.
/// let _: &mut u8 = puns::tail_mut(&mut Ordered { a: 1, b: 2 });
/// ```
#[inline(always)]
pub const fn tail_mut<S: ?Sized + Pun, D: ?Sized + Pun>(value: &mut S) -> &mut D {
  const { Plan::<S, D>::TAIL_MUT };
  let source: *mut S = value;
  // SAFETY: checked at compile time.
  unsafe { &mut *Plan::<S, D>::tail_cast(source).cast_mut() }
}

/// Reinterpret a reference as a reference to another type, if the value is a valid one.
///
/// Only what could not be proven at compile time is checked: the size if it depends on a slice
/// length, the address if the target has a greater alignment, and the scalars, strings and rules
/// of the target that the source does not guarantee.
///
/// ```
/// assert_eq!(puns::try_pun_ref::<[u8], str>(b"caf\xc3\xa9"), Ok("café"));
/// assert!(puns::try_pun_ref::<[u8], str>(b"caf\xc3").is_err());
/// ```
///
/// This is a `const fn`, but a target of greater alignment than the source cannot be made at
/// compile time, where addresses are not known.
///
/// # Errors
/// [`Error::Alignment`] if the address of `value` is not aligned for `D`, [`Error::Size`] if its
/// size is not one that `D` can have, [`Error::Validity`] if it is not a valid `D`.
#[inline]
pub const fn try_pun_ref<S: ?Sized + Pun, D: ?Sized + Pun>(value: &S) -> Result<&D, Error> {
  const { Plan::<S, D>::TRY_REF };
  match Plan::<S, D>::try_cast(value) {
    // SAFETY: validated.
    Ok(target) => Ok(unsafe { &*target }),
    Err(error) => Err(error),
  }
}

/// Reinterpret a mutable reference as a mutable reference to another type, if the value is a
/// valid one.
///
/// See [`try_pun_ref`] and [`pun_mut`]: what is checked is what `try_pun_ref` checks, and what
/// can be written through the result must be a valid `S`, as for `pun_mut`.
///
/// ```
/// let mut byte = 1u8;
/// let flag: &mut bool = puns::try_pun_mut(&mut byte).unwrap();
/// *flag = false;
/// assert_eq!(byte, 0);
///
/// let mut two = 2u8;
/// assert_eq!(puns::try_pun_mut::<u8, bool>(&mut two), Err(puns::Error::Validity));
/// ```
///
/// ```compile_fail
/// // A `u8` can be written through the result, which a `bool` cannot hold.
/// let _ = puns::try_pun_mut::<bool, u8>(&mut true);
/// ```
///
/// # Errors
/// As [`try_pun_ref`].
#[inline]
pub const fn try_pun_mut<S: ?Sized + Pun, D: ?Sized + Pun>(value: &mut S) -> Result<&mut D, Error> {
  const { Plan::<S, D>::TRY_MUT };
  let source: *mut S = value;
  match Plan::<S, D>::try_cast(source) {
    // SAFETY: validated.
    Ok(target) => Ok(unsafe { &mut *target.cast_mut() }),
    Err(error) => Err(error),
  }
}

/// A value whose bytes are all `0`.
///
/// ```
/// assert_eq!(puns::zeroed::<(u32, [bool; 2])>(), (0, [false; 2]));
/// ```
///
/// ```compile_fail
/// let _ = puns::zeroed::<core::num::NonZero<u8>>();
/// ```
#[inline(always)]
#[must_use]
pub const fn zeroed<T: Pun + Copy>() -> T {
  const {
    assert!(T::LAYOUT.zeroable(), "zero is not a valid value of this type");
    // No `Copy` type holds one; were there one, it would not be made from nothing.
    assert!(!T::LAYOUT.has_pointers(), "this type holds a self-relative pointer");
  };
  // SAFETY: checked at compile time.
  unsafe { core::mem::zeroed() }
}

/// The value of an integer field, as `#[pun(get(..))]` expressions see it.
#[doc(hidden)]
#[inline(always)]
pub const fn int<T: Pun>(value: &T) -> u128 {
  let spec = const {
    match T::LAYOUT.as_integer() {
      Some(spec) => spec,
      None => panic!(
        "a getter reads the unsigned integer fields of its struct as `u128`s, and this field is \
         not one (a signed or floating-point number, a `bool`, a `char` or an enum is not)"
      ),
    }
  };
  // SAFETY: a `T` is that integer, which is not allowed to be uninitialized.
  unsafe { spec.load(core::ptr::from_ref::<T>(value).cast()) }
}

/// The bytes of a value that has no padding.
///
/// Convenient sugar for [`pun_ref::<S, [u8]>`](pun_ref), with identical behaviour: it exists
/// because it names the target, bytes, so that only the source is left to infer.
///
/// ```
/// assert_eq!(puns::as_bytes(&[1u16.to_be(), 2u16.to_be()]), &[0, 1, 0, 2]);
/// ```
///
/// ```compile_fail
/// // The second byte is padding.
/// let _ = puns::as_bytes(&(0u8, 0u16));
/// ```
#[inline(always)]
pub const fn as_bytes<S: ?Sized + Pun>(value: &S) -> &[u8] {
  pun_ref(value)
}

/// A reference to bytes as a reference to a `D`, if they are one.
///
/// Convenient sugar for [`try_pun_ref::<[u8], D>`](try_pun_ref), with identical behaviour. It
/// exists because it pins the source to bytes, which a call to `try_pun_ref` leaves to inference
/// (`try_pun_ref(&[1, 0])` is a pun of `[i32; 2]`: an unsuffixed integer literal is an `i32`
/// unless something says otherwise), and because it takes one type parameter.
///
/// The address is checked if `D` has an alignment greater than 1, then the size if it depends on
/// a slice length, then what bytes do not prove of `D`.
///
/// ```
/// use puns::{Error, Pun};
///
/// #[derive(Pun, Debug, PartialEq)]
/// #[repr(C)]
/// struct Position {
///   x: u8,
///   visible: bool,
/// }
///
/// assert_eq!(puns::from_bytes(&[7, 1]), Ok(&Position { x: 7, visible: true }));
/// assert_eq!(puns::from_bytes::<Position>(&[7, 2]), Err(Error::Validity));
/// assert_eq!(puns::from_bytes::<Position>(&[7, 1, 0]), Err(Error::Size));
/// assert_eq!(puns::from_bytes::<str>(b"caf\xc3\xa9"), Ok("caf\u{e9}"));
/// ```
///
/// The source is bytes, whatever the literal:
///
/// ```compile_fail
/// let _ = puns::from_bytes::<[u8; 2]>(&[7i32, 1]);
/// ```
///
/// # Errors
/// As [`try_pun_ref`]: [`Error::Alignment`], [`Error::Size`], [`Error::Validity`].
#[inline]
pub const fn from_bytes<D: ?Sized + Pun>(bytes: &[u8]) -> Result<&D, Error> {
  try_pun_ref::<[u8], D>(bytes)
}

/// A mutable reference to bytes as a mutable reference to a `D`, if they are one.
///
/// Convenient sugar for [`try_pun_mut::<[u8], D>`](try_pun_mut), with identical behaviour; see
/// [`from_bytes`] for why it exists. As anything can be written through the result, `D` must not
/// have padding: bytes would be de-initialized.
///
/// ```
/// let mut bytes = [0u8, 0, 0, 1];
/// *puns::from_bytes_mut::<[bool; 2]>(&mut bytes[2..]).unwrap() = [true, false];
/// assert_eq!(bytes, [0, 0, 1, 0]);
/// assert!(puns::from_bytes_mut::<[bool; 2]>(&mut [2, 0]).is_err());
/// ```
///
/// ```compile_fail
/// let _ = puns::from_bytes_mut::<(u8, u16)>(&mut [0; 4]);
/// ```
///
/// # Errors
/// As [`try_pun_mut`].
#[inline]
pub const fn from_bytes_mut<D: ?Sized + Pun>(bytes: &mut [u8]) -> Result<&mut D, Error> {
  try_pun_mut::<[u8], D>(bytes)
}

// -------------------------------------------------------------------------------------------------
// Parsing

impl<D: ?Sized + Pun> Plan<[u8], D> {
  /// The `D` at the start of the `len` bytes at `addr`, and its size, with every check that
  /// could not be made at compile time (`from_head`, `from_head_mut`).
  ///
  /// # Safety
  /// `addr` must be valid for reads of `n` initialized bytes.
  #[inline(always)]
  const unsafe fn front(addr: *const u8, n: usize) -> Result<(*const D, usize), Error> {
    // SAFETY: guaranteed by the caller; the marks of this pun, from bytes.
    unsafe { Self::front_by::<Self>(addr, n, usize::MAX) }
  }

  /// [`Plan::front`], checking what `C` marks rather than everything that bytes do not prove, of
  /// a `D` whose extent (its size without the trailing padding of a slice) is `extent` (any
  /// extent if `usize::MAX`): one of another extent is not valid.
  ///
  /// # Safety
  /// `addr` must be valid for reads of `n` initialized bytes, and `C` must mark of the target
  /// what bytes do not prove ([`Plan`] itself), or what a source does not, of which the first
  /// `extent` bytes are a valid value.
  #[inline(always)]
  const unsafe fn front_by<C: Checks>(
    addr: *const u8,
    n: usize,
    extent: usize,
  ) -> Result<(*const D, usize), Error> {
    let (layout, rule) = const { (D::LAYOUT, D::LAYOUT.length_rule()) };
    let (head, stride) = (layout.head(), layout.stride());
    if n < head {
      return Err(Error::Size);
    }
    if const { D::LAYOUT.align() > 1 } && !ptr::is_aligned(addr, layout.align()) {
      return Err(Error::Alignment);
    }
    let len = match rule {
      _ if stride == 0 => 0,
      // As many elements as fit, trailing padding included.
      None => match n & !(layout.align() - 1) {
        room if room >= head => (room - head) / stride,
        _ => return Err(Error::Size),
      },
      Some(rule) => {
        // SAFETY: the sized fields of the target are in the first `head` bytes, and what a `size`
        // or `len` rule reads beyond them in the `n` bytes.
        let Some(value) = (unsafe { validate::length::<D>(addr, n) }) else {
          return Err(Error::Validity);
        };
        // From the size of the struct that has the rule, down to a number of elements: in a
        // `usize` (where the value is a field, the optimizer knows that it fits one), else a
        // size or a length that no bytes have.
        let before = head - rule.base;
        if value > usize::MAX as u128 {
          let whole = rule.elements || (value - before as u128).is_multiple_of(stride as u128);
          return Err(if whole { Error::Size } else { Error::Validity });
        }
        let value = value as usize;
        let len = match () {
          () if rule.elements => value,
          () if value >= before && (value - before).is_multiple_of(stride) => {
            (value - before) / stride
          }
          () => return Err(Error::Validity),
        };
        if len > (n - head) / stride {
          return Err(Error::Size);
        }
        len
      }
    };
    // At most the `n` bytes: no overflow.
    if extent != usize::MAX && head + len * stride != extent {
      return Err(Error::Validity);
    }
    // With its trailing padding, if any.
    let size = match layout.size_of_val(len) {
      Some(size) if size <= n => size,
      _ => return Err(Error::Size),
    };
    let target = ptr::from_raw_parts::<D>(addr, len);
    // SAFETY: the target is in the bounds of the bytes, which are initialized; what `C` does not
    // mark, they prove (guaranteed by the caller).
    if !unsafe { validate::<C, Root, D>(target) } {
      return Err(Error::Validity);
    }
    Ok((target, size))
  }

  /// The `D` at the end of the `n` bytes at `addr`, and where it starts, with every check that
  /// could not be made at compile time (`from_tail`, `from_tail_mut`). `D` is sized.
  ///
  /// # Safety
  /// `addr` must be valid for reads of `n` initialized bytes.
  #[inline(always)]
  const unsafe fn back(addr: *const u8, n: usize) -> Result<(usize, *const D), Error> {
    let layout = const { D::LAYOUT };
    let Some(start) = n.checked_sub(layout.size()) else { return Err(Error::Size) };
    // SAFETY: `start <= n`.
    let addr = unsafe { addr.add(start) };
    if const { D::LAYOUT.align() > 1 } && !ptr::is_aligned(addr, layout.align()) {
      return Err(Error::Alignment);
    }
    let target = ptr::from_raw_parts::<D>(addr, 0);
    // SAFETY: the target is the last bytes, which are initialized.
    if !unsafe { validate::<Self, Root, D>(target) } {
      return Err(Error::Validity);
    }
    Ok((start, target))
  }
}

/// Take a `D` from the start of `bytes`: returns it and what remains.
///
/// A `Sized` target takes its size. An unsized one takes what its `size` or `len` rule says
/// (see `#[derive(Pun)]`), or else as many elements of its slice as there are.
///
/// ```
/// use puns::Pun;
/// use puns::num::u16be;
///
/// #[derive(Pun)]
/// #[repr(C)]
/// #[pun(size = length)]
/// struct Chunk {
///   kind: u8,
///   length: u16be,
///   data: [u8],
/// }
///
/// let (chunk, rest) = puns::from_head::<Chunk>(&[7, 0, 5, b'h', b'i', 1, 2, 3]).unwrap();
/// assert_eq!((chunk.kind, &chunk.data, rest), (7, &b"hi"[..], &[1, 2, 3][..]));
/// ```
///
/// # Errors
/// [`Error::Alignment`] if `bytes` are not aligned for `D`, [`Error::Size`] if they are fewer
/// than a `D` takes, [`Error::Validity`] if its `size` or `len` rule cannot be computed or does
/// not give a size it can have, or if what it takes is not a valid `D`.
#[inline]
pub const fn from_head<D: ?Sized + Pun>(bytes: &[u8]) -> Result<(&D, &[u8]), Error> {
  const { Plan::<[u8], D>::TRY_REF };
  // SAFETY: bytes.
  match unsafe { Plan::<[u8], D>::front(bytes.as_ptr(), bytes.len()) } {
    // SAFETY: validated.
    Ok((target, size)) => Ok((unsafe { &*target }, bytes.split_at(size).1)),
    Err(error) => Err(error),
  }
}

/// Whatever `bytes` can neither be nor start with a `D`, as the first scalar that `D` requires
/// some values of (an EtherType, a version) tells: [`from_bytes`] and [`from_head`] would check
/// it, and fail.
///
/// A view of one thing or another (an enum of references, see `#[derive(Pun)]`) tests this
/// before it decides each of its variants (`view::decide`): the same bytes are then compared
/// with what each variant requires there, which the optimizer makes one decision, rather than
/// each conversion computing sizes and rules before it gets there.
#[doc(hidden)]
#[inline(always)]
#[must_use]
pub const fn rejects<D: ?Sized + Pun>(bytes: &[u8]) -> bool {
  let Some((offset, spec)) = (const { D::LAYOUT.key() }) else { return false };
  // The key is in the sized part of `D`: if it is not in `bytes`, neither is a `D`.
  if bytes.len() < offset + spec.size {
    return true;
  }
  // SAFETY: in the bounds of `bytes`, which are initialized; `matches` reads a scalar of at most
  // 16 bytes, without alignment.
  !unsafe { spec.matches(bytes.as_ptr().add(offset)) }
}

/// Take a `D` from the end of `bytes`: returns what precedes it, and it.
///
/// The target is the last `size_of::<D>()` bytes, which are checked as [`from_bytes`] checks
/// them: their address if `D` has an alignment greater than 1, then what bytes do not prove of
/// `D`.
///
/// ```
/// use puns::Pun;
/// use puns::num::u16be;
///
/// /// A trailer: the length of what precedes it, and a check.
/// #[derive(Pun)]
/// #[repr(C)]
/// struct Trailer {
///   length: u16be,
///   check: u8,
/// }
///
/// let (body, trailer) = puns::from_tail::<Trailer>(&[b'h', b'i', 0, 2, 0xff]).unwrap();
/// assert_eq!((body, trailer.length.get(), trailer.check), (&b"hi"[..], 2, 0xff));
/// assert_eq!(puns::from_tail::<Trailer>(&[0, 2]).err(), Some(puns::Error::Size));
/// ```
///
/// Where an unsized target starts cannot be found from the end of the bytes:
///
/// ```compile_fail
/// let _ = puns::from_tail::<[u16]>(&[0; 4]);
/// ```
///
/// # Errors
/// [`Error::Size`] if `bytes` are fewer than `size_of::<D>()`, [`Error::Alignment`] if the last
/// ones are not aligned for `D`, [`Error::Validity`] if they are not a valid `D`.
#[inline]
pub const fn from_tail<D: ?Sized + Pun>(bytes: &[u8]) -> Result<(&[u8], &D), Error> {
  const { Plan::<[u8], D>::FROM_TAIL };
  // SAFETY: bytes.
  match unsafe { Plan::<[u8], D>::back(bytes.as_ptr(), bytes.len()) } {
    // SAFETY: validated.
    Ok((start, target)) => Ok((bytes.split_at(start).0, unsafe { &*target })),
    Err(error) => Err(error),
  }
}

/// Take a `D` from the start of mutable `bytes`: returns it and what remains, both mutable.
///
/// As [`from_head`]; as anything can be written through the result, `D` must not have padding
/// (bytes would be de-initialized), as [`from_bytes_mut`] requires.
///
/// ```
/// let mut bytes = [1u8, 0, 7, 7];
/// let (flags, rest) = puns::from_head_mut::<[bool; 2]>(&mut bytes).unwrap();
/// flags[1] = true;
/// rest[0] = 9;
/// assert_eq!(bytes, [1, 1, 9, 7]);
/// assert!(puns::from_head_mut::<[bool; 2]>(&mut [1, 2, 0]).is_err());
/// ```
///
/// ```compile_fail
/// let _ = puns::from_head_mut::<(u8, u16)>(&mut [0; 4]);
/// ```
///
/// # Errors
/// As [`from_head`].
#[inline]
pub const fn from_head_mut<D: ?Sized + Pun>(
  bytes: &mut [u8],
) -> Result<(&mut D, &mut [u8]), Error> {
  const { Plan::<[u8], D>::TRY_MUT };
  // SAFETY: bytes.
  match unsafe { Plan::<[u8], D>::front(bytes.as_mut_ptr().cast_const(), bytes.len()) } {
    Ok((target, size)) => {
      let len = ptr::metadata(target);
      let (front, rest) = bytes.split_at_mut(size);
      let target = ptr::from_raw_parts::<D>(front.as_mut_ptr().cast_const(), len).cast_mut();
      // SAFETY: the bytes of `front` were validated.
      Ok((unsafe { &mut *target }, rest))
    }
    Err(error) => Err(error),
  }
}

/// Take a `D` from the end of mutable `bytes`: returns what precedes it, and it, both mutable.
///
/// As [`from_tail`]; as anything can be written through the result, `D` must not have padding,
/// as [`from_bytes_mut`] requires.
///
/// ```
/// let mut bytes = [7u8, 7, 0, 1];
/// let (rest, flags) = puns::from_tail_mut::<[bool; 2]>(&mut bytes).unwrap();
/// flags[0] = true;
/// rest[0] = 9;
/// assert_eq!(bytes, [9, 7, 1, 1]);
/// assert!(puns::from_tail_mut::<[bool; 2]>(&mut [1, 2]).is_err());
/// ```
///
/// ```compile_fail
/// let _ = puns::from_tail_mut::<(u8, u16)>(&mut [0; 4]);
/// ```
///
/// # Errors
/// As [`from_tail`].
#[inline]
pub const fn from_tail_mut<D: ?Sized + Pun>(
  bytes: &mut [u8],
) -> Result<(&mut [u8], &mut D), Error> {
  const { Plan::<[u8], D>::FROM_TAIL_MUT };
  // SAFETY: bytes.
  match unsafe { Plan::<[u8], D>::back(bytes.as_mut_ptr().cast_const(), bytes.len()) } {
    Ok((start, _)) => {
      let (rest, back) = bytes.split_at_mut(start);
      let target = ptr::from_raw_parts::<D>(back.as_mut_ptr().cast_const(), 0).cast_mut();
      // SAFETY: the bytes of `back` were validated.
      Ok((rest, unsafe { &mut *target }))
    }
    Err(error) => Err(error),
  }
}

/// The bytes of the `index`th region of the tail of a struct, see `#[derive(Pun)]`.
///
/// Regions are not something that the type system knows of: if the fields of `value` tell of
/// regions that do not fit in its tail, they are cut short.
#[doc(hidden)]
#[inline(always)]
pub const fn region<S: ?Sized + Pun>(value: &S, index: usize) -> &[u8] {
  // The bytes of the value can be read: it has no padding.
  const { Plan::<S, [u8]>::BY_REF };
  let head = const { S::LAYOUT.tail_shape().start };
  let (_, tail, size) = validate::tail_of::<S>(value);
  // SAFETY: `value` is a struct of this layout, whose bytes are readable.
  unsafe {
    validate::region_at::<S, S::Rules>(
      core::ptr::from_ref::<S>(value).cast::<u8>(),
      size,
      (head, tail),
      index,
      (0, 0),
    )
  }
}
