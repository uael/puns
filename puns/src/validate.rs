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
//! # Opaque scalars and positions
//!
//! Two constants of [`Checks`] change what a check does, each fixed for an instance. Where
//! `OPAQUE` is false, an opaque scalar is never accepted ([`accepts`]), so a variant that holds one
//! fails. Where it is true, as for a [`Deferred<T>`](crate::Deferred) target, an opaque scalar is
//! checked against its values, as any scalar is. A check returns a [`Failure`]: null if the value is
//! valid, otherwise the address of the part that fails where `POSITION` is true, or one constant
//! address that is not null where it is false. The test for null then folds into the `bool` that
//! a check without positions computed before, and the machine code of `examples/codegen.rs` does
//! not change. The elements of a slice are checked in blocks without a branch. After a block fails,
//! [`locate`] checks its elements one by one, for the position, out of line and only where
//! `POSITION` is true.
//!
//! # No interpreter
//!
//! Every operation of every rule is applied by an instance of a function of its own ([`steps`]), in
//! which the operation is a constant. What is compiled is the arithmetic of the expression and the
//! loop of each walk, never a `match` on operations at run time.
//!
//! The derive writes the rules of a struct that have an expression as a list at the type level
//! (`Pun::Rules`, `RuleAt<RULE, STEPS, NEXT>`). It writes the operations of each rule as a balanced
//! tree (`steps::One<I>`, `steps::Two<A, B>`), and a walk as `steps::Walk<I, COND, NEXT>`, whose
//! two expressions are trees of their own at any depth. The recursions down a tree are as deep as
//! the logarithm of the number of operations. Because the list is a type, a struct without rules
//! costs nothing more to compile.
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

use core::marker::PhantomData;

use crate::{
  Fields, Path, Pun, Root, Rules, Steps, Variants,
  analysis::Marks,
  layout::{self, Dfa, Enum, Kind, Machine, Op, Rule, Shape, Spec, load, op_at, walk_limit},
  narrow::{Planned, fit},
  ptr,
  steps::{InRule, ONE, Ops, TWO, WALK},
};

/// The parts of a target that a validation checks.
pub trait Checks {
  /// The leaves to check (scalars, patterns, enums, unions and rules), by index in the target.
  const MARKS: Marks;
  /// Whether an opaque scalar ([`Spec::opaque`](layout::Spec::opaque)) is checked against its
  /// values, instead of refused unread: a check that makes no value, for the crate that proves
  /// the scalar.
  const OPAQUE: bool = false;
  /// Whether a failure returns where it is ([`validate_at`]), instead of an address that only says
  /// that it failed ([`failed`]).
  const POSITION: bool = false;
}

/// The [`Checks`] that check every part, an opaque scalar against its values.
pub struct AllButOpaque;

impl Checks for AllButOpaque {
  const MARKS: Marks = Marks::ALL;
  const OPAQUE: bool = true;
}

/// The [`Checks`] that check every part.
pub struct All;

impl Checks for All {
  const MARKS: Marks = Marks::ALL;
}

/// The [`Checks`] of `C`, whose failures return where they are.
pub struct Located<C>(PhantomData<C>);

impl<C: Checks> Checks for Located<C> {
  const MARKS: Marks = C::MARKS;
  const OPAQUE: bool = C::OPAQUE;
  const POSITION: bool = true;
}

/// Where a check failed: null if it did not. Otherwise, if [`Checks::POSITION`], the address of
/// the part that failed: a scalar, a field restricted by `#[pun(valid(..))]`, a struct whose rule
/// does not hold, an enum whose tag is no variant that has values, a union none of whose fields
/// is valid, or a pattern. Otherwise any address that is not null ([`failed`]).
pub type Failure = *const u8;

/// Returns the [`Failure`] of the part at `at`: `at` if `C` asks where, and otherwise one address
/// that is not null, the same for every part.
///
/// Where `C` does not ask where, every failure is then the same constant, and a caller that tests
/// it for null compiles to what a `bool` would. The address is never read.
#[inline(always)]
const fn failed<C: Checks>(at: *const u8) -> Failure {
  if C::POSITION { at } else { core::ptr::dangling() }
}

/// Returns whether the scalar `spec` at `ptr` is valid as `C` checks it.
///
/// An opaque scalar is refused unread, unless `C` checks it against its values ([`Checks::OPAQUE`]):
/// no function that makes a value accepts one.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - If the scalar is not opaque, or `C::OPAQUE` is `true`, the requirements of `Spec::matches`.
#[inline(always)]
const unsafe fn accepts<C: Checks>(spec: Spec, ptr: *const u8) -> bool {
  // SAFETY: `matches` is called only if the scalar is not opaque or `C::OPAQUE` is `true`, and its
  // requirements then hold, by contract on the caller.
  (!spec.is_opaque() || C::OPAQUE) && unsafe { spec.matches(ptr) }
}

/// Returns whether the parts of the `T` at `ptr` that `C` marks are valid.
///
/// It is [`validate_at`], of which it keeps only whether the check failed.
///
/// # Safety
///
/// The requirements of [`validate_at`].
pub const unsafe fn validate<C: Checks, P: Path, T: ?Sized + Pun>(ptr: *const T) -> bool {
  // SAFETY: The requirements of `validate_at`, by contract on the caller.
  unsafe { validate_at::<C, P, T>(ptr).is_null() }
}

/// Returns where the parts of the `T` at `ptr` that `C` marks are not valid: null if they are.
///
/// `P` is where `T` is in the target, which tells the indexes of its leaves. Parts that `C` does
/// not mark are not checked, and their checks are not compiled. An enum and a union are checked
/// as a whole once one of their leaves is marked. The [`Failure`] is the first part that fails,
/// in the order of the checks: the fields of a struct in order, then its rules.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `ptr` is valid for reads of a `T`. It need not be aligned.
/// - Every byte that a check marked by `C` reads is initialized. A marked scalar reads its own
///   bytes, unless it is opaque and `C::OPAQUE` is `false`: it is then refused unread. A marked
///   pattern reads its tail. An enum or a union with a marked leaf reads its tag and what the checks
///   of its variants read, which check every part. Nothing is read of a variant, or of a field of a
///   union, that has no value. A struct whose rules are marked reads the fields that its rules load
///   and, if one of its rules reads bytes by offset ([`Layout::rules_read_bytes`]), all of its
///   bytes, padding included.
///
/// [`Layout::rules_read_bytes`]: layout::Layout::rules_read_bytes
pub const unsafe fn validate_at<C: Checks, P: Path, T: ?Sized + Pun>(ptr: *const T) -> Failure {
  if const { !C::MARKS.any(P::LEAF, T::LAYOUT.leaves) } {
    return core::ptr::null();
  }
  // SAFETY: `ptr` is valid for reads of a `T`, and every byte that a marked check reads is
  // initialized, by contract on the caller. A leaf of `T` is marked, since the function did not
  // return above. `T::LAYOUT` is the layout of `T`, and `T::Elem`, `T::Fields` and `T::Variants`
  // are its lists (`Pun`, section Safety). For each shape:
  // - A scalar is `T` itself, of `spec.size` bytes, and is marked. If it is opaque and `C::OPAQUE`
  //   is `false`, nothing reads it. Any other scalar reads its own bytes, so they are initialized,
  //   as `accepts` requires.
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
  unsafe {
    let at = ptr.cast::<u8>();
    // Each arm returns its own failure. One failure for every arm, returned at the end of the
    // function, costs an instruction in the machine code of `examples/codegen.rs`
    // (`from_tail::<Trailer>`).
    match const { T::LAYOUT.shape() } {
      Shape::Scalar(spec) => {
        if accepts::<C>(spec, at) {
          core::ptr::null()
        } else {
          failed::<C>(at)
        }
      }
      Shape::Array(len) => validate_elems::<C, P, T::Elem>(ptr.cast(), len),
      Shape::Slice => validate_elems::<C, P, T::Elem>(ptr.cast(), ptr::metadata(ptr)),
      Shape::Struct => {
        let fields = validate_fields::<C, P, T, T::Fields>(ptr);
        if !fields.is_null() {
          return fields;
        }
        if (const { !rules_marked::<C, P, T>() }) || check_rules(ptr) {
          core::ptr::null()
        } else {
          failed::<C>(at)
        }
      }
      // `str::from_utf8` checks the bytes where the automaton is that of UTF-8, by its states,
      // since an `id` proves nothing. This is decided at compile time.
      Shape::Pattern(_) if const { matches!(T::LAYOUT.kind, Kind::Pattern(dfa) if dfa.is_utf8()) } => {
        if Dfa::matches_utf8(at, ptr::metadata(ptr)) {
          core::ptr::null()
        } else {
          failed::<C>(at)
        }
      }
      Shape::Pattern(dfa) => {
        if dfa.matches(at, ptr::metadata(ptr)) {
          core::ptr::null()
        } else {
          failed::<C>(at)
        }
      }
      Shape::Enum(layout) if layout.union => {
        if validate_union::<C, T, T::Variants>(ptr) {
          core::ptr::null()
        } else {
          failed::<C>(at)
        }
      }
      Shape::Enum(layout) => validate_enum::<C, T>(ptr, layout),
    }
  }
}

/// Returns where the enum at `ptr` fails: null if it has the tag of one of its variants and the
/// fields of that variant are valid, the enum if no variant that has values has its tag, and
/// otherwise the part of the variant that fails.
///
/// Every part of the variant is checked, since nothing is known of what is in an enum, with an
/// opaque scalar refused unless `C` checks it against its values.
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
/// - `layout` is the layout of `T`.
/// - The tag of the enum is initialized, and so is every byte that the checks of the fields of that
///   tag's variant read, which check every part.
#[inline(always)]
const unsafe fn validate_enum<C: Checks, T: ?Sized + Pun>(ptr: *const T, layout: &Enum) -> Failure {
  let Kind::Scalar(spec) = layout.tag.layout.kind else { panic!() };
  // SAFETY: `Layout::enumeration` asserts that the tag is a scalar that `Spec::is_integer`
  // accepts, so of at most 16 bytes, at `layout.tag.offset` within the size of `T`. So `add`
  // stays within the `T` at `ptr`, and the tag is valid for reads. The tag is initialized, and so
  // is what the checks of the fields of its variant read, by contract on the caller. These are
  // the requirements of `load` and of `validate_variants`, with `T::Variants` the list of the
  // variants of `T`.
  unsafe {
    let tag = load(ptr.cast::<u8>().add(layout.tag.offset), spec.size, spec.endian);
    validate_variants::<C, T, T::Variants>(ptr, tag)
  }
}

/// Returns where the variant of tag `tag`, among `V` and the ones after it, fails in the enum at
/// `ptr`: null if its fields are valid, with an opaque scalar refused unless `C` checks it against
/// its values.
///
/// Fails at the enum if no such variant has the tag. A variant whose tag is a range of values (the
/// variant of a niche) has the tag if `tag` is in that range. Fails at the enum too, without
/// reading anything, if the variant that has the tag has no value, because one of its fields has
/// none.
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
) -> Failure {
  let Some(expected) = (const { V::TAG }) else { return failed::<C>(ptr.cast()) };
  let this = match const { V::RANGES } {
    None => tag == expected,
    // The variant of a niche, or one that no value is (no range).
    Some(ranges) => layout::contains(ranges, tag),
  };
  // SAFETY: `ptr` is valid for reads of a `T`, and `V` is in the list of the variants of `T`, by
  // contract on the caller. If `this`, `V` has the tag `tag`, and `V::Fields` are its fields
  // (`Variants`, section Safety). What their checks read is initialized, by contract on the caller,
  // as `validate_fields` requires. Otherwise the requirements of the call for the next variants are
  // those of this call.
  unsafe {
    if !this {
      validate_variants::<C, T, V::Next>(ptr, tag)
    } else if const { inhabited::<V::Fields>() } {
      every::<C, T, V::Fields>(ptr)
    } else {
      // No value is a variant of which a field has no value, so nothing of it is read. The
      // analysis does not look at such a variant either (`Variant::is_present`).
      failed::<C>(ptr.cast())
    }
  }
}

/// Returns whether every field of the list `F` has values, so that a value of a variant of these
/// fields exists.
const fn inhabited<F: Fields>() -> bool {
  match F::FIELD {
    Some(field) => field.layout.is_inhabited() && inhabited::<F::Next>(),
    None => true,
  }
}

/// Returns where a part of the fields `F` of the `T` at `ptr` fails, every part checked, with an
/// opaque scalar refused unless `C` checks it against its values, and positions if `C` asks.
///
/// # Safety
///
/// The requirements of `validate_fields` with every part marked.
#[inline(always)]
const unsafe fn every<C: Checks, T: ?Sized + Pun, F: Fields>(ptr: *const T) -> Failure {
  // SAFETY: The requirements of `validate_fields`, by contract on the caller.
  unsafe {
    match const { (C::OPAQUE, C::POSITION) } {
      (true, false) => validate_fields::<AllButOpaque, Root, T, F>(ptr),
      (false, false) => validate_fields::<All, Root, T, F>(ptr),
      (true, true) => validate_fields::<Located<AllButOpaque>, Root, T, F>(ptr),
      (false, true) => validate_fields::<Located<All>, Root, T, F>(ptr),
    }
  }
}

/// Returns whether the union at `ptr` is a valid value of one of its fields, `V` or one after it.
/// A union holds no opaque scalar (`Layout::union` asserts it), so `C::OPAQUE` changes nothing
/// here. A union that fails fails as a whole, since no field tells which one it was meant to be.
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
const unsafe fn validate_union<C: Checks, T: ?Sized + Pun, V: Variants>(ptr: *const T) -> bool {
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
    (const { inhabited::<V::Fields>() } && every::<C, T, V::Fields>(ptr).is_null())
      || validate_union::<C, T, V::Next>(ptr)
  }
}

/// Returns where the parts that `C` marks of the `len` elements at `ptr` fail: null if they are
/// valid.
///
/// Elements whose checks are a few comparisons are checked in blocks of about 64 bytes, without a
/// branch within a block. The result is the same as one element at a time. If a block fails and
/// `C` asks where, its elements are checked again one at a time ([`locate`]), only then.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `ptr` is valid for reads of `len` consecutive `T`s, each as [`validate_at`] requires.
const unsafe fn validate_elems<C: Checks, P: Path, T: Pun>(ptr: *const T, len: usize) -> Failure {
  let mut i = 0;
  // Without a stop within a block, the optimizer can vectorize the checks, which it cannot do in a
  // loop that may stop at each element.
  let block = const { T::LAYOUT.block() };
  if block > 1 {
    while len - i >= block {
      let (mut valid, mut j) = (true, 0);
      while j < block {
        // SAFETY: `i + j < len`, since `len - i >= block` and `j < block`. `ptr` is valid for
        // reads of `len` elements, by contract on the caller, so `add(i + j)` stays within them
        // and points to an element that meets the requirements of `validate`.
        valid &= unsafe { validate::<C, P, T>(ptr.add(i + j)) };
        j += 1;
      }
      if !valid {
        return if C::POSITION {
          // SAFETY: The `block` elements from `i` are within the `len` elements at `ptr`, as
          // above, each as `validate_at` requires, as `locate` requires.
          unsafe { locate::<C, P, T>(ptr.add(i), block) }
        } else {
          failed::<C>(ptr.cast())
        };
      }
      i += block;
    }
  }
  while i < len {
    // SAFETY: `i < len`. `ptr` is valid for reads of `len` elements, by contract on the caller,
    // so `add(i)` stays within them and points to an element that meets the requirements of
    // `validate_at`.
    let failure = unsafe { validate_at::<C, P, T>(ptr.add(i)) };
    if !failure.is_null() {
      return failure;
    }
    i += 1;
  }
  core::ptr::null()
}

/// Returns where the first of the `len` elements at `ptr` that fails fails, after a block of them
/// failed: the check of the block keeps no position, so that it has no branch.
///
/// It runs only after a failure, so it is out of line, and compiled away where `C` does not ask
/// where.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `ptr` is valid for reads of `len` consecutive `T`s, each as [`validate_at`] requires.
#[cold]
#[inline(never)]
const unsafe fn locate<C: Checks, P: Path, T: Pun>(ptr: *const T, len: usize) -> Failure {
  let mut i = 0;
  while i < len {
    // SAFETY: `i < len`. `ptr` is valid for reads of `len` elements, by contract on the caller,
    // so `add(i)` stays within them and points to an element that meets the requirements of
    // `validate_at`.
    let failure = unsafe { validate_at::<C, P, T>(ptr.add(i)) };
    if !failure.is_null() {
      return failure;
    }
    i += 1;
  }
  // The block failed, so one of its elements does: not reached.
  failed::<C>(ptr.cast())
}

/// Returns where the parts that `C` marks of the field `F` of the `T` at `ptr`, and of the fields
/// after it, fail: null if they are valid.
///
/// A field restricted by `#[pun(valid(..))]` is checked against its restriction, not against its
/// type, and fails at its place.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The requirements of [`validate_at`] hold, with the same `C`, `P` and `T`.
/// - `F` is a field of `T`: of the struct, or of the variant of the enum that the value at `ptr`
///   is.
///
/// # Compile-time errors
///
/// An instance compiles only if `F::Path<P>` is the leaf of the field in the layout. The derive
/// writes the list with paths that the user may redefine, so this is checked.
const unsafe fn validate_fields<C: Checks, P: Path, T: ?Sized + Pun, F: Fields>(
  ptr: *const T,
) -> Failure {
  let Some(field) = (const { F::FIELD }) else { return core::ptr::null() };
  // Where the list says that the field is must be where its layout says it is: the list is made
  // by the derive with paths it is told (`#[pun(crate = ..)]`), which could name another `At`.
  const {
    let leaf = match F::FIELD {
      Some(field) => P::LEAF + field.leaf(),
      None => P::LEAF,
    };
    assert!(<F::Path<P> as Path>::LEAF == leaf, "not where the field is");
  };
  // SAFETY: `F` is a field of `T`, by contract on the caller, and `field` is its place in the
  // layout of `T`, `F::FIELD` (`Fields`, section Safety). `ptr` points to the start of a `T` that
  // is valid for reads, by the requirements of `validate_at`, so in one allocation, and `T` has a
  // field of type `F::Type` at `field.offset()`. These are the requirements of `ptr::field` and
  // `add`, and that field is valid for reads.
  // - A restricted field is a scalar leaf. It is read only if it is marked, and is not an opaque
  //   scalar that `C` refuses unread, so its bytes are initialized, as `accepts` requires.
  // - Any other field is a `F::Type` at the place `F::Path<P>`, which the assertion above shows
  //   is the leaf of the field. What `C` marks there is what it marks of the field in `T`. So the
  //   field meets the requirements of `validate_at`.
  // The requirements of the call for the next fields are those of this call.
  unsafe {
    let failure = match field.restriction() {
      // A scalar that accepts less than its type does: there is no type to recurse into.
      Some(spec) => {
        let at = ptr.cast::<u8>().add(field.offset());
        if (const { !C::MARKS.any(<F::Path<P> as Path>::LEAF, 1) }) || accepts::<C>(spec, at) {
          core::ptr::null()
        } else {
          failed::<C>(at)
        }
      }
      None => validate_at::<C, F::Path<P>, F::Type>(ptr::field::<T, F::Type>(ptr, field.offset())),
    };
    if !failure.is_null() {
      return failure;
    }
    validate_fields::<C, P, T, F::Next>(ptr)
  }
}

/// Returns whether `C` marks the rules of the struct `T` at the place `P`.
///
/// The rules of a struct are its last leaf.
const fn rules_marked<C: Checks, P: Path, T: ?Sized + Pun>() -> bool {
  !T::LAYOUT.rules().is_empty() && C::MARKS.any(P::LEAF + T::LAYOUT.leaves - 1, 1)
}

/// Returns whether the struct at `ptr` meets the rules of the layout of `T`.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `ptr` is valid for reads of a `T`. It need not be aligned.
/// - The bytes of that `T` that its rules read are initialized: those of the fields that the rules
///   load and, if one of the rules reads bytes by offset ([`Layout::rules_read_bytes`]), all of
///   them, padding included.
///
/// # Compile-time errors
///
/// An instance compiles only if `T::Rules` lists every rule of `T` that has an expression, once,
/// in order, with every operation of each ([`listed`]).
///
/// [`Layout::rules_read_bytes`]: layout::Layout::rules_read_bytes
#[inline(always)]
pub const unsafe fn check_rules<T: ?Sized + Pun>(ptr: *const T) -> bool {
  const { assert!(listed::<T, T::Rules>(0), "not the rules of the struct") };
  let (len, tail, size) = tail_of(ptr);
  // SAFETY: `tail_of` returns `size`, the size of the `T` at `ptr`. That `T` is valid for reads,
  // and the bytes that its rules read are initialized, by contract on the caller, as
  // `check_listed` requires.
  unsafe { check_listed::<T, T::Rules>(ptr.cast(), size, tail, len, 0) }
}

/// Returns whether the struct at `base` meets the rule `L` of `T` and the ones after it.
///
/// The struct is `size` bytes, and its tail is `tail` bytes of `len` elements. The regions before
/// `L` take the first `regions` bytes of the tail. A `size` rule must be `size`, a `len` rule
/// `len`, and an assertion not 0. A region must fit in what is left of the tail.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `base` is valid for reads of the `size` bytes of a `T`.
/// - The bytes of that `T` that its rules read are initialized, as [`check_rules`] requires.
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn check_listed<T: ?Sized + Pun, L: Rules>(
  base: *const u8,
  size: usize,
  tail: usize,
  len: usize,
  regions: usize,
) -> bool {
  if const { L::RULE == usize::MAX } {
    return true;
  }
  // SAFETY: `InRule<T, L>` is the rule `L::RULE` of `T`. `base` is valid for reads of the `size`
  // bytes of a `T`, which cover its sized fields, by contract on the caller. The bytes that the
  // rule reads are initialized, by contract on the caller: those of the fields that it loads and,
  // if it reads bytes by offset, all `size` bytes, since a rule of `T` then does. This is what
  // `eval` requires.
  let Some(value) = (unsafe { eval::<InRule<T, L>, L::Expr>(base, size) }) else { return false };
  let regions = match const { T::LAYOUT.rule(L::RULE) } {
    Some(Rule::Size(_)) if value == size as u128 => regions,
    Some(Rule::Len(_)) if value == len as u128 => regions,
    Some(Rule::Assert(_)) if value != 0 => regions,
    Some(Rule::Region(_)) if value <= (tail - regions) as u128 => regions + value as usize,
    _ => return false,
  };
  // SAFETY: The requirement of the call for the next rules is that of this call.
  unsafe { check_listed::<T, L::Next>(base, size, tail, len, regions) }
}

/// Returns whether `L` lists the rules of `T` that have an expression, from the `from`th rule on.
///
/// Each must be listed once, in order, with every operation of its expression ([`covers`]).
const fn listed<T: ?Sized + Pun, L: Rules>(from: usize) -> bool {
  let rules = T::LAYOUT.rules();
  let mut i = from;
  while i < rules.len() && rules[i].ops().is_none() {
    i += 1;
  }
  if L::RULE == usize::MAX {
    return i >= rules.len();
  }
  L::RULE == i && covers::<InRule<T, L>, L::Expr>() && listed::<T, L::Next>(i + 1)
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
#[inline(always)]
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
#[inline(always)]
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

/// Returns the bytes of the `index`th region of the tail of the struct `T` at `base`.
///
/// The struct is `size` bytes. Its tail starts at `head` and is `tail` bytes long. The search
/// starts at the rule `L`, after `count` regions that take the first `start` bytes of the tail. A
/// region whose expression fails, or does not fit in what is left of the tail, takes what is
/// left: the fields that size it are not trusted. The last region, which has no expression,
/// takes what is left too.
///
/// # Panics
///
/// Panics if `T` has no `index`th region.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `base` is valid for reads of the `size` bytes of a `T`, initialized, for `'a`.
/// - These bytes are not written for `'a`.
/// - `head + tail` is at most `size`, and `start` is at most `tail`.
///
/// # Compile-time errors
///
/// An instance compiles only if `T::Rules` lists the rules of `T` ([`listed`]).
#[inline(always)]
pub const unsafe fn region_at<'a, T: ?Sized + Pun, L: Rules>(
  base: *const u8,
  size: usize,
  (head, tail): (usize, usize),
  index: usize,
  (start, count): (usize, usize),
) -> &'a [u8] {
  const { assert!(listed::<T, T::Rules>(0), "not the rules of the struct") };
  let rest = tail - start;
  // The last region, which takes what remains, has no expression: it is not in the list.
  if const { L::RULE == usize::MAX } {
    assert!(count == index && T::LAYOUT.has_last_region(), "no such region");
    // SAFETY: `head + start + rest` is `head + tail`, at most `size`, by contract on the caller. So
    // `add(head + start)` stays within the `size` bytes at `base`. This satisfies the safety
    // contract of `slice::from_raw_parts` [1]: the `rest` bytes from there are in one allocation,
    // valid for reads for `'a`, so non-null [2], initialized, and not written for `'a`, by contract
    // on the caller. A `u8` is aligned at any address, and `rest` bytes fit in an `isize`, since
    // they are in an allocation.
    //
    // [1] Per https://doc.rust-lang.org/1.99.0/core/slice/fn.from_raw_parts.html:
    //
    //     Behavior is undefined if any of the following conditions are violated:
    //
    //     - `data` must be non-null, valid for reads for `len * size_of::<T>()` many bytes, and
    //       it must be properly aligned. [...]
    //     - `data` must point to `len` consecutive properly initialized values of type `T`.
    //     - The memory referenced by the returned slice must not be mutated for the duration of
    //       lifetime `'a`, except inside an `UnsafeCell`.
    //     - The total size `len * size_of::<T>()` of the slice must be no larger than
    //       `isize::MAX`, and adding that size to `data` must not "wrap around" the address
    //       space.
    //
    // [2] Per https://doc.rust-lang.org/1.99.0/core/ptr/index.html#safety:
    //
    //     A null pointer is never valid for reads/writes.
    return unsafe { core::slice::from_raw_parts(base.add(head + start), rest) };
  }
  if const { !matches!(T::LAYOUT.rule(L::RULE), Some(Rule::Region(_))) } {
    // SAFETY: The requirements of the call for the next rules are those of this call.
    return unsafe { region_at::<T, L::Next>(base, size, (head, tail), index, (start, count)) };
  }
  // A region that does not fit is truncated: the fields that size it are not trusted.
  // SAFETY: `InRule<T, L>` is a rule of `T`. `base` is valid for reads of the `size` bytes of a
  // `T`, initialized, by contract on the caller, and these cover its sized fields. This is what
  // `eval` requires.
  let region = match unsafe { eval::<InRule<T, L>, L::Expr>(base, size) } {
    Some(region) if region <= rest as u128 => region as usize,
    _ => rest,
  };
  if count == index {
    // SAFETY: `region <= rest`, so `head + start + region <= head + tail <= size`. As for the last
    // region above, the bytes are within the `size` bytes at `base`, and meet the safety contract
    // of `slice::from_raw_parts`.
    return unsafe { core::slice::from_raw_parts(base.add(head + start), region) };
  }
  // SAFETY: `start + region <= tail`, since `region <= rest`. The other requirements of the call
  // for the next rules are those of this call.
  unsafe { region_at::<T, L::Next>(base, size, (head, tail), index, (start + region, count + 1)) }
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
#[inline(always)]
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
#[inline(always)]
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
