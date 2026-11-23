//! [Views](macro@crate::Pun#views), for the `from_bytes` and `from_head` that `#[derive(Pun)]`
//! writes for them.
//!
//! Both functions return the first variant, in order, that the bytes are (or start with, for
//! `from_head`). The order of the variants is the order of preference. They answer what trying each
//! variant in turn would answer, but each variant is decided at most once, and only for what is not
//! already known of it.
//!
//! - A variant is passed over on its first constrained scalar alone, before anything else of it is
//!   computed, if the bytes do not have a value that this scalar accepts (`crate::rejects`).
//!   Examples of such scalars are an EtherType and a version. These comparisons of the same bytes,
//!   one for each variant, become one decision of the optimizer. A VLAN frame meets the EtherTypes
//!   of IPv4 and of ARP as in a `match`.
//! - A variant has a *base*. It is the first later variant whose type accepts every value of the
//!   type of the variant, over the same bytes. The analysis decides it at compile time, as it
//!   decides that `pun_ref` is proven. `Frame<Ipv4>` accepts every `Frame<Ipv4<Udp>>`, and `Frame`
//!   accepts both. The base is decided first. Bytes that are not a value of the base are not a
//!   value of the variant either. For bytes that are, the variant is checked only for what its base
//!   does not prove of it, which is the marks of `try_pun_ref` from the base. For a packet, that is
//!   its protocol, and not its checksums again.
//! - What is decided is recorded in [`Known`], a state for each variant. A base that several
//!   variants share is decided once, as `Frame<Ipv4>` is for UDP, TCP and ICMP. So is a base that
//!   is tried in its own turn after it was a base. Each variant is a function that the derive
//!   writes. It decides the base of the variant before the variant, by calling the function of the
//!   base, chosen by a `match` on a constant. Once inlined, the states are constants on each path,
//!   and no state remains at run time.
//! - For `from_head`, a base must also take the same bytes as the variant (`same_extent`). Both
//!   must tell their extent (a size, or a `size` or `len` rule, which a valid value meets). Or
//!   neither does, and both take as many elements as fit in the same way: the same alignment and
//!   element, and a head of the same remainder. The variant then takes the extent of its base,
//!   without trailing padding, which each adds on its own.
//!
//! # Why the answer is that of trying each variant in turn
//!
//! This follows from what the analysis proves and checks. Bytes that are not a value of the base
//! are not a value of the variant, since every value of the variant is a value of the base. Bytes
//! that are a value of the base meet everything that the base requires. From such a value,
//! `try_pun_ref` checks exactly what the variant requires beyond it. What it leaves unchecked is
//! implied by the requirements of the source, which the model check checks of every pair. A base is
//! a later variant. It is decided before its turn only as a step of an earlier variant, and it is
//! returned only in its turn. The error is `Error::Validity`. A view does not tell why no variant
//! fits.
//!
//! # Soundness
//!
//! Soundness does not rely on the code that the derive writes. The types of the variants form a
//! balanced tree at the type level ([`List`]), so finding a type in it recurses as deep as the
//! logarithm of the number of variants. A sealed type ([`First`]`<`[`Here`]`>`, an [`Index`]) names
//! a place in the tree, and selects both the type and the layout at that place. Only [`decide`]
//! writes the state of a place, for the type at that place. A variant is decided from its base only
//! where the state of the base says that the bytes are a value of the base, of the extent recorded
//! with it. The derive chooses the order in which variants are decided. A variant whose base was
//! not decided first is not found. This is never unsound, and the model check would detect it.
//!
//! # Cost
//!
//! The bases are found by the analysis of pairs of variants, at compile time. For each variant, the
//! analysis tries the later variants until one takes it. It skips those that a scalar at the same
//! place tells apart, such as another EtherType or another protocol (`apart`, a walk of the
//! layouts). It then analyzes what is left of the variant from the base found. The base of
//! `from_head` is that of `from_bytes` if it takes the same bytes, which it does where both tell
//! their extent. One instance of the validation serves a variant for both functions. It checks what
//! the two bases do not prove, which is the same marks where the base is the same, or everything
//! that bytes do not prove where there is no base. The function of each variant names the functions
//! of all the variants after it, so what the derive writes grows as the square of the number of
//! variants, and the `match` on a constant keeps one of them. `benches/checks.rs` measures the
//! checks of a view.

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

/// The type of one variant of a view: a leaf of a [`List`].
///
/// The types of the variants of a view, in order, form a balanced tree: `Leaf<A>`,
/// `Pair<Leaf<A>, Pair<Leaf<B>, Leaf<C>>>`, and so on. A tree rather than a list keeps the depth
/// of the trait solving that finds a type in it to the logarithm of the number of variants. The
/// trait solver stops at a depth of 128 by default.
pub struct Leaf<T: ?Sized>(PhantomData<T>);

/// The types of the variants of `A`, followed by those of `B`.
pub struct Pair<A, B>(PhantomData<A>, PhantomData<B>);

/// The types of the variants of a view: their number and their layouts.
///
/// This trait is sealed. Only [`Leaf`] and [`Pair`] implement it.
pub trait List: sealed::Sealed {
  /// The number of types.
  const LEN: usize;
  /// The layouts of the types, in order.
  const TREE: Tree;
}

/// The layouts of the types of a [`List`], as a tree of the same shape.
#[derive(Copy, Clone)]
pub struct Tree {
  /// The number of layouts in the tree.
  len: usize,
  /// The layout of a leaf.
  leaf: Option<&'static Layout>,
  /// The two sides of a pair.
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

/// The place of a variant in a [`List`], as a type.
///
/// A place is [`Here`], the type of a [`Leaf`], or [`First`] or [`Second`] of a place in that side
/// of a [`Pair`]. This trait is sealed: only these types implement it.
pub trait Index: sealed::Sealed {}

/// The place of the type of a [`Leaf`].
pub struct Here;

/// The place `I` in the first side of a [`Pair`].
pub struct First<I>(PhantomData<I>);

/// The place `I` in the second side of a [`Pair`].
pub struct Second<I>(PhantomData<I>);

impl sealed::Sealed for Here {}
impl<I: Index> sealed::Sealed for First<I> {}
impl<I: Index> sealed::Sealed for Second<I> {}
impl Index for Here {}
impl<I: Index> Index for First<I> {}
impl<I: Index> Index for Second<I> {}

/// A [`List`] that has a type at the place `I`.
pub trait Get<I: Index>: List {
  /// The type at the place `I`.
  type Type: ?Sized + Pun;
  /// The index of the type in the list, from 0.
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

/// A view: an enum whose variants each hold a reference to one of the types of
/// [`List`](Views::List), in order.
///
/// `#[derive(Pun)]` implements this trait for such an enum.
pub trait Views {
  /// The types of the variants, in order.
  type List: List;
}

/// Returns the layout at `index` of a list.
///
/// # Panics
///
/// Panics if the list has no layout at `index`.
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

/// The index of the base of a variant that has none.
const NONE: usize = usize::MAX;

/// The base of the variant `I` of the view `V`, and what is checked of the variant from it.
struct Choice<V: ?Sized, I>(PhantomData<V>, PhantomData<I>);

impl<V: ?Sized + Views, I: Index> Choice<V, I>
where
  V::List: Get<I>,
{
  /// The index of the base for `from_bytes`, or `NONE`, and what is left to check of `I` from it.
  ///
  /// The base is the first later variant whose type accepts every value of the type of `I`, over
  /// the same bytes, and from which what is left to check of `I` can be checked.
  const WHOLE: (usize, Marks) = {
    let index = <V::List as Get<I>>::INDEX;
    base(&V::List::TREE, index, index + 1, false)
  };

  /// The index of the base for `from_head`, or `NONE`, and what is left to check of `I` from it.
  ///
  /// The base must also take the same bytes as `I` (`same_extent`). It is the base for
  /// `from_bytes` if that base does, since no variant before it is a base of `I` for
  /// `from_bytes`. Otherwise it is a later one.
  const HEAD: (usize, Marks) = {
    let (tree, index, whole) = (&V::List::TREE, <V::List as Get<I>>::INDEX, Self::WHOLE);
    match whole.0 {
      NONE => whole,
      j if same_extent(layout_at(tree, index), layout_at(tree, j)) => whole,
      j => base(tree, index, j + 1, true),
    }
  };

  /// Returns the index of the base of `I`, or `NONE`, for `from_head` if `head`, else for
  /// `from_bytes`.
  const fn base(head: bool) -> usize {
    if head { Self::HEAD.0 } else { Self::WHOLE.0 }
  }
}

/// What is checked of a variant: what its base does not prove of it, or, where it has no base,
/// what bytes do not prove of it.
///
/// One set serves both functions, so that the variant has one instance of its validation. It is
/// the same set where both functions have the same base, and otherwise what either needs.
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

/// Returns the index of the first base of the variant at `index` of a list, among the variants
/// from `from` on, and the marks of the variant from it.
///
/// With `head`, the base is for `from_head`, and must take the same bytes as the variant. The
/// index is `NONE` if no variant is a base ([`Choice::WHOLE`]).
const fn base(tree: &'static Tree, index: usize, from: usize, head: bool) -> (usize, Marks) {
  let layout = layout_at(tree, index);
  let mut j = from;
  while j < tree.len {
    let other = layout_at(tree, j);
    if !apart(layout, 0, other) && (!head || same_extent(layout, other)) {
      // Every value of the variant is one of the other: what `pun_ref` proves.
      let into = analyze(layout, other);
      if matches!(into.fit.exact, Exact::Always) && into.aligned && into.relation.infallible() {
        // What the variant is checked for from the other: what `try_pun_ref` checks.
        let from = analyze(other, layout);
        let relation = from.relation;
        if !matches!(from.fit.exact, Exact::Never(_))
          && relation.uninit.is_none()
          && relation.disjoint.is_none()
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

/// Returns whether no value of `a`, at `at` in a value of `b`, is a value of `b`, by a scalar.
///
/// The scalar is in the sized part of `a`, and only some values are valid for it. At the same
/// place, `b` has a scalar of the same size and byte order, for which none of these values is
/// valid: another EtherType, or another protocol. This walk of `a` rules out most pairs of variants
/// before their analysis.
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

/// Returns the scalar of `size` bytes at `at` in the sized part of `layout`, if there is one.
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

/// Returns whether no value is valid for both scalars, of the same size.
///
/// It is so if they have the same byte order, and are each restricted to ranges that do not
/// meet.
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

/// Returns whether `from_head` takes the same bytes as `a` and as `b`, given that every `a` is a
/// `b` of the same extent, which the caller proves.
///
/// It does if both say how long they are, by a size, a `size` rule or a `len` rule: a valid value
/// meets its rule, so the extent that `a` gives is the one that `b` gives. It does too if neither
/// says, and both take as many elements as fit, of the same size, with the same alignment, from a
/// head of the same remainder.
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
/// The bytes are a value of the variant, or start with one for `from_head`.
const IS: u8 = 2;

/// What the bytes are known to be, for each of the `N` variants of the view `V`.
///
/// `HEAD` selects what `from_head` decides, a value at the start of the bytes. Otherwise it is
/// what `from_bytes` decides, a value of all the bytes.
pub struct Known<'a, V: ?Sized, const N: usize, const HEAD: bool> {
  /// The bytes that are decided.
  bytes: &'a [u8],
  /// For each variant: `OPEN`, `NOT` or `IS`.
  state: [u8; N],
  /// For a variant that the bytes are, the length of its slice and its size, with the trailing
  /// padding of its slice, if any.
  taken: [(usize, usize); N],
  /// The view whose variants these are.
  view: PhantomData<fn() -> V>,
}

impl<'a, V: ?Sized + Views, const N: usize, const HEAD: bool> Known<'a, V, N, HEAD> {
  /// Creates a record of `bytes` in which nothing is decided yet.
  ///
  /// # Compile-time errors
  ///
  /// A call compiles only if `N` is the number of variants of `V` (`V::List::LEN`). This is
  /// checked after monomorphization.
  #[doc(hidden)]
  #[inline(always)]
  #[must_use]
  pub const fn new(bytes: &'a [u8]) -> Self {
    const { assert!(N == V::List::LEN, "one state for each variant") };
    Self { bytes, state: [OPEN; N], taken: [(0, 0); N], view: PhantomData }
  }

  /// Returns the bytes after the value of the variant at `index`, for `from_head`, if the bytes
  /// start with one.
  ///
  /// # Panics
  ///
  /// Panics if `index` is not less than `N`, the number of variants.
  #[doc(hidden)]
  #[inline(always)]
  #[must_use]
  pub const fn rest(&self, index: usize) -> &'a [u8] {
    self.bytes.split_at(self.taken[index].1).1
  }
}

/// Returns whether the variant `I` is still to be decided and the bytes have a value that its
/// first constrained scalar accepts.
///
/// If so, the derive decides the base of `I`, if it has one, before `I`.
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

/// Returns the index of the base of the variant `I`, or `usize::MAX` if it has none.
///
/// `HEAD` selects the base for `from_head`, which must also take the same bytes as `I`.
/// Otherwise it is the base for `from_bytes`. The base is the first later variant whose type
/// accepts every value of the type of `I`, over the same bytes, and from which what is left to
/// check of `I` can be checked.
#[must_use]
pub const fn base_of<V: ?Sized + Views, I: Index, const HEAD: bool>() -> usize
where
  V::List: Get<I>,
{
  Choice::<V, I>::base(HEAD)
}

/// Returns the variant `I` if the bytes are a value of its type, or start with one for
/// `from_head`.
///
/// The variant is decided once, and the answer is recorded in `known`. A variant that has a base
/// is decided from it. It is not found if its base was not decided before it, or if the bytes are
/// not a value of the base. A variant without a base is checked for everything that bytes do not
/// prove, as `from_bytes` and `from_head` check it.
///
/// # Compile-time errors
///
/// A call compiles only under the compile-time conditions of [`from_bytes`](crate::from_bytes) for
/// the type of `I`, which are checked after monomorphization, as [the guide](crate::guide::errors)
/// explains. Among them:
///
/// - The type of `I` holds no scalar that a pun never makes in a part that the check covers, except
///   in a variant of an enum, which a check never accepts.
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
      // SAFETY: `known.state[index]` is `IS` only where this function set it below, with
      // `known.taken[index]` the tail length and the size of the value that `find` found for
      // the type at `I`. The fields of `Known` are private, and only `Known::new` and this
      // function write them. `INDEX` tells the places of `V::List` apart, so the type at `index`
      // is the type at `I`, and `known.bytes` is the same since `Known::new`. So `target` is the
      // value found then, and the reference is valid, as below.
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
  // SAFETY: `find` returned `target`, a value of the type at `I` at the address of `known.bytes`,
  // of size at most their length. The reference is valid, as in `from_head` and `try_pun_ref`:
  // - It is not null, because `known.bytes` is a reference at the same address.
  // - It is aligned, because `Plan::front_by` and `Plan::try_cast_by` checked the address.
  // - It is not dangling, because it is within `known.bytes`.
  // - It points to a valid value of the type at `I`, as `find` returns. `validate` found every part
  //   that `Choice<V, I>` marks to be valid, and the base of `I`, or bytes, prove the others.
  // The reference has the lifetime `'a` of the shared borrow `known.bytes`, and the type at `I`
  // holds no `UnsafeCell` (`Pun`, section Safety).
  Some(unsafe { &*target })
}

/// Returns the variant `I` at the start of the bytes, and its size, if the bytes are one, or
/// start with one for `from_head`.
///
/// A returned pointer is at the address of the bytes, and points to a valid value of the type at
/// `I`, of the size returned, which is at most the length of the bytes.
///
/// A variant that has a base is decided from it. The derive decides the base before the variant,
/// with [`open`] and then the function of the base. A variant whose base is not decided is not
/// found. A variant without a base is checked for everything that bytes do not prove, as
/// `from_bytes` and `from_head` check it.
///
/// # Compile-time errors
///
/// An instance compiles only if [`try_pun_ref`](crate::try_pun_ref) from bytes to the type of `I`
/// compiles.
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
  // `Choice<V, I>` marks what is not proven of the type of `I`, so what `take` returns is a valid
  // value of it. If `I` has a base, its marks include what the base does not prove, and
  // `known.state[base]` is `IS`: the bytes are a value of the base, or start with one of extent
  // `extent` for `from_head`. Otherwise its marks include those of `Plan` from bytes. More marks
  // only check more.
  take::<<V::List as Get<I>>::Type, Choice<V, I>, HEAD>(bytes, extent)
}

/// Returns the `D` that `bytes` are, or start with for `from_head`, and its size, after checking
/// what `C` marks.
///
/// For `from_head`, the value has the extent `extent`, or any extent if `extent` is `usize::MAX`.
/// Returns `None` if the bytes are not such a `D`.
///
/// The returned pointer is a valid `D` only if `C` marks everything that the bytes do not prove of
/// `D`. This is so of the marks of [`Plan`] from bytes. It is so too of the marks from a source of
/// which the first `extent` bytes, or all of them except for `from_head`, are a valid value.
#[inline(always)]
const fn take<D: ?Sized + Pun, C: Checks, const HEAD: bool>(
  bytes: &[u8],
  extent: usize,
) -> Option<(*const D, usize)> {
  // SAFETY: `bytes` is valid for reads of `bytes.len()` initialized bytes, which `front_by`
  // requires of `addr` and `n`, and which makes every byte that `try_cast_by` reads valid for
  // reads and initialized.
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
