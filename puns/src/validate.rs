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
//! ([`check_aim`]): it places the pointee ([`place`]), at the cursor of the tail ([`cursor_step`])
//! or within the bytes that the check reads ([`Input`], [`stored`]), measures it against its type
//! ([`measure`]), and checks it if the aim is eager ([`check_pointee`]), with [`validate`], which
//! checks the eager aims of the pointee in turn. A type that points at itself is checked by a
//! recursive call of the same instance, bounded by the depth and the bytes that [`Input`] counts.
//!
//! What reads a pointee finds it as the check does: [`region_at`] takes the same steps of the
//! cursor ([`cursor_step`]), which truncate rather than fail, [`pointer_at`] evaluates where a
//! pointee at a stored place starts, and [`thin`] makes a pointer to a pointee of its own size,
//! with the length that it stores. Both the check and the read evaluate that length over the sized
//! fields of the pointee only ([`stores_length`]).
//!
//! # No interpreter
//!
//! Every operation of every rule is applied by an instance of a function of its own ([`steps`]), in
//! which the operation is a constant. What is compiled is the arithmetic of the expression and the
//! loop of each walk, never a `match` on operations at run time.
//!
//! The derive writes the rules of a struct that have an expression or are aims as a list at the
//! type level (`Pun::Rules`, `RuleAt<RULE, STEPS, NEXT, TARGET>`, where `TARGET` is the type of
//! the target of an aim). It writes the operations of each rule as a balanced tree
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

use crate::{
  Fields, Path, Pun, Root, Rules, Steps, Variants,
  analysis::Marks,
  layout::{
    self, Aim, Check, Dfa, Enum, Extent, Kind, Machine, Op, Rule, Shape, Start, load, op_at,
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
}

/// The [`Checks`] that check every part.
pub struct All;

impl Checks for All {
  const MARKS: Marks = Marks::ALL;
}

/// Returns whether the parts of the `T` at `ptr` that `C` marks are valid.
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
/// [`Layout::rules_read_bytes`]: layout::Layout::rules_read_bytes
pub const unsafe fn validate<C: Checks, P: Path, T: ?Sized + Pun>(
  ptr: *const T,
  input: &mut Input,
) -> bool {
  if const { !C::MARKS.any(P::LEAF, T::LAYOUT.leaves) } {
    return true;
  }
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
  unsafe {
    match const { T::LAYOUT.shape() } {
      Shape::Scalar(spec) => spec.matches(ptr.cast()),
      Shape::Array(len) => validate_elems::<C, P, T::Elem>(ptr.cast(), len, input),
      Shape::Slice => validate_elems::<C, P, T::Elem>(ptr.cast(), ptr::metadata(ptr), input),
      Shape::Struct => {
        validate_fields::<C, P, T, T::Fields>(ptr, input)
          && (const { !rules_marked::<C, P, T>() } || check_rules(ptr, input))
      }
      // `str::from_utf8` checks the bytes where the automaton is that of UTF-8, by its states,
      // since an `id` proves nothing. This is decided at compile time.
      Shape::Pattern(_) if const { matches!(T::LAYOUT.kind, Kind::Pattern(dfa) if dfa.is_utf8()) } => {
        Dfa::matches_utf8(ptr.cast(), ptr::metadata(ptr))
      }
      Shape::Pattern(dfa) => dfa.matches(ptr.cast(), ptr::metadata(ptr)),
      Shape::Enum(layout) if layout.union => validate_union::<T, T::Variants>(ptr, input),
      Shape::Enum(layout) => validate_enum::<T>(ptr, layout, input),
    }
  }
}

/// Returns whether the enum at `ptr` has the tag of one of its variants, and the fields of that
/// variant are valid.
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
/// - `layout` is the layout of `T`.
/// - The tag of the enum is initialized, and so is every byte that the checks of the fields of that
///   tag's variant read, which check every part.
#[inline(always)]
const unsafe fn validate_enum<T: ?Sized + Pun>(
  ptr: *const T,
  layout: &Enum,
  input: &mut Input,
) -> bool {
  let Kind::Scalar(spec) = layout.tag.layout.kind else { panic!() };
  // SAFETY: `Layout::enumeration` asserts that the tag is a scalar that `Spec::is_integer`
  // accepts, so of at most 16 bytes, at `layout.tag.offset` within the size of `T`. So `add`
  // stays within the `T` at `ptr`, and the tag is valid for reads. The tag is initialized, and so
  // is what the checks of the fields of its variant read, by contract on the caller. These are
  // the requirements of `load` and of `validate_variants`, with `T::Variants` the list of the
  // variants of `T`.
  unsafe {
    let tag = load(ptr.cast::<u8>().add(layout.tag.offset), spec.size, spec.endian);
    validate_variants::<T, T::Variants>(ptr, tag, input)
  }
}

/// Returns whether the variant of tag `tag`, among `V` and the ones after it, has valid fields
/// in the enum at `ptr`.
///
/// Returns `false` if no such variant has the tag. A variant whose tag is a range of values (the
/// variant of a niche) has the tag if `tag` is in that range. Returns `false` too, without reading
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
const unsafe fn validate_variants<T: ?Sized + Pun, V: Variants>(
  ptr: *const T,
  tag: u128,
  input: &mut Input,
) -> bool {
  let Some(expected) = (const { V::TAG }) else { return false };
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
    if this {
      // No value is a variant of which a field has no value, so nothing of it is read. The
      // analysis does not look at such a variant either (`Variant::is_present`).
      (const { inhabited::<V::Fields>() }) && validate_fields::<All, Root, T, V::Fields>(ptr, input)
    } else {
      validate_variants::<T, V::Next>(ptr, tag, input)
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
const unsafe fn validate_union<T: ?Sized + Pun, V: Variants>(
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
    (const { inhabited::<V::Fields>() } && validate_fields::<All, Root, T, V::Fields>(ptr, input))
      || validate_union::<T, V::Next>(ptr, input)
  }
}

/// Returns whether the parts that `C` marks of the `len` elements at `ptr` are valid.
///
/// Elements whose checks are a few comparisons are checked in blocks of about 64 bytes, without a
/// branch within a block. The result is the same as one element at a time.
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
) -> bool {
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
        valid &= unsafe { validate::<C, P, T>(ptr.add(i + j), input) };
        j += 1;
      }
      if !valid {
        return false;
      }
      i += block;
    }
  }
  while i < len {
    // SAFETY: `i < len`. `ptr` is valid for reads of `len` elements, by contract on the caller,
    // so `add(i)` stays within them and points to an element that meets the requirements of
    // `validate`.
    if !unsafe { validate::<C, P, T>(ptr.add(i), input) } {
      return false;
    }
    i += 1;
  }
  true
}

/// Returns whether the parts that `C` marks of the field `F` of the `T` at `ptr`, and of the
/// fields after it, are valid.
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
) -> bool {
  let Some(field) = (const { F::FIELD }) else { return true };
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
  // is valid for reads, by the requirements of `validate`, so in one allocation, and `T` has a
  // field of type `F::Type` at `field.offset()`. These are the requirements of `ptr::field` and
  // `add`, and that field is valid for reads.
  // - A restricted field is a scalar leaf. It is read only if it is marked, so its bytes are
  //   initialized, as `Spec::matches` requires.
  // - Any other field is a `F::Type` at the place `F::Path<P>`, which the assertion above shows
  //   is the leaf of the field. What `C` marks there is what it marks of the field in `T`. So the
  //   field meets the requirements of `validate`.
  // The requirements of the call for the next fields are those of this call.
  unsafe {
    let valid = match field.restriction() {
      // A scalar that accepts less than its type does: there is no type to recurse into.
      Some(spec) => {
        (const { !C::MARKS.any(<F::Path<P> as Path>::LEAF, 1) })
          || spec.matches(ptr.cast::<u8>().add(field.offset()))
      }
      None => {
        validate::<C, F::Path<P>, F::Type>(ptr::field::<T, F::Type>(ptr, field.offset()), input)
      }
    };
    valid && validate_fields::<C, P, T, F::Next>(ptr, input)
  }
}

/// Returns whether `C` marks the rules of the struct `T` at the place `P`.
///
/// The rules of a struct are its last leaf.
const fn rules_marked<C: Checks, P: Path, T: ?Sized + Pun>() -> bool {
  !T::LAYOUT.rules().is_empty() && C::MARKS.any(P::LEAF + T::LAYOUT.leaves - 1, 1)
}

/// Returns whether the struct at `ptr` meets the rules of the layout of `T`, and the pointees of
/// its eager aims are valid within the bytes of `input`.
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
#[inline(always)]
pub const unsafe fn check_rules<T: ?Sized + Pun>(ptr: *const T, input: &mut Input) -> bool {
  const { assert!(listed::<T, T::Rules>(0), "not the rules of the struct") };
  let (len, tail, size) = tail_of(ptr);
  let head = const { T::LAYOUT.tail_shape().start };
  // SAFETY: `tail_of` returns `size`, the size of the `T` at `ptr`, and the tail of `tail` bytes
  // starts at `head`, so `head + tail <= size`. That `T` is valid for reads, the bytes that its
  // rules read are initialized, and it is within `input` if it has an aim at a stored place, by
  // contract on the caller, as `check_listed` requires.
  unsafe { check_listed::<T, T::Rules>(ptr.cast(), size, (head, tail), len, 0, input) }
}

/// Returns whether the struct at `base` meets the rule `L` of `T` and the ones after it.
///
/// The struct is `size` bytes, and its tail is the `tail` bytes from `head`, of `len` elements.
/// The aims before `L` at the cursor end at `cursor` in the tail. A `size` rule must be `size`, a
/// `len` rule `len`, and an assertion not 0. The pointee of an aim must fit, at the cursor
/// ([`cursor_step`]) or within the bytes of `input` ([`stored`]), with a size that its type has,
/// and if the aim is eager, be valid ([`check_pointee`]).
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
const unsafe fn check_listed<T: ?Sized + Pun, L: Rules>(
  base: *const u8,
  size: usize,
  (head, tail): (usize, usize),
  len: usize,
  cursor: usize,
  input: &mut Input,
) -> bool {
  if const { L::RULE == usize::MAX } {
    return true;
  }
  // The end of the list, which is not reached here, has no rule.
  let rule = const {
    match T::LAYOUT.rule(L::RULE) {
      Some(rule) => rule,
      None => Rule::Assert(&[]),
    }
  };
  let cursor = if let Rule::Aim(_) = rule {
    // SAFETY: The requirements of `check_aim` are those of this call.
    match unsafe { check_aim::<T, L>(base, size, (head, tail), cursor, input) } {
      Some(cursor) => cursor,
      None => return false,
    }
  } else {
    {
      // SAFETY: `InRule<T, L>` is the rule `L::RULE` of `T`, whose expression is `L::Expr`. `base`
      // is valid for reads of the `size` bytes of a `T`, which cover its sized fields, and the
      // bytes that the rule reads are initialized, by contract on the caller. This is what `eval`
      // requires.
      let Some(value) = (unsafe { eval::<InRule<T, L>, L::Expr>(base, size) }) else {
        return false;
      };
      match rule {
        Rule::Size(_) if value == size as u128 => cursor,
        Rule::Len(_) if value == len as u128 => cursor,
        Rule::Assert(_) if value != 0 => cursor,
        _ => return false,
      }
    }
  };
  // SAFETY: `cursor <= tail`: an aim at the cursor moves it to at most `tail`. The other
  // requirements of the call for the next rules are those of this call.
  unsafe { check_listed::<T, L::Next>(base, size, (head, tail), len, cursor, input) }
}

/// Returns where the cursor of the tail of the struct `T` at `base` is after the aim `L`, if the
/// pointee of the aim fits ([`place`]), and, if the aim is eager, has a size that its type has and
/// is valid ([`check_pointee`]).
///
/// # Safety
///
/// The caller must ensure the requirements of [`check_listed`].
#[cfg_attr(not(debug_assertions), inline(always))]
const unsafe fn check_aim<T: ?Sized + Pun, L: Rules>(
  base: *const u8,
  size: usize,
  (head, tail): (usize, usize),
  cursor: usize,
  input: &mut Input,
) -> Option<usize> {
  let (eager, align) = const {
    pointee_checked::<T, L>();
    match T::LAYOUT.rule(L::RULE) {
      Some(Rule::Aim(Aim { check, align, .. })) => (matches!(check, Check::Eager), align),
      _ => (false, 1),
    }
  };
  // SAFETY: The requirements of `place` are those of this call.
  let Some((addr, len, cursor)) =
    (unsafe { place::<T, L>(base, size, (head, tail), cursor, input) })
  else {
    return None;
  };
  if eager {
    // An eager pointee is aligned for its type, which its read relies on, has a size that its type
    // has, and is valid.
    if align > 1 && !ptr::is_aligned(addr, align) {
      return None;
    }
    let Some(len) = len else { return None };
    // SAFETY: The pointee is at `addr` with the length `len`, within the bytes of `input` or of
    // the `T`, which are valid for reads and initialized (`place`), as `check_pointee` requires.
    if !unsafe { check_pointee::<L::Pointee>(addr, len, input) } {
      return None;
    }
  }
  Some(cursor)
}

/// Returns where the pointee of the aim `L` of the struct `T` at `base` is, the length of its tail
/// if its size is one that its type has, and where the cursor of the tail is after it, if it fits:
/// at the cursor ([`cursor_step`]), or within the bytes of `input` ([`stored`]). The cursor moves
/// past a pointee at the cursor.
///
/// In a build without optimizations, it is a function of its own, which returns before the pointee
/// is checked, so that the frames of the recursion of eager aims do not hold what it computes.
///
/// # Safety
///
/// The caller must ensure the requirements of [`check_listed`].
#[cfg_attr(not(debug_assertions), inline(always))]
#[cfg_attr(debug_assertions, inline(never))]
const unsafe fn place<T: ?Sized + Pun, L: Rules>(
  base: *const u8,
  size: usize,
  (head, tail): (usize, usize),
  cursor: usize,
  input: &Input,
) -> Option<(*const u8, Option<usize>, usize)> {
  let next =
    const { matches!(T::LAYOUT.rule(L::RULE), Some(Rule::Aim(Aim { start: Start::Next, .. }))) };
  if next {
    // SAFETY: The requirements of `cursor_step` are those of this call.
    match unsafe { cursor_step::<T, L, true>(base, size, (head, tail), cursor) } {
      Some(Step { start, size, len }) => {
        // SAFETY: `start + size <= tail`, so `head + start` is within the `size` bytes at `base`
        // (`cursor_step`).
        Some((unsafe { base.add(head + start) }, len, start + size))
      }
      None => None,
    }
  } else {
    // SAFETY: The requirements of `stored` are those of this call.
    match unsafe { stored::<T, L>(base, size, input) } {
      Some((addr, len)) => Some((addr, len, cursor)),
      None => None,
    }
  }
}

/// What the pointee of the aim `L` of `T` must be, asserted where it is checked or read: one of
/// its own size stores its length ([`stores_length`]), and one that is checked with the struct
/// depends on no layout that Rust leaves to the compiler, without the `unstable` feature.
pub const fn pointee_checked<T: ?Sized + Pun, L: Rules>() {
  let Some(Rule::Aim(aim)) = T::LAYOUT.rule(L::RULE) else { return };
  assert!(!matches!(aim.extent, Extent::Own) || stores_length::<L::Pointee>(), "{}", STORES_LENGTH);
  assert!(
    matches!(aim.check, Check::Lazy) || <L::Pointee as Pun>::LAYOUT.checks_kept(),
    "the pointee of an eager aim has a part whose layout Rust leaves to the compiler (a struct, \
     a tuple or an enum of the `Rust` representation), and something to check in it.\nAdd \
     `#[repr(C)]` (or `#[repr(u8)]` to an enum), or enable the `unstable` feature of `puns`"
  );
}

/// Returns whether a `D` stores the length of its tail in its sized fields, as the pointee of an
/// aim of its own size does: it is sized, or its `size` or `len` rule reads only its sized fields
/// ([`LengthRule::in_head`](layout::LengthRule::in_head)). The check of an aim ([`measure`]) and
/// the read of its pointee ([`thin`]) both evaluate the rule over the sized fields, so that a
/// pointee that the check accepts is one that the read reads.
const fn stores_length<D: ?Sized + Pun>() -> bool {
  let layout = D::LAYOUT;
  match layout.length_rule() {
    _ if !layout.is_unsized() => true,
    Some(rule) => rule.in_head(),
    None => false,
  }
}

/// Why the pointee of an aim of its own size does not compile, if [`stores_length`] is false.
const STORES_LENGTH: &str = "the pointee of an aim of its own size is sized, or stores its length \
                             in its sized fields: a `size` or `len` rule that reads no byte by \
                             offset";

/// Where the pointee of an aim at the cursor is, and how large ([`cursor_step`]).
#[derive(Copy, Clone)]
pub struct Step {
  /// Where it starts, in bytes from the start of the tail.
  pub start: usize,
  /// Its size, in bytes.
  pub size: usize,
  /// The length of its tail, if its size is one that its type has ([`measure`]).
  pub len: Option<usize>,
}

/// Returns where the pointee of the aim at the cursor `L` of the struct `T` at `base` is, after the
/// pointees of the aims at the cursor before it, which end at `cursor` in the tail: one step of the
/// cursor, which the check of the struct ([`check_listed`]) and the reads of its pointees
/// ([`region_at`]) take alike.
///
/// The struct is `size` bytes, and its tail is the `tail` bytes from `head`. The pointee starts at
/// the cursor, rounded up to the alignment of its type by its offset from the start of the struct,
/// so that where it is depends on the bytes, not on their address. Its size is that of its type, the
/// value of its expression, or the rest of the tail ([`Extent`]), measured against its type
/// ([`measure`]).
///
/// With `STRICT`, the step fails where the pointee does not fit in the tail, or its size fails.
/// Otherwise it never fails: a pointee that would start past the tail starts at its end, and one
/// whose size fails, or does not fit, takes the rest of the tail, since safe code may have written
/// the fields that give it after the struct was checked. Either way, the length is `None` where the
/// size is not one that the type of the pointee has, which the check of an eager aim refuses.
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
#[inline(always)]
pub const unsafe fn cursor_step<T: ?Sized + Pun, L: Rules, const STRICT: bool>(
  base: *const u8,
  size: usize,
  (head, tail): (usize, usize),
  cursor: usize,
) -> Option<Step> {
  // Named for every rule of a struct, and called for an aim at the cursor only.
  let (extent, align) = const {
    match T::LAYOUT.rule(L::RULE) {
      Some(Rule::Aim(Aim { start: Start::Next, extent, align, .. })) => (extent, align),
      _ => (Extent::Rest, 1),
    }
  };
  // The offset of the cursor from the start of the struct, rounded up: `head + tail <= size`, so
  // no overflow below `size` (an `align` larger than the struct moves past the tail).
  let start = match (head + cursor).checked_next_multiple_of(align) {
    Some(at) if at - head <= tail => at - head,
    _ if STRICT => return None,
    _ => tail,
  };
  let rest = tail - start;
  let wanted = match extent {
    Extent::Own => Size::Own,
    // SAFETY: `InRule<T, L, 1>` is the size of the pointee, the second expression of the rule,
    // whose operations are `L::Extent`. `base` is valid for reads of the `size` bytes of a `T`,
    // which cover its sized fields, and the bytes that the rule reads are initialized, by contract
    // on the caller. This is what `eval` requires.
    Extent::Given(_) => match unsafe { eval::<InRule<T, L, 1>, L::Extent>(base, size) } {
      Some(n) => Size::Bytes(n),
      None if STRICT => return None,
      None => Size::Rest,
    },
    Extent::Rest => Size::Rest,
  };
  // SAFETY: `head + start <= head + tail <= size`, and the `rest` bytes from there are the end of
  // the tail, valid for reads and initialized, by contract on the caller, as `measure` requires.
  let measured = unsafe { measure::<L::Pointee>(wanted, base.add(head + start), rest) };
  match measured {
    Some((len, size)) => Some(Step { start, size, len }),
    None if STRICT => None,
    // What remains, which may not be a size of the type, for what reads it to refuse.
    None => Some(Step { start, size: rest, len: sized_as::<L::Pointee>(rest) }),
  }
}

/// How large the pointee of an aim is, as its rule says, before it is measured against its type
/// ([`measure`]).
#[derive(Copy, Clone)]
pub enum Size {
  /// The size of its type ([`Extent::Own`]).
  Own,
  /// This many bytes, the value of an expression ([`Extent::Given`]).
  Bytes(u128),
  /// What remains of the bytes ([`Extent::Rest`]).
  Rest,
}

/// Returns the size of the pointee `P` at `addr`, of which `room` bytes can be read, as large as
/// `size` says, if it fits in them, and the length of its tail if the size is one that `P` has: the
/// size of its type, with the length that it stores in its sized fields if it is unsized, or the
/// bytes that are given or remain, which are a size of `P` if they are the size of a sized `P`, or
/// of an unsized one with as many elements as make it ([`sized_as`]). The check of a lazy aim
/// requires only that the pointee fits, so that its type plays no part in the value of the struct,
/// and that of an eager one that its size is one of its type too.
///
/// # Safety
///
/// The caller must ensure that `addr` is valid for reads of `room` initialized bytes.
#[inline(always)]
pub const unsafe fn measure<P: ?Sized + Pun>(
  size: Size,
  addr: *const u8,
  room: usize,
) -> Option<(Option<usize>, usize)> {
  let (layout, measured) = const { (P::LAYOUT, stores_length::<P>()) };
  let n = match size {
    Size::Own if !layout.is_unsized() => layout.size(),
    Size::Own if !measured || room < layout.head() => return None,
    Size::Own => {
      // SAFETY: `addr` is valid for reads of `room` initialized bytes, by contract on the caller,
      // which cover the `head` bytes of the sized fields of `P`, and of the struct in it that has
      // the rule, which `stores_length` found. These are the requirements of `length`.
      let value = unsafe { length::<P>(addr, layout.head()) };
      let len = match value {
        Some(value) if value <= usize::MAX as u128 => elements::<P>(value as usize),
        _ => None,
      };
      return match len {
        Some(len) if layout.stride() == 0 || len <= (room - layout.head()) / layout.stride() => {
          match layout.size_of_val(len) {
            Some(size) if size <= room => Some((Some(len), size)),
            _ => None,
          }
        }
        _ => None,
      };
    }
    Size::Bytes(n) if n <= room as u128 => n as usize,
    Size::Bytes(_) => return None,
    Size::Rest => room,
  };
  if n > room {
    return None;
  }
  Some((sized_as::<P>(n), n))
}

/// Returns the length of the tail of a `P` of `size` bytes, if `size` is the size of a `P`: the
/// size of a sized one, or of an unsized one with as many elements as fit in `size`, trailing
/// padding included, as `from_bytes` finds them.
#[inline(always)]
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

/// Returns where the pointee of the aim at a stored place `L` of the struct `T` at `base` is, and
/// the length of its tail if its size is one that its type has: at the address of the struct plus
/// the value of the expression of the aim, an `i128` in two's complement, which must be within the
/// bytes of `input`, with room for it there ([`measure`]). The check of an eager aim requires it to be
/// aligned for its type too ([`check_aim`]).
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `base` is valid for reads of the `size` bytes of a `T`, which are within the bytes of
///   `input`, and the bytes that its rules read are initialized.
/// - The bytes of `input` are in one allocation, valid for reads and initialized.
#[inline(always)]
const unsafe fn stored<T: ?Sized + Pun, L: Rules>(
  base: *const u8,
  size: usize,
  input: &Input,
) -> Option<(*const u8, Option<usize>)> {
  // Named for every rule of a struct, and called for an aim at a stored place only.
  let extent = const {
    match T::LAYOUT.rule(L::RULE) {
      Some(Rule::Aim(Aim { start: Start::Stored(_), extent, .. })) => extent,
      _ => Extent::Rest,
    }
  };
  // SAFETY: `InRule<T, L>` is where the pointee starts, the first expression of the rule, whose
  // operations are `L::Expr`. `base` is valid for reads of the `size` bytes of a `T`, which cover
  // its sized fields, and the bytes that the rule reads are initialized, by contract on the caller.
  // This is what `eval` requires.
  let Some(at) = (unsafe { eval::<InRule<T, L>, L::Expr>(base, size) }) else { return None };
  // SAFETY: `base` is within the bytes of `input`, at or after their start, in one allocation, by
  // contract on the caller, as `offset_from_unsigned` requires.
  let from = unsafe { base.offset_from_unsigned(input.start) };
  #[expect(clippy::cast_possible_wrap, reason = "two's complement, as the aim says")]
  let at = at as i128;
  let to = match (from as i128).checked_add(at) {
    Some(to) if to >= 0 && to <= input.len as i128 => to.cast_unsigned() as usize,
    _ => return None,
  };
  // SAFETY: `to <= input.len`, so `add` stays within the bytes of `input`, in one allocation.
  let addr = unsafe { input.start.add(to) };
  let wanted = match extent {
    Extent::Own => Size::Own,
    // SAFETY: As above, with `InRule<T, L, 1>`, the size of the pointee, the second expression.
    Extent::Given(_) => match unsafe { eval::<InRule<T, L, 1>, L::Extent>(base, size) } {
      Some(n) => Size::Bytes(n),
      None => return None,
    },
    Extent::Rest => Size::Rest,
  };
  // SAFETY: The `input.len - to` bytes at `addr` are the end of the bytes of `input`, which are
  // valid for reads and initialized, by contract on the caller, as `measure` requires.
  match unsafe { measure::<L::Pointee>(wanted, addr, input.len - to) } {
    Some((len, _)) => Some((addr, len)),
    None => None,
  }
}

/// How deep the check of eager aims goes: the pointee of an aim that holds an aim, and so on, this
/// many times. The check recurses once for each level, and returns
/// [`Error::Limit`](crate::Error::Limit) deeper.
///
/// The bound is set by the stack that the check takes. For the pairs, the trees and the
/// expressions of `tests/support/aims.rs`, a level takes 12 to 15 KiB in a build without
/// optimizations, and at most 96 bytes in an optimized build (Rust 1.100 on aarch64; the frames of
/// x86-64 are smaller), so their deepest check takes at most 944 KiB, and fits in 1 MiB, which
/// `tests/aims.rs` checks. A type whose eager aims are nested deeper in each of its values takes
/// more for each level.
pub const AIM_DEPTH: usize = 64;

/// The bytes that a check reads, and what the check of eager aims may still spend in them.
///
/// The pointee of an aim at a stored place must lie within these bytes ([`stored`]). Each pointee
/// that is checked takes its size from a budget of twice the length of the bytes, at least 1 each.
/// The pointees of a tree, which share no byte, take at most that: their sizes add up to at most
/// the length, and each pointee of size 0 is reached by an aim of at least one byte, and at most as
/// many. So the time of a check grows with the bytes, though pointees may be shared, and a cycle
/// ends.
pub struct Input {
  /// The first byte.
  start: *const u8,
  /// The number of bytes.
  len: usize,
  /// What the pointees that are still to be checked may take, in bytes.
  budget: usize,
  /// How many more aims may be followed from here, one within the pointee of another.
  depth: usize,
  /// Whether the check stopped at the depth or at the budget, rather than at an invalid value.
  pub(crate) limit: bool,
}

impl Input {
  /// The `len` bytes at `start`.
  #[inline(always)]
  pub const fn new(start: *const u8, len: usize) -> Self {
    Self { start, len, budget: len.saturating_mul(2), depth: AIM_DEPTH, limit: false }
  }

  /// Takes a pointee of `size` bytes, and one level of depth, if there are enough left.
  const fn enter(&mut self, size: usize) -> bool {
    let cost = if size == 0 { 1 } else { size };
    if self.depth == 0 || self.budget < cost {
      self.limit = true;
      return false;
    }
    self.depth -= 1;
    self.budget -= cost;
    true
  }

  /// Gives back the level of depth that [`Input::enter`] took.
  const fn leave(&mut self) {
    self.depth += 1;
  }
}

/// Returns whether the pointee `P` at `addr`, the length of whose tail is `len`, is valid, every
/// part of it checked, its own eager aims one level deeper.
///
/// # Safety
///
/// The caller must ensure that the `P` at `addr` with that length is within the bytes of `input`
/// or of a value within them, which are in one allocation, valid for reads and initialized.
const unsafe fn check_pointee<P: ?Sized + Pun>(
  addr: *const u8,
  len: usize,
  input: &mut Input,
) -> bool {
  let pointee = ptr::from_raw_parts::<P>(addr, len);
  // The value exists in the bytes: its size does not overflow.
  let Some(size) = const { P::LAYOUT }.size_of_val(len) else { return false };
  if !input.enter(size) {
    return false;
  }
  // SAFETY: The `P` at `addr` is within the bytes of `input`, which are valid for reads and
  // initialized, by contract on the caller, so every byte that a check reads is. These are the
  // requirements of `validate`, and the pointees of the aims of the `P` are within `input` too.
  let valid = unsafe { validate::<All, Root, P>(pointee, input) };
  input.leave();
  valid
}

/// Returns the number of elements of the tail of an unsized `D` whose `size` or `len` rule
/// ([`length`]) gives `value`, which fits a `usize`: the value of a `len` rule, or the elements
/// that make the size that a `size` rule gives, if they make exactly that size. A `D` without such
/// a rule has no length to read, and this function is not called for one.
#[inline(always)]
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
    () if value >= before && (value - before).is_multiple_of(stride) => {
      Some((value - before) / stride)
    }
    () => None,
  }
}

/// Returns a pointer to the `D` at `addr`, the pointee of an aim of its own size, with the length
/// of its tail if it is unsized: the length that its `size` or `len` rule gives, as [`measure`]
/// finds it.
///
/// The rule is evaluated over the sized fields of `D` only, where a value that stores its length
/// stores it, as [`measure`] evaluates it.
///
/// # Safety
///
/// The caller must ensure that `D` stores its length ([`stores_length`]), that a value of `D`,
/// aligned, valid as Rust requires of the referent of a `&D`, is at `addr`, and, if `D` is
/// unsized, that its `size` or `len` rule, evaluated over its sized fields, gives the length of
/// its tail. A `D` of its own size that [`check_pointee`] checked is one: the check of an aim of
/// its own size requires that its pointee stores its length, at compile time
/// ([`pointee_checked`]), and so does its read.
#[inline(always)]
pub const unsafe fn thin<D: ?Sized + Pun>(addr: *const u8) -> *const D {
  let (unsized_, stride, head) = const {
    let layout = D::LAYOUT;
    (layout.is_unsized(), layout.stride(), layout.head())
  };
  if !unsized_ || stride == 0 {
    return ptr::from_raw_parts::<D>(addr, 0);
  }
  // SAFETY: The `head` bytes at `addr` are the sized fields of the valid `D` there, valid for
  // reads and initialized, by contract on the caller. They cover the sized fields of the struct in
  // `D` that has the rule. These are the requirements of `length`. An unsized `D` has the rule, as
  // the assertion above shows.
  let value = unsafe { length::<D>(addr, head) };
  let Some(len) = (match value {
    Some(value) if value <= usize::MAX as u128 => elements::<D>(value as usize),
    _ => None,
  }) else {
    // SAFETY: The rule of the `D` at `addr`, evaluated over its sized fields, gives the length of
    // its tail, by contract on the caller: the length itself for a `len` rule, or the size of a
    // `D` with that many elements for a `size` rule, which `elements` turns into that length, as
    // it does in `measure`.
    unsafe { core::hint::unreachable_unchecked() }
  };
  ptr::from_raw_parts::<D>(addr, len)
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
    // SAFETY: `L::Pointee::PROBE` is a probe of its type (`Pun`, section Safety), as `head_size`
    // requires, and `align_of_val_raw` has the same requirements.
    Some(aim) => unsafe {
      ptr::head_size(<L::Pointee as Pun>::PROBE) == aim.size
        && core::mem::align_of_val_raw(<L::Pointee as Pun>::PROBE) == aim.align
        && (!aim.id.known() || aim.id.equals(&<L::Pointee as Pun>::ID))
    },
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

/// Returns where the pointee of the aim at the cursor that the rule `rule` of the struct `T` at
/// `base` is starts, in bytes from `base`, and how many bytes it has: what the steps of the cursor
/// of the check of the struct find ([`cursor_step`]), for what reads the pointee.
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
#[inline(always)]
pub const unsafe fn region_at<T: ?Sized + Pun, L: Rules>(
  base: *const u8,
  size: usize,
  (head, tail): (usize, usize),
  rule: usize,
  cursor: usize,
) -> (usize, usize) {
  const { assert!(listed::<T, T::Rules>(0), "not the rules of the struct") };
  let next =
    const { matches!(T::LAYOUT.rule(L::RULE), Some(Rule::Aim(Aim { start: Start::Next, .. }))) };
  assert!(L::RULE != usize::MAX, "no such aim");
  let cursor = if next {
    // SAFETY: The requirements of `cursor_step` are those of this call.
    let step = unsafe { cursor_step::<T, L, false>(base, size, (head, tail), cursor) };
    let Some(Step { start, size: n, .. }) = step else { unreachable!() };
    if L::RULE == rule {
      return (head + start, n);
    }
    start + n
  } else {
    cursor
  };
  // SAFETY: `cursor <= tail`: a step ends within the tail. The other requirements of the call for
  // the next rules are those of this call.
  unsafe { region_at::<T, L::Next>(base, size, (head, tail), rule, cursor) }
}

/// Returns where the pointee of the aim at a stored place `L` of the struct `T` at `base` starts,
/// in bytes from `base`: the value of its expression, an `i128` in two's complement, or `None` if
/// the expression fails. A check of the struct found the same ([`stored`]).
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
#[inline(always)]
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

/// Returns the size in bytes that the aim `L` of the struct `T` at `base` gives its pointee, the
/// value of its second expression, or `None` if it fails.
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
#[inline(always)]
pub const unsafe fn given<T: ?Sized + Pun, L: Rules>(base: *const u8, size: usize) -> Option<u128> {
  let given = const {
    assert!(listed::<T, T::Rules>(0), "not the rules of the struct");
    matches!(T::LAYOUT.rule(L::RULE), Some(Rule::Aim(Aim { extent: Extent::Given(_), .. })))
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
