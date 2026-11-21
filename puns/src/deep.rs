//! What self-relative pointers aim at, proven through the types.
//!
//! A [`Layout`](crate::layout::Layout) does not hold what a pointer aims at: the layout of a
//! recursive type would have no end, and const evaluation has no identity of a type or of a
//! static (`TypeId` is not compared in `const`, pointers neither) by which an analysis could tell
//! that it has been somewhere before. Types have one: an instance of a generic function exists
//! once, however often it is named. So the proof that a `D` can be read where an `S` is,
//! pointers followed, is a set of instances:
//!
//! - `target::<X, Y>` ([`target`]), one per pair of types `X` (aimed at by a pointer of the
//!   source) and `Y` (aimed at by the pointer of the target at the same place). Its `const` block
//!   is the whole proof of that pair up to the next pointers ([`behind`]): every `X` is a valid
//!   `Y` over the bytes of `Y`, with nothing left to check, and every pointer of `Y` is, in `X`,
//!   a pointer of which it can read the target ([`local`]);
//! - `pair::<A, B>` ([`pair`]) names `target` for the pointers that `A` and `B` hold at the same
//!   place, through their fields, variants and elements. It *names* it (a function pointer that
//!   nothing calls), so a recursive type closes a cycle of instances, which the compiler makes
//!   once: an induction over the pairs of types that can be met from `S` and `D`.
//!
//! [`proof`] is called by [`view_ref`](crate::view_ref), and with it everything up to the first
//! pointers: `const` blocks of functions that run. Behind a pointer, the `const` block of
//! `target` is evaluated because the compiler makes every function that code it compiles names,
//! whether that code ever runs or not: this is what the build of a function pointer needs, and
//! what `rustc` does whatever the optimizations (compile-fail tests hold it to that).
//!
//! `pair` cannot choose what it names: the compiler also makes the instances that a branch names
//! when a constant rules the branch out. So it pairs by position (the fields of two structs and
//! the variants of two enums in the order of their declarations, elements with elements), which
//! names more pairs than matter: where the source has nothing, its side is `()`, and `target`
//! has nothing to prove of what has no size (no pointer aims at such a type: [`Pointer::one`],
//! [`Pointer::sequence`]; the target type of text is `()`).
//! The pairs that matter are decided exactly by `local`, which is `const` code: every pointer of
//! the target must be, in the source, a pointer at the same offset that `pair` pairs it with,
//! or the build fails. Hence two limits, which are refusals: a field that holds a pointer keeps
//! its position among the fields of its struct, a variant that does keeps its position among
//! the variants of its enum (new ones go last), and a struct that only holds a pointer is not
//! that pointer.
//!
//! [`Pointer::one`]: crate::layout::Pointer::one
//! [`Pointer::sequence`]: crate::layout::Pointer::sequence

use crate::{
  Fields, Message, Pun, Variants,
  analysis::{Analysis, analyze},
  layout::{Field, Kind, Variant},
};

const LARGER: &str = "behind a pointer, the target is larger than what it is read from";
const UNSPECIFIED: &str = "behind a pointer, the pun depends on a part of the source or of the \
                           target whose layout Rust does not keep from one compilation to the \
                           next (a struct or a tuple of the `Rust` representation, an enum \
                           without `repr`): only the same type at the same place proves \
                           it.\nAdd `#[repr(C)]` (or `#[repr(u8)]` to an enum), or enable the \
                           `unstable` feature of `puns`";
const UNINIT: &str = "behind a pointer, a byte that the target requires to be initialized is \
                      padding in what it is read from";
const UNPROVEN: &str = "behind a pointer, a pointer of the target is not a pointer of the source \
                        whose target it can read: only a pointer proves a pointer";
const UNCHECKED: &str = "behind a pointer, some values of the source are not valid for the \
                         target, or do not prove one of its rules: nothing is checked behind a \
                         pointer.\nGive the target a type that every value of the source is valid \
                         for";
const UNPAIRED: &str = "a pointer of the target is not, in the source, a pointer of the same \
                        scheme and offset that aims at the same kind of thing (a value, a \
                        sequence, text): only a pointer proves a pointer";
const FIELDS: &str = "a field of the target that holds a pointer is not paired with a field of \
                      the source at the same offset.\nFields are paired in the order of their \
                      declarations: give both structs the same fields, in the same order, up to \
                      the last one that holds a pointer";
const VARIANTS: &str = "a variant of the target that holds a pointer is not paired with the \
                        variant of the source of the same tag.\nVariants are paired in the order \
                        of their declarations: keep the variants of the source where they are, and \
                        add new ones after them";
const ELEMENTS: &str = "elements of the target that hold a pointer are not paired with elements \
                        of the source of the same size";
const LISTS: &str = "an implementation of `Pun` lists fields, variants, elements or a target of a \
                     pointer (`Fields`, `Variants`, `Elem`, `Pointee`) that are not those of its \
                     layout: pointers cannot be followed through it";

/// Fail to compile if `why` says why.
const fn require(why: Option<&'static str>) {
  if let Some(why) = why {
    Message::new(why).panic();
  }
}

/// The proof of what the pointers of `D` aim at, when a `D` is read where an `S` is: called by
/// [`view_ref`](crate::view_ref), whose plan proved the bytes of `S` as a `D`.
#[inline(always)]
pub fn proof<S: ?Sized + Pun, D: ?Sized + Pun>() {
  const { require(local::<S, D>(0)) };
  pair::<S, D>();
}

/// The `X` that a pointer of the source aims at is read as a `Y`, which the pointer of the target
/// at the same place aims at. Never called: see the module documentation.
fn target<X: Pun, Y: Pun>() {
  const { require(behind::<X, Y>(0)) };
  pair::<X, Y>();
}

/// Names the [`target`] of every pair of pointers that `A` and `B`, at the same place, may hold.
#[inline(always)]
fn pair<A: ?Sized + Pun, B: ?Sized + Pun>() {
  // Named, not called: this is what makes a recursive type a finite proof.
  let _: fn() = target::<A::Pointee, B::Pointee>;
  if const { B::LAYOUT.has_pointers() } {
    fields::<A::Fields, B::Fields>();
    variants::<A::Variants, B::Variants>();
    pair::<A::Elem, B::Elem>();
  }
}

#[inline(always)]
fn fields<FA: Fields, FB: Fields>() {
  if const { FB::FIELD.is_some() } {
    pair::<FA::Type, FB::Type>();
    fields::<FA::Next, FB::Next>();
  }
}

#[inline(always)]
fn variants<VA: Variants, VB: Variants>() {
  if const { VB::TAG.is_some() } {
    fields::<VA::Fields, VB::Fields>();
    variants::<VA::Next, VB::Next>();
  }
}

// -------------------------------------------------------------------------------------------------
// What is proven of a pair of types, up to the next pointers: `const` code, exact.
//
// With `fuel`, the next pointers are followed as well, that many times: this is how the model
// check asks what `view_ref` would do without failing to compile (`hook::view_ref`). The proof
// itself asks with none, the instances being what follows pointers there.

/// Why an `X` that a pointer aims at cannot be read as a `Y`, if it cannot: a `Y` is the first
/// bytes of an `X` (the pointers themselves told that it is not more aligned, and that elements
/// of sequences have the same size), every value of which is valid for it, with nothing left to
/// check: nothing is checked behind a pointer.
pub const fn behind<X: Pun, Y: Pun>(fuel: usize) -> Option<&'static str> {
  // Not a pair of pointers: no pointer aims at a type without size (`Pointer::one`,
  // `Pointer::sequence`), and the target type of text is `()`.
  if X::LAYOUT.size() == 0 || Y::LAYOUT.size() == 0 {
    return None;
  }
  let Analysis { fit, relation, .. } = analyze(&X::LAYOUT, &Y::LAYOUT);
  if !fit.shrink {
    return Some(LARGER);
  }
  if relation.unspecified.is_some() {
    return Some(UNSPECIFIED);
  }
  if relation.uninit.is_some() {
    return Some(UNINIT);
  }
  if relation.pointer.is_some() {
    return Some(UNPROVEN);
  }
  if relation.disjoint.is_some() || relation.partial.is_some() {
    return Some(UNCHECKED);
  }
  local::<X, Y>(fuel)
}

/// Why a pointer of `B` is not, in `A` at the same place, the pointer that [`pair`] pairs it
/// with, if one is not: `A` and `B` are at the same offset of a source and of a target.
pub const fn local<A: ?Sized + Pun, B: ?Sized + Pun>(fuel: usize) -> Option<&'static str> {
  let (a, b) = (A::LAYOUT, B::LAYOUT);
  if !b.has_pointers() {
    return None;
  }
  // This walk, and the one of `pair`, are over the lists of the two types: they are what the
  // layouts hold, which the analysis went by, or a pointer could be missed, or taken for another.
  if !listed::<A>() || !listed::<B>() {
    return Some(LISTS);
  }
  match b.kind {
    Kind::Pointer(to) => match a.kind {
      Kind::Pointer(from) if from.reads_as(&to).is_some() => {
        if fuel == 0 {
          None
        } else {
          behind::<A::Pointee, B::Pointee>(fuel - 1)
        }
      }
      _ => Some(UNPAIRED),
    },
    Kind::Struct(..) => local_fields::<A::Fields, B::Fields>(fuel),
    Kind::Enum(_) => local_variants::<A::Variants, A::Variants, B::Variants>(fuel),
    Kind::Array(..) | Kind::Slice(_) => {
      let elements = matches!(a.kind, Kind::Array(..) | Kind::Slice(_))
        && <A::Elem as Pun>::LAYOUT.size() == <B::Elem as Pun>::LAYOUT.size();
      if elements { local::<A::Elem, B::Elem>(fuel) } else { Some(ELEMENTS) }
    }
    Kind::Scalar(_) | Kind::Pattern(_) => None,
  }
}

/// The field `FB` of the target, and the ones after it, with the field `FA` of the source and
/// the ones after it.
const fn local_fields<FA: Fields, FB: Fields>(fuel: usize) -> Option<&'static str> {
  let Some(b) = FB::FIELD else { return None };
  if b.layout.has_pointers() {
    match FA::FIELD {
      Some(a) if a.offset() == b.offset() => {
        if let Some(why) = local::<FA::Type, FB::Type>(fuel) {
          return Some(why);
        }
      }
      _ => return Some(FIELDS),
    }
  }
  local_fields::<FA::Next, FB::Next>(fuel)
}

/// The variant `VB` of the target, and the ones after it, with the variant `VA` of the source,
/// whose first variant is `A0`, and the ones after it.
const fn local_variants<A0: Variants, VA: Variants, VB: Variants>(
  fuel: usize,
) -> Option<&'static str> {
  let Some(tag) = VB::TAG else { return None };
  if has_pointers::<VB::Fields>() {
    match VA::TAG {
      Some(other) if other == tag => {
        if let Some(why) = local_fields::<VA::Fields, VB::Fields>(fuel) {
          return Some(why);
        }
      }
      // A variant that the source never is, is never read through the target: it only has to
      // be where `pair` names nothing that matters, which is where the source has no pointer.
      _ if !has_tag::<A0>(tag) && !has_pointers::<VA::Fields>() => {}
      _ => return Some(VARIANTS),
    }
  }
  local_variants::<A0, VA::Next, VB::Next>(fuel)
}

/// Whatever the lists of `T` are what its layout holds, as [`Pun`] requires: its fields in the
/// order of their declarations, its variants, the type of its elements, and the size and the
/// alignment of what it aims at if it is a pointer (of `()` for text). One level: [`local`] asks
/// again further down.
const fn listed<T: ?Sized + Pun>() -> bool {
  match T::LAYOUT.kind {
    Kind::Struct(fields, _) => listed_fields::<T::Fields>(fields, 0),
    Kind::Enum(layout) => listed_variants::<T::Variants>(layout.variants, 0),
    Kind::Array(elem, _) | Kind::Slice(elem) => <T::Elem as Pun>::LAYOUT.equals(elem),
    Kind::Pointer(pointer) => {
      let target = <T::Pointee as Pun>::LAYOUT;
      target.size() == pointer.size && target.align() == pointer.align
    }
    Kind::Scalar(_) | Kind::Pattern(_) => true,
  }
}

/// The list `F` is the `fields` from the one declared at `index` on, each of the type that the
/// list says where it holds a pointer.
const fn listed_fields<F: Fields>(fields: &[Field], index: usize) -> bool {
  match F::FIELD {
    None => index == fields.len(),
    Some(field) => {
      index < fields.len()
        && field.index == index
        && (!field.layout.has_pointers() || <F::Type as Pun>::LAYOUT.equals(field.layout))
        && listed_fields::<F::Next>(fields, index + 1)
    }
  }
}

/// The list `V` is the `variants` from the one at `index` on: the same tags, in the same order,
/// with the same fields.
const fn listed_variants<V: Variants>(variants: &[Variant], index: usize) -> bool {
  match V::TAG {
    None => index == variants.len(),
    Some(tag) => {
      index < variants.len()
        && variants[index].tag == tag
        && listed_fields::<V::Fields>(variants[index].fields(), 0)
        && listed_variants::<V::Next>(variants, index + 1)
    }
  }
}

const fn has_pointers<F: Fields>() -> bool {
  match F::FIELD {
    Some(field) => field.layout.has_pointers() || has_pointers::<F::Next>(),
    None => false,
  }
}

const fn has_tag<V: Variants>(tag: u128) -> bool {
  match V::TAG {
    Some(other) => other == tag || has_tag::<V::Next>(tag),
    None => false,
  }
}
