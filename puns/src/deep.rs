//! Proofs of what the self-relative pointers of a target aim at.
//!
//! A self-relative pointer ([`Pointer`](crate::layout::Pointer)) guarantees that its target is
//! where it says, valid, and its own. This is not a property of its bytes. As a *source*, a pointer
//! is initialized bytes of which nothing more is known, so every function that reads (`as_bytes`,
//! `head_ref`, and so on) reinterprets a value that holds pointers as plain data. As a *target*, a
//! pointer is made only as this module and the [three rules](#rules-that-every-function-keeps) say.
//!
//! A [`Layout`](crate::layout::Layout) does not hold what a pointer aims at. The layout of `struct
//! Node { next: Opt<Node> }` would have no end. Const evaluation has no identity of a type or of a
//! static by which an analysis could tell that it has been somewhere before: `TypeId` cannot be
//! compared in `const`, nor can pointers, and a static reached through an associated constant is a
//! cycle. So what a pointer aims at is a type, `Pun::Pointee`. It is the element for a sequence,
//! and `()` for text. The layout keeps the size and the alignment of the target, which the compiler
//! gives without looking into the target.
//!
//! # The proof behind pointers
//!
//! [`view_ref`](crate::view_ref) runs the plan of `pun_ref` with pointers allowed. It then proves,
//! for every pointer of `D`, that the `X` that its source pointer aims at can be reinterpreted as
//! the `Y` that it aims at itself. `Y` must be the first bytes of `X`, every `X` must be valid for
//! them, and nothing may be left to check, since nothing can be checked behind a pointer. The
//! pointers of `Y` are then pointers of `X`, and so on. For a recursive type, this never ends as an
//! analysis. An instance of a generic function has an identity: it exists once, however often it is
//! named. So the proof is a set of instances, and it ends:
//!
//! - [`target::<X, Y>`](target) exists once for each pair of a type `X` that a pointer of the
//!   source aims at and a type `Y` that the pointer of the target at the same place aims at. Its
//!   `const` block is the whole proof of that pair up to the next pointers ([`behind`], the
//!   analysis of `X` as `Y`). It requires that every `X` is a valid `Y` over the bytes of `Y`, with
//!   nothing left to check, and that every pointer of `Y` is, in `X`, a pointer whose target can be
//!   reinterpreted as what the pointer of `Y` aims at ([`local`]).
//! - [`pair::<A, B>`](pair) names `target` for the pointers that `A` and `B` hold at the same
//!   place, through their fields, variants and elements. It names `target` as a function pointer
//!   that nothing calls. A recursive type thus closes a cycle of instances, which the compiler
//!   makes once. The proof is an induction over the pairs of types that can be met from `S` and
//!   `D`.
//!
//! [`view_ref`](crate::view_ref) and [`try_view_ref`](crate::try_view_ref) call [`proof`], and
//! through it `pair` over everything up to the first pointers. These are `const` blocks of code
//! that runs, which the language evaluates ([`r-expr.block.const.evaluation`][const evaluated]).
//! Behind a pointer, `target` is named and never called, since calling it would not end. Its
//! `const` block is evaluated because the compiler makes every function that the code it compiles
//! names. It does so whether that code runs or not, and whatever the optimizations. It has to build
//! the function pointer, and `rustc` collects what a function mentions before it optimizes, so that
//! errors of const evaluation do not depend on the optimization level.
//!
//! This is a behavior of the compiler. The Reference says that a `const` block that is not executed
//! may or may not be evaluated ([`r-expr.block.const.not-executed`][const not executed]). The
//! compile-fail test `view_behind_pointers` (a `u8` reinterpreted as a `bool` behind the pointers
//! of a recursive type) checks this behavior in a debug build. The workflow `model.yml` builds the
//! same program optimized, with LTO, every night, and it must be refused there too. Run that check
//! when `rust-toolchain` changes. The program declares its own pointer and needs no other crate.
//!
//! For the same reason, `view_ref` is not a `const fn`: a call in a constant would make no
//! instance, and an interpreter that evaluates only the constants of the functions that it runs, as
//! Miri does, would not refuse such a view. The refusal happens when the crate is built.
//!
//! # Pairing by position
//!
//! The compiler also makes the instances that a branch names when a constant rules the branch out.
//! So `pair` cannot choose what it names. It pairs by position: the fields of two structs and the
//! variants of two enums in the order of their declarations, and elements with elements. This names
//! more pairs than matter. Where the source has nothing, its side is `()`. `target` has nothing to
//! prove of a type of size 0, since no pointer aims at one ([`Pointer::one`],
//! [`Pointer::sequence`]), and the type that text aims at is `()`.
//!
//! `local`, which is `const` code, decides exactly which pairs matter. Every pointer of the target
//! must be, in the source, a pointer at the same offset that `pair` pairs it with, or the build
//! fails. So what `local` accepts, `pair` names, by the same walk of the same lists. A pair that is
//! named without mattering has one side without pointers.
//!
//! Those lists (`Fields`, `Variants`, `Elem`, `Pointee`) are types, written next to the layout that
//! the analysis reads. On its way, `local` checks them against the layouts ([`listed`]): as many
//! fields as the layout has, in the order of their declarations, with the type of the listed layout
//! where a field holds a pointer, the same variants by their tags, the same elements, and a target
//! of the size and the alignment that the pointer says. So an implementation of `Pun` whose list
//! leaves out a field, or gives a field that holds a pointer another type, cannot make a pointer be
//! missed, or taken for another (compile-fail test `lists_not_the_layout`). That the offset and the
//! layout of each listed field are those of the layout is taken from the `# Safety` of `Pun`, which
//! the derive meets by taking each field from the layout (`Layout::field`). Hence the limits, each
//! of which is a refusal:
//!
//! - A field that holds a pointer keeps its position among the fields of its struct.
//! - A variant that holds a pointer keeps its position among the variants of its enum, and new
//!   variants go last: a variant found by its tag at another position is refused (compile-fail test
//!   `view_variant_moved`).
//! - A struct that only holds a pointer is not that pointer.
//!
//! [`try_view_ref`](crate::try_view_ref) checks what is not proven of the value itself, with the
//! marks of the analysis. That may be a variant that the target does not have, a restricted scalar,
//! or a nullable pointer reinterpreted as one that is not nullable, whose bytes must not be all
//! zero. For what the pointers of the value aim at, it has the same requirements as `view_ref`.
//!
//! # Rules that every function keeps
//!
//! What a pointer guarantees is not a property of its bytes. Three rules follow, which every
//! function keeps.
//!
//! ## No bytes prove a pointer
//!
//! A pointer of the target is proven only by a pointer of the source at the same place, when what
//! that pointer aims at can be reinterpreted as what the pointer of the target aims at
//! ([`Pointer::reads_as`](crate::layout::Pointer::reads_as)). That pointer must have the same
//! scheme, the same offset (width and byte order), and the same kind of aim. Its target must be no
//! larger and need no more alignment. For one value, that is its size and alignment. For a
//! sequence, the elements must have the same size, and the declared alignment of the target must
//! not be greater than that of the source. For text, nothing more is needed. A zero offset in the
//! source needs a check where the pointer of the target may not be null, and zero is not a pointer
//! of the target: because zero is null in the source, or because the offset of the target is a
//! `NonZero`. `try_view_ref` then checks that the offset is not zero, and `view_ref` refuses the
//! pun. Anything else sets the `pointer` of the [`Relation`](crate::analysis::Relation), which no
//! function accepts and no check can replace. An enum that holds pointers is proven by an enum,
//! variant by variant, or not at all.
//!
//! ## Only a proof of what pointers aim at makes a value that holds one
//!
//! `view_ref` and `try_view_ref` prove what pointers aim at, and every other function refuses a
//! target that holds a pointer ([`Plan::gate`](crate::Plan::gate)). A crate that has pointers
//! checks bytes from outside with a walk of its own ([`validate_in`](crate::validate_in)), as
//! `nearest` does, since `puns` does not load a buffer.
//!
//! ## No `&mut` to a pointer, nor next to one
//!
//! [`mutable`](crate::analysis::mutable) and [`mutable_part`](crate::analysis::mutable_part)
//! refuse a source or a target that holds a pointer, wherever it is. Through a `&mut`, safe code
//! could write over a pointer, swap two pointers, or move one to where it aims at something else.
//!
//! # Why each public function is safe with pointers
//!
//! - `pun`, `head`, `tail`, `try_pun` and `zeroed` take and make `Copy` values. The target is
//!   gated ([Only a proof of what pointers aim at makes a value that holds one][only views]), and
//!   `zeroed` asserts it. A source that holds a pointer is reinterpreted as bytes.
//! - `pun_ref`, `head_ref`, `tail_ref` and `try_pun_ref` refuse a target that holds a pointer
//!   ([only views]). They read the bytes of a source that does, and make nothing of them.
//! - `pun_mut`, `head_mut`, `tail_mut` and `try_pun_mut` refuse a pointer on either side
//!   ([No `&mut` to a pointer, nor next to one][no mut]), after the target is gated ([only views]).
//! - `as_bytes` is `pun_ref::<S, [u8]>`. It reads the bytes of a pointer.
//! - `from_bytes`, `from_head` and `from_tail` are `try_pun_ref` from bytes (the plans `TRY_REF`
//!   and `FROM_TAIL`). They refuse the target ([only views]).
//! - `from_bytes_mut`, `from_head_mut` and `from_tail_mut` use the same plans, then `MUTABLE`.
//!   They refuse it ([only views], [no mut]).
//! - `from_bytes` and `from_head` of a view enum, which the derive writes, call `view::decide` for
//!   the type of each variant. It requires of that type what `from_bytes` does (the plan
//!   `TRY_REF`), so it refuses a pointer ([only views]).
//! - The getters of `#[pun(get(..))]` and `int` read an integer field of a `&self`. A pointer is
//!   not one (`Layout::as_integer`), so it fails to compile.
//! - The accessors of regions and `region` need `pun_ref::<S, [u8]>`, and return bytes of the tail.
//!   They read.
//! - The accessors of typed regions (`regions(name: P)`) call `from_bytes::<P>` on the bytes of the
//!   region. They refuse a pointer ([only views]).
//! - The rules (`size`, `len`, `assert`, `regions`) read unsigned integer fields, which a pointer
//!   is not (an error of the generated code), or bytes (`le`, `be`, `crc32`, and so on), which they
//!   read.
//! - `Tag`, `num::*`, `crc32` and `sum16` involve no pointer.
//! - `view_ref` and `try_view_ref` require each pointer of the target to be a pointer of the source
//!   ([No bytes prove a pointer](#no-bytes-prove-a-pointer)), prove what it aims at, and return a
//!   `&`.
//! - `validate_in` and `validate_elems_in` read bytes, and hand each pointer to a `Visit` by its
//!   position. They make no value of a type and follow no pointer. `pointers_in` and
//!   `pointers_elems_in` do the same with bytes that the caller promises hold a value, and check
//!   nothing. `copy_in` copies such a value except its pointers, which the caller writes. These
//!   three are `unsafe fn`, and their callers take on the requirements of their `# Safety`.
//! - `SelfRelative` and `self_relative!` are a declaration, whose promise is the `unsafe impl`. The
//!   macro writes `Pun` from it, and asserts what can be asserted
//!   ([Pointer layouts come from `SelfRelative`](#pointer-layouts-come-from-selfrelative)).
//! - `layout::*` holds values that describe layouts, for the code that the derive and
//!   `self_relative!` write. They make no value of a type, and a layout stands for a type only
//!   through `Pun`, an `unsafe trait`. `Pointer` is public, and made only by its constructors,
//!   since its fields are private.
//! - `hook::*`, behind the test-only feature `verdict`, runs the same plans and answers instead of
//!   enforcing. Its `view_ref` follows pointers a bounded number of times, which covers the types
//!   of the model check, for which it exists.
//!
//! # Invariants
//!
//! Each invariant is kept where it is stated.
//!
//! ## Pointer layouts come from `SelfRelative`
//!
//! A layout of a pointer is made only by `Layout::pointer`, which `self_relative!` calls for a
//! [`SelfRelative`](crate::SelfRelative) type, an `unsafe trait`. The type is the offset and
//! nothing else, which its size asserts. It aims at its `Target`, for which the description was
//! made, which the size and the alignment assert. That target is a type with a size
//! ([`Pointer::one`], [`Pointer::sequence`]), or `()` for text.
//!
//! ## Every unproven pointer is recorded
//!
//! The `pointer` of a [`Relation`](crate::analysis::Relation) is set for every pointer of a target
//! that a pointer of the source does not prove. `relate_range` sets it in three cases: a pointer of
//! the target where the source has no pointer whose target can be reinterpreted as its target, a
//! target that holds a pointer over a pattern of the source, and an enum of the target that holds a
//! pointer and is checked whole. `relate_enums` and `relate_enum_from` relate each variant in a
//! `Relation` of its own, and pass on the pointer that it records. Every plan refuses it, fallible
//! or not (`check` in `lib.rs`). Without the `unstable` feature, a part of the target whose layout
//! Rust leaves to the compiler is recorded as `unspecified` instead, which every plan refuses too.
//!
//! ## Only views make a value that holds a pointer
//!
//! Every plan but `VIEW` and `TRY_VIEW` refuses a target that holds a pointer
//! ([`Plan::gate`](crate::Plan::gate)). `Plan::check` and `Plan::check_tail` call it, and every
//! plan goes through one of them.
//!
//! ## No `&mut` with a pointer
//!
//! With a pointer on either side, [`mutable`](crate::analysis::mutable) returns `false`, and
//! [`mutable_part`](crate::analysis::mutable_part) returns a reason to refuse the pun.
//!
//! ## The views prove behind pointers
//!
//! `view_ref` and `try_view_ref` are the only functions whose plans accept a target that holds a
//! pointer ([Only views make a value that holds a
//! pointer](#only-views-make-a-value-that-holds-a-pointer)). Both call [`proof`] for the same pair
//! of types, and neither is a `const fn`.
//!
//! ## What `local` accepts, `pair` names
//!
//! Both walk the fields, the variants and the elements of the two types by position. [`local`]
//! refuses whatever is not at the position where [`pair`] looks for it, and a type whose lists are
//! not what its layout holds ([`listed`]).
//!
//! ## `reads_as` compares whole descriptions
//!
//! [`Pointer::reads_as`](crate::layout::Pointer::reads_as) relates only pointers of the same
//! scheme, width, byte order and kind of aim, with the numbers of their descriptions (sizes,
//! alignments, the declared alignment of a sequence). It says when nullability, or an offset of
//! zero reinterpreted as a `NonZero`, is to be checked. `Pointer::same`, by which two layouts are
//! equal, compares every field of the description.
//!
//! ## The contract of `SelfRelative`, and nothing more
//!
//! `puns` relies on the requirements of the `# Safety` section of
//! [`SelfRelative`](crate::SelfRelative#safety), and on nothing more. That section says which
//! requirement each proof of this module relies on.
//!
//! [only views]: #only-a-proof-of-what-pointers-aim-at-makes-a-value-that-holds-one
//! [no mut]: #no-mut-to-a-pointer-nor-next-to-one
//! [`Pointer::one`]: crate::layout::Pointer::one
//! [`Pointer::sequence`]: crate::layout::Pointer::sequence
//! [const evaluated]: https://doc.rust-lang.org/reference/expressions/block-expr.html#r-expr.block.const.evaluation
//! [const not executed]: https://doc.rust-lang.org/reference/expressions/block-expr.html#r-expr.block.const.not-executed

use crate::{
  Fields, Message, Pun, Variants,
  analysis::{Analysis, analyze},
  layout::{Field, Kind, Variant},
};

const LARGER: &str = "behind a pointer, the target is larger than what it is read from";
const UNSPECIFIED: &str = "behind a pointer, the pun depends on a part of the source or of the \
                           target whose layout Rust leaves to the compiler (a struct, a tuple or \
                           an enum of the `Rust` representation): \
                           only the same type at the same place proves it.\nAdd `#[repr(C)]` \
                           (or `#[repr(u8)]` to an enum), or enable the `unstable` feature of \
                           `puns`";
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

/// Fails with the message `why`, if there is one.
///
/// # Panics
///
/// Panics if `why` is `Some`. In const evaluation, the panic is a compile error.
const fn require(why: Option<&'static str>) {
  if let Some(why) = why {
    Message::new(why).panic();
  }
}

/// Proves what the pointers of `D` aim at, when an `S` is reinterpreted as a `D`.
///
/// [`view_ref`](crate::view_ref) and [`try_view_ref`](crate::try_view_ref) call it, after their
/// plans have related the bytes of `S` to `D`. It does nothing at run time.
///
/// # Compile-time errors
///
/// A call compiles only if all of the following hold:
///
/// - Every pointer of `D` is, in `S`, a pointer at the same place whose target can be reinterpreted
///   as what the pointer of `D` aims at ([`local`]).
/// - For every pair of types that [`pair`] names behind pointers, [`behind`] finds no reason to
///   refuse the pair.
///
/// These conditions are checked after monomorphization, as [the crate
/// documentation](crate#compile-time-errors) explains.
#[inline(always)]
pub fn proof<S: ?Sized + Pun, D: ?Sized + Pun>() {
  const { require(local::<S, D>(0)) };
  pair::<S, D>();
}

/// Holds the proof that an `X` that a pointer of the source aims at can be reinterpreted as the `Y`
/// that the pointer of the target at the same place aims at.
///
/// It is never called, only named by [`pair`]. Its `const` block is evaluated because the
/// compiler makes every function that compiled code names.
///
/// # Compile-time errors
///
/// An instance compiles only if `behind::<X, Y>(0)` returns `None`: an `X` can be reinterpreted as
/// a `Y`, without following their pointers further.
fn target<X: Pun, Y: Pun>() {
  const { require(behind::<X, Y>(0)) };
  pair::<X, Y>();
}

/// Names the [`target`] of every pair of pointers that `A` and `B`, at the same place, may hold.
///
/// It names the `target` of `A::Pointee` and `B::Pointee`, which has nothing to prove unless
/// both are pointers. If `B` holds a pointer, it then pairs by position: the fields of `A` and
/// `B` in the order of their declarations, their variants in the same order, and their elements.
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

/// Applies [`pair`] to the field `FB` of the target and the ones after it, each with the field
/// `FA` of the source at the same position.
#[inline(always)]
fn fields<FA: Fields, FB: Fields>() {
  if const { FB::FIELD.is_some() } {
    pair::<FA::Type, FB::Type>();
    fields::<FA::Next, FB::Next>();
  }
}

/// Applies [`fields`] to the variant `VB` of the target and the ones after it, each with the
/// variant `VA` of the source at the same position.
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
// With `fuel`, the next pointers are followed as well, that many times: this is how the model check
// asks what `view_ref` would do without failing to compile (`hook::view_ref`). The proof itself
// asks with none, since there the instances follow the pointers.

/// Returns why an `X` that a pointer aims at cannot be reinterpreted as a `Y`, or `None` if it can.
///
/// The first bytes of an `X` are reinterpreted as a `Y`. The pointers already showed that `Y` needs
/// no more alignment than `X`, and that the elements of two sequences have the same size. Every
/// value of `X` must be a valid `Y` with nothing left to check, since nothing is checked behind a
/// pointer. If `X` or `Y` has size 0, the pair is not a pair of pointers, and `None` is returned.
///
/// With `fuel` greater than 0, the pointers of `Y` are followed that many more times, as the model
/// check asks (`hook::views`). The proof asks with 0, and follows pointers through the instances of
/// [`target`].
///
/// # Errors
///
/// Returns the message of the first reason found: `Y` is larger than `X`, the pun depends on a
/// layout that Rust leaves to the compiler, a byte that `Y` requires to be initialized is padding
/// in `X`, a pointer of `Y` is not proven by a pointer of `X`, or some value of `X` is not a
/// valid `Y` or does not meet one of its rules. Then the reasons of [`local`].
pub const fn behind<X: Pun, Y: Pun>(fuel: usize) -> Option<&'static str> {
  // Not a pair of pointers: no pointer aims at a type of size 0 (`Pointer::one`,
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

/// Returns why a pointer of `B` is not, in `A`, the pointer that [`pair`] pairs it with, or
/// `None` if every pointer of `B` is.
///
/// `A` and `B` are at the same offset of a source and of a target. A pointer of `B` must be paired
/// with a pointer of `A` whose target can be reinterpreted as what the pointer of `B` aims at
/// ([`Pointer::reads_as`]). Fields are paired in the order of their declarations, and must be at
/// the same offset. Variants are paired in the order of their declarations, and must have the same
/// tag. Elements are paired only with elements of the same size. A variant of `B` whose tag no
/// variant of `A` has needs no pair, if the variant of `A` at its position holds no pointer. With
/// `fuel` greater than 0, what two paired pointers aim at is checked as well, by [`behind`] with
/// one less `fuel`.
///
/// # Errors
///
/// Returns a message if the lists of `A` or `B` are not what their layouts hold ([`listed`]), or
/// if a pointer of `B` is not paired with a pointer of `A` as above.
///
/// [`Pointer::reads_as`]: crate::layout::Pointer::reads_as
pub const fn local<A: ?Sized + Pun, B: ?Sized + Pun>(fuel: usize) -> Option<&'static str> {
  let (a, b) = (A::LAYOUT, B::LAYOUT);
  if !b.has_pointers() {
    return None;
  }
  // This walk and that of `pair` go over the lists of the two types. The lists must be what the
  // layouts hold, which the analysis used, or a pointer could be missed, or taken for another.
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

/// Applies [`local`] to each field of the target from `FB` on that holds a pointer, with the
/// field of the source at the same position.
///
/// # Errors
///
/// Returns a message if such a field of the target has no field of the source at the same
/// position and offset, or if [`local`] returns one for the two fields.
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

/// Applies [`local_fields`] to each variant of the target from `VB` on that holds a pointer, with
/// the variant of the source at the same position, from `VA` on.
///
/// `A0` is the first variant of the source. A variant of the target whose tag no variant of the
/// source has is never read through the target, so it needs no pair. It only has to be where
/// the variant of the source holds no pointer, since [`pair`] names nothing there that matters.
///
/// # Errors
///
/// Returns a message if a variant of the target that holds a pointer is not paired as above, or
/// if [`local_fields`] returns one for two paired variants.
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

/// Returns whether the lists of `T` agree with its layout, as far as pointers need.
///
/// The lists must be as many fields as the layout has, in the order of their declarations, its
/// variants with their tags, the type of its elements, and, for a pointer, the type it aims at
/// (`()` for text), of the size and alignment that the pointer says. The type of a field is
/// checked only if the field holds a pointer, and its offset not at all, which [`Pun`] guarantees.
/// Only `T` itself is checked. [`local`] checks the types further down.
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

/// Returns whether the list `F` is the `fields` from the one declared at `index` on.
///
/// The type that the list gives a field is checked only if the field holds a pointer.
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

/// Returns whether the list `V` is the `variants` from the one at `index` on: the same tags, in
/// the same order, with the same fields.
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

/// Returns whether a field of the list `F` holds a pointer.
const fn has_pointers<F: Fields>() -> bool {
  match F::FIELD {
    Some(field) => field.layout.has_pointers() || has_pointers::<F::Next>(),
    None => false,
  }
}

/// Returns whether a variant of the list `V` has the tag `tag`.
const fn has_tag<V: Variants>(tag: u128) -> bool {
  match V::TAG {
    Some(other) => other == tag || has_tag::<V::Next>(tag),
    None => false,
  }
}
