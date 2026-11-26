//! Run-time checks of what the analysis could not prove.
//!
//! The `try_` and `from_` functions recurse over types, not over values, in `const fn`s, through
//! associated types: the element of an array, the list of the fields of a struct. Each call is an
//! instance of its own, in which the layout of the type and the marks of the analysis are
//! constants. Each check sits behind an `if const { .. }` on the marks. A check that is not marked
//! is not compiled, so what the analysis proved never reaches code generation. A marked check of a
//! scalar is a load and a comparison. A marked check of a pattern, an enum or a rule runs the
//! automaton, reads the tag, or computes the rule, with its checksums and walks.
//! `examples/codegen.rs` records the machine code that this gives.
//!
//! `from_head` turns the value of a `size` or `len` rule into a number of elements in a `usize`.
//! There, the optimizer knows that the value of a field fits a `usize`.
//!
//! # Aims
//!
//! A rule of a struct may be an aim ([`layout::Aim`]), whose pointee type is the `Pointee` of its
//! item in `Pun::Rules`. [`check_rules`] checks each aim in the order of the rules
//! ([`check_aim`]): it places the pointee at the cursor of the tail ([`check_next`],
//! [`cursor_step`]) or within the bytes that the check reads ([`Input`], [`find_stored`],
//! [`stored_at`]), and, if the aim is eager, checks what the pointee stores before its value,
//! measures the value with its length ([`measure`]), and checks it ([`check_value`]) with
//! [`validate`], which checks the eager aims of the pointee in turn. An aim at a stored place is
//! eager. A lazy aim, at the cursor, requires that its pointee fits in the tail.
//!
//! The `try_` and `from_` functions check a pointee in place, nested in the check of the value that
//! holds its aim, bounded by the bytes and by the parts of the layouts that [`Input`] counts
//! ([`AIM_PARTS`]), which bound the stack. A pointee whose type holds an eager aim, and has an
//! identity, is handed over to a loop of its type ([`run`]) where the check in place would stop
//! for its parts, and where it is a link of a chain nested past [`IN_PLACE`] parts
//! ([`check_value`], [`waits`]): the check goes on in one pass over the bytes, from where it
//! stands, with one frame for each type on the path, whatever the data. In the value of a loop,
//! every such pointee waits for a loop: the first pointee of its type is the next one that the
//! loop checks, and the others wait in entries, a list for each loop; a pointee whose type has a
//! loop lower on the path waits for that loop ([`Loops::wait_below`]). So a chain takes no entry, a
//! tree takes entries as it is deep, and data that alternates types takes the frames of their
//! loops. The pointees that hold no eager aim, those of a type without an identity, and those that
//! find no entry are checked in place: a type without an identity that points at itself is checked
//! by a recursive call of the same instance, and stops at [`AIM_PARTS`]. A pointee that holds no
//! eager aim, or an empty slice, takes no parts, since it nests no other pointee.
//!
//! The pointee of an aim at a stored place is claimed where it is found ([`Input::claim`]): it
//! starts at or after the end of the claim before it, the cursor, so the pointees form a tree in
//! the order of a copy. A pointee that is checked where it is found has its own pointees claimed
//! right after it, as a copy writes them. One that waits for a loop has them claimed later, in a
//! sequence of their own, which must lie in its window: from the end of its claim to the start of
//! the next claim of the sequence that claimed it, which that claim writes where it waits, or else
//! to the end of the window of that sequence ([`run`], which says when a sequence is over). The
//! cursor at the end of a sequence is compared with the end of its window once, there, and not at
//! each claim. A pointee at the cursor, or one that ends with the tail of its struct, is within the
//! struct and claims nothing, so it never waits, which would give it a window without a claim. The
//! pointee of a shared aim is checked with [`Shared`], under which nothing is claimed and nothing
//! waits. What is claimed shares no byte, so the claims bound the time of a check; a pointee at
//! the cursor is within its struct, which no other pointee at that cursor shares, and so is one
//! that ends with the tail of a struct that holds no other eager aim; what other aims may reach
//! takes its size from a budget ([`Input`]).
//!
//! # Where a check fails
//!
//! A check returns where it found a part that is not valid, as its address, or null if every part
//! that it checks is valid ([`Fault`]): a scalar, a pattern, an enum whose tag is the tag of no
//! variant, a union none of whose fields fits, a struct one of whose rules does not hold or one of
//! whose aims leads to no pointee that fits (out of the bytes, not aligned, of no size of its type,
//! or before the end of the claims), or a pointee whose own pointees end past its window. Where a
//! variant, a field or a pointee is not valid, it is the part within it that is not. The functions
//! that check bytes turn the address into the place of [`Error::Validity`]. It is returned, not
//! written into [`Input`]: a check of a type without eager aims then reads and writes nothing of
//! `Input`, which the optimizer drops, where a write on each failure made the check of an IPv6
//! packet build it, 19 instructions more. Elements checked in blocks keep no address, so that a
//! block has no branch, and a block that fails is checked again one element at a time.
//!
//! [`Error::Validity`]: crate::Error::Validity
//!
//! # The stack of a check
//!
//! A check nests the frames of the validation of each pointee that it checks in place in those of
//! the value that points at it, and the frames of each loop in those of the value whose pointee
//! started it, so its stack grows with the parts of the layouts that it nests, which
//! [`AIM_PARTS`] bounds in place, and again in each loop, and with the size of those frames. In a build without optimizations, a
//! frame holds every local of its function, and of each function inlined into it, while the
//! functions that it calls run. So the functions on that path ([`validate`], [`validate_fields`],
//! [`validate_variants`], [`check_listed`] and the check of an aim) hold no copy of a layout, a
//! field or a rule: they take what they need from constants, and the checks that read a layout
//! ([`validate_scalar`], [`validate_pattern`], [`tag_of`], [`validate_restricted`], [`has_tag`])
//! and every function that they inline are functions of their own without optimizations
//! (`cfg_attr(not(debug_assertions), inline(always))`), whose frames are on the stack only while
//! they run. With optimizations, they are all inlined, but the first loop of a check, which holds
//! the entries of the loops ([`start`]), so that a check that takes no loop does not hold them.
//! [`AIM_PARTS`] gives the figures.
//!
//! What reads a pointee finds it as the check does: [`region_at`] takes the same steps of the
//! cursor ([`cursor_step`]), which truncate rather than fail, [`pointer_at`] and [`value_of`]
//! evaluate where a pointee at a stored place is, and [`own_length`] reads the length that a
//! pointee stores: a `Thin` before its value, or a value in its sized fields, over which the check
//! and the read evaluate it alike (`Layout::stores_length`).
//!
//! # No interpreter
//!
//! Every operation of every rule is applied by an instance of a function of its own ([`steps`]), in
//! which the operation is a constant. What is compiled is the arithmetic of the expression and the
//! loop of each walk, never a `match` on operations at run time.
//!
//! The derive writes the rules of a struct that have an expression or are aims as a list at the
//! type level (`Pun::Rules`, `RuleAt<RULE, STEPS, NEXT, POINTEE>`, where `POINTEE` is the type of
//! the pointee of an aim). It writes the operations of each rule as a balanced tree
//! (`steps::One<I>`, `steps::Two<A, B>`), and a walk as `steps::Walk<I, COND, NEXT>`, whose two
//! expressions are trees of their own at any depth. The recursions down a tree are as deep as the
//! logarithm of the number of operations. Because the list is a type, a struct without rules costs
//! nothing more to compile.
//!
//! Nothing that a list says is trusted. Where a list is used, it is checked against the layout
//! ([`listed`], [`covers`]). Every rule with an expression must be listed once and in order, with
//! every operation of each, and with a walk exactly where the expression has one. The functions
//! name every part of the tree whatever its kind. Where a part is not a walk, they name the
//! expression that it is in again, which adds nothing new.
//! [`Machine::apply`](layout::Machine::apply) never sees a walk. `from_head` also finds a `size`
//! rule of the last field through the types (`Fields::Last`), and checks that it is the layout's.
//!
//! # The machine
//!
//! The machine that applies the operations ([`Machine`]) has a stack of 16 values and 8 variables.
//! Each is a variable of the machine, not an element of an array, reached by a `match` whose cases
//! are all values. The last case is the default case, because a `match` with one more case costs
//! instructions. Once inlined, each slot of the stack is a local variable. An expression has at
//! most 256 operations, and nests at most 4 walks. The derive says where an expression goes beyond
//! a limit, and by how much, and [`check_ops`](layout::check_ops) checks the limits again. In a
//! debug build, where nothing is inlined, each operation is a call.
//!
//! The derive puts an [`Op::Shortcut`] after the left side of a `&&` or a `||`, and an [`Op::Then`]
//! and an [`Op::Else`] before the branches of an `if`. They skip what follows when Rust would not
//! compute it. `check_ops` checks that what they skip is exactly a right side or a branch, so that
//! the stack has the same depth after it either way. So `protocol != 17 || be(..) == 0` holds for a
//! packet that is not UDP, however short, and costs nothing more for it.
//!
//! What is not computed is told by an index (the `skip` of [`Machine`]). The operations before it
//! are not applied. A `Shortcut` that decides sets the index past the right side. Where the
//! operations are constants, so is the index on each path through the code, and the optimizer makes
//! jumps of it. No test is left where nothing is skipped. The alternative, a flag on each value of
//! the stack that tells whether it failed, makes every operation carry failures along, and computes
//! the side of a `||` that is not needed.
//!
//! Nothing that is computed can be left out of the value of an expression, so its first failure is
//! its failure: the evaluator stops there, and what follows is compiled knowing that nothing
//! failed.

use core::{marker::PhantomData, mem::MaybeUninit};

use crate::{
  Fields, Path, Pointee, Pun, Root, Rules, Steps, Variants,
  analysis::Marks,
  layout::{
    self, Aim, Check, Dfa, Extent, Id, Kind, Machine, Op, Rule, Shape, Start, load, op_at,
    walk_limit,
  },
  narrow::{Planned, fit},
  ptr,
  steps::{InRule, ONE, Ops, TWO, WALK},
};

/// The parts of a target that a validation checks.
pub trait Checks {
  /// The leaves to check (scalars, patterns, enums, unions and rules), by index in the target.
  const MARKS: Marks;
  /// The type of the pointees of the loop ([`run`]) whose value this checks, or
  /// [`PointeeType::NONE`], which is no type: a pointee of that type waits for the loop
  /// ([`waits`]).
  const LOOP: PointeeType = PointeeType::NONE;
  /// The type of the pointee whose value this checks in place ([`InPlace`]), or
  /// [`PointeeType::NONE`]: a pointee of that type within it is nested in a value of its type.
  const MARK: PointeeType = PointeeType::NONE;
  /// Whether this checks a value within the pointee of a shared aim ([`Shared`]), where nothing is
  /// claimed, a constant so that no other check compiles the test.
  const SHARED: bool = false;
  /// What checks every part, in the same loop: an enum, a union, a pointee that holds an eager
  /// aim, and what a pointee stores before its value. An associated type, so that only its
  /// instances are compiled: a choice by a constant compiles both.
  type Whole: Checks;
}

/// The [`Checks`] that check every part.
pub struct All;

impl Checks for All {
  const MARKS: Marks = Marks::ALL;
  type Whole = Self;
}

/// The [`Checks`] of the value of a pointee `P` in the loop of its type ([`run`]): every part, and
/// the pointees of that type wait for the loop.
pub struct Looped<P: ?Sized>(PhantomData<P>);

impl<P: ?Sized + Pointee> Checks for Looped<P> {
  const MARKS: Marks = Marks::ALL;
  const LOOP: PointeeType = PointeeType {
    key: P::POINTEE_ID.key(),
    id: &P::POINTEE_ID,
    size: P::POINTEE_SIZE,
    align: P::POINTEE_ALIGN,
  };
  type Whole = Self;
}

/// The [`Checks`] of the value of a pointee `P` that is a link of a chain ([`links`]), checked in
/// place: every part, and a pointee of its type within it is a link nested in another
/// ([`check_value`]).
pub struct InPlace<P: ?Sized>(PhantomData<P>);

impl<P: ?Sized + Pointee> Checks for InPlace<P> {
  const MARKS: Marks = Marks::ALL;
  const MARK: PointeeType = Looped::<P>::LOOP;
  type Whole = Self;
}

/// The [`Checks`] of the pointee of a shared aim and of what it reaches: every part, in place,
/// and nothing claimed ([`check_stored`]), since other aims may reach it.
pub struct Shared;

impl Checks for Shared {
  const MARKS: Marks = Marks::ALL;
  const SHARED: bool = true;
  type Whole = Self;
}

/// Where a check failed: the address of the part that is not valid, or null if every part that
/// it checks is valid ("Where a check fails" in the module documentation).
pub type Fault = *const u8;

/// No failure: every part that a check checks is valid.
pub const VALID: Fault = core::ptr::null();

/// Returns where a part of the `T` at `ptr` is not valid ([`Fault`]): [`validate`] with
/// `C::Whole`, as the check of an enum, a union and a pointee checks them: [`All`], or [`Looped`]
/// or [`InPlace`] in the value of a pointee.
///
/// # Safety
///
/// The caller must ensure the requirements of [`validate`], with every part marked.
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn validate_whole<C: Checks, T: ?Sized + Pun>(
  ptr: *const T,
  input: &mut Input,
) -> Fault {
  // SAFETY: The requirements of `validate` are those of this call, and `C::Whole` marks every
  // part.
  unsafe { validate::<C::Whole, Root, T>(ptr, input) }
}

/// Returns where a part of the `T` at `ptr` that `C` marks is not valid ([`Fault`]).
///
/// `P` is where `T` is in the target, which tells the indexes of its leaves. Parts that `C` does
/// not mark are not checked, and their checks are not compiled. An enum and a union are checked
/// as a whole once one of their leaves is marked.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `ptr` is valid for reads of a `T`. It need not be aligned.
/// - Every byte that a check marked by `C` reads is initialized. A marked scalar reads its own
///   bytes. A marked pattern reads its tail. An enum or a union with a marked
///   leaf reads its tag and what the checks of its variants read, which check every part. Nothing
///   is read of a variant, or of a field of a union, that has no value. A struct whose rules are
///   marked reads the fields that its rules load and, if one of its rules reads bytes by offset
///   ([`Layout::rules_read_bytes`]), all of its bytes, padding included.
///
/// It is inlined where the compiler can: the check of eager aims calls it once for each pointee,
/// and that call of its own made the check of a tree of pointers about 13% slower.
///
/// [`Layout::rules_read_bytes`]: layout::Layout::rules_read_bytes
#[cfg_attr(not(debug_assertions), inline)]
pub const unsafe fn validate<C: Checks, P: Path, T: ?Sized + Pun>(
  ptr: *const T,
  input: &mut Input,
) -> Fault {
  if const { !C::MARKS.any(P::LEAF, T::LAYOUT.leaves) } {
    return VALID;
  }
  // Where a part of `T` itself is not valid: a scalar, a pattern, an enum, a union or a struct.
  let here = ptr.cast::<u8>();
  // SAFETY: `ptr` is valid for reads of a `T`, and every byte that a marked check reads is
  // initialized, by contract on the caller. A leaf of `T` is marked, since the function did not
  // return above. `T::LAYOUT` is the layout of `T`, and `T::Elem`, `T::Fields` and `T::Variants`
  // are its lists (`Pun`, section Safety). For each shape:
  // - A scalar is `T` itself, of `spec.size` bytes, and is marked. So its bytes are initialized,
  //   as `Spec::matches` requires.
  // - An array or a slice is `len` or `ptr::metadata(ptr)` elements of type `T::Elem`, one after
  //   the other, each valid for reads. Its elements share their leaves, so what is marked in one
  //   is marked in each, as `validate_elems` requires.
  // - A struct meets the requirements of `validate_fields`, since `T::Fields` are its fields. Its
  //   rules are checked only if they are marked, and then the bytes that they read are
  //   initialized, as `check_rules` requires.
  // - A pattern is its tail of `ptr::metadata(ptr)` bytes, which are marked, so initialized, as
  //   `Dfa::matches_utf8` and `Dfa::matches` require.
  // - An enum or a union has a marked leaf, so its tag and what the checks of its variants read
  //   are initialized, by contract on the caller. These are the requirements of `validate_enum`,
  //   with `layout` the layout of `T`, and of `validate_union`, with `T::Variants` the list of
  //   its variants.
  //
  // The shapes whose checks hold the layout, a scalar, a pattern and an enum, are checked in
  // functions of their own, which take it from `T`: a value of it would be a local of this
  // function, which is on the stack while a field or a pointee is checked, in a build without
  // optimizations ("The stack of a check" in the module documentation).
  unsafe {
    match const { T::LAYOUT.shape() } {
      Shape::Scalar(_) => fails(validate_scalar::<T>(ptr), here),
      Shape::Array(len) => validate_elems::<C, P, T::Elem>(ptr.cast(), len, input),
      Shape::Slice => validate_elems::<C, P, T::Elem>(ptr.cast(), ptr::metadata(ptr), input),
      Shape::Struct => {
        let fault = validate_fields::<C, P, T, T::Fields>(ptr, input);
        if !fault.is_null() || const { !rules_marked::<C, P, T>() } {
          fault
        } else {
          check_rules::<C, P, T>(ptr, input)
        }
      }
      Shape::Pattern(_) => fails(validate_pattern::<T>(ptr), here),
      Shape::Enum(layout) if layout.union => {
        fails(validate_union::<C, T, T::Variants>(ptr, input), here)
      }
      Shape::Enum(_) => validate_enum::<C, T>(ptr, input),
    }
  }
}

/// Returns [`VALID`] if `valid`, or else `at`, where the part that is not valid is.
#[cfg_attr(not(debug_assertions), inline(always))]
const fn fails(valid: bool, at: *const u8) -> Fault {
  if valid { VALID } else { at }
}

/// Returns whether the scalar `T` at `ptr` has one of its values.
///
/// # Safety
///
/// The caller must ensure that `T` is a scalar, and that `ptr` is valid for reads of a `T`, whose
/// bytes are initialized. It need not be aligned.
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn validate_scalar<T: ?Sized + Pun>(ptr: *const T) -> bool {
  // `None` for another shape, of which this function is never called, though `validate`, which
  // calls it, compiles it for every shape.
  let Some(spec) = (const {
    match T::LAYOUT.shape() {
      Shape::Scalar(spec) => Some(spec),
      _ => None,
    }
  }) else {
    return false;
  };
  // SAFETY: `spec` is the scalar of `T`, of `spec.size` bytes, which are initialized and valid for
  // reads, by contract on the caller, as `Spec::matches` requires.
  unsafe { spec.matches(ptr.cast()) }
}

/// Returns whether the pattern `T` at `ptr` matches its automaton.
///
/// # Safety
///
/// The caller must ensure that `T` is a pattern, and that `ptr` is valid for reads of a `T`, whose
/// tail of `ptr::metadata(ptr)` bytes is initialized.
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn validate_pattern<T: ?Sized + Pun>(ptr: *const T) -> bool {
  // `None` for another shape, as in `validate_scalar`.
  let Some((dfa, utf8)) = (const {
    match T::LAYOUT.shape() {
      Shape::Pattern(dfa) => Some((dfa, dfa.is_utf8())),
      _ => None,
    }
  }) else {
    return false;
  };
  // SAFETY: The tail of `ptr::metadata(ptr)` bytes is initialized and valid for reads, by contract
  // on the caller, as `Dfa::matches_utf8` and `Dfa::matches` require. `str::from_utf8` checks the
  // bytes where the automaton is that of UTF-8, by its states, since an `id` proves nothing. This
  // is decided at compile time.
  unsafe {
    if utf8 {
      Dfa::matches_utf8(ptr.cast(), ptr::metadata(ptr))
    } else {
      dfa.matches(ptr.cast(), ptr::metadata(ptr))
    }
  }
}

/// Returns where the enum at `ptr` is not valid ([`Fault`]): the enum, if it has the tag of none
/// of its variants, or else where the fields of that variant are not.
///
/// Every part of the variant is checked, since nothing is known of what is in an enum.
///
/// # Panics
///
/// Panics if the tag of `layout` is not a scalar. No layout made by this crate has such a tag.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `ptr` is valid for reads of a `T`. It need not be aligned.
/// - `T` is an enum.
/// - The tag of the enum is initialized, and so is every byte that the checks of the fields of that
///   tag's variant read, which check every part.
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn validate_enum<C: Checks, T: ?Sized + Pun>(
  ptr: *const T,
  input: &mut Input,
) -> Fault {
  // SAFETY: `T` is an enum, by contract on the caller, and its tag is initialized, as `tag_of`
  // requires. What the checks of the fields of the variant of that tag read is initialized, by
  // contract on the caller, as `validate_variants` requires, with `T::Variants` the list of the
  // variants of `T`.
  unsafe { validate_variants::<C, T, T::Variants>(ptr, tag_of::<T>(ptr), input) }
}

/// Returns the tag of the enum `T` at `ptr`.
///
/// # Panics
///
/// Panics if the tag of the layout of `T` is not a scalar. No layout made by this crate has such
/// a tag.
///
/// # Safety
///
/// The caller must ensure that `T` is an enum, and that `ptr` is valid for reads of a `T` whose tag
/// is initialized. It need not be aligned.
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn tag_of<T: ?Sized + Pun>(ptr: *const T) -> u128 {
  // `None` for another shape, as in `validate_scalar`.
  let Some((offset, size, endian)) = (const {
    match T::LAYOUT.shape() {
      Shape::Enum(layout) => {
        let Kind::Scalar(spec) = layout.tag.layout.kind else { panic!("the tag is not a scalar") };
        Some((layout.tag.offset, spec.size, spec.endian))
      }
      _ => None,
    }
  }) else {
    return 0;
  };
  // SAFETY: `Layout::enumeration` asserts that the tag is a scalar that `Spec::is_integer`
  // accepts, so of at most 16 bytes, at `offset` within the size of `T`. So `add` stays within the
  // `T` at `ptr`, and the tag is valid for reads, and initialized, by contract on the caller. These
  // are the requirements of `load`.
  unsafe { load(ptr.cast::<u8>().add(offset), size, endian) }
}

/// Returns where the fields of the variant of tag `tag`, among `V` and the ones after it, are not
/// valid in the enum at `ptr` ([`Fault`]).
///
/// Returns the enum if no such variant has the tag. A variant whose tag is a range of values (the
/// variant of a niche) has the tag if `tag` is in that range. Returns the enum too, without reading
/// anything, if the variant that has the tag has no value, because one of its fields has none.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `ptr` is valid for reads of a `T`.
/// - `V` is `T::Variants`, or a list that `Variants::Next` reaches from it.
/// - If a variant that has values has the tag `tag`, every byte that the checks of its fields read,
///   which check every part, is initialized.
const unsafe fn validate_variants<C: Checks, T: ?Sized + Pun, V: Variants>(
  ptr: *const T,
  tag: u128,
  input: &mut Input,
) -> Fault {
  if const { V::TAG.is_none() } {
    return ptr.cast();
  }
  let this = has_tag::<V>(tag);
  // SAFETY: `ptr` is valid for reads of a `T`, and `V` is in the list of the variants of `T`, by
  // contract on the caller. If `this`, `V` has the tag `tag`, and `V::Fields` are its fields
  // (`Variants`, section Safety). What their checks read is initialized, by contract on the caller,
  // as `validate_fields` requires. Otherwise the requirements of the call for the next variants are
  // those of this call.
  unsafe {
    if this {
      // No value is a variant of which a field has no value, so nothing of it is read. The
      // analysis does not look at such a variant either (`Variant::is_present`).
      if const { inhabited::<V::Fields>() } {
        validate_variant::<C, T, V::Fields>(ptr, input)
      } else {
        ptr.cast()
      }
    } else {
      validate_variants::<C, T, V::Next>(ptr, tag, input)
    }
  }
}

/// Returns whether the variant `V` has the tag `tag`: its `TAG`, or, for the variant of a niche or
/// one that no value is, one of its `RANGES`. In a function of its own, so that the ranges are no
/// local of [`validate_variants`] while the fields of a variant are checked, in a build without
/// optimizations.
#[cfg_attr(not(debug_assertions), inline(always))]
const fn has_tag<V: Variants>(tag: u128) -> bool {
  match (const { V::TAG }, const { V::RANGES }) {
    (Some(expected), None) => tag == expected,
    (Some(_), Some(ranges)) => layout::contains(ranges, tag),
    (None, _) => false,
  }
}

/// Returns where a part of the fields `F` of a variant of the enum at `ptr` is not valid
/// ([`Fault`]): [`validate_fields`] with `C::Whole`.
///
/// # Safety
///
/// The caller must ensure the requirements of [`validate_fields`], with every part marked.
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn validate_variant<C: Checks, T: ?Sized + Pun, F: Fields>(
  ptr: *const T,
  input: &mut Input,
) -> Fault {
  // SAFETY: The requirements of `validate_fields` are those of this call, and `C::Whole` marks
  // every part.
  unsafe { validate_fields::<C::Whole, Root, T, F>(ptr, input) }
}

/// Returns whether every field of the list `F` has values, so that a value of a variant of these
/// fields exists.
const fn inhabited<F: Fields>() -> bool {
  match F::FIELD {
    Some(field) => field.layout.is_inhabited() && inhabited::<F::Next>(),
    None => true,
  }
}

/// Returns whether the union at `ptr` is a valid value of one of its fields, `V` or one after it.
///
/// Each field of a union is a variant of one field in its layout.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `ptr` is valid for reads of a `T`. It need not be aligned.
/// - `V` is `T::Variants`, or a list that `Variants::Next` reaches from it.
/// - Every byte that the checks of `V` or of a field after it read, which check every part, is
///   initialized, for the fields that have values. The analysis refuses a union of which such a
///   field would read a byte that may be padding in the source.
const unsafe fn validate_union<C: Checks, T: ?Sized + Pun, V: Variants>(
  ptr: *const T,
  input: &mut Input,
) -> bool {
  if const { V::TAG.is_none() } {
    return false;
  }
  // SAFETY: `V` is in the list of the variants of `T`, by contract on the caller, so `V::Fields` is
  // the one field of a variant in the layout of the union, at offset 0 (`Variants`, section
  // Safety). Every byte that its checks read is initialized, by contract on the caller, as
  // `validate_fields` requires. The requirements of the call for the next fields are those of this
  // call. Nothing is read of a field that has no value, which no bytes are, as in
  // `validate_variants`.
  unsafe {
    (const { inhabited::<V::Fields>() }
      && validate_variant::<C, T, V::Fields>(ptr, input).is_null())
      || validate_union::<C, T, V::Next>(ptr, input)
  }
}

/// Returns where a part that `C` marks of the `len` elements at `ptr` is not valid ([`Fault`]).
///
/// Elements whose checks are a few comparisons are checked in blocks of about 64 bytes, without a
/// branch within a block, and a block that fails is checked again one element at a time, to find
/// where. The result is the same as one element at a time.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `ptr` is valid for reads of `len` consecutive `T`s, each as [`validate`] requires.
const unsafe fn validate_elems<C: Checks, P: Path, T: Pun>(
  ptr: *const T,
  len: usize,
  input: &mut Input,
) -> Fault {
  let mut i = 0;
  // Without a stop within a block, the optimizer can vectorize the checks, which it cannot do in a
  // loop that may stop at each element.
  let block = const { T::LAYOUT.block() };
  if block > 1 {
    // The elements in whole blocks. A bound computed once gave a loop of fewer instructions than
    // a comparison with what is left of `len`.
    let blocks = len - len % block;
    while i < blocks {
      let (mut valid, mut j) = (true, 0);
      while j < block {
        // SAFETY: `i + j < len`, since `i < blocks`, both multiples of `block`, so
        // `i + block <= blocks <= len`, and `j < block`. `ptr` is valid for
        // reads of `len` elements, by contract on the caller, so `add(i + j)` stays within them
        // and points to an element that meets the requirements of `validate`.
        valid &= unsafe { validate::<C, P, T>(ptr.add(i + j), input) }.is_null();
        j += 1;
      }
      if !valid {
        // The loop below checks the block again, one element at a time, and finds where.
        break;
      }
      i += block;
    }
  }
  while i < len {
    // SAFETY: `i < len`. `ptr` is valid for reads of `len` elements, by contract on the caller,
    // so `add(i)` stays within them and points to an element that meets the requirements of
    // `validate`.
    let fault = unsafe { validate::<C, P, T>(ptr.add(i), input) };
    if !fault.is_null() {
      return fault;
    }
    i += 1;
  }
  VALID
}

/// Returns where a part that `C` marks of the field `F` of the `T` at `ptr`, or of the fields after
/// it, is not valid ([`Fault`]).
///
/// A field restricted by `#[pun(valid(..))]` is checked against its restriction, not against its
/// type.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The requirements of [`validate`] hold, with the same `C`, `P` and `T`.
/// - `F` is a field of `T`: of the struct, or of the variant of the enum that the value at `ptr`
///   is.
///
/// # Compile-time errors
///
/// An instance compiles only if `F::Path<P>` is the leaf of the field in the layout. The derive
/// writes the list with paths that the user may redefine, so this is checked.
const unsafe fn validate_fields<C: Checks, P: Path, T: ?Sized + Pun, F: Fields>(
  ptr: *const T,
  input: &mut Input,
) -> Fault {
  if const { F::FIELD.is_none() } {
    return VALID;
  }
  // The field as constants: its offset, and whether it is restricted. A copy of `F::FIELD` would be
  // a local of this function, which is on the stack while the field is checked, in a build without
  // optimizations.
  let (offset, restricted) = const {
    match F::FIELD {
      Some(field) => (field.offset(), field.restriction().is_some()),
      None => (0, false),
    }
  };
  const { FieldAt::<P, F>::PLACED };
  // SAFETY: `F` is a field of `T`, by contract on the caller, and `F::FIELD` is its place in the
  // layout of `T` (`Fields`, section Safety). `ptr` points to the start of a `T` that is valid for
  // reads, by the requirements of `validate`, so in one allocation, and `T` has a field of type
  // `F::Type` at `offset`. These are the requirements of `ptr::field`, and that field is valid for
  // reads.
  // - A restricted field meets the requirements of `validate_restricted`.
  // - Any other field is a `F::Type` at the place `F::Path<P>`, which the assertion above shows
  //   is the leaf of the field. What `C` marks there is what it marks of the field in `T`. So the
  //   field meets the requirements of `validate`.
  // The requirements of the call for the next fields are those of this call.
  unsafe {
    let field = ptr::field::<T, F::Type>(ptr, offset);
    let fault = if restricted {
      fails(validate_restricted::<C, P, T, F>(ptr), field.cast())
    } else {
      validate::<C, F::Path<P>, F::Type>(field, input)
    };
    if !fault.is_null() {
      return fault;
    }
    validate_fields::<C, P, T, F::Next>(ptr, input)
  }
}

/// The field `F` at the place `P`, whose place is asserted once, whatever checks check it, so that
/// a refusal is reported once, not once for each instance of [`validate_fields`].
struct FieldAt<P, F>(PhantomData<P>, PhantomData<F>);

impl<P: Path, F: Fields> FieldAt<P, F> {
  /// Where the list says that the field is must be where its layout says it is: the list is made
  /// by the derive with paths it is told (`#[pun(crate = ..)]`), which could name another `At`.
  const PLACED: () = {
    let leaf = match F::FIELD {
      Some(field) => P::LEAF + field.leaf(),
      None => P::LEAF,
    };
    assert!(<F::Path<P> as Path>::LEAF == leaf, "not where the field is");
  };
}

/// Returns whether the restricted field `F` of the struct at `ptr`, a scalar that accepts less
/// than its type does, has one of its values, if `C` marks it.
///
/// # Safety
///
/// The caller must ensure the requirements of [`validate_fields`], with `F` a restricted field.
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn validate_restricted<C: Checks, P: Path, T: ?Sized + Pun, F: Fields>(
  ptr: *const T,
) -> bool {
  // `None` for a field that is not restricted, of which this function is never called, though
  // `validate_fields`, which calls it, compiles it for every field.
  let Some((offset, spec)) = (const {
    match F::FIELD {
      Some(field) => match field.restriction() {
        Some(spec) => Some((field.offset(), spec)),
        None => None,
      },
      None => None,
    }
  }) else {
    return true;
  };
  // SAFETY: `ptr` points to the start of a `T` that is valid for reads, which has the field at
  // `offset`, by contract on the caller, so `add` stays within it. A restricted field is a scalar
  // leaf, read only if it is marked, so its bytes are initialized, as `Spec::matches` requires.
  (const { !C::MARKS.any(<F::Path<P> as Path>::LEAF, 1) })
    || unsafe { spec.matches(ptr.cast::<u8>().add(offset)) }
}

/// Returns whether `C` marks a rule of the struct `T` at the place `P`.
///
/// The rules of a struct are its last leaves, one each ([`Layout::rule_leaf`]).
///
/// [`Layout::rule_leaf`]: layout::Layout::rule_leaf
const fn rules_marked<C: Checks, P: Path, T: ?Sized + Pun>() -> bool {
  C::MARKS.any(P::LEAF + T::LAYOUT.rule_leaf(0), T::LAYOUT.rules().len())
}

/// Returns whether `C` marks the rule `r` of the struct `T` at the place `P`. The end of the list
/// of the rules is not marked.
const fn rule_marked<C: Checks, P: Path, T: ?Sized + Pun>(r: usize) -> bool {
  r < T::LAYOUT.rules().len() && C::MARKS.any(P::LEAF + T::LAYOUT.rule_leaf(r), 1)
}

/// Returns whether the rule `r` of the struct `T` is an aim at the cursor whose step a marked aim
/// at the cursor after it needs: it starts where that step ends.
const fn step_needed<C: Checks, P: Path, T: ?Sized + Pun>(r: usize) -> bool {
  let rules = T::LAYOUT.rules();
  if r >= rules.len() || !rules[r].at_cursor() {
    return false;
  }
  let mut i = r + 1;
  while i < rules.len() {
    if rules[i].at_cursor() && rule_marked::<C, P, T>(i) {
      return true;
    }
    i += 1;
  }
  false
}

/// Returns where the struct at `ptr` does not meet the rules of the layout of `T` that `C` marks at
/// the place `P`, or the pointees of those of its eager aims are not valid within the bytes of
/// `input` ([`Fault`]): the struct, or where a pointee is not valid.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `ptr` is valid for reads of a `T`. It need not be aligned.
/// - The bytes of that `T` that its rules read are initialized: those of the fields that the rules
///   load and, if one of the rules reads bytes by offset ([`Layout::rules_read_bytes`]), all of
///   them, padding included.
/// - If `T` has an aim at a stored place, the `T` is within the bytes of `input`, which are valid
///   for reads and initialized ([`Input`]).
///
/// # Compile-time errors
///
/// An instance compiles only if `T::Rules` lists every rule of `T` that has an expression or is an
/// aim, once, in order, with every operation of each, and the pointee of each aim ([`listed`]).
///
/// [`Layout::rules_read_bytes`]: layout::Layout::rules_read_bytes
#[cfg_attr(not(debug_assertions), inline(always))]
pub const unsafe fn check_rules<C: Checks, P: Path, T: ?Sized + Pun>(
  ptr: *const T,
  input: &mut Input,
) -> Fault {
  const { RulesOf::<T>::LISTED };
  let (len, tail, size) = tail_of(ptr);
  let head = const { T::LAYOUT.tail_shape().start };
  // SAFETY: `tail_of` returns `size`, the size of the `T` at `ptr`, and the tail of `tail` bytes
  // starts at `head`, so `head + tail <= size`. That `T` is valid for reads, the bytes that its
  // rules read are initialized, and it is within `input` if it has an aim at a stored place, by
  // contract on the caller, as `check_listed` requires.
  unsafe { check_listed::<C, P, T, T::Rules>(ptr.cast(), size, (head, tail), len, 0, input) }
}

/// Returns where the struct at `base` does not meet the rule `L` of `T` or one after it, of those
/// that `C` marks at the place `P` ([`Fault`]): `base`, or where the pointee of an aim is not
/// valid.
///
/// The struct is `size` bytes, and its tail is the `tail` bytes from `head`, of `len` elements.
/// The aims before `L` at the cursor end at `cursor` in the tail. A `size` rule must be `size`, a
/// `len` rule `len`, and an assertion not 0. An aim requires what [`check_aim`] checks. A rule
/// that is not marked is proven, and not checked, but the step of an aim at the cursor is still
/// taken where a marked aim at the cursor follows it, which starts where the step ends.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `base` is valid for reads of the `size` bytes of a `T`, and `head + tail <= size`.
/// - `cursor <= tail`.
/// - The bytes of that `T` that its rules read are initialized, as [`check_rules`] requires, and
///   so are the bytes of `input`, within which the `T` is if it has an aim at a stored place.
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn check_listed<C: Checks, P: Path, T: ?Sized + Pun, L: Rules>(
  base: *const u8,
  size: usize,
  (head, tail): (usize, usize),
  len: usize,
  cursor: usize,
  input: &mut Input,
) -> Fault {
  if const { L::RULE == usize::MAX } {
    return VALID;
  }
  // What the rule is, as constants, which a copy of the rule would be a local of this function,
  // on the stack while a pointee is checked, in a build without optimizations: whether it is an
  // aim, and what the value of its expression must be. The end of the list, which is not reached
  // here, has no rule.
  let (aim, wanted) = const {
    match T::LAYOUT.rule(L::RULE) {
      Some(Rule::Aim(_)) => (true, Wanted::NotZero),
      Some(Rule::Size(_)) => (false, Wanted::Size),
      Some(Rule::Len(_)) => (false, Wanted::Len),
      Some(Rule::Assert(_)) | None => (false, Wanted::NotZero),
    }
  };
  let cursor = if const { !rule_marked::<C, P, T>(L::RULE) } {
    if const { !step_needed::<C, P, T>(L::RULE) } {
      cursor
    } else {
      // SAFETY: The requirements of `cursor_step` are those of this call.
      match unsafe { cursor_step::<T, L, true>(base, size, (head, tail), cursor) } {
        Some(Step { end, .. }) => end,
        None => return base,
      }
    }
  } else if aim {
    // SAFETY: The requirements of `check_aim` are those of this call.
    match unsafe { check_aim::<C, T, L>(base, size, (head, tail), cursor, input) } {
      Ok(cursor) => cursor,
      Err(fault) => return fault,
    }
  } else {
    {
      // SAFETY: `InRule<T, L>` is the rule `L::RULE` of `T`, whose expression is `L::Expr`. `base`
      // is valid for reads of the `size` bytes of a `T`, which cover its sized fields, and the
      // bytes that the rule reads are initialized, by contract on the caller. This is what `eval`
      // requires.
      let Some(value) = (unsafe { eval::<InRule<T, L>, L::Expr>(base, size) }) else {
        return base;
      };
      let holds = match wanted {
        Wanted::Size => value == size as u128,
        Wanted::Len => value == len as u128,
        Wanted::NotZero => value != 0,
      };
      if !holds {
        return base;
      }
      cursor
    }
  };
  // SAFETY: `cursor <= tail`: an aim at the cursor moves it to at most `tail`. The other
  // requirements of the call for the next rules are those of this call.
  unsafe { check_listed::<C, P, T, L::Next>(base, size, (head, tail), len, cursor, input) }
}

/// The rules of the struct `T`, whose list is asserted once, whatever checks check them, so that a
/// refusal is reported once, not once for each instance of [`check_rules`].
struct RulesOf<T: ?Sized>(PhantomData<T>);

impl<T: ?Sized + Pun> RulesOf<T> {
  /// `T::Rules` lists the rules of `T` ([`listed`]).
  const LISTED: () = assert!(listed::<T, T::Rules>(0), "not the rules of the struct");
}

/// What the value of the expression of a rule must be ([`check_listed`]).
#[derive(Clone, Copy)]
enum Wanted {
  /// The size of the value (`size`).
  Size,
  /// The length of its tail (`len`).
  Len,
  /// Not zero (`assert`).
  NotZero,
}

/// Returns where the cursor of the tail of the struct `T` at `base` is after the aim `L`, if the
/// struct meets what the aim requires of it, or else where it does not ([`Fault`]): `base`, or
/// where the pointee is not valid.
///
/// A pointee at the cursor ([`check_next`]) must fit in the tail. The pointee of an eager aim is
/// checked here ([`check_stored`], [`check_placed`]), and an aim at a stored place is eager.
///
/// # Safety
///
/// The caller must ensure the requirements of [`check_listed`].
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn check_aim<C: Checks, T: ?Sized + Pun, L: Rules>(
  base: *const u8,
  size: usize,
  (head, tail): (usize, usize),
  cursor: usize,
  input: &mut Input,
) -> Result<usize, Fault> {
  let next = const {
    let () = AimOf::<T, L>::CHECKED;
    matches!(T::LAYOUT.rule(L::RULE), Some(Rule::Aim(Aim { start: Start::Next(_), .. })))
  };
  if next {
    // SAFETY: The requirements of `check_next` are those of this call.
    return unsafe { check_next::<C, T, L>(base, size, (head, tail), cursor, input) };
  }
  // SAFETY: The requirements of `check_stored` are those of this call.
  let fault = unsafe { check_stored::<C, T, L>(base, size, input) };
  if fault.is_null() { Ok(cursor) } else { Err(fault) }
}

/// Returns where the cursor of the tail of the struct `T` at `base` is after the aim at the cursor
/// `L`, if its pointee fits ([`cursor_step`]), and, if the aim is eager, its pointee is aligned for
/// its type, has a size that its type has, and is valid ([`check_placed`]), or else where it is not
/// ([`Fault`]): `base`, or where the pointee is not valid.
///
/// It is a function of its own in a build without optimizations, so that the frames of the
/// recursion of eager aims do not hold what it computes.
///
/// # Safety
///
/// The caller must ensure the requirements of [`check_listed`].
#[cfg_attr(not(debug_assertions), inline(always))]
#[cfg_attr(debug_assertions, inline(never))]
const unsafe fn check_next<C: Checks, T: ?Sized + Pun, L: Rules>(
  base: *const u8,
  size: usize,
  (head, tail): (usize, usize),
  cursor: usize,
  input: &mut Input,
) -> Result<usize, Fault> {
  let (eager, align) = const {
    match T::LAYOUT.rule(L::RULE) {
      Some(Rule::Aim(aim)) => (matches!(aim.check(), Check::Eager), aim.align),
      _ => (false, 1),
    }
  };
  // SAFETY: The requirements of `cursor_step` are those of this call.
  let Some(Step { value, end, len, .. }) =
    (unsafe { cursor_step::<T, L, true>(base, size, (head, tail), cursor) })
  else {
    return Err(base);
  };
  if !eager {
    return Ok(end);
  }
  // SAFETY: `value <= head + tail <= size` (`cursor_step`), within the bytes of the `T`.
  let addr = unsafe { base.add(value) };
  // An eager pointee is aligned for its type, which its read relies on, and has a size that its
  // type has.
  let Some(len) = len else { return Err(base) };
  if !aligned(addr, align) {
    return Err(base);
  }
  // SAFETY: The value of the pointee is at `addr` with the length `len`, and its prefix before it,
  // within the bytes of the `T` (`cursor_step`), which are valid for reads and initialized, by
  // contract on the caller, and within those of `input` if the `T` is, as `check_placed` requires.
  let fault = unsafe { check_placed::<C, L::Pointee>(addr, len, input) };
  if fault.is_null() { Ok(end) } else { Err(fault) }
}

/// Returns where the pointee of the eager aim at a stored place `L` of the struct `T` at `base`, of
/// `size` bytes, is not where it must be ([`find_stored`]), or not valid ([`Fault`]): `base` where
/// the aim leads to no pointee that fits, or else what is stored before its value
/// ([`Pointee::Prefix`]), then its length ([`measure`]), then its value ([`check_value`]).
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `base` is valid for reads of the `size` bytes of a `T`, which are within the bytes of
///   `input`, and the bytes that its rules read are initialized.
/// - The bytes of `input` are in one allocation, valid for reads and initialized.
const unsafe fn check_stored<C: Checks, T: ?Sized + Pun, L: Rules>(
  base: *const u8,
  size: usize,
  input: &mut Input,
) -> Fault {
  let before = const { size_of::<<L::Pointee as Pointee>::Prefix>() };
  // SAFETY: The requirements of `find_stored` are those of this call.
  let Some((to, wanted)) = (unsafe { find_stored::<T, L>(base, size, input) }) else {
    return base;
  };
  // SAFETY: `before <= to <= input.len` (`find_stored`), so `add` stays within the bytes of
  // `input`, in one allocation, and so does `sub`.
  let (addr, start) = unsafe { (input.start.add(to), input.start.add(to - before)) };
  // The pointee of a shared aim, and what it reaches, what it stores before its value included,
  // claim nothing and wait for no loop.
  let (shared, within) = const {
    match T::LAYOUT.rule(L::RULE) {
      Some(Rule::Aim(aim)) => (aim.shared || C::SHARED, matches!(aim.extent, Extent::Rest)),
      _ => (C::SHARED, false),
    }
  };
  if before > 0 {
    // SAFETY: The `before` bytes of the prefix are at `start`, within the bytes of `input`, which
    // are valid for reads and initialized, as `validate` requires.
    let fault = unsafe {
      if shared {
        validate_whole::<Shared, <L::Pointee as Pointee>::Prefix>(start.cast(), input)
      } else {
        validate_whole::<C, <L::Pointee as Pointee>::Prefix>(start.cast(), input)
      }
    };
    if !fault.is_null() {
      return fault;
    }
  }
  // SAFETY: The `input.len - to` bytes at `addr` are the end of the bytes of `input`, which are
  // valid for reads and initialized, and the prefix of the pointee is before them, as `measure`
  // requires. The size of the value is at most these bytes, so `to + size` does not overflow.
  let Some((Some(len), size)) = (unsafe { measure::<L::Pointee>(wanted, addr, input.len - to) })
  else {
    return base;
  };
  if shared {
    // SAFETY: As below.
    return unsafe { check_value::<Shared, L::Pointee, false, true>(addr, (len, size), input) };
  }
  // A pointee that ends where the tail of the struct ends is within the struct, claimed with it,
  // as a pointee at the cursor is, and is checked in place: no pointee waits without a claim. It
  // takes its size from the budget where the struct holds another eager aim, whose pointee may
  // share its bytes, so that checks of the same bytes do not multiply at each level of nesting.
  // The only one of its struct is checked once for each check of the struct, as a pointee at the
  // cursor is, as deep as the parts allow.
  if within {
    // SAFETY: As below.
    return unsafe {
      if const { T::LAYOUT.most_eager_aims() > 1 } {
        check_value::<C, L::Pointee, false, true>(addr, (len, size), input)
      } else {
        check_value::<C, L::Pointee, false, false>(addr, (len, size), input)
      }
    };
  }
  if !input.claim(to - before, to + size, const { C::LOOP.some() }) {
    return base;
  }
  // SAFETY: The value is at `addr` with the length `len`, within the bytes of `input`, which are
  // valid for reads and initialized, as `check_value` requires.
  unsafe { check_value::<C, L::Pointee, true, false>(addr, (len, size), input) }
}

/// Returns where the value of the pointee of the eager aim at a stored place `L` of the struct `T`
/// at `base`, of `size` bytes, is, in bytes from the start of `input`, and how large the aim says
/// it is, if it is within the bytes of `input` ([`stored_at`]), aligned for its type, and the sized
/// part of its value fits in them.
///
/// It is a function of its own in a build without optimizations, so that the frames of the
/// recursion of eager aims do not hold what it computes.
///
/// # Safety
///
/// The caller must ensure the requirements of [`check_stored`].
#[cfg_attr(not(debug_assertions), inline(always))]
#[cfg_attr(debug_assertions, inline(never))]
const unsafe fn find_stored<T: ?Sized + Pun, L: Rules>(
  base: *const u8,
  size: usize,
  input: &Input,
) -> Option<(usize, Size)> {
  let (align, head) = const {
    let align = match T::LAYOUT.rule(L::RULE) {
      Some(Rule::Aim(aim)) => aim.align,
      _ => 1,
    };
    (align, <<L::Pointee as Pointee>::Value as Pun>::LAYOUT.head())
  };
  // SAFETY: The requirements of `stored_at` are those of this call.
  let Some((to, wanted)) = (unsafe { stored_at::<T, L>(base, size, input) }) else {
    return None;
  };
  // SAFETY: `to <= input.len` (`stored_at`), so `add` stays within the bytes of `input`, in one
  // allocation.
  let addr = unsafe { input.start.add(to) };
  if !aligned(addr, align) || input.len - to < head {
    return None;
  }
  Some((to, wanted))
}

/// Returns where the pointee `P` whose value is at `addr`, the length of whose tail is `len`, is
/// not valid ([`Fault`]): what is stored before its value, then its value ([`check_value`]). The
/// pointee of an eager aim at the cursor is checked so.
///
/// # Safety
///
/// The caller must ensure that the `P` at `addr` with that length, and its prefix before it, are
/// within the bytes of `input` or of a value within them, which are in one allocation, valid for
/// reads and initialized.
const unsafe fn check_placed<C: Checks, P: ?Sized + Pointee>(
  addr: *const u8,
  len: usize,
  input: &mut Input,
) -> Fault {
  let before = const { size_of::<P::Prefix>() };
  let start = addr.wrapping_sub(before);
  if before > 0 {
    // SAFETY: The `before` bytes of the prefix are at `start`, within the bytes of `input` or of a
    // value within them, which are valid for reads and initialized, as `validate` requires.
    let fault = unsafe { validate_whole::<C, P::Prefix>(start.cast(), input) };
    if !fault.is_null() {
      return fault;
    }
  }
  // The value exists in the bytes: its size does not overflow.
  let Some(size) = const { <P::Value as Pun>::LAYOUT }.size_of_val(len) else { return addr };
  // The pointees at the cursor of a struct share no byte, and lie within it: each is checked once
  // for each check of the struct, so it takes nothing from the budget.
  // SAFETY: The requirements of `check_value` are those of this call.
  unsafe { check_value::<C, P, false, false>(addr, (len, size), input) }
}

/// Returns where the value of the pointee `P` at `addr`, the length of whose tail is `len` and
/// whose size is `size`, is not valid ([`Fault`]), every part of it checked, its own eager aims
/// included. The caller computes the size, which a claim needs first.
///
/// A pointee is checked in place, nested in the check of the value that holds its aim: it takes
/// its parts from the budget of [`AIM_PARTS`] while it runs ([`Input::nest`]), and, if other aims
/// may reach its bytes (`SPENDS`), its size from the budget of bytes ([`Input::spends`]): one of a
/// shared aim, or one that ends with the tail of a struct that holds another eager aim. A pointee
/// that is claimed (`WAITS`),
/// or at the cursor of its struct, takes 1 if it has no byte and holds an eager aim. A link of a
/// chain
/// ([`links`]) whose type has an identity is checked with [`InPlace`], so that a link of its type
/// within it is known to be nested in it. A pointee of a type that holds an eager aim and has an
/// identity waits for a loop of its type instead ([`waits`]), so that the frames stop growing with
/// the data:
///
/// - in a value of a loop ([`Looped`]), always;
/// - where its parts are not left, where a check without loops stops ([`Error::Limit`]);
/// - nested in a value of its type that leads to one pointee at most, a link of a chain, past
///   [`IN_PLACE`] parts: a loop checks a chain with no entry, and a check in a `const` stays within
///   the frames that Rust allows by default.
///
/// So data that a check in place takes is checked as it is, and a check takes loops only past
/// it, and for chains.
///
/// [`Error::Limit`]: crate::Error::Limit
///
/// # Safety
///
/// The caller must ensure that the value of `P` at `addr` with that length is within the bytes of
/// `input` or of a value within them, which are in one allocation, valid for reads and initialized.
const unsafe fn check_value<
  C: Checks,
  P: ?Sized + Pointee,
  const WAITS: bool,
  const SPENDS: bool,
>(
  addr: *const u8,
  (len, size): (usize, usize),
  input: &mut Input,
) -> Fault {
  let (before, parts, nested) = const {
    let parts =
      <P::Value as Pun>::LAYOUT.parts().saturating_add(<P::Prefix as Pun>::LAYOUT.parts());
    (size_of::<P::Prefix>(), parts, parts.saturating_add(AIM_PARTS - IN_PLACE))
  };
  let value = ptr::from_raw_parts::<P::Value>(addr, len);
  // An empty slice holds no aim, whatever its elements hold, and is checked in place at once. A
  // pointee waits for a loop in a value of a loop, or, in place, if it is nested in a value of its
  // type past `IN_PLACE` parts, or where its parts are not left.
  let is_empty = const { matches!(<P::Value as Pun>::LAYOUT.kind, Kind::Slice(_)) } && len == 0;
  // A pointee that was claimed takes nothing from the budget of bytes: its claim moved the cursor
  // forward, which bounds the claims. One at the cursor is within the bytes of its struct, which
  // no other pointee at the cursor shares. One of no byte moved nothing: it takes 1 if it holds an
  // eager aim, as pointees of no byte may nest each other at any width (`Input::spends`), and
  // nothing otherwise, since it nests no other pointee.
  let bytes = size.saturating_add(before);
  let spends = if bytes == 0 { !is_empty && const { holds_aims::<P>() } } else { SPENDS };
  if spends && !input.spends(bytes) {
    return addr;
  }
  if const { WAITS && loops::<P>() }
    && !is_empty
    && (const { C::LOOP.some() }
      || input.parts < if const { chained::<C, P>() } { nested } else { parts })
  {
    // SAFETY: The requirements of `waits` are those of this call, and a pointee that waits was
    // claimed just before (`WAITS`).
    return unsafe { waits::<C, P>(addr, len, input) };
  }
  // An empty slice has no part to check, so that no check of it is called where a check of its
  // type is not inlined, as for the leaves of a tree.
  if is_empty {
    return VALID;
  }
  // A pointee that holds no eager aim nests the check of no other pointee: its check is as deep as
  // its own parts, a constant of its type, as the check of a value at the top of the bytes is, so
  // it takes no parts. Counting them stopped a chain of values, each with a list of numbers, where
  // the last value fitted in the parts and its list did not.
  let parts = if const { !holds_aims::<P>() } { 0 } else { parts };
  if !input.nest(parts) {
    return addr;
  }
  // SAFETY: The value of `P` at `addr` is within the bytes of `input` or of a value within them,
  // which are valid for reads and initialized, by contract on the caller, so every byte that a
  // check reads is. These are the requirements of `validate`, and the pointees of its aims are
  // within `input` too.
  let fault = unsafe { check_whole::<C, P>(value, input) };
  input.leave(parts);
  fault
}

/// Returns where a part of the value of the pointee `P` at `value` is not valid ([`Fault`]):
/// [`validate_whole`], but with [`All`] where the value holds no eager aim, whatever the loop, so
/// that a value without pointees has one instance of its check.
///
/// # Safety
///
/// The caller must ensure the requirements of [`validate`].
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn check_whole<C: Checks, P: ?Sized + Pointee>(
  value: *const P::Value,
  input: &mut Input,
) -> Fault {
  // SAFETY: The requirements of `validate_whole` and `validate` are those of this call.
  unsafe {
    if const { C::SHARED } {
      validate_whole::<C, P::Value>(value, input)
    } else if const { !holds_aims::<P>() } {
      validate::<All, Root, P::Value>(value, input)
    } else if const { loops::<P>() && !C::LOOP.some() && links::<P>() } {
      validate::<InPlace<P>, Root, P::Value>(value, input)
    } else {
      validate_whole::<C, P::Value>(value, input)
    }
  }
}

/// Returns whether the pointees of `P` wait for a loop: those of a type that holds an eager aim, in
/// its value or in what it stores before it, and that has an identity, by which a loop tells it
/// ([`PointeeType`]).
const fn loops<P: ?Sized + Pointee>() -> bool {
  holds_aims::<P>() && P::POINTEE_ID.known()
}

/// Returns whether the pointee `P`, checked in place with `C`, is nested in a value of its type that
/// leads to one pointee at most: a link of a chain, which a loop checks with no entry.
const fn chained<C: Checks, P: ?Sized + Pointee>() -> bool {
  C::MARK.is(&Looped::<P>::LOOP) && links::<P>()
}

/// Returns whether a value of the pointee `P`, with what it stores before it, leads to one pointee
/// at most: the pointees of its type are links of a chain. Only such a pointee is checked in place
/// with [`InPlace`], which tells that a link is nested in another; any other is checked as in a
/// check without loops, with one instance.
const fn links<P: ?Sized + Pointee>() -> bool {
  <P::Value as Pun>::LAYOUT
    .most_eager_aims()
    .saturating_add(<P::Prefix as Pun>::LAYOUT.most_eager_aims())
    <= 1
}

/// Returns whether the pointee `P` holds an eager aim, in its value or in what it stores before it,
/// so that its check checks other pointees.
const fn holds_aims<P: ?Sized + Pointee>() -> bool {
  <P::Value as Pun>::LAYOUT.holds_eager_aims() || <P::Prefix as Pun>::LAYOUT.holds_eager_aims()
}

/// Returns where the pointee of the type `P` whose value is at `value`, the length of whose tail
/// is `len`, is not valid ([`Fault`]), found by the check of a value with `C`: it waits for the
/// loop of its type, or starts one. A pointee that waits is valid here, and checked by its loop.
///
/// Where no loop runs, which `C` says, it starts the first loop ([`start`]), out of line. In a
/// value of a loop, it waits for the innermost loop if that loop is of its type, which `C` says
/// ([`Loops::wait`]), or else for the innermost loop of its type lower on the path, or a loop of
/// its own ([`waits_below`]). Where no entry is left, it is checked in place, nested in the check
/// of the value that holds its aim, with `C`, whose loop its pointees still wait for ([`full`]).
///
/// A pointee that waits was claimed just before: it is the last claim of its sequence ([`run`])
/// until the next one, which ends its window ([`Input::open`]).
///
/// What is out of line is called last, with the pointee in registers, so that the check of a value
/// of a loop that puts its pointee in the loop, a link of a chain, has no frame.
///
/// # Safety
///
/// The caller must ensure the requirements of [`check_value`] for the pointee, the last claim of
/// the check, and that a loop runs only if `C` checks a value of the innermost loop (`C::LOOP`).
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn waits<C: Checks, P: ?Sized + Pointee>(
  value: *const u8,
  len: usize,
  input: &mut Input,
) -> Fault {
  // A loop runs only where a value of a loop is checked, with `Looped`, which the checks within it
  // keep, so whether one runs is a constant.
  if const { !C::LOOP.some() } {
    // SAFETY: No loop runs, and the requirements of `start` for the pointee are those of this
    // call.
    return unsafe { start::<P>(value, len, input) };
  }
  // The types of `P` and of the loop are constants, so that the check of a value of a loop compares
  // nothing to put a pointee of its type in it.
  if const { !C::LOOP.is(&Looped::<P>::LOOP) } {
    // SAFETY: The requirements of `waits_below` are those of this call.
    return unsafe { waits_below::<C, P>(value, len, input) };
  }
  // SAFETY: A loop runs, so `loops` points to the loops of the check, in the frame of the first
  // loop (`start`), which nothing else uses while this runs, with the innermost loop and the
  // entries, as `Loops::wait` requires.
  match unsafe { (*input.loops).wait(Waiting { value, len, hi: input.hi }) } {
    Some(window) => {
      input.open = window;
      VALID
    }
    // SAFETY: The requirements of `full` are those of this call.
    None => unsafe { full::<C, P>(value, len, input) },
  }
}

/// Returns where the pointee of the type `P` whose value is at `value`, the length of whose tail
/// is `len`, is not valid ([`Fault`]), found by the check of a value of a loop of another type,
/// with `C` ([`waits`]): it waits for the innermost loop of its type lower on the path
/// ([`Loops::wait_below`]), which checks it once the loops above it return. Where no loop of its
/// type runs, a loop of its own checks it now ([`run`]). Where no entry is left, it is checked in
/// place ([`full`]).
///
/// # Safety
///
/// The caller must ensure the requirements of [`check_value`] for the pointee, the last claim of
/// the check, and that a loop runs, whose value `C` checks.
#[inline(never)]
const unsafe fn waits_below<C: Checks, P: ?Sized + Pointee>(
  value: *const u8,
  len: usize,
  input: &mut Input,
) -> Fault {
  let pointee = Waiting { value, len, hi: input.hi };
  // SAFETY: A loop runs, so `loops` points to the loops of the check, in the frame of the first
  // loop (`start`), which nothing else uses while this runs, with the loops that run and the
  // entries, as `Loops::wait_below` requires.
  let wait = unsafe { (*input.loops).wait_below::<P>(pointee) };
  // SAFETY: The requirements of `run` and `full` are those of this call, and a loop runs, whose
  // entries are those of the first loop (`start`).
  unsafe {
    match wait {
      Wait::Waits(window) => {
        input.open = window;
        VALID
      }
      Wait::NoLoop => {
        // The loop of a type takes its parts from a budget of its own, as the first loop does: the
        // loops that run at once are of different types, one frame each.
        let parts = input.parts;
        input.parts = AIM_PARTS;
        let fault = run::<P>(pointee, input);
        input.parts = parts;
        fault
      }
      Wait::Full => full::<C, P>(value, len, input),
    }
  }
}

/// Returns where the pointee of the type `P` whose value is at `value`, the length of whose tail
/// is `len`, is not valid ([`Fault`]), checked in place with `C`, where no entry is left for it
/// ([`waits`]): it is the last claim, so its pointees go on in the sequence.
///
/// It is never inlined, and cold, so that the check of a value of a loop, which holds the call,
/// keeps a small frame where it puts its pointees in entries, as it does but for very wide data.
///
/// # Safety
///
/// The caller must ensure the requirements of [`check_value`] for the pointee, and that a loop
/// runs, whose value `C` checks.
#[cold]
#[inline(never)]
const unsafe fn full<C: Checks, P: ?Sized + Pointee>(
  value: *const u8,
  len: usize,
  input: &mut Input,
) -> Fault {
  let parts =
    const { <P::Value as Pun>::LAYOUT.parts().saturating_add(<P::Prefix as Pun>::LAYOUT.parts()) };
  if !input.nest(parts) {
    return value;
  }
  let pointee = ptr::from_raw_parts::<P::Value>(value, len);
  // SAFETY: The requirements of `check_whole` are those of this call.
  let fault = unsafe { check_whole::<C, P>(pointee, input) };
  input.leave(parts);
  fault
}

/// Whether a pointee waits for a loop below the innermost one ([`Loops::wait_below`]).
#[derive(Clone, Copy)]
enum Wait {
  /// It waits, and the loop checks it: the end of its window, where it waits.
  Waits(*mut usize),
  /// A loop of its type runs, but no entry is left.
  Full,
  /// No loop of its type runs.
  NoLoop,
}

/// A pointee that waits for the loop of its type ([`run`]): what [`check_value`] checks.
#[derive(Clone, Copy)]
struct Waiting {
  /// Where its value starts, within the bytes of the check: null for no pointee.
  value: *const u8,
  /// The length of the tail of its value.
  len: usize,
  /// Where the claims of its pointees must end, in bytes from the start of the check: the start of
  /// the next claim of the sequence that claimed it ([`run`]), which that claim writes
  /// ([`Input::open`]), or else the end of the window of that sequence, which it holds from the
  /// start. It is [`Waiting::OPEN`] in the next pointee of a loop ([`Loops::next`]) until a claim
  /// follows its own.
  hi: usize,
}

impl Waiting {
  /// No pointee.
  const NONE: Self = Self { value: core::ptr::null(), len: 0, hi: 0 };

  /// The `hi` of the next pointee of a loop while it is the last claim of its sequence: no place in
  /// the bytes, which are at most `isize::MAX`.
  const OPEN: usize = usize::MAX;
}

/// How many pointees may wait for the loops of their types at once: 64 entries of three words and
/// their 64 links, 1.6 KiB on a target of 64 bits, in the frame of the first loop ([`start`]).
const WAITING: usize = 64;

/// The link of the last entry of a list ([`Loops::links`]): no entry.
const NO_ENTRY: u8 = WAITING as u8;

/// The loops of a check ([`run`]): the loops that run, the next pointee of the innermost one, and
/// the entries of all of them, in the frame of the first loop, if one runs.
///
/// The entries of a loop are a list ([`Running::head`]), linked by index, so that an entry never
/// moves: the window of a pointee that waits for a loop below the innermost one is written through
/// a pointer after loops above it took and gave back entries ([`Input::open`]).
pub struct Loops {
  /// The innermost loop that runs, or null.
  running: *mut Running,
  /// The pointee that the innermost loop checks next, before its entries: the first of its type
  /// that the check of a value of the loop finds, so that a chain takes no entry. It is
  /// [`Waiting::NONE`] if there is none.
  next: Waiting,
  /// `WAITING` entries, in the frame of the first loop ([`start`]).
  entries: *mut Waiting,
  /// For each entry in the list of a loop, the entry after it in that list, and for each free
  /// entry that was used, the next such one: an index, or [`NO_ENTRY`].
  links: [u8; WAITING],
  /// The first free entry that was used, or [`NO_ENTRY`].
  free: u8,
  /// How many entries were ever used: those from this index on are free.
  used: u8,
  /// Which loop the pointee that waits since the last claim waits for, if it is in an entry
  /// ([`Running::depth`]). A loop that ends a sequence on it tells from this whether it checks the
  /// pointee itself, or whether a loop below does, after claims that still end its window
  /// ([`run`]).
  waits_at: usize,
  /// The entry of that pointee: a loop that takes this entry goes on in the sequence.
  waits_in: u8,
}

impl Loops {
  /// Takes a free entry, if one is left, and returns its index.
  const fn take(&mut self) -> Option<usize> {
    if self.free != NO_ENTRY {
      let index = self.free as usize;
      // SAFETY: `index < WAITING`, a free entry that was used, whose link is the next such one.
      self.free = unsafe { self.links.as_ptr().add(index).read() };
      Some(index)
    } else if (self.used as usize) < WAITING {
      self.used += 1;
      Some(self.used as usize - 1)
    } else {
      None
    }
  }

  /// Puts `pointee` in a free entry, on top of the list of the loop `of`, and returns where the
  /// end of its window is, or `None` if no entry is left.
  ///
  /// # Safety
  ///
  /// The caller must ensure that `of` points to a loop that runs, in the frame of its `run`, which
  /// nothing else uses while this runs, and that `entries` is that of the first loop ([`start`]).
  #[cfg_attr(not(debug_assertions), inline(always))]
  const unsafe fn put(&mut self, of: *mut Running, pointee: Waiting) -> Option<*mut usize> {
    let Some(index) = self.take() else { return None };
    // SAFETY: `index < WAITING`, an entry that no list holds, within `entries`, and `of` is a loop
    // that runs, by contract on the caller.
    unsafe {
      let entry = self.entries.add(index);
      entry.write(pointee);
      self.links.as_mut_ptr().add(index).write((*of).head);
      (*of).head = index as u8;
      (self.waits_at, self.waits_in) = ((*of).depth, index as u8);
      Some(&raw mut (*entry).hi)
    }
  }

  /// Puts `pointee` where the innermost loop takes it ([`run`]): as the next pointee that it
  /// checks, if it has none, or else on top of its entries. Returns where the end of its window
  /// is, or `None` if no entry is left.
  ///
  /// It is called in the check of a value of that loop only (`Checks::LOOP`), of the type of
  /// `pointee`, and inlined there with optimizations.
  ///
  /// # Safety
  ///
  /// The caller must ensure that a loop runs: `running` is the innermost one, in the frame of its
  /// `run`, which nothing else uses while this runs, and `entries` is that of the first loop
  /// ([`start`]).
  #[cfg_attr(not(debug_assertions), inline(always))]
  const unsafe fn wait(&mut self, pointee: Waiting) -> Option<*mut usize> {
    if self.next.value.is_null() {
      self.next = Waiting { hi: Waiting::OPEN, ..pointee };
      return Some(&raw mut self.next.hi);
    }
    // SAFETY: The requirements of `put` for the innermost loop are those of this call.
    unsafe { self.put(self.running, pointee) }
  }

  /// Puts `pointee`, of the type `P`, on top of the entries of the innermost loop of its type, if
  /// one runs and there is room, so that the loop checks it once the loops above it return. A loop
  /// is of the type of `P` as [`PointeeType::is`] tells.
  ///
  /// # Safety
  ///
  /// The caller must ensure that `running` is null or the innermost loop that runs, each loop in
  /// the frame of its `run` with the loop below it as its `outer`, which nothing else uses while
  /// this runs, and that `entries` is that of the first loop ([`start`]).
  const unsafe fn wait_below<P: ?Sized + Pointee>(&mut self, pointee: Waiting) -> Wait {
    let of = const { &Looped::<P>::LOOP };
    let mut at = self.running;
    loop {
      if at.is_null() {
        return Wait::NoLoop;
      }
      // SAFETY: `at` is a loop that runs, in the frame of its `run`, by contract on the caller.
      let running = unsafe { &*at };
      if running.of.is(of) {
        break;
      }
      at = running.outer;
    }
    // SAFETY: `at` is a loop that runs, as above, and the entries are those of the first loop, by
    // contract on the caller.
    match unsafe { self.put(at, pointee) } {
      Some(window) => Wait::Waits(window),
      None => Wait::Full,
    }
  }
}

/// The type of a pointee, as a check of its pointees tells it apart: its identity (`layout::Id`),
/// its size and its alignment, which a proof of an aim compares (`Aim::equals`). A loop takes two
/// pointees for one type where that proof takes them for one.
pub struct PointeeType {
  /// A hash of the identity ([`Id::key`]), compared first.
  key: u64,
  /// The identity ([`Id::equals`]).
  id: &'static Id,
  /// The size of its value, without the elements of its tail.
  size: usize,
  /// The alignment of its value and of what it stores before it.
  align: usize,
}

impl PointeeType {
  /// No type: [`Id::NONE`], which equals no identity.
  const NONE: Self = Self { key: 0, id: &Id::NONE, size: 0, align: 0 };

  /// Returns whether this is a type, not [`PointeeType::NONE`].
  const fn some(&self) -> bool {
    self.id.known()
  }

  /// Returns whether `self` and `other` are one type.
  const fn is(&self, other: &Self) -> bool {
    self.key == other.key
      && self.size == other.size
      && self.align == other.align
      && self.id.equals(other.id)
  }
}

/// A loop that runs ([`run`]), in the frame of its function: the type of its pointees, its
/// entries, and the loop below it.
struct Running {
  /// The type of its pointees.
  of: &'static PointeeType,
  /// The entry on top of its list in `Loops::entries`, the last one put there, or [`NO_ENTRY`].
  head: u8,
  /// How many loops run below it.
  depth: usize,
  /// The loop below it, or null.
  outer: *mut Running,
}

/// Returns where the pointee of the type `P` whose value is at `value`, the length of whose tail
/// is `len`, or a pointee of its type that waits for its loop, is not valid ([`run`]), with the
/// loops of the check and their entries in this frame, which the loops that run within this one
/// share: the first loop of a check.
///
/// It is never inlined, and cold, so that the entries are on the stack only while loops run, and
/// that a check in place, which may start a loop, is laid out as one that may not: its frame is
/// set up only where it nests a pointee. Each loop takes parts from a budget of [`AIM_PARTS`] of
/// its own, since it starts where the check in place, or the loop of another type, took its parts.
///
/// # Safety
///
/// The caller must ensure that no loop runs, and the requirements of [`check_value`] for the
/// pointee, the last claim of the check.
#[cold]
#[inline(never)]
const unsafe fn start<P: ?Sized + Pointee>(
  value: *const u8,
  len: usize,
  input: &mut Input,
) -> Fault {
  let mut entries = [MaybeUninit::<Waiting>::uninit(); WAITING];
  let mut loops = Loops {
    running: core::ptr::null_mut(),
    next: Waiting::NONE,
    entries: (&raw mut entries).cast(),
    links: [NO_ENTRY; WAITING],
    free: NO_ENTRY,
    used: 0,
    waits_at: 0,
    waits_in: NO_ENTRY,
  };
  input.loops = &raw mut loops;
  // The loop takes parts of a budget of its own, since it starts where the check in place has taken
  // its parts.
  let parts = input.parts;
  input.parts = AIM_PARTS;
  // SAFETY: `entries` holds `WAITING` entries, of which none is in use, since no loop ran before
  // this one, and the requirements of `run` for `pointee` are those of this call.
  let fault = unsafe { run::<P>(Waiting { value, len, hi: input.hi }, input) };
  input.parts = parts;
  input.loops = core::ptr::null_mut();
  // No loop is below the first one, so no pointee waits once it returns.
  input.open = core::ptr::null_mut();
  fault
}

/// Returns where `pointee`, of the type `P`, or a pointee of its type that waits for this loop, is
/// not valid ([`Fault`]): the next one that the check of a value finds, first, then the entries of
/// the loop, the last first. A pointee whose sequence ends past its window is not valid there.
///
/// A value of `P` is checked with [`Looped<P>`], under which a pointee of its type waits for this
/// loop ([`waits`]), so that a chain of values of `P` takes one frame, whatever its length. A
/// pointee of another type that holds an eager aim waits for the innermost loop of its type, if one
/// runs, or starts a loop of its own: the loops that run at once are of different types, so their
/// frames do not grow with the data. The values of the loop are checked one after another, so they
/// take their parts from the budget of [`AIM_PARTS`] of the loop once, for as long as the loop
/// runs.
///
/// # Sequences
///
/// The claims of a check are made in sequences. In a sequence, each claim starts at or after the
/// end of the one before it ([`Input::claim`]), and the last one ends at or before the end of the
/// window of the sequence ([`Input::hi`]). The first sequence is that of the value that the check
/// reads, whose window ends with the bytes. A pointee that is checked where it is found goes on in
/// the sequence that claimed it: its pointees are claimed after it, as a copy writes them. So does
/// the first pointee of a loop, and the pointee that a loop takes while nothing was claimed after
/// it, its next pointee ([`Waiting::OPEN`]) or the entry that a loop above just put: a chain is one
/// sequence.
///
/// Any other pointee that waits has a sequence of its own, which starts where its loop takes it,
/// at the end of its claim. Its window ends where the claim that followed it in the sequence that
/// claimed it starts. So the windows of the pointees that wait in a sequence lie between its
/// claims, and share no byte with them nor with each other.
///
/// A pointee that no claim followed is the last claim of its sequence. Once that sequence is over,
/// the window of the pointee ends where the window of the sequence does, as its entry says from
/// the start. But the sequence in which a loop starts is not over where the loop returns: the
/// value that found the first pointee of the loop is still being checked, and its next claim is in
/// that sequence. So is every claim of the loop up to the first entry that it takes. That sequence
/// goes on in the entry of its last claim, the tail, which the loop marks ([`Waiting::OPEN`]), and
/// in the tail of that one, and so on. Where a tail ends on a pointee that waits for a loop below
/// this one, the loop keeps [`Input::open`] for where it returns, so that the next claim of the
/// sequence ends the window of that pointee. It returns with the cursor of that sequence, the
/// largest end of its claims: every other window ends at or before a claim of the sequence that
/// its pointee waited in.
///
/// A loop compares the cursor of a sequence with the end of its window where the sequence ends,
/// and returns the pointee whose sequence it is if its claims end past it. So it does not compare
/// the sequence in which it started, which is not over: the loop below does, where that sequence
/// ends, and names the pointee whose own pointees end past its window, not the first pointee of
/// this loop. The first sequence of a check has the bytes as its window, which no claim passes.
///
/// # Safety
///
/// The caller must ensure that a loop runs, the first one or this one ([`start`]), and the
/// requirements of [`check_value`] for `pointee`, a `P`, the last claim of the check.
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn run<P: ?Sized + Pointee>(pointee: Waiting, input: &mut Input) -> Fault {
  let parts =
    const { <P::Value as Pun>::LAYOUT.parts().saturating_add(<P::Prefix as Pun>::LAYOUT.parts()) };
  let of = const { &Looped::<P>::LOOP };
  // The loops of the check, in the frame of the first loop, which nothing else uses while this
  // runs, read and written through this pointer, which the checks within the loop use as well.
  let loops = input.loops;
  // SAFETY: A loop runs, the first one or this one (`start`), so `loops` points to the loops, and
  // the innermost loop, if any, is in the frame of its `run`, below this one.
  let (below, depth) = unsafe {
    let below = (*loops).running;
    (below, if below.is_null() { 0 } else { (*below).depth + 1 })
  };
  let mut running = Running { of, head: NO_ENTRY, depth, outer: below };
  // The loop is read and written through this pointer only, which `Loops::running` holds while it
  // runs: the checks within it put entries in its list.
  let this = &raw mut running;
  // The next pointee of the loop below, which it takes again once this one returns.
  // SAFETY: As above.
  let outer = unsafe {
    (*loops).running = this;
    let outer = (*loops).next;
    (*loops).next = Waiting::NONE;
    outer
  };
  let mut pointee = pointee;
  // The sequence in which the loop starts: the end of its window, whether the sequence of
  // `pointee` is that one, and, from the first entry that the loop takes, the largest end of the
  // claims of that sequence and the pointee below this loop that waits since the last of them.
  let window = input.hi;
  let mut tail = true;
  let mut aside: Option<(usize, *mut usize, usize, u8)> = None;
  // Each value takes the same parts while it is checked, and gives them back.
  let nested = input.nest(parts);
  // The pointee whose sequence the check is in, once the loop takes an entry: before that, and in
  // a tail, the sequence is that of a pointee of a loop below, or the first one.
  let mut owner = pointee.value;
  let fault = if nested {
    loop {
      let value = ptr::from_raw_parts::<P::Value>(pointee.value, pointee.len);
      // SAFETY: The value of `pointee` is a `P` within the bytes of `input`, by contract on the
      // caller for the first, and as `waits` put the others, and this loop is the innermost one, as
      // `Looped<P>` requires of the pointees that wait for it.
      let fault = unsafe { validate::<Looped<P>, Root, P::Value>(value, input) };
      if !fault.is_null() {
        break fault;
      }
      // SAFETY: `this` points to this loop, in this frame, and `loops` to the loops, as above.
      let (this, loops) = unsafe { (&mut *this, &mut *loops) };
      let next = loops.next;
      if !next.value.is_null() {
        // A null value is no pointee: the rest of the entry is not read.
        loops.next.value = core::ptr::null();
        pointee = next;
        if next.hi == Waiting::OPEN {
          // Nothing was claimed since its claim: its pointees are claimed after it, in the same
          // sequence, and `Input::open`, which pointed to it, points to nothing.
          input.open = core::ptr::null_mut();
          continue;
        }
      } else if this.head != NO_ENTRY {
        let index = this.head as usize;
        // SAFETY: `index < WAITING`, an entry of the list of this loop, which `Loops::put` wrote,
        // within `entries` and `links`.
        unsafe {
          pointee = loops.entries.add(index).read();
          this.head = loops.links.as_ptr().add(index).read();
          loops.links.as_mut_ptr().add(index).write(loops.free);
        }
        loops.free = index as u8;
        if !input.open.is_null() && loops.waits_at == depth && loops.waits_in == index as u8 {
          // The entry is that of the pointee that waits since the last claim: a loop above put it
          // while this loop had no next pointee. Nothing was claimed since its claim, so it goes
          // on in the same sequence, as the next pointee does.
          input.open = core::ptr::null_mut();
          continue;
        }
      } else {
        // The last sequence ends within its window, unless it is the one in which the loop
        // started, which is not over.
        break if tail { VALID } else { fails(input.cursor <= input.hi, owner) };
      }
      // The sequence ends within its window, likewise.
      if !tail && input.cursor > input.hi {
        break owner;
      }
      let (mut high, mut kept, mut kept_at, mut kept_in) = match aside {
        Some(aside) => aside,
        None => (0, core::ptr::null_mut(), 0, NO_ENTRY),
      };
      if !input.open.is_null() {
        // A pointee waits since the last claim. After a sequence of an entry, its window ends
        // where that sequence does, as it was put. After a tail, the sequence is not over: it goes
        // on in the pointee if this loop checks it, or else where this loop returns.
        if tail {
          if loops.waits_at == depth {
            // SAFETY: `open` is the end of the window of an entry of this loop, which nothing else
            // uses while this runs, and entries do not move.
            unsafe { *input.open = Waiting::OPEN };
          } else {
            (kept, kept_at, kept_in) = (input.open, loops.waits_at, loops.waits_in);
          }
        }
        input.open = core::ptr::null_mut();
      }
      if input.cursor > high {
        high = input.cursor;
      }
      aside = Some((high, kept, kept_at, kept_in));
      // The sequence of `pointee` starts at the end of its claim.
      // SAFETY: The value of `pointee` is within the bytes of `input`, in one allocation, at or
      // after their start, as `offset_from_unsigned` requires.
      let from = unsafe { pointee.value.offset_from_unsigned(input.start) };
      let Some(size) = const { <P::Value as Pun>::LAYOUT }.size_of_val(pointee.len) else {
        // SAFETY: The size of the pointee was measured where it was claimed, and does not
        // overflow.
        unsafe { core::hint::unreachable_unchecked() }
      };
      tail = pointee.hi == Waiting::OPEN;
      (input.cursor, input.hi) = (from + size, if tail { window } else { pointee.hi });
      owner = pointee.value;
    }
  } else {
    pointee.value
  };
  if nested {
    input.leave(parts);
  }
  // SAFETY: As above.
  unsafe {
    if let Some((high, kept, kept_at, kept_in)) = aside {
      // The loop took entries: the sequence in which it started goes on where it returns, from the
      // largest end of its claims, with the pointee below this loop that its tail ended on.
      if !tail || input.open.is_null() {
        (input.open, (*loops).waits_at, (*loops).waits_in) = (kept, kept_at, kept_in);
      }
      input.cursor = if input.cursor > high { input.cursor } else { high };
      input.hi = window;
    }
    (*loops).running = running.outer;
    (*loops).next = outer;
  }
  fault
}

/// What the pointee of the aim `L` of `T` must be, asserted where it is checked or read: one whose
/// extent is what it stores stores its length (`Layout::stores_length`), as [`Aim::new`] cannot
/// assert, since the layout of the pointee may hold the aim, and one that is checked with the
/// struct has no layout that Rust leaves to the compiler, without the `unstable` feature.
pub const fn pointee_checked<T: ?Sized + Pun, L: Rules>() {
  let Some(Rule::Aim(aim)) = T::LAYOUT.rule(L::RULE) else { return };
  assert!(
    !matches!(aim.extent, Extent::Pointee)
      || L::Pointee::LENGTH.is_some()
      || <<L::Pointee as Pointee>::Value as Pun>::LAYOUT.stores_length(),
    "{}",
    STORES_LENGTH
  );
  assert!(
    matches!(aim.check(), Check::Lazy)
      || <<L::Pointee as Pointee>::Value as Pun>::LAYOUT.checks_kept()
        && <<L::Pointee as Pointee>::Prefix as Pun>::LAYOUT.checks_kept(),
    "the pointee of an eager aim has a part whose layout Rust leaves to the compiler (a struct, \
     a tuple or an enum of the `Rust` representation), and something to check in it.\nAdd \
     `#[repr(C)]` (or `#[repr(u8)]` to an enum), or enable the `unstable` feature of `puns`"
  );
}

/// The aim `L` of `T`, whose pointee is asserted once ([`pointee_checked`]), whatever checks check
/// it: an instance for each [`Checks`] would assert it again, and a refusal would be reported once
/// for each.
struct AimOf<T: ?Sized, L>(PhantomData<L>, PhantomData<T>);

impl<T: ?Sized + Pun, L: Rules> AimOf<T, L> {
  /// What the pointee of the aim must be ([`pointee_checked`]).
  const CHECKED: () = pointee_checked::<T, L>();
}

/// Why the pointee of an aim whose extent is what it stores does not compile, if it does not store
/// its length.
const STORES_LENGTH: &str = "the pointee of an aim whose extent is what it stores is sized, a \
                             `Thin`, or stores its length in its sized fields: a `size` or `len` \
                             rule that reads no byte by offset";

/// Where the pointee of an aim at the cursor is, and how large ([`cursor_step`]).
#[derive(Copy, Clone)]
pub struct Step {
  /// Where its value starts, in bytes from the start of the struct.
  pub value: usize,
  /// The size of its value, in bytes.
  pub size: usize,
  /// Where the cursor is after it, in bytes from the start of the tail.
  pub end: usize,
  /// The length of its tail, if its size is one that its type has ([`measure`]).
  pub len: Option<usize>,
}

/// Returns where the pointee of the aim at the cursor `L` of the struct `T` at `base` is, after the
/// pointees of the aims at the cursor before it, which end at `cursor` in the tail: one step of the
/// cursor, which the check of the struct ([`check_listed`]) and the reads of its pointees
/// ([`region_at`]) take alike.
///
/// The struct is `size` bytes, and its tail is the `tail` bytes from `head`. The value of the
/// pointee starts at the cursor, after what the pointee stores before it, rounded up to the
/// alignment of the pointee by its offset from the start of the struct, so that where it is depends
/// on the bytes, not on their address. Its size is what it stores, the size of its length or the
/// size that its expression gives, or the rest of the tail ([`Extent`]), measured against its type
/// ([`measure`]).
///
/// With `STRICT`, the step fails where the pointee does not fit in the tail, or its expression
/// fails. Otherwise it never fails: a pointee that would start past the tail is empty at its end,
/// and one whose expression fails, or that does not fit, takes the rest of the tail, since safe
/// code may have written the fields that give it after the struct was checked. Either way, the
/// length is `None` where the size is not one that the type of the pointee has, which the check of
/// an eager aim refuses.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `base` is valid for reads of the `size` bytes of a `T`, the bytes that its rules read are
///   initialized, and so are the bytes of its tail.
/// - `head + tail <= size`, and `cursor <= tail`.
///
/// The function is named for every rule of a struct, and called for an aim at the cursor only.
#[cfg_attr(not(debug_assertions), inline(always))]
pub const unsafe fn cursor_step<T: ?Sized + Pun, L: Rules, const STRICT: bool>(
  base: *const u8,
  size: usize,
  (head, tail): (usize, usize),
  cursor: usize,
) -> Option<Step> {
  // Named for every rule of a struct, and called for an aim at the cursor only.
  let (extent, align, before) = const {
    let before = size_of::<<L::Pointee as Pointee>::Prefix>();
    match T::LAYOUT.rule(L::RULE) {
      Some(Rule::Aim(Aim { start: Start::Next(_), extent, align, .. })) => (extent, align, before),
      _ => (Extent::Rest, 1, before),
    }
  };
  // SAFETY: `head + tail <= size` and `cursor <= tail`, and `size <= isize::MAX`, the size of the
  // bytes of a `T`, which `base` is valid for reads of, by contract on the caller. Said to the
  // compiler, these remove the checks of overflow that a read of a region would otherwise make.
  unsafe {
    core::hint::assert_unchecked(
      size <= isize::MAX as usize && head <= size && tail <= size - head && cursor <= tail,
    );
  };
  let (end, room) = (head + tail, tail - cursor);
  // The bytes before the value, from the cursor: its prefix, then up to its alignment by its offset
  // from the start of the struct. They are 0 for a pointee of alignment 1 that stores nothing,
  // which the compiler then checks not at all. `head + cursor <= size`, so the sums do not
  // overflow below `isize::MAX`, and an alignment past the end of the tail gives no room.
  let at = head + cursor;
  let pad = match (at + before).checked_next_multiple_of(align) {
    Some(value) => value - at,
    None => usize::MAX,
  };
  if pad > room {
    if STRICT {
      return None;
    }
    // Empty, at the end of the tail, with no room for what it stores before its value.
    return Some(Step { value: end, size: 0, end: tail, len: None });
  }
  let (value, rest) = (at + pad, room - pad);
  let fallback = if STRICT { None } else { Some(Size::Bytes(rest as u128)) };
  let wanted = match extent {
    Extent::Pointee => Some(Size::Pointee),
    // SAFETY: `InRule<T, L, 1>` is the length or the size of the pointee, the second expression of
    // the rule, whose operations are `L::Extent`. `base` is valid for reads of the `size` bytes of
    // a `T`, which cover its sized fields, and the bytes that the rule reads are initialized, by
    // contract on the caller. This is what `eval` requires.
    Extent::Length(_) => match unsafe { eval::<InRule<T, L, 1>, L::Extent>(base, size) } {
      Some(n) => Some(Size::Length(n)),
      None => fallback,
    },
    // SAFETY: As above.
    Extent::Bytes(_) => match unsafe { eval::<InRule<T, L, 1>, L::Extent>(base, size) } {
      Some(n) => Some(Size::Bytes(n)),
      None => fallback,
    },
    Extent::Rest => Some(Size::Bytes(rest as u128)),
  };
  let Some(wanted) = wanted else { return None };
  // SAFETY: `value <= end <= size`, and the `rest` bytes from there are the end of the tail, valid
  // for reads and initialized, by contract on the caller, with the prefix of the pointee before
  // them, after the cursor, in the tail, as `measure` requires.
  let measured = unsafe { measure::<L::Pointee>(wanted, base.add(value), rest) };
  match measured {
    Some((len, n)) => Some(Step { value, size: n, end: value + n - head, len }),
    None if STRICT => None,
    // What remains, which may not be a size of the type, for what reads it to refuse.
    None => Some(Step {
      value,
      size: rest,
      end: tail,
      len: sized_as::<<L::Pointee as Pointee>::Value>(rest),
    }),
  }
}

/// How large the pointee of an aim is, as its rule says, before it is measured against its type
/// ([`measure`]).
#[derive(Copy, Clone)]
pub enum Size {
  /// What the pointee stores ([`Extent::Pointee`]).
  Pointee,
  /// The length of its tail, the value of an expression ([`Extent::Length`]).
  Length(u128),
  /// This many bytes ([`Extent::Bytes`], [`Extent::Rest`]).
  Bytes(u128),
}

/// Returns the size of the value of the pointee `P` at `addr`, of which `room` bytes can be read,
/// as large as `size` says, if it fits in them, and the length of its tail if the size is one that
/// `P` has: the size of its type, with the length that it stores if it is unsized, or with the
/// length that is given, or the bytes that are given, which are a size of `P` if they are the size
/// of a sized `P`, or of an unsized one with as many elements as make it ([`sized_as`]). The check
/// of a lazy aim at the cursor requires only that the pointee fits, so that its type plays no part
/// in the value of the struct but for where it is and what it stores, and that of an eager one that
/// its size is one of its type too.
///
/// # Safety
///
/// The caller must ensure that `addr` is valid for reads of `room` initialized bytes, and, if `P`
/// stores its length before its value, that its prefix, before `addr`, is.
#[cfg_attr(not(debug_assertions), inline(always))]
pub const unsafe fn measure<P: ?Sized + Pointee>(
  size: Size,
  addr: *const u8,
  room: usize,
) -> Option<(Option<usize>, usize)> {
  let (layout, thin) = const { (<P::Value as Pun>::LAYOUT, P::LENGTH.is_some()) };
  let len = match size {
    Size::Pointee if !layout.is_unsized() => {
      return if layout.size() <= room { Some((Some(0), layout.size())) } else { None };
    }
    Size::Pointee if !thin && room < layout.head() => return None,
    Size::Pointee => {
      let value = if thin {
        // SAFETY: The length that a `Thin` stores is right before `addr`, in initialized bytes, by
        // contract on the caller, as `stored_length` requires.
        unsafe { stored_length::<P>(addr) }
      } else {
        // SAFETY: `addr` is valid for reads of `room` initialized bytes, by contract on the
        // caller, which cover the `head` bytes of the sized fields of the value, and of the struct
        // in it that has the rule, which stores the length (`Aim::new`). These are the
        // requirements of `length`.
        unsafe { length::<P::Value>(addr, layout.head()) }
      };
      match value {
        Some(value) if value <= usize::MAX as u128 && thin => value as usize,
        Some(value) if value <= usize::MAX as u128 => match elements::<P::Value>(value as usize) {
          Some(len) => len,
          None => return None,
        },
        _ => return None,
      }
    }
    Size::Length(len) if len <= usize::MAX as u128 => len as usize,
    Size::Bytes(n) if n <= room as u128 => {
      let n = n as usize;
      return Some((sized_as::<P::Value>(n), n));
    }
    Size::Length(_) | Size::Bytes(_) => return None,
  };
  // A length: its value fits only if its elements do.
  if room < layout.head() || layout.stride() != 0 && len > (room - layout.head()) / layout.stride()
  {
    return None;
  }
  match layout.size_of_val(len) {
    Some(size) if size <= room => Some((Some(len), size)),
    _ => None,
  }
}

/// Returns the length that the `Thin` pointee `P` whose value is at `addr` stores right before it,
/// the integer at the end of its prefix, of the size, the byte order and the sign of its type, if
/// it is not negative.
///
/// # Safety
///
/// The caller must ensure that the bytes of the prefix of `P`, before `addr`, are valid for reads
/// and initialized.
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn stored_length<P: ?Sized + Pointee>(addr: *const u8) -> Option<u128> {
  let Some(spec) = (const { P::LENGTH }) else { return None };
  // SAFETY: The length is the last `spec.size` bytes of the prefix, right before `addr`, which are
  // valid for reads and initialized, by contract on the caller, as `load` requires.
  let value = unsafe { layout::load(addr.sub(spec.size), spec.size, spec.endian) };
  if matches!(spec.number, layout::Number::Signed) && value >> (8 * spec.size - 1) != 0 {
    return None;
  }
  Some(value)
}

/// Returns the length of the tail of the pointee `P` whose value is at `addr`, where `P` stores it:
/// what [`measure`] finds, from the length that a `Thin` stores before it, or from the `size` or
/// `len` rule of the value, evaluated over its sized fields, and 0 for a sized value.
///
/// # Safety
///
/// The caller must ensure that `P` stores its length (`Layout::stores_length`, or a `Thin`), that
/// a valid value of `P`, aligned, is at `addr`, after the prefix of `P`, and that the length that
/// it stores is the length of its tail. A pointee whose extent is what it stores, which
/// [`check_stored`] or [`check_next`] checked, is one.
#[cfg_attr(not(debug_assertions), inline(always))]
pub const unsafe fn own_length<P: ?Sized + Pointee>(addr: *const u8) -> usize {
  let (unsized_, head, thin) = const {
    let layout = <P::Value as Pun>::LAYOUT;
    (layout.is_unsized(), layout.head(), P::LENGTH.is_some())
  };
  if !unsized_ {
    return 0;
  }
  let len = if thin {
    // SAFETY: The prefix of `P` is before `addr`, valid and initialized, by contract on the
    // caller, as `stored_length` requires.
    match unsafe { stored_length::<P>(addr) } {
      Some(value) if value <= usize::MAX as u128 => Some(value as usize),
      _ => None,
    }
  } else {
    // SAFETY: The `head` bytes at `addr` are the sized fields of the valid value there, valid for
    // reads and initialized, by contract on the caller. They cover the sized fields of the struct
    // in it that has the rule, which stores the length. These are the requirements of `length`.
    match unsafe { length::<P::Value>(addr, head) } {
      Some(value) if value <= usize::MAX as u128 => elements::<P::Value>(value as usize),
      _ => None,
    }
  };
  let Some(len) = len else {
    // SAFETY: The pointee at `addr` stores the length of its tail, by contract on the caller: the
    // length itself for a `Thin` or a `len` rule, or the size of a value with that many elements
    // for a `size` rule, which `elements` turns into that length, as `measure` does.
    unsafe { core::hint::unreachable_unchecked() }
  };
  len
}

/// Returns the length of the tail of a `P` of `size` bytes, if `size` is the size of a `P`: the
/// size of a sized one, or of an unsized one with as many elements as fit in `size`, trailing
/// padding included, as `from_bytes` finds them.
#[cfg_attr(not(debug_assertions), inline(always))]
pub const fn sized_as<P: ?Sized + Pun>(size: usize) -> Option<usize> {
  let layout = const { P::LAYOUT };
  if !layout.is_unsized() {
    return if size == layout.size() { Some(0) } else { None };
  }
  let (head, stride) = (layout.head(), layout.stride());
  let len = match size & !(layout.align() - 1) {
    _ if stride == 0 => 0,
    room if room >= head => (room - head) / stride,
    _ => return None,
  };
  match layout.size_of_val(len) {
    Some(fits) if fits == size => Some(len),
    _ => None,
  }
}

/// Returns where the value of the pointee of the aim at a stored place `L` of the struct `T` at
/// `base` is, in bytes from the start of `input`, and how large the aim says it is: at the address
/// of the struct plus the value of the expression of the aim, an `i128` in two's complement, plus
/// what is before the value where the aim points (`Pointee::VALUE_AT`). The value must be within
/// the bytes of `input`, after what is stored before it (`Pointee::Prefix`). A pointee that takes
/// the rest of the tail of the struct is within the struct, which is why it is not claimed
/// ([`check_stored`]): it starts, with what it stores before its value, at or after the start of
/// the struct, and its value at or before the end of that tail.
///
/// # Safety
///
/// The caller must ensure that `base` is valid for reads of the `size` bytes of a `T`, which are
/// within the bytes of `input`, and that the bytes that its rules read are initialized.
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn stored_at<T: ?Sized + Pun, L: Rules>(
  base: *const u8,
  size: usize,
  input: &Input,
) -> Option<(usize, Size)> {
  // Named for every rule of a struct, and called for an aim at a stored place only.
  let (extent, before, value_at) = const {
    let (before, at) = (size_of::<<L::Pointee as Pointee>::Prefix>(), L::Pointee::VALUE_AT);
    match T::LAYOUT.rule(L::RULE) {
      Some(Rule::Aim(Aim { start: Start::Stored(_), extent, .. })) => (extent, before, at),
      _ => (Extent::Rest, before, at),
    }
  };
  // SAFETY: `base` is within the bytes of `input`, at or after their start, in one allocation, by
  // contract on the caller, as `offset_from_unsigned` requires.
  let from = unsafe { base.offset_from_unsigned(input.start) };
  // SAFETY: As for `pointer_at`, whose requirements are those of this call.
  let Some(to) = (unsafe { value_of::<T, L>(base, size, from, value_at) }) else { return None };
  if to < before || to > input.len {
    return None;
  }
  let wanted = match extent {
    Extent::Pointee => Size::Pointee,
    // SAFETY: `InRule<T, L, 1>` is the length or the size of the pointee, the second expression,
    // and the requirements of `eval` are those of this call, as above.
    Extent::Length(_) => match unsafe { eval::<InRule<T, L, 1>, L::Extent>(base, size) } {
      Some(n) => Size::Length(n),
      None => return None,
    },
    // SAFETY: As above.
    Extent::Bytes(_) => match unsafe { eval::<InRule<T, L, 1>, L::Extent>(base, size) } {
      Some(n) => Size::Bytes(n),
      None => return None,
    },
    // To the end of the tail of the struct, which is within its `size` bytes.
    Extent::Rest => {
      // An unsized struct ends with its tail, and the tail of a sized one is its last field.
      let end = from
        + if const { T::LAYOUT.is_unsized() } {
          size
        } else {
          const {
            let tail = T::LAYOUT.tail_shape();
            tail.start + tail.bytes
          }
        };
      // `before <= to`, checked above. A pointee that starts before its struct is not within it:
      // other aims may reach its bytes, which nothing claims.
      if to - before < from || to > end {
        return None;
      }
      Size::Bytes((end - to) as u128)
    }
  };
  Some((to, wanted))
}

/// Returns where the value of the pointee of the aim at a stored place `L` of the struct `T` at
/// `base` is, in bytes from where `from` counts, the struct being `from` bytes from there: the
/// value of the expression of the aim, an `i128` in two's complement, plus `value_at`, if it is not
/// negative and fits a `usize`, the arithmetic checked, so that no value of the expression
/// overflows.
///
/// # Safety
///
/// The caller must ensure the requirements of [`pointer_at`].
#[cfg_attr(not(debug_assertions), inline(always))]
pub const unsafe fn value_of<T: ?Sized + Pun, L: Rules>(
  base: *const u8,
  size: usize,
  from: usize,
  value_at: usize,
) -> Option<usize> {
  // SAFETY: The requirements of `pointer_at` are those of this call.
  let Some(at) = (unsafe { pointer_at::<T, L>(base, size) }) else { return None };
  let to = match at.checked_add(value_at as i128) {
    Some(at) => (from as i128).checked_add(at),
    None => None,
  };
  match to {
    #[expect(clippy::cast_sign_loss, reason = "not negative, as the guard finds")]
    Some(to) if to >= 0 && to <= usize::MAX as i128 => Some(to as usize),
    _ => None,
  }
}

/// How many parts of the layouts of pointees the check of eager aims nests, one within another,
/// at most: in place, and again in each of its loops.
///
/// It is the bound of the stack of [`from_head`](crate::from_head) and the other functions that
/// check bytes in place.
///
/// The check of a pointee in place nests the check of its parts in the one of the struct whose aim
/// leads to it, and the check of each part nests a few calls of the validation, whose frames depend
/// on the code of `puns` and on how it is compiled, not on the type. So the check counts the parts
/// of the layouts of the values that it nests, one within another (`Layout::parts`). A pointee that
/// holds no eager aim, or an empty slice, nests no other pointee, so it takes no parts: its check is
/// as deep as its own parts, a constant of its type, as the check of a value at the top of the
/// bytes is. Where a pointee of a type that holds an eager aim and has an identity would nest past
/// this many, it waits for a loop of its type instead, which nests a frame for each type on the
/// path, not for each value, and each loop counts its parts again from this many: the loops that
/// run at once are of different types, so the types bound them. What nests in a loop is the value
/// that it checks, the pointees of a type without an identity, and those for which none of the 64
/// entries of the loops is left. The check returns [`Error::Limit`](crate::Error::Limit) where these
/// nest past this many parts, whatever the bytes: a chain of a type without an identity, or a chain
/// each of whose values has several deep pointees of its type.
///
/// A check takes at most 37 KiB of stack in a build without optimizations, and at most 3 KiB with
/// them, whatever the depth of the data, and, for each loop of another type that runs within a
/// loop and for one pointee without eager aims, what its parts nest (Rust 1.99 on aarch64: the
/// stack of a thread filled with a pattern, then the deepest byte that a check changed, on data of
/// the shapes of `tests/aims.rs`, a list, a pair, trees, an object of a JSON document, a cycle of
/// three types, expressions and combs, 10 to 100,000 deep). A check that takes no loop takes what
/// a check without loops takes, at most 30 KiB and 1.2 KiB: the entries of the loops, 1.6 KiB on a
/// target of 64 bits, are in the frame of the first loop (`start`), which is never inlined. The
/// frames of the validation are about 0.9 times as large on x86-64 (1.08 times for the largest
/// ratio of a function), and about half on `thumbv7em-none-eabihf`, in the assembly of each target
/// without optimizations, so about 34 KiB, at most 40 KiB, and about 19 KiB. `tests/aims.rs` checks
/// data of each shape, past this many parts, on a thread of 64 KiB.
pub const AIM_PARTS: usize = 64;

/// How many parts of the layouts of a chain the check nests in place before a link of the chain
/// waits for a loop of its type ([`check_value`]). A loop checks a chain with no entry, and a check
/// of a chain of 1,000 links in a `const` takes 85 frames of evaluation with this many, as many as
/// for 10 links, within the 128 that Rust allows by default.
const IN_PLACE: usize = 24;

/// The bytes that a check reads, and what the check of eager aims may still spend in them.
///
/// The pointee of an aim at a stored place must lie within these bytes ([`stored_at`]). A pointee
/// that is claimed ([`Input::claim`]) shares no byte with another, so the claims of a check add up
/// to at most the bytes, and its time grows with them. A pointee at the cursor of its struct is
/// within it and shares no byte with the other pointees at that cursor, so it is checked once for
/// each check of its struct, as deep as the parts allow, and so is one that ends with the tail of a
/// struct that holds no other eager aim. A pointee that is not claimed may be reached again: that
/// of a shared aim, and one that ends with the tail of a struct that holds another eager aim, whose
/// pointee may share its bytes. Each of these takes its size from a budget of twice the
/// length of the bytes, plus [`AIM_PARTS`] ([`Input::spends`]), so that a check of a cycle through
/// them ends. So does each pointee of no byte that holds an eager aim, wherever it is, whose claim
/// moves nothing: it takes 1, so that pointees of no byte that each nest several others end, and a
/// chain of them as deep as the parts allow fits.
///
/// It is `repr(C)`, so that `budget` and `parts` are not next to each other: a check writes them
/// apart, and a load of both at once, which the compiler makes of two fields next to each other,
/// waits for both stores. That took 10% more cycles on a check that nests pointees in place, with
/// fewer instructions.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Input {
  /// The first byte.
  start: *const u8,
  /// What the pointees that are still to be checked may take, in bytes.
  budget: usize,
  /// The number of bytes.
  len: usize,
  /// How many more parts of the layouts of values the check may nest ([`AIM_PARTS`]).
  parts: usize,
  /// Whether the check stopped at a budget, rather than at an invalid value.
  limit: bool,
  /// The loops of the check, in which the pointees that hold eager aims wait ([`waits`]), in the
  /// frame of the first loop ([`start`]), or null while no loop runs.
  loops: *mut Loops,
  /// The end of the last claim of the sequence that the check is in ([`run`]), in bytes from
  /// `start`: where the next pointee may start ([`Input::claim`]).
  pub(crate) cursor: usize,
  /// Where the claims of that sequence must end, in bytes from `start`: the end of the bytes, or of
  /// the window of the pointee that waited whose sequence it is. The cursor is compared with it
  /// where the sequence ends ([`run`]), and with the end of the claims of the bytes where the check
  /// ends, by the reader whose claims end before that (`from_tail`).
  hi: usize,
  /// The end of the window of the pointee that waits since the last claim in the check of the
  /// value of a loop, which the next claim sets to where it starts, or null. While it is not null,
  /// it points to the `hi` of the next pointee of a loop or of an entry ([`Loops`]), in the frame
  /// of the first loop, which is valid for writes and does not move: only [`waits`] and [`run`]
  /// set it, and [`start`] sets it to null before that frame ends.
  open: *mut usize,
}

impl Input {
  /// The `len` bytes at `start`, the pointees of whose value are claimed from `cursor`
  /// ([`Input::claim`]): after the value, or from the start of the bytes if it ends them, whose
  /// reader then compares the cursor with where the value starts ([`Input::cursor`]).
  #[inline(always)]
  pub const fn new(start: *const u8, len: usize, cursor: usize) -> Self {
    Self {
      start,
      budget: len.saturating_mul(2).saturating_add(AIM_PARTS),
      len,
      parts: AIM_PARTS,
      limit: false,
      loops: core::ptr::null_mut(),
      cursor,
      hi: len,
      open: core::ptr::null_mut(),
    }
  }

  /// Returns the error of a check of these bytes that found `fault` ([`Fault`], not null):
  /// [`Error::Limit`] if it stopped at a budget, or else [`Error::Validity`] at the place of
  /// `fault`, in bytes from `start`.
  ///
  /// [`Error::Limit`]: crate::Error::Limit
  /// [`Error::Validity`]: crate::Error::Validity
  #[inline(always)]
  pub const fn error(&self, fault: Fault) -> crate::Error {
    if self.limit {
      return crate::Error::Limit;
    }
    // SAFETY: A check returns the address of a part that it read, a value, a struct or a pointee,
    // within the bytes of `input`, at or after `start`, in their allocation, as
    // `offset_from_unsigned` requires.
    crate::Error::Validity { at: unsafe { fault.offset_from_unsigned(self.start) } }
  }

  /// Claims the bytes of a pointee, from `from` to `to` in bytes from `start`, with what it stores
  /// before its value, in the order of a copy, if they start at or after the end of every claim
  /// before them in the sequence ([`run`]), the value that holds the first included. Whether they
  /// end within the window ([`Input::hi`]) is checked where the sequence ends. The pointee that
  /// waits since the last claim ([`Input::open`]) has its window end where this one starts. Only
  /// the check of a value of a loop finds one (`looped`), a constant where it is called, so that a
  /// check in place compiles no test of it.
  ///
  /// So each claim starts at or after the cursor, which never goes back: two claims share no byte,
  /// a pointee reached through two aims, or that overlaps a value claimed before it, starts before
  /// the cursor, and so does one that a cycle reaches again. A pointee of zero bytes holds no byte
  /// that a write could change, and may be reached twice.
  #[cfg_attr(not(debug_assertions), inline(always))]
  const fn claim(&mut self, from: usize, to: usize, looped: bool) -> bool {
    if from < self.cursor {
      return false;
    }
    if looped && !self.open.is_null() {
      // SAFETY: `open` is not null, so it is valid for writes, as the field says.
      unsafe { *self.open = from };
      self.open = core::ptr::null_mut();
    }
    self.cursor = to;
    true
  }

  /// Takes a pointee of `size` bytes that other aims may reach, or 1 for one of no byte, from the
  /// budget of bytes, if there are enough left.
  const fn spends(&mut self, size: usize) -> bool {
    let cost = if size == 0 { 1 } else { size };
    if self.budget < cost {
      self.limit = true;
      return false;
    }
    self.budget -= cost;
    true
  }

  /// Takes `parts` parts, if there are enough left, for a pointee that is checked in place, or for
  /// the values that a loop checks.
  const fn nest(&mut self, parts: usize) -> bool {
    if self.parts < parts {
      self.limit = true;
      return false;
    }
    self.parts -= parts;
    true
  }

  /// Gives back the `parts` that [`Input::nest`] took.
  const fn leave(&mut self, parts: usize) {
    self.parts += parts;
  }
}

/// Returns whether the pointee at `addr` is aligned to `align`, the alignment of its aim, by its
/// address. An alignment of 1 reads no address, which a check in `const` cannot read.
#[cfg_attr(not(debug_assertions), inline(always))]
const fn aligned(addr: *const u8, align: usize) -> bool {
  align <= 1 || ptr::is_aligned(addr, align)
}

/// Returns the number of elements of the tail of an unsized `D` whose `size` or `len` rule
/// ([`length`]) gives `value`, which fits a `usize`: the value of a `len` rule, or the elements
/// that make the size that a `size` rule gives, if they make exactly that size. A `D` without such
/// a rule has no length to read, and this function is not called for one.
#[cfg_attr(not(debug_assertions), inline(always))]
const fn elements<D: ?Sized + Pun>(value: usize) -> Option<usize> {
  let (stride, before, count) = const {
    let layout = D::LAYOUT;
    match layout.length_rule() {
      Some(rule) => (layout.stride(), layout.head() - rule.base, rule.elements),
      None => (0, 0, true),
    }
  };
  match () {
    () if count => Some(value),
    // Elements of size 0 make no size: a size tells no length but 0.
    () if stride == 0 => {
      if value == before {
        Some(0)
      } else {
        None
      }
    }
    () if value >= before && (value - before).is_multiple_of(stride) => {
      Some((value - before) / stride)
    }
    () => None,
  }
}

/// Returns whether `L` lists the rules of `T` that have an expression or are aims, from the
/// `from`th rule on.
///
/// Each must be listed once, in order, with every operation of each of its expressions
/// ([`covers`]), and an aim with a pointee of its size and alignment, and of its identity if it is
/// known.
const fn listed<T: ?Sized + Pun, L: Rules>(from: usize) -> bool {
  let rules = T::LAYOUT.rules();
  let mut i = from;
  while i < rules.len() && !rules[i].listed() {
    i += 1;
  }
  if L::RULE == usize::MAX {
    return i >= rules.len();
  }
  if L::RULE != i {
    return false;
  }
  let first = match rules[i].ops() {
    Some(_) => covers::<InRule<T, L>, L::Expr>(),
    None => <L::Expr as Steps>::KIND == crate::steps::NONE,
  };
  let second = match rules[i].extent_ops() {
    Some(_) => covers::<InRule<T, L, 1>, L::Extent>(),
    None => <L::Extent as Steps>::KIND == crate::steps::NONE,
  };
  let pointee = match rules[i].aim() {
    Some(aim) => {
      aim.size == L::Pointee::POINTEE_SIZE
        && aim.align == L::Pointee::POINTEE_ALIGN
        && (!aim.id.known() || aim.id.equals(&L::Pointee::POINTEE_ID))
    }
    None => true,
  };
  first && second && pointee && listed::<T, L::Next>(i + 1)
}

/// Returns whether `S` spans every operation of the expression `X`, from the first to the last.
///
/// That `S` has a walk where `X` has one, and only there, and the same of the expressions of each
/// walk, is asserted where each operation is applied ([`steps`], [`walk`]).
const fn covers<X: Ops, S: Steps>() -> bool {
  !X::OPS.is_empty() && S::FIRST == 0 && S::END == X::OPS.len()
}

/// Returns the value of the `size` or `len` rule that sets the length of the `T` at `base`.
///
/// The rule is that of `T`, or of its last field, and so on. [`layout::Layout::length_rule`]
/// tells which rule it is and where, and this function finds the same rule through the types, so
/// that `from_head` can find how long a value is.
///
/// # Errors
///
/// Returns `None` if the expression of the rule fails, as when it reads beyond the `n` bytes.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `base` is valid for reads of `n` bytes, initialized.
/// - These bytes cover the sized fields of the struct that has the rule.
/// - `T` has such a rule, by `length_rule`.
///
/// # Compile-time errors
///
/// An instance compiles only if, where the rule is not that of `T`, the last field of
/// `T::Fields` is the last field of the layout, and has that rule.
#[cfg_attr(not(debug_assertions), inline(always))]
pub const unsafe fn length<T: ?Sized + Pun>(base: *const u8, n: usize) -> Option<u128> {
  // The rule is the struct's own, or of its last field: the one that `length_rule` finds.
  let (own, offset) = const {
    let rule = T::LAYOUT.length_rule();
    let own = match rule {
      Some(rule) => rule.base == 0 && sizing::<T, T::Rules>() != usize::MAX,
      None => false,
    };
    let fields = T::LAYOUT.struct_fields();
    let offset = if fields.is_empty() { 0 } else { fields[fields.len() - 1].offset };
    if let (Some(rule), false) = (rule, own) {
      // The last field, by the types, is the last field of the layout, which has that rule.
      let last = <<T::Fields as Fields>::Last as Fields>::FIELD;
      let inner = <<<T::Fields as Fields>::Last as Fields>::Type as Pun>::LAYOUT.length_rule();
      assert!(
        matches!(last, Some(field) if field.offset() == offset)
          && matches!(inner, Some(inner) if inner.base + offset == rule.base
            && inner.elements == rule.elements
            && layout::same_ops(inner.ops, rule.ops)),
        "not the last field of the struct"
      );
    }
    (own, offset)
  };
  if own {
    // SAFETY: `own` is `true`, so the rule is that of `T`. The requirements of `length_listed` are
    // those of this call.
    return unsafe { length_listed::<T, T::Rules>(base, n) };
  }
  // SAFETY: The rule is that of the last field of `T`, at `offset`, whose type has the rule,
  // which the assertion above shows. The `n` bytes at `base` cover the sized fields of the struct
  // that has the rule, which is in that field, by contract on the caller. So `offset <= n`, and
  // `add(offset)` stays within the `n` bytes, as `add` requires. The `n - offset` bytes from there
  // are valid for reads, initialized, and cover the same sized fields. These are the
  // requirements of `length` for the type of the last field.
  unsafe { length::<<<T::Fields as Fields>::Last as Fields>::Type>(base.add(offset), n - offset) }
}

/// Returns the index of the first `size` or `len` rule of `T` in `L`, or `usize::MAX` if there
/// is none.
const fn sizing<T: ?Sized + Pun, L: Rules>() -> usize {
  match T::LAYOUT.rule(L::RULE) {
    _ if L::RULE == usize::MAX => usize::MAX,
    Some(Rule::Size(_) | Rule::Len(_)) => L::RULE,
    _ => sizing::<T, L::Next>(),
  }
}

/// Returns the value of the first `size` or `len` rule of `T` in `L`.
///
/// # Errors
///
/// Returns `None` if there is no such rule or if its expression fails.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The requirements of [`length`] hold.
///
/// # Compile-time errors
///
/// An instance compiles only if `T::Rules` lists the rules of `T` ([`listed`]).
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn length_listed<T: ?Sized + Pun, L: Rules>(
  base: *const u8,
  n: usize,
) -> Option<u128> {
  if const { L::RULE == usize::MAX } {
    return None;
  }
  if const { matches!(T::LAYOUT.rule(L::RULE), Some(Rule::Size(_) | Rule::Len(_))) } {
    const { assert!(listed::<T, T::Rules>(0), "not the rules of the struct") };
    // SAFETY: `InRule<T, L>` is a rule of `T`, the struct that has the rule. `base` is valid for
    // reads of `n` initialized bytes, which cover its sized fields, by contract on the caller.
    // This is what `eval` requires.
    return unsafe { eval::<InRule<T, L>, L::Expr>(base, n) };
  }
  // SAFETY: The requirements of the call for the next rules are those of this call.
  unsafe { length_listed::<T, L::Next>(base, n) }
}

/// Returns the step of the cursor ([`cursor_step`]) to the pointee of the aim at the cursor that
/// the rule `rule` of the struct `T` at `base` is: where its value starts, in bytes from `base`,
/// how many bytes it has, and its length, as the check of the struct finds them, for what reads
/// the pointee.
///
/// The struct is `size` bytes. Its tail starts at `head` and is `tail` bytes long. The search
/// starts at the rule `L`, after the pointees of aims at the cursor that end at `cursor` in the
/// tail. A step never fails here: a pointee that does not fit in the tail is truncated to it, and
/// one whose size fails takes what is left, since safe code may have written the fields that give
/// it. So the pointee ends within the tail, at most `size` bytes from `base`.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `base` is valid for reads of the `size` bytes of a `T`, the bytes that its rules read are
///   initialized, and so are the bytes of its tail.
/// - `head + tail` is at most `size`, and `cursor` is at most `tail`.
///
/// # Compile-time errors
///
/// An instance compiles only if `T::Rules` lists the rules of `T` ([`listed`]), and the rule
/// `rule` is an aim at the cursor.
#[cfg_attr(not(debug_assertions), inline(always))]
pub const unsafe fn region_at<T: ?Sized + Pun, L: Rules>(
  base: *const u8,
  size: usize,
  (head, tail): (usize, usize),
  rule: usize,
  cursor: usize,
) -> Step {
  const { assert!(listed::<T, T::Rules>(0), "not the rules of the struct") };
  let next =
    const { matches!(T::LAYOUT.rule(L::RULE), Some(Rule::Aim(Aim { start: Start::Next(_), .. }))) };
  assert!(L::RULE != usize::MAX, "no such aim");
  let cursor = if next {
    // SAFETY: The requirements of `cursor_step` are those of this call.
    let step = unsafe { cursor_step::<T, L, false>(base, size, (head, tail), cursor) };
    let Some(step) = step else { unreachable!() };
    if L::RULE == rule {
      return step;
    }
    step.end
  } else {
    cursor
  };
  // SAFETY: `cursor <= tail`: a step ends within the tail. The other requirements of the call for
  // the next rules are those of this call.
  unsafe { region_at::<T, L::Next>(base, size, (head, tail), rule, cursor) }
}

/// Returns where the pointee of the aim at a stored place `L` of the struct `T` at `base` starts,
/// in bytes from `base`: the value of its expression, an `i128` in two's complement, or `None` if
/// the expression fails. A check of the struct found the same ([`stored_at`]).
///
/// # Safety
///
/// The caller must ensure that `base` is valid for reads of the `size` bytes of a `T`, and that
/// the bytes that its rules read are initialized.
///
/// # Compile-time errors
///
/// An instance compiles only if `T::Rules` lists the rules of `T` ([`listed`]). It returns `None`
/// where `L` is not an aim at a stored place.
#[cfg_attr(not(debug_assertions), inline(always))]
pub const unsafe fn pointer_at<T: ?Sized + Pun, L: Rules>(
  base: *const u8,
  size: usize,
) -> Option<i128> {
  let stored = const {
    assert!(listed::<T, T::Rules>(0), "not the rules of the struct");
    matches!(T::LAYOUT.rule(L::RULE), Some(Rule::Aim(Aim { start: Start::Stored(_), .. })))
  };
  if !stored {
    return None;
  }
  // SAFETY: `InRule<T, L>` is a rule of `T`. `base` is valid for reads of the `size` bytes of a
  // `T`, which cover its sized fields, and the bytes that the rule reads are initialized, by
  // contract on the caller. This is what `eval` requires.
  #[expect(clippy::cast_possible_wrap, reason = "two's complement, as the aim says")]
  match unsafe { eval::<InRule<T, L>, L::Expr>(base, size) } {
    Some(at) => Some(at as i128),
    None => None,
  }
}

/// Returns the length or the size in bytes that the aim `L` of the struct `T` at `base` gives its
/// pointee, the value of its second expression, or `None` if it fails.
///
/// # Safety
///
/// The caller must ensure that `base` is valid for reads of the `size` bytes of a `T`, and that
/// the bytes that its rules read are initialized.
///
/// # Compile-time errors
///
/// An instance compiles only if `T::Rules` lists the rules of `T` ([`listed`]). It returns `None`
/// where `L` is not an aim whose size is given.
#[cfg_attr(not(debug_assertions), inline(always))]
pub const unsafe fn given<T: ?Sized + Pun, L: Rules>(base: *const u8, size: usize) -> Option<u128> {
  let given = const {
    assert!(listed::<T, T::Rules>(0), "not the rules of the struct");
    matches!(
      T::LAYOUT.rule(L::RULE),
      Some(Rule::Aim(Aim { extent: Extent::Length(_) | Extent::Bytes(_), .. }))
    )
  };
  if !given {
    return None;
  }
  // SAFETY: `InRule<T, L, 1>` is the second expression of the rule, whose operations are
  // `L::Extent`. The requirements of `eval` are those of this call.
  unsafe { eval::<InRule<T, L, 1>, L::Extent>(base, size) }
}

// -------------------------------------------------------------------------------------------------
// Expressions

/// Returns the value of the expression `X`, whose operations are `S`, over the struct at `base`.
///
/// The struct is `size` bytes. For a `size` or `len` rule, `size` is the number of bytes that can
/// be read. Each operation is a constant of an instance of [`steps`] or [`walk`] of its own, in
/// which it is the only operation compiled. So what is compiled is the arithmetic of the
/// expression and the loops of its walks, never an interpreter. The recursions go down a
/// balanced tree of the operations, so they are as deep as the logarithm of their number.
///
/// # Errors
///
/// Returns `None` if the expression fails, as each [`Op`] documents: for example an overflow, a
/// division by zero, a shift by 128 or more, a read out of the `size` bytes, or a walk that does
/// not end.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `base` is valid for reads of `size` bytes.
/// - These bytes cover the sized fields of the struct of which `X` is a rule.
/// - The bytes that `X` reads are initialized: those of the fields that it loads and, if it reads
///   bytes by offset ([`Layout::rules_read_bytes`]), all `size` bytes.
///
/// [`Layout::rules_read_bytes`]: layout::Layout::rules_read_bytes
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn eval<X: Ops, S: Steps>(base: *const u8, size: usize) -> Option<u128> {
  let mut machine = Machine::NEW;
  // SAFETY: The requirements of `steps` are those of this call.
  if unsafe { steps::<X, S>(&mut machine, base, size) } { Some(machine.result()) } else { None }
}

/// Applies the operations `S` of the expression `X` to `machine`.
///
/// # Errors
///
/// Returns `false` if one of the operations fails. That failure is the failure of the
/// expression.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The requirements of [`eval`] hold.
///
/// # Compile-time errors
///
/// An instance compiles only if `S` is a walk exactly where `X` has an [`Op::Walk`].
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn steps<X: Ops, S: Steps>(
  machine: &mut Machine,
  base: *const u8,
  size: usize,
) -> bool {
  // An operation that `S` says is not a walk must not be one. This makes every walk applied by
  // `walk`, since `Machine::apply` does not apply walks.
  const {
    assert!(
      S::KIND != ONE || matches!(op_at(X::OPS, S::AT), Some(op) if !matches!(op, Op::Walk { .. })),
      "not the operations of the expression"
    );
  };
  // SAFETY: `op` is the operation at `S::AT` of `X::OPS`, the expression of a rule in the layout of
  // a struct, or of a walk in one (`InRule`, `InWalk`). `Layout::structure` accepted these
  // operations (`check_ops`): a load reads 1 to 16 bytes within the sized fields. `base` is valid
  // for reads of `size` bytes, which cover the sized fields of the struct, by contract on the
  // caller. `op` is an operation of `X`, so the bytes that it reads are initialized, by contract on
  // the caller. These are the requirements of `Machine::apply`. The requirements of the calls
  // for the parts and the walks are those of this call.
  unsafe {
    match const { S::KIND } {
      ONE => match const { op_at(X::OPS, S::AT) } {
        Some(op) => machine.apply(op, S::AT, const { Planned::<X>::PLAN.how[S::AT] }, base, size),
        None => false,
      },
      TWO => {
        if !steps::<X, S::A>(machine, base, size) {
          return false;
        }
        steps::<X, S::B>(machine, base, size)
      }
      WALK => walk::<X, S>(machine, base, size),
      _ => true,
    }
  }
}

/// Applies the [`Op::Walk`] `W` of the expression `X` to `machine`.
///
/// The state starts as the top of the stack, and while the condition holds, the next expression
/// replaces it. A walk in a part of the expression that is not computed does nothing.
///
/// # Errors
///
/// Returns `false` if the condition or the next expression fails, or if the walk takes more steps
/// than [`walk_limit`] allows for `size`.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The requirements of [`eval`] hold.
///
/// # Compile-time errors
///
/// An instance compiles only if `X` has an [`Op::Walk`] where `W` is a walk, and `W` spans every
/// operation of its two expressions.
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn walk<X: Ops, W: Steps>(
  machine: &mut Machine,
  base: *const u8,
  size: usize,
) -> bool {
  // A walk where `W` says, whose expressions are every operation of both of its own.
  const {
    assert!(
      W::KIND != WALK
        || matches!(op_at(X::OPS, W::AT), Some(Op::Walk { .. }))
          && covers::<W::InCond<X>, W::Cond>()
          && covers::<W::InThen<X>, W::Then>(),
      "not the operations of the expression"
    );
  };
  let Some(Op::Walk { var, .. }) = (const { op_at(X::OPS, W::AT) }) else {
    return false;
  };
  // The walk is in a part of the expression that is not computed.
  if !machine.computes(W::AT) {
    return true;
  }
  // The width that the state fits, which the optimizer cannot find in a loop.
  let fits = const { Planned::<X>::PLAN.how[W::AT].result };
  let mut state = fit(machine.top(), fits);
  let (mut steps, limit) = (0, walk_limit(size));
  let result = loop {
    let (mut cond, mut next) = (machine.enter(var, state), machine.enter(var, state));
    // SAFETY: The condition of the walk is an expression of the same rule, over the same bytes.
    // The requirements of `steps` are those of this call.
    if !unsafe { self::steps::<W::InCond<X>, W::Cond>(&mut cond, base, size) } {
      break None;
    }
    if cond.result() == 0 {
      break Some(state);
    }
    if steps == limit {
      break None;
    }
    steps += 1;
    // SAFETY: As for the condition above.
    if !unsafe { self::steps::<W::InThen<X>, W::Then>(&mut next, base, size) } {
      break None;
    }
    state = fit(next.result(), fits);
  };
  machine.replace(result)
}

// -------------------------------------------------------------------------------------------------

/// Returns the length of the tail of the struct at `ptr`, the size of that tail in bytes, and the
/// extent of the struct.
///
/// The tail of a `Sized` struct is its last field, and its extent is its size. The extent of an
/// unsized struct ends with its slice, without the padding that may follow it, which
/// `size_of_val` counts.
#[cfg_attr(not(debug_assertions), inline(always))]
pub const fn tail_of<T: ?Sized + Pun>(ptr: *const T) -> (usize, usize, usize) {
  let (layout, shape) = const { (T::LAYOUT, T::LAYOUT.tail_shape()) };
  if const { T::LAYOUT.is_unsized() } {
    // The value exists: its size does not overflow.
    let len = ptr::metadata(ptr);
    (len, len * layout.stride(), layout.head() + len * layout.stride())
  } else {
    (shape.len, shape.bytes, layout.size())
  }
}
