//! Views of one thing or another: what `#[derive(Pun)]` writes for an enum whose variants each
//! hold a reference (`from_bytes` and `from_head`, the first variant that bytes are).
//!
//! Each variant is decided once, and only for what it is not known to be. Where the type of a
//! later variant takes every value of the type of an earlier one (`Frame<Ipv4>` takes every
//! `Frame<Ipv4<Udp>>`), the later one is its *base*: it is decided first, and the earlier one
//! then fails with it, or is checked for what its base does not prove of it (the protocol of the
//! packet, not its checksums again). What is decided is kept ([`Known`]), so that a variant that
//! is the base of several, or that is tried in its turn after it was a base, is checked once.
//! Before any of this, a variant whose first constrained scalar the bytes do not have (an
//! EtherType) is rejected by that scalar alone ([`crate::rejects`]).
//!
//! The answer is the one of trying each variant in turn: the base of a variant is a type that
//! every value of the variant is (the analysis proves it, as it proves `pun_ref`), over the same
//! bytes, and what a variant is checked for from its base is what `try_pun_ref` from the base
//! checks, which is exact for a base that met everything it requires.
//!
//! Soundness does not rest on the code that the derive writes: the types of the variants are a
//! list ([`List`]) whose layouts and types are both taken from it, by a place in it that only this
//! module makes ([`Index`]); what a variant is known to be is only written by [`decide`], for the
//! type of its index; the base of a variant is used only where [`Known`] says that the bytes were
//! found to be one. The derive chooses the order in which variants are decided: it decides the
//! base of a variant before the variant (else the variant is not found, which the model check
//! would see), and returns the first variant found, in order.

use core::marker::PhantomData;

use crate::{
  Plan, Pun,
  analysis::{Exact, Marks, analyze},
  layout::{Kind, Layout, Spec, Valid},
  ptr, rejects,
  validate::Checks,
};

mod sealed {
  pub trait Sealed {}
}

/// The type of one variant of a view: the leaf of a tree of them (see [`List`]).
///
/// The types of the variants of a view, in order, are a balanced tree: `Leaf<A>`,
/// `Pair<Leaf<A>, Pair<Leaf<B>, Leaf<C>>>`... A tree rather than a list, so that what proves a
/// type of it (the trait solver, which stops at a depth of 128 by default) goes as deep as the
/// logarithm of the number of variants.
pub struct Leaf<T: ?Sized>(PhantomData<T>);

/// The types of the variants of `A`, then those of `B`.
pub struct Pair<A, B>(PhantomData<A>, PhantomData<B>);

/// The types of the variants of a view: their number and their layouts. Sealed: [`Leaf`] and
/// [`Pair`].
pub trait List: sealed::Sealed {
  /// The number of types.
  const LEN: usize;
  /// Their layouts, in order.
  const TREE: Tree;
}

/// The layouts of the types of a [`List`], as a tree of the same shape.
#[derive(Copy, Clone)]
pub struct Tree {
  len: usize,
  leaf: Option<&'static Layout>,
  pair: Option<(&'static Tree, &'static Tree)>,
}

impl<T: ?Sized + Pun> sealed::Sealed for Leaf<T> {}
impl<A: List, B: List> sealed::Sealed for Pair<A, B> {}

impl<T: ?Sized + Pun> List for Leaf<T> {
  const LEN: usize = 1;
  const TREE: Tree = Tree { len: 1, leaf: Some(&T::LAYOUT), pair: None };
}

impl<A: List, B: List> List for Pair<A, B> {
  const LEN: usize = A::LEN + B::LEN;
  const TREE: Tree = Tree { len: A::LEN + B::LEN, leaf: None, pair: Some((&A::TREE, &B::TREE)) };
}

/// The place of a variant in a [`List`], as a type: [`Here`] (a [`Leaf`]), [`First`] or [`Second`]
/// (the side of a [`Pair`]) of a place in it. Sealed.
pub trait Index: sealed::Sealed {}

/// The type of a [`Leaf`].
pub struct Here;

/// In the first side of a [`Pair`], at `I`.
pub struct First<I>(PhantomData<I>);

/// In the second side of a [`Pair`], at `I`.
pub struct Second<I>(PhantomData<I>);

impl sealed::Sealed for Here {}
impl<I: Index> sealed::Sealed for First<I> {}
impl<I: Index> sealed::Sealed for Second<I> {}
impl Index for Here {}
impl<I: Index> Index for First<I> {}
impl<I: Index> Index for Second<I> {}

/// The type at the place `I` of a list, and its index.
pub trait Get<I: Index>: List {
  /// The type.
  type Type: ?Sized + Pun;
  /// Its index in the list, from 0.
  const INDEX: usize;
}

impl<T: ?Sized + Pun> Get<Here> for Leaf<T> {
  type Type = T;
  const INDEX: usize = 0;
}

impl<A: Get<I>, B: List, I: Index> Get<First<I>> for Pair<A, B> {
  type Type = A::Type;
  const INDEX: usize = A::INDEX;
}

impl<A: List, B: Get<I>, I: Index> Get<Second<I>> for Pair<A, B> {
  type Type = B::Type;
  const INDEX: usize = A::LEN + B::INDEX;
}

/// A view: an enum whose variants each hold a reference to one of the types of `List`, in order.
pub trait Views {
  /// The types of the variants.
  type List: List;
}

/// The layout at `index` of a list.
const fn layout_at(mut tree: &'static Tree, mut index: usize) -> &'static Layout {
  loop {
    match (tree.leaf, tree.pair) {
      (Some(layout), _) if index == 0 => return layout,
      (_, Some((first, _))) if index < first.len => tree = first,
      (_, Some((first, second))) => (tree, index) = (second, index - first.len),
      _ => panic!("no such variant"),
    }
  }
}

/// No base.
const NONE: usize = usize::MAX;

/// The base of the variant `I` of `V`, and what is checked of it from its base.
struct Choice<V: ?Sized, I>(PhantomData<V>, PhantomData<I>);

impl<V: ?Sized + Views, I: Index> Choice<V, I>
where
  V::List: Get<I>,
{
  /// The base for `from_bytes`: the first later variant whose type takes every value of the type
  /// of `I`, over the same bytes, and from which what is left to check of `I` can be checked
  /// (`NONE` if none), with what is left.
  const WHOLE: (usize, Marks) = {
    let index = <V::List as Get<I>>::INDEX;
    base(&V::List::TREE, index, index + 1, false)
  };

  /// The base for `from_head`, which must also take the same bytes as `I` (see [`same_extent`]):
  /// the one of `from_bytes` if it does, as no variant before it is a base of `I` for
  /// `from_bytes`, else a later one.
  const HEAD: (usize, Marks) = {
    let (tree, index, whole) = (&V::List::TREE, <V::List as Get<I>>::INDEX, Self::WHOLE);
    match whole.0 {
      NONE => whole,
      j if same_extent(layout_at(tree, index), layout_at(tree, j)) => whole,
      j => base(tree, index, j + 1, true),
    }
  };

  /// The base of `I` for `from_head` if `head`, else for `from_bytes`.
  const fn base(head: bool) -> usize {
    if head { Self::HEAD.0 } else { Self::WHOLE.0 }
  }
}

/// What is checked of a variant: what its base does not prove of it, or what bytes do not where it
/// has no base. One set for both functions (one instance of the validation of the variant): the
/// same where they have the same base, else what either needs.
impl<V: ?Sized + Views, I: Index> Checks for Choice<V, I>
where
  V::List: Get<I>,
{
  const MARKS: Marks = {
    let all = <Plan<[u8], <V::List as Get<I>>::Type> as Checks>::MARKS;
    let (whole, head) = (Self::WHOLE, Self::HEAD);
    let whole = if whole.0 == NONE { all } else { whole.1 };
    let head = if head.0 == NONE { all } else { head.1 };
    whole.union(&head)
  };
}

/// The index of the first base of the variant at `index` of a list, among the variants from
/// `from`, for `from_head` if `head`, and the marks of the variant from it (see
/// [`Choice::WHOLE`]).
const fn base(tree: &'static Tree, index: usize, from: usize, head: bool) -> (usize, Marks) {
  let layout = layout_at(tree, index);
  let mut j = from;
  while j < tree.len {
    let other = layout_at(tree, j);
    if !apart(layout, 0, other) && (!head || same_extent(layout, other)) {
      // Every value of the variant is one of the other: what `pun_ref` proves.
      let into = analyze(layout, other);
      let relation = into.relation;
      let proven = matches!(into.fit.exact, Exact::Always)
        && into.aligned
        && relation.uninit.is_none()
        && relation.disjoint.is_none()
        && relation.partial.is_none()
        && relation.pointer.is_none()
        && relation.unspecified.is_none();
      if proven {
        // What the variant is checked for from the other: what `try_pun_ref` checks.
        let from = analyze(other, layout);
        let relation = from.relation;
        if !matches!(from.fit.exact, Exact::Never(_))
          && relation.uninit.is_none()
          && relation.disjoint.is_none()
          && relation.pointer.is_none()
          && relation.unspecified.is_none()
        {
          return (j, relation.marks);
        }
      }
    }
    j += 1;
  }
  (NONE, Marks::NONE)
}

/// Whatever no value of `a` (at `at` in a value of `b`) is one of `b`, by a scalar of the sized
/// part of `a` that only some values are valid for, at the same place in `b` as a scalar of the
/// same size and byte order, which has none of them (another EtherType, another protocol). A
/// walk of `a` that spares the analysis of most pairs of variants.
const fn apart(a: &Layout, at: usize, b: &Layout) -> bool {
  match a.kind {
    Kind::Scalar(spec) => match scalar_at(b, at, spec.size) {
      Some(other) => disjoint(&spec, &other),
      None => false,
    },
    Kind::Struct(fields, _) => {
      let mut i = 0;
      while i < fields.len() {
        if apart(fields[i].layout, at + fields[i].offset, b) {
          return true;
        }
        i += 1;
      }
      false
    }
    Kind::Array(elem, len) if len > 0 => apart(elem, at, b),
    _ => false,
  }
}

/// The scalar of `size` bytes at `at` in the sized part of `layout`, if there is one.
const fn scalar_at(layout: &Layout, at: usize, size: usize) -> Option<Spec> {
  match layout.kind {
    Kind::Scalar(spec) if at == 0 && spec.size == size => Some(spec),
    Kind::Struct(fields, _) => {
      let mut i = 0;
      while i < fields.len() {
        let field = &fields[i];
        if field.offset <= at && at < field.offset + field.layout.size() {
          return scalar_at(field.layout, at - field.offset, size);
        }
        i += 1;
      }
      None
    }
    Kind::Array(elem, len) if elem.size() > 0 && at < elem.size() * len => {
      scalar_at(elem, at % elem.size(), size)
    }
    _ => None,
  }
}

/// Whatever no value is valid for both scalars, of the same size: of the same byte order, each
/// restricted to ranges that do not meet.
const fn disjoint(a: &Spec, b: &Spec) -> bool {
  let (Valid::In(a_ranges), Valid::In(b_ranges)) = (a.valid, b.valid) else { return false };
  if !a.endian.equals(b.endian) {
    return false;
  }
  let (mut i, mut j) = (0, 0);
  while i < a_ranges.len() && j < b_ranges.len() {
    let ((a_lo, a_hi), (b_lo, b_hi)) = (a_ranges[i], b_ranges[j]);
    if a_hi < b_lo {
      i += 1;
    } else if b_hi < a_lo {
      j += 1;
    } else {
      return false;
    }
  }
  true
}

/// Whatever `from_head` takes the same bytes as `a` and as `b` wherever they are a valid `a`
/// and a valid `b`: both say how long they are (a size, a `size` or a `len` rule), which a valid
/// value meets, or neither does, and both take as many elements as fit, of the same size, with
/// the same alignment, from a head of the same remainder.
const fn same_extent(a: &Layout, b: &Layout) -> bool {
  const fn told(layout: &Layout) -> bool {
    layout.stride() == 0 || layout.length_rule().is_some()
  }
  match (told(a), told(b)) {
    (true, true) => true,
    (false, false) => {
      a.align() == b.align()
        && a.stride() == b.stride()
        && a.head() % a.stride() == b.head() % b.stride()
    }
    _ => false,
  }
}

/// Not decided yet.
const OPEN: u8 = 0;
/// The bytes are not a value of the variant.
const NOT: u8 = 1;
/// The bytes are (start with, for `from_head`) a value of the variant.
const IS: u8 = 2;

/// What the bytes are known to be, of the `N` variants of `V`: `from_head` if `HEAD`, else
/// `from_bytes`.
pub struct Known<'a, V: ?Sized, const N: usize, const HEAD: bool> {
  bytes: &'a [u8],
  /// For each variant: `OPEN`, `NOT` or `IS`.
  state: [u8; N],
  /// For a variant that the bytes are: the length of its slice, and its size (with the trailing
  /// padding of its slice, if any).
  taken: [(usize, usize); N],
  view: PhantomData<fn() -> V>,
}

impl<'a, V: ?Sized + Views, const N: usize, const HEAD: bool> Known<'a, V, N, HEAD> {
  /// Nothing known yet of `bytes`.
  #[doc(hidden)]
  #[inline(always)]
  #[must_use]
  pub const fn new(bytes: &'a [u8]) -> Self {
    const { assert!(N == V::List::LEN, "one state for each variant") };
    Self { bytes, state: [OPEN; N], taken: [(0, 0); N], view: PhantomData }
  }

  /// The bytes after the value of the variant at `index` (for `from_head`), if the bytes are
  /// one.
  #[doc(hidden)]
  #[inline(always)]
  #[must_use]
  pub const fn rest(&self, index: usize) -> &'a [u8] {
    self.bytes.split_at(self.taken[index].1).1
  }
}

/// Whatever the variant `I` is still to be decided, and the bytes have its first constrained
/// scalar: its base, if any, is then to be decided before it.
#[inline(always)]
#[must_use]
pub const fn open<V: ?Sized + Views, I: Index, const N: usize, const HEAD: bool>(
  known: &Known<'_, V, N, HEAD>,
) -> bool
where
  V::List: Get<I>,
{
  known.state[<V::List as Get<I>>::INDEX] == OPEN
    && !rejects::<<V::List as Get<I>>::Type>(known.bytes)
}

/// The index of the base of the variant `I`, `usize::MAX` if it has none (see `Choice`).
#[must_use]
pub const fn base_of<V: ?Sized + Views, I: Index, const HEAD: bool>() -> usize
where
  V::List: Get<I>,
{
  Choice::<V, I>::base(HEAD)
}

/// Whatever the bytes are the variant `I`: decided once, from its base if the base was decided.
#[inline(always)]
pub const fn decide<'a, V: ?Sized + Views, I: Index, const N: usize, const HEAD: bool>(
  known: &mut Known<'a, V, N, HEAD>,
) -> Option<&'a <V::List as Get<I>>::Type>
where
  V::List: Get<I>,
{
  let index = <V::List as Get<I>>::INDEX;
  let value = match known.state[index] {
    IS => {
      let target = ptr::from_raw_parts(known.bytes.as_ptr(), known.taken[index].0);
      // SAFETY: the bytes were found to be (to start with) a value of the type of `I`, with this
      // length (`Known` is only written below, by this function, for the type of the index).
      return Some(unsafe { &*target });
    }
    NOT => None,
    _ => find::<V, I, N, HEAD>(known),
  };
  let Some((target, size)) = value else {
    known.state[index] = NOT;
    return None;
  };
  (known.state[index], known.taken[index]) = (IS, (ptr::metadata(target), size));
  // SAFETY: validated.
  Some(unsafe { &*target })
}

/// The variant `I` at the start of the bytes, and its size, if the bytes are (start with) one.
///
/// A variant that has a base is decided from it, which the derive decides before it (`open`,
/// then the function of the base): one whose base is not decided is not found. A variant without
/// a base is checked for everything that bytes do not prove, as `from_bytes` and `from_head` do.
#[inline(always)]
const fn find<V: ?Sized + Views, I: Index, const N: usize, const HEAD: bool>(
  known: &Known<'_, V, N, HEAD>,
) -> Option<(*const <V::List as Get<I>>::Type, usize)>
where
  V::List: Get<I>,
{
  const { Plan::<[u8], <V::List as Get<I>>::Type>::TRY_REF };
  let bytes = known.bytes;
  if rejects::<<V::List as Get<I>>::Type>(bytes) {
    return None;
  }
  let extent = if const { Choice::<V, I>::base(HEAD) < N } {
    // (Evaluated where the variant has no base as well, which reads no layout.)
    let (base, head, stride) = const {
      match Choice::<V, I>::base(HEAD) {
        NONE => (NONE, 0, 0),
        base => {
          (base, layout_at(&V::List::TREE, base).head(), layout_at(&V::List::TREE, base).stride())
        }
      }
    };
    // Every value of the variant is one of its base.
    if known.state[base] != IS {
      return None;
    }
    // Its size without the trailing padding of its slice, which both take.
    head + known.taken[base].0 * stride
  } else {
    usize::MAX
  };
  // SAFETY: bytes, of which `Choice` marks what is not proven of the variant: by its base, of
  // which they are (start with) a value of that extent (all of them for `from_bytes`), or else by
  // bytes.
  unsafe { take::<<V::List as Get<I>>::Type, Choice<V, I>, HEAD>(bytes, extent) }
}

/// The `D` at the start of `bytes` (all of them, but for `from_head`), checked for what `C` marks,
/// and its size, if they are one; for `from_head`, of the given extent (any if `usize::MAX`).
///
/// # Safety
/// `C` must mark what the bytes do not prove of `D`: [`Plan`] from bytes, or what a source does
/// not, of which the first `extent` bytes (all of them, but for `from_head`) are a valid value.
#[inline(always)]
const unsafe fn take<D: ?Sized + Pun, C: Checks, const HEAD: bool>(
  bytes: &[u8],
  extent: usize,
) -> Option<(*const D, usize)> {
  // SAFETY: bytes; the marks are guaranteed by the caller.
  let found = unsafe {
    if HEAD {
      Plan::<[u8], D>::front_by::<C>(bytes.as_ptr(), bytes.len(), extent)
    } else {
      match Plan::<[u8], D>::try_cast_by::<C>(bytes) {
        Ok(target) => Ok((target, bytes.len())),
        Err(error) => Err(error),
      }
    }
  };
  match found {
    Ok(found) => Some(found),
    Err(_) => None,
  }
}
