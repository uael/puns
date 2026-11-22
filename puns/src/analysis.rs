//! The analysis of a pun, made of relations between two [`Layout`]s, computed at compile time.
//!
//! For a source `S` and a target `D`, [`analyze`] decides the inclusion of languages, whether every
//! value of `S` is a [valid] value of `D`. It walks both layouts together, as the product of two
//! automata, and answers the following questions:
//!
//! - **Fit** ([`Fit`]): how the size of a source maps to the size of a target. The sizes fit
//!   always, for some lengths of a slice, or never, or the source is truncated to the size of the
//!   target. It accounts for the trailing padding of unsized structs, and is checked against
//!   `size_of_val_raw`.
//! - **Uninitialized bytes**: whether a byte that the target requires to be initialized would be
//!   read from padding. This is always an error.
//! - **Validity** ([`Relation`]): whether the pun is infallible, fallible, or never valid. Each
//!   constrained scalar of the target must accept every value that the source may hold under it.
//! - **Marks** ([`Marks`]): which scalars, patterns, enums, unions and rules of the target the
//!   source does not prove. These are exactly what a checked pun checks at run time.
//! - **Mutability** ([`mutable`], [`mutable_part`]): whether the reverse inclusion holds, which is
//!   what a `&mut` requires. Every value written through the target must leave a valid source.
//!
//! Without the `unstable` feature, the analysis does not look into a part whose layout Rust
//! leaves to the compiler ([`STRICT`], see
//! [Layouts that Rust leaves to the compiler](#layouts-that-rust-leaves-to-the-compiler)).
//!
//! The errors of the analysis are panics of `const { .. }` blocks in the generic functions of the
//! crate. Such a block depends on type parameters, so it is evaluated after monomorphization, and
//! `cargo check` does not report these errors while `cargo build` does.
//!
//! # Cost
//!
//! A [`Cursor`] walks a layout as *runs*, each of one kind of [`RunKind`]: padding, initialized
//! bytes of any value, a constrained scalar, bytes that an automaton accepts, or a part whose
//! layout Rust leaves to the compiler. A subtree without constraint is one run whatever its size,
//! so `[u8; 1 << 40]` is one step.
//!
//! Scalars are compared as sets of values, not byte by byte. Two aligned scalars compare their
//! lists of ranges, while scalars that are misaligned, or of different byte orders, go through an
//! exact decomposition of ranges of integers into boxes of bytes ([`digit_box`], [`box_meets`]). So
//! `NonZero<u16>` in little endian is known to be `NonZero<u16>` in big endian, and `u32 >= 0x100`
//! is known not to be.
//!
//! Arrays and slices are periodic, so they are not walked element by element. When the target is at
//! the start of an element of a repetition and the source is inside a repetition too, what happens
//! over the least common multiple of both strides happens again for as long as both repetitions
//! last, so one period is analyzed and the rest is skipped ([`find_skip`]). This is also what makes
//! the analysis of two slice tails, which never end, terminate. A period does not cut an enum or a
//! union of the target, which is related as a whole. The bytes of which the values are known, as
//! the tag of the variant that is related ([`Known`]), are not a period: nothing is known of the
//! bytes a period later, such as the elements of an array after the one that holds a niche.
//!
//! Patterns run with sets of states over the runs of the other side, each box of a scalar on its
//! own. A repetition ends as soon as one of its elements leaves the set unchanged. Two patterns are
//! compared by the product of their automata. The automaton must accept wherever the source may
//! end, the start of the string included: a source that may leave no byte to the string proves it
//! only if the automaton accepts the empty string.
//!
//! So the cost of an analysis grows with the number of fields, variants, ranges of values and
//! states of automata of the types. Where it would grow with their size, a search stops at a limit:
//! [`PLACES`] places of fields, a budget of boxes of a scalar, a budget of runs under a pattern,
//! and [`CUT_ELEMENTS`] elements of an array looked at one by one. These limits keep the analysis
//! within the bound of const evaluation (see "Minimum Rust version" in the [crate
//! documentation](crate#minimum-rust-version)).
//!
//! # Enums and unions
//!
//! Enums are related variant by variant. Each variant of the source is related, as two structs, to
//! the variants of the target whose tags it has, knowing the tag ([`Known`]). Knowing the tags of
//! both proves a niche of the target that the source variant has as padding. A variant of the
//! source is proven if all its tags are tags of one variant of the target, which it proves, and is
//! not valid if it has no tag of the target.
//!
//! When the source is not an enum, or is an enum whose tag is elsewhere, the analysis finds what
//! the source holds at the place of the tag ([`tag_values`]), either what a scalar of the same size
//! and byte order there is known to be, or else any value. Each variant whose tags meet those
//! values is related to the source over the bytes of the enum, knowing the tag, and a variant that
//! the source can never be is not looked at. The enum is proven if every value that the source may
//! hold there is the tag of a variant that has values, and if each such variant is proven
//! ([`relate_enum_from`]). Otherwise it is checked as a whole at run time. A byte that a possible
//! variant requires to be initialized must be initialized in the source, and a source that may be
//! padding at the tag is an error.
//!
//! An enum as a source is known by its tag, and by the bytes that every variant with values
//! initializes.
//!
//! A union as a target is proven by one of its fields that the source proves. Otherwise it is
//! checked field by field at run time if the check of no field reads a byte that may be padding in
//! the source, and refused if one does ([`relate_union_from`]). The marks of the analysis cannot
//! tell the validation which fields to skip, so a union is not checked by some of its fields only.
//! As a source, a union is padding, and from a union only the same union proves a union
//! ([`Layout::equals`]). It may hold a value even if none of its fields has one, so a part of the
//! source, a variant included, is taken to have no value only if none may exist
//! ([`Layout::may_exist`]).
//!
//! A pun from a type that has values to a type without values is never valid, and [`relate_at`]
//! reports it so. A variant or a struct that holds a type without values has no value either.
//!
//! A slice has a value whatever its element, the empty one, and the element of a slice that a
//! target ends with is under no byte if it has no value ([`tail_slice`]). Such a target is valid
//! only with no element, so a source that gives it one is never valid, unless the source ends with
//! a slice of the same elements at the same place, each of which is then an element of the source.
//!
//! # Uninitialized bytes
//!
//! A scalar may be required to be some values while nothing is known of it as a source, not even
//! that it is initialized ([`Spec::maybe_uninit`]). Such a scalar is padding wherever a source is
//! read: in [`atom_relation`], in a run of the target without constraints, in a pattern, and in
//! [`Layout::segment`]. Two such scalars are never equal ([`Layout::equals`]), because two types
//! can require the same values and be known to hold the same values while one may be padding where
//! the other may not.
//!
//! Two layouts are the same ([`Layout::equals`], by which an enum proves an enum without looking
//! into it) only if they require the same values and guarantee the same values. A
//! `V(#[pun(valid(0..=1))] u8)` is not a `V(bool)`.
//!
//! # What a check reads
//!
//! A proof reads nothing, so some parts are proven whatever their bytes are in the source: an enum
//! or a union by the same type ([`equal`]), a union by one of its fields, and the rules of a struct
//! by the same rules. A check reads. A part that is proven is checked all the same where its leaf
//! is marked for another reason:
//!
//! - The elements of an array share their leaves, so each is checked if one is.
//! - A pattern of the source under something that is not one marks every leaf.
//! - An enum or a union is checked as a whole, with every part of its variant.
//! - The leaves past the capacity of a set of marks are all marked, or none is ([`Marks`]).
//!
//! So each proof that reads nothing also tells whether a check of the part would read a byte that
//! may be uninitialized in the source ([`Relation::unreadable`]). Such a byte is a byte of a union,
//! padding that a rule reads ([`checkable`]), what a field of a union reads before the field that
//! proves it ([`relate_union_from`]), or any byte under a part whose layout Rust leaves to the
//! compiler and that the source does not prove (see
//! [Layouts that Rust leaves to the compiler](#layouts-that-rust-leaves-to-the-compiler)). A rule
//! that reads the bytes of its struct reads what the source has under every part of the struct, a
//! part that is proven without being read included ([`unreadable_rules`]). A leaf that is both
//! marked and unreadable is refused, as a byte read from padding is ([`Relation::settle`]).
//!
//! # Shifted relations
//!
//! [`relate_at`] and [`analyze_tail`] relate layouts whose starts differ. The cursors of both
//! layouts count offsets from the start of the source, and each starts at its own base: the target
//! of `tail` at `shift = size(S) - size(D)`, as does the source of the reverse relation of
//! `tail_mut`. Nothing else changes with the shift, because every step compares only what is at the
//! same offset of both sides:
//!
//! - A run of the target is compared with what lies under it. Scalars that the shift cuts are
//!   decomposed into boxes of bytes, as misaligned scalars are.
//! - An enum is related only with an enum whose base is at the same offset.
//! - The rules of a struct are proven only by the same rules of a struct of the same extent at that
//!   offset of the source ([`mark_rules`] from the shift, see [Rules](#rules)).
//! - Periods stay correct, because a period is taken only when the target is at the start of one of
//!   its elements, and what lies under a run of the target one period later does not depend on the
//!   phase of the two repetitions, which is all that a shift changes.
//!
//! The relation reports offsets of the source, and messages subtract the shift. `tail_ref` requires
//! the target to be no more aligned than the source, so the shift is a multiple of the alignment of
//! the target, since sizes are multiples of alignments.
//!
//! # Mutable parts
//!
//! Through `head_mut` or `tail_mut`, safe code may write any value of the target over the bytes
//! `shift..shift + size` of the source, and the source must stay valid, so [`mutable_part`]
//! requires two conditions of that part:
//!
//! - **The part takes what is written.** The reverse relation over those bytes must be infallible,
//!   as `pun_mut` requires of the whole, which means that every value of the target, padding
//!   included, must be valid for what the source has there. A scalar of the source that the part
//!   covers only in part is not proven, since what its other bytes hold is not known, and neither
//!   is an enum whose base is outside the part.
//! - **Nothing outside the part depends on it** ([`cut`]). A struct of the source with rules
//!   (`size`, `len`, `assert`, regions, walks) that covers any of the bytes is refused, because its
//!   rules read its fields. So is an enum or a string of the source that the bytes cover only in
//!   part, such as an enum with its tag on one side and fields on the other. A target that takes as
//!   many elements as fit may end anywhere in the source, so it is refused over a source with
//!   enums. One exception holds for a struct with rules that lies within the bytes. It is accepted
//!   if the target has the same rules over the same extent there ([`proves_rules`], the test of
//!   [Rules](#rules) in the other direction), since what is written through the target then meets
//!   the rules of the source if it meets those of the target. The same holds for the elements of an
//!   array that lie within the bytes, which are accepted at once where the target has elements of
//!   the same size at the same place, or else one by one, up to 64 elements.
//!
//! This is conservative. A rule that does not read the bytes of the part is refused all the same,
//! and so is an enum all of whose variants would accept what is written. The model check counts
//! these refusals as imprecise, never as unsound.
//!
//! # Rules
//!
//! Rules are checked when a value is made from something else, and tell `from_head` how long a
//! value is. A rule is proven only by the same rules, at the same place of the source, in a struct
//! of the same extent ([`Layout::same_rules`], [`proves_rules`]). A rule is a function of four
//! things: the bytes of its struct, its size (`size_of_val(self)` or a `size` rule), the number of
//! elements of its tail (a `len` rule), and the bytes of that tail (regions). So a struct of the
//! source with the same rules in the same order ([`Rule::equals`](crate::layout::Rule::equals)),
//! the same sized part and the same tail computes the same rules over the same bytes, whatever its
//! fields, which are related to the target as any others. The same tail means the same start, the
//! same size of an element and, for a sized struct, the same bytes and elements of the array that
//! it ends with. This is how `Frame<Ipv4<Udp>>` proves the rules of `Frame<Ipv4>`, since the
//! payload of `Ipv4` is a typed region of its bytes, not a field of another type. A part of a
//! struct of a dynamic size has another size, so it does not prove the rules of a struct of its own
//! size.
//!
//! The elements of an array or a slice share their leaves, so their rules are proven together.
//! Where the source has elements of the same size at the same place, each element is to the source
//! what the first is, and the rules of all are proven at once ([`same_elements`]). Otherwise each
//! element is checked. A rule that reads the bytes of its struct reads its padding as well, so the
//! same tests decide whether the rule is checked and whether the padding is read
//! ([`Cursor::padding_is_read`]).
//!
//! # Layouts that Rust leaves to the compiler
//!
//! Without the `unstable` feature ([`STRICT`]), the analysis does not look into a node that is not
//! `fixed` ([`Layout::is_opaque`], [`Stable`](crate::layout::Stable)), and the cursor makes it one
//! run ([`RunKind::Opaque`]). The exception is a node whose size is the same in every layout, that
//! has no rule, and whose bytes are all of any value or all padding, since what such bytes are does
//! not depend on where its fields are.
//!
//! - **As a source**, an opaque node is what every layout of it is (all initialized bytes if it is
//!   `init`, and otherwise all bytes that may be padding). That the first byte of a `(u8, u16)` is
//!   never padding is not used.
//! - **As a target**, an opaque node is proven by the same type at the same place ([`equal`]). A
//!   struct or a tuple whose size is the same in every layout may also be proven field by field
//!   ([`relate_fields`]). A field may be anywhere, so it must be proven everywhere that it may be,
//!   and since a struct is valid if each of its fields is valid at its place, the source proves the
//!   struct in every order of its fields if, and only if, it proves each field at each offset that
//!   the field has in some order. This is exact, and tries no order. The offsets taken are the
//!   multiples of the alignment of the field where the field fits or, where no order needs padding,
//!   the multiples of the greatest common divisor of the sizes, of which every sum of sizes is one.
//!   A few of these offsets are in no order, such as the second byte of a `(bool, [u8; 3])`, so
//!   more is then required, never less. A struct that has a rule, which names fields, proves only
//!   itself, as an enum does. As a target, a part that holds a scalar that a pun never makes is
//!   proven by nothing, not even by itself ([`Layout::holds_never`]), and is refused in every
//!   layout.
//! - **A check does not replace a proof in an opaque node,** since a check would read where the
//!   compiler may not have put the field. What is not proven does not compile
//!   ([`Relation::unspecified`]), and whatever reads a relation reads this, including what a
//!   variant of a view takes from another (`base` in `view.rs`), and an enum or a union that would
//!   be checked as a whole, fields included, which the relations of its variants report. So the
//!   marks of the analysis, which tell validation what to check, are at places that every layout
//!   shares. Such a node is still read by a check in two cases, whose answer is the same in every
//!   layout. A union that a later field proves is not refused for an earlier field that is not
//!   proven, and where its leaf is marked for another reason, its check tries that field first,
//!   where `rustc` put its fields: the later field accepts whatever that field does not. And a
//!   part that holds a scalar that a pun never makes is never valid, so the enum of a variant that
//!   holds it is checked as a whole, and the check reads the fields of the part. Since a field may
//!   be anywhere in the node, such a node is refused there if any byte under it may be
//!   uninitialized in the source ([`Relation::unreadable`]).
//!
//! A check must not read a byte of the source that, in another layout that Rust allows, holds
//! another field ([`reads_moved`]). The analysis counts the bytes that a check reads as "What a
//! check may read" in the [crate documentation](crate#what-a-check-may-read) says. Under an array
//! whose elements the source does not match, it counts every byte, including bytes that no check
//! reads.
//!
//! The analysis looks for nothing inside a part of the source that is not fixed, at the place where
//! `rustc` put it. A rule is proven by the same rule of the same struct at the same place, which
//! the analysis looks for through the fields of the source ([`proves_rules`], [`covered`],
//! [`same_elements`]), but not through a node that is not fixed. Two layouts are the same there
//! only if what Rust does not specify of them is of the same type ([`equal`]). A `&mut` to a part
//! covers such a node whole, by the same type, or not at all ([`cut`]). A pun of a type whose size
//! Rust does not specify compiles only if both sides are the same type ([`sizes_kept`]).
//! [`validate_but_never`](crate::validate_but_never) compiles only if nothing that Rust does not
//! specify has something to check ([`Layout::checks_kept`]), and [`zeroed`](crate::zeroed) does not
//! accept the zeros of an enum that is not fixed.
//!
//! [valid]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity

use crate::layout::{Dfa, Endian, Kind, Layout, Spec, Tags, UNSTABLE, Uniform, Valid, max_value};

/// Whether a part of a layout that Rust leaves to the compiler is opaque to the analysis.
///
/// It is, without the `unstable` feature. A pun then has the same verdict, and accepts the same
/// values, whatever layout the compiler chose, as with `-Zrandomize-layout`.
const STRICT: bool = !UNSTABLE;

// -------------------------------------------------------------------------------------------------
// `Cursor`

/// The maximum nesting of arrays, slices and structs that a [`Cursor`] follows.
const MAX_DEPTH: usize = 32;

/// The kind of a [`Run`].
#[derive(Copy, Clone)]
pub enum RunKind {
  /// Bytes that may be uninitialized.
  Pad,
  /// Initialized bytes of any value.
  Any,
  /// A constrained scalar.
  Atom(Spec),
  /// Bytes, up to the end, that an automaton accepts.
  Pattern(&'static Dfa),
  /// Without the `unstable` feature, a part whose layout Rust leaves to the compiler.
  ///
  /// As a source, it is what every layout of it has, either initialized bytes or bytes that may be
  /// padding. As a target, it is proven by the same type at the same place, or by a source that
  /// proves each of its fields wherever the field may be ([`relate_fields`]).
  Opaque(&'static Layout),
}

impl RunKind {
  /// Returns whether a source may hold uninitialized bytes in a run of this kind.
  const fn may_be_uninit(&self) -> bool {
    match self {
      RunKind::Pad => true,
      RunKind::Atom(spec) => spec.may_be_uninit(),
      RunKind::Opaque(layout) => !layout.always_init(),
      RunKind::Any | RunKind::Pattern(_) => false,
    }
  }

  /// Returns what a source is known to hold in a run of this kind.
  ///
  /// An opaque run is what every layout of it has, either initialized bytes or bytes that may be
  /// padding.
  const fn seen(self) -> Self {
    match self {
      RunKind::Opaque(layout) if layout.always_init() => RunKind::Any,
      RunKind::Opaque(_) => RunKind::Pad,
      kind => kind,
    }
  }
}

/// Consecutive bytes of the same kind, at `start..end` from the root of the layout.
#[derive(Copy, Clone)]
pub struct Run {
  /// The kind of the bytes.
  kind: RunKind,
  /// The first byte of the run.
  ///
  /// For padding or initialized bytes within a struct or an enum, it may be the position that the
  /// cursor was asked for, after the first byte.
  start: usize,
  /// One past the last byte of the run.
  end: usize,
  /// The index of the leaf of the run (the scalar of an atom, a pattern or an opaque part). It is 0
  /// for padding and for initialized bytes.
  leaf: usize,
}

/// A layout on the stack of a [`Cursor`], and where it is.
#[derive(Copy, Clone)]
struct Frame {
  /// The layout.
  layout: &'static Layout,
  /// The offset of the layout from the root.
  base: usize,
  /// The index of the first leaf of the layout.
  leaf: usize,
  /// For a struct, the index of the first field that is not behind the cursor.
  field: usize,
}

impl Frame {
  /// Returns the offset one past the last byte of the layout, or `usize::MAX` if it has no end.
  const fn end(&self) -> usize {
    if self.layout.is_dynamic() { usize::MAX } else { self.base + self.layout.size }
  }
}

/// A repetition that a cursor is in, where `stride` bytes repeat until `end`.
#[derive(Copy, Clone)]
struct Repeat {
  /// The size of what repeats.
  stride: usize,
  /// One past the last byte of the repetition, or `usize::MAX` if it has no end.
  end: usize,
  /// Whether the repetition is a run of identical bytes rather than an array or a slice.
  run: bool,
  /// What tells an array or a slice from another, its depth in the stack of the cursor and its
  /// offset.
  id: (usize, usize),
}

/// A forward-only iterator over the runs of a layout.
#[derive(Copy, Clone)]
pub struct Cursor {
  /// The layouts that the cursor is in, the root first.
  stack: [Frame; MAX_DEPTH],
  /// The number of layouts on the stack.
  depth: usize,
  /// Whether the layout is that of the target.
  ///
  /// A byte that only some variants of an enum initialize must then be initialized, rather than
  /// be known to be maybe uninitialized.
  target: bool,
  /// Which rules of the target may be checked at run time.
  checked: Checked,
  /// What the source is known to hold under some bytes of the target, whatever its own layout
  /// says there.
  ///
  /// This is the tag of an enum, when a variant of the target is related to a variant of the
  /// source, or to a source whose bytes there are one of the tags of that variant.
  known: Known,
}

/// The most ranges that a [`Known`] holds. Past them, nothing is known.
const KNOWN: usize = 8;

/// The values that the source is known to hold at `start..end`, as an integer of `endian` byte
/// order.
///
/// These are values of the tag of an enum. Nothing is known if there are no ranges.
#[derive(Copy, Clone)]
struct Known {
  /// The first byte.
  start: usize,
  /// One past the last byte.
  end: usize,
  /// The byte order of the integer.
  endian: Endian,
  /// The values, as sorted ranges, of which the first `len` are used.
  ranges: [(u128, u128); KNOWN],
  /// The number of ranges.
  len: usize,
}

impl Known {
  /// Nothing known.
  const NONE: Self =
    Self { start: 0, end: 0, endian: Endian::Little, ranges: [(0, 0); KNOWN], len: 0 };

  /// Returns the values of both `a` and `b`, at `start..start + size`.
  ///
  /// Nothing is known if they take more than [`KNOWN`] ranges.
  const fn both(start: usize, size: usize, endian: Endian, a: &Tags, b: &Tags) -> Self {
    let mut known = Self { start, end: start + size, endian, ..Self::NONE };
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
      let ((lo_a, hi_a), (lo_b, hi_b)) = (a.get(i), b.get(j));
      let (lo, hi) = (if lo_a > lo_b { lo_a } else { lo_b }, if hi_a < hi_b { hi_a } else { hi_b });
      if lo <= hi {
        if known.len == KNOWN {
          return Self::NONE;
        }
        known.ranges[known.len] = (lo, hi);
        known.len += 1;
      }
      if hi_a < hi_b {
        i += 1;
      } else {
        j += 1;
      }
    }
    known
  }

  /// Returns whether the byte at `pos` holds a known value.
  const fn covers(&self, pos: usize) -> bool {
    self.len > 0 && self.start <= pos && pos < self.end
  }

  /// Returns whether a byte of `start..end` holds a known value.
  const fn meets(&self, start: usize, end: usize) -> bool {
    self.len > 0 && self.start < end && start < self.end
  }

  /// Returns the values known of exactly `start..end`, read in `endian` byte order, if any.
  const fn of(&self, start: usize, end: usize, endian: Endian) -> Option<&[(u128, u128)]> {
    let same =
      self.start == start && self.end == end && (end - start == 1 || self.endian.equals(endian));
    if self.len > 0 && same { Some(self.ranges.split_at(self.len).0) } else { None }
  }

  /// Returns where the bytes from `pos` stop being all known or all unknown, at most `next`.
  ///
  /// What is known holds of its own bytes only, so a step that reads it must not go past them.
  const fn step(&self, pos: usize, next: usize) -> usize {
    if self.len == 0 || pos >= self.end {
      next
    } else if pos < self.start {
      min(next, self.start)
    } else {
      min(next, self.end)
    }
  }
}

/// Which rules of a target may be checked at run time.
///
/// A rule that reads the bytes of its struct (`le`, `crc32` and the like) reads its padding,
/// which must then be initialized.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Checked {
  /// None. The layout is not the target of a pun but its source, in the reverse relation of a
  /// `&mut`.
  No,
  /// The rules that [`mark_rules`] marks, which the source does not prove.
  Marked,
  /// All of them. The target is a variant of an enum, which is checked as a whole.
  All,
}

impl Cursor {
  /// Returns a cursor over `root`, whose first byte is at `base`.
  ///
  /// Positions are offsets from the start of the source, wherever the target is in it.
  pub(crate) const fn new(
    root: &'static Layout,
    base: usize,
    target: bool,
    checked: Checked,
  ) -> Self {
    let frame = Frame { layout: root, base, leaf: 0, field: 0 };
    Self { stack: [frame; MAX_DEPTH], depth: 1, target, checked, known: Known::NONE }
  }

  /// Returns whether a rule that may be checked reads the padding at the top of the stack.
  ///
  /// Such a rule is a rule that reads bytes, of a struct that holds the padding, unless `source`
  /// proves it. The tests are those of [`mark_rules`], in the same order.
  const fn padding_is_read(&self, source: &'static Layout) -> bool {
    let (mut i, mut marked) = (0, matches!(self.checked, Checked::All));
    if matches!(self.checked, Checked::No) {
      return false;
    }
    while i < self.depth {
      let frame = &self.stack[i];
      if !marked && covered(source, frame.base, frame.layout) {
        // Nothing in it is checked.
        return false;
      }
      if let Kind::Array(..) | Kind::Slice(_) = frame.layout.kind
        && (marked || !same_elements(source, frame.base, frame.layout))
      {
        // The elements share their rules: proven all at once, or checked everywhere. (Where the
        // source has the same elements, what holds for one element holds for the one at the top.)
        marked = true;
      }
      if frame.layout.rules_read_bytes()
        && (marked || !proves_rules(source, frame.base, frame.layout))
      {
        return true;
      }
      i += 1;
    }
    false
  }

  /// Returns the enum that the last run is in, if any, with its offset and its first leaf.
  ///
  /// Without the `unstable` feature, an enum whose layout Rust leaves to the compiler is not
  /// returned.
  const fn enumeration(&self) -> Option<(&'static Layout, usize, usize)> {
    let frame = &self.stack[self.depth - 1];
    match frame.layout.kind {
      Kind::Enum(_) if STRICT && !frame.layout.stable.fixed => None,
      Kind::Enum(_) => Some((frame.layout, frame.base, frame.leaf)),
      _ => None,
    }
  }

  /// Pushes `layout`, at `base`, whose first leaf is `leaf`.
  ///
  /// # Panics
  ///
  /// Panics if the stack already holds [`MAX_DEPTH`] layouts.
  const fn push(&mut self, layout: &'static Layout, base: usize, leaf: usize) {
    assert!(self.depth < MAX_DEPTH, "type is nested too deeply");
    self.stack[self.depth] = Frame { layout, base, leaf, field: 0 };
    self.depth += 1;
  }

  /// Returns the run at `pos`.
  ///
  /// `pos` must not be less than the previous position given to the cursor.
  ///
  /// # Panics
  ///
  /// Panics if `pos` is past the end of the root layout, or if the layouts nest deeper than
  /// [`MAX_DEPTH`].
  const fn run_at(&mut self, pos: usize) -> Run {
    loop {
      let frame = self.stack[self.depth - 1];
      let (layout, end) = (frame.layout, frame.end());
      if pos >= end {
        assert!(self.depth > 1, "internal error: analysis out of the bounds of a type");
        self.depth -= 1;
        continue;
      }
      // Not looked into.
      if layout.is_opaque() {
        return Run { kind: RunKind::Opaque(layout), start: frame.base, end, leaf: frame.leaf };
      }
      match layout.uniform {
        Uniform::Pad => return Run { kind: RunKind::Pad, start: frame.base, end, leaf: 0 },
        Uniform::Any => return Run { kind: RunKind::Any, start: frame.base, end, leaf: 0 },
        Uniform::No => {}
      }
      match layout.kind {
        Kind::Scalar(spec) => {
          return Run { kind: RunKind::Atom(spec), start: frame.base, end, leaf: frame.leaf };
        }
        Kind::Array(elem, _) | Kind::Slice(elem) => {
          let index = (pos - frame.base) / elem.size;
          self.push(elem, frame.base + index * elem.size, frame.leaf);
        }
        Kind::Pattern(dfa) => {
          return Run { kind: RunKind::Pattern(dfa), start: frame.base, end, leaf: frame.leaf };
        }
        Kind::Enum(variants) => {
          let Kind::Scalar(spec) = variants.tag.layout.kind else { panic!() };
          let (offset, tag) = (pos - frame.base, variants.tag.offset);
          if offset >= tag && offset < tag + spec.size {
            let kind = if spec.is_constrained() { RunKind::Atom(spec) } else { RunKind::Any };
            let (start, end) = (frame.base + tag, frame.base + tag + spec.size);
            return Run { kind, start, end, leaf: frame.leaf };
          }
          // What follows, for as long as it is known to be initialized or not.
          let checked = !matches!(self.checked, Checked::No);
          let (init, stop) = layout.segment(offset, !self.target, checked);
          let kind = if init { RunKind::Any } else { RunKind::Pad };
          return Run { kind, start: pos, end: frame.base + stop, leaf: 0 };
        }
        Kind::Struct(fields, _) => {
          // Skip the fields that end before `pos`.
          let mut i = frame.field;
          while i < fields.len() {
            let field = &fields[i];
            let dynamic = field.layout.is_dynamic();
            if dynamic || frame.base + field.offset + field.layout.size > pos {
              break;
            }
            i += 1;
          }
          self.stack[self.depth - 1].field = i;
          if i == fields.len() {
            // Trailing padding.
            return Run { kind: RunKind::Pad, start: pos, end, leaf: 0 };
          }
          let start = frame.base + fields[i].offset;
          if pos < start {
            // Padding between two fields.
            return Run { kind: RunKind::Pad, start: pos, end: start, leaf: 0 };
          }
          self.push(fields[i].layout, start, frame.leaf + fields[i].leaf);
        }
      }
    }
  }

  /// Writes to `out` the repetitions that the cursor is in at `pos`, outermost first, and returns
  /// their number.
  ///
  /// `run` is the run at `pos`. If it is padding or initialized bytes, it is the last
  /// repetition, of stride 1. With `aligned`, only the arrays and slices of which `pos` is the
  /// start of an element are written.
  const fn repeats(
    &self,
    pos: usize,
    run: &Run,
    aligned: bool,
    out: &mut [Repeat; MAX_DEPTH + 1],
  ) -> usize {
    let (mut len, mut i) = (0, 0);
    while i + 1 < self.depth {
      let frame = &self.stack[i];
      let (elem, end) = match frame.layout.kind {
        Kind::Array(elem, _) => (elem, frame.end()),
        Kind::Slice(elem) => (elem, usize::MAX),
        _ => {
          i += 1;
          continue;
        }
      };
      // The next frame is the element the cursor is in.
      if !aligned || self.stack[i + 1].base == pos {
        out[len] = Repeat { stride: elem.size, end, run: false, id: (i, frame.base) };
        len += 1;
      }
      i += 1;
    }
    if matches!(run.kind, RunKind::Pad | RunKind::Any) {
      out[len] = Repeat { stride: 1, end: run.end, run: true, id: (usize::MAX, 0) };
      len += 1;
    }
    len
  }
}

// -------------------------------------------------------------------------------------------------
// `Marks`

/// A set of leaves of a type, by index.
///
/// A leaf is a part that a check may test, such as a constrained scalar, an enum, or the rules of
/// a struct.
#[derive(Copy, Clone)]
pub struct Marks {
  /// The leaves whose index is less than [`CAPACITY`](Self::CAPACITY), one bit each.
  bits: [u64; Self::CAPACITY / 64],
  /// Whether the leaves whose index is `CAPACITY` or more are in the set. They are all in it, or
  /// none is.
  overflow: bool,
}

impl Marks {
  /// The number of leaves that the set tells apart.
  const CAPACITY: usize = 1024;

  /// The empty set.
  pub(crate) const NONE: Self = Self { bits: [0; Self::CAPACITY / 64], overflow: false };
  /// The set of every leaf.
  pub(crate) const ALL: Self = Self { bits: [!0; Self::CAPACITY / 64], overflow: true };

  /// Adds `leaf` to the set.
  const fn set(&mut self, leaf: usize) {
    if leaf < Self::CAPACITY {
      self.bits[leaf / 64] |= 1 << (leaf % 64);
    } else {
      self.overflow = true;
    }
  }

  /// Returns the leaves that are in either set.
  pub(crate) const fn union(&self, other: &Self) -> Self {
    let (mut bits, mut i) = (self.bits, 0);
    while i < bits.len() {
      bits[i] |= other.bits[i];
      i += 1;
    }
    Self { bits, overflow: self.overflow || other.overflow }
  }

  /// Returns whether a leaf is in both sets.
  const fn meets(&self, other: &Self) -> bool {
    let mut i = 0;
    while i < self.bits.len() {
      if self.bits[i] & other.bits[i] != 0 {
        return true;
      }
      i += 1;
    }
    self.overflow && other.overflow
  }

  /// Returns whether one of the `count` leaves from `first` on is in the set.
  pub(crate) const fn any(&self, first: usize, count: usize) -> bool {
    let (mut i, end) = (first, first + count);
    while i < end {
      if i >= Self::CAPACITY {
        return self.overflow;
      }
      if i % 64 == 0 && end - i >= 64 {
        if self.bits[i / 64] != 0 {
          return true;
        }
        i += 64;
      } else {
        if self.bits[i / 64] & (1 << (i % 64)) != 0 {
          return true;
        }
        i += 1;
      }
    }
    false
  }
}

// -------------------------------------------------------------------------------------------------
// `Relation`

/// How the values that a source may hold relate to the values that a target accepts.
///
/// Each offset is the first one found of its kind.
#[derive(Copy, Clone)]
pub struct Relation {
  /// The offset of a byte that the target requires to be initialized and that may be padding in
  /// the source.
  pub(crate) uninit: Option<usize>,
  /// The offset of a part of the target that no value of the source is valid for.
  pub(crate) disjoint: Option<usize>,
  /// The offset of a part of the target that some values of the source are not valid for.
  pub(crate) partial: Option<usize>,
  /// The leaves of the target that may receive an invalid value, which a check tests at run time.
  pub(crate) marks: Marks,
  /// The offset of a part of the target that is not proven in every layout that Rust allows,
  /// without the `unstable` feature ([`RunKind::Opaque`]).
  ///
  /// No check can replace it, since a check would read at a place that the compiler may move.
  /// Whatever reads the fields of a relation one by one must read this one too. With it, nothing
  /// is proven.
  pub(crate) unspecified: Option<usize>,
  /// The leaves of the target whose check would read a byte that may be uninitialized in the
  /// source.
  ///
  /// Such a leaf that the source does not prove is refused where it is found (`uninit`). One that
  /// the source proves needs no check: a proof reads nothing, so it holds whatever these bytes
  /// are. But it may be checked all the same, where it shares its leaf with a part that is not
  /// proven (the elements of an array), where every leaf is marked, or where it is in an enum or
  /// a union that is checked as a whole. A leaf in both this set and `marks` is refused
  /// ([`Relation::settle`]).
  ///
  /// This is not `uninit`, which also records a byte that the target requires to be initialized
  /// and that no check reads, such as a byte of any value.
  unreadable: Marks,
  /// The offset of the first such byte that was found, if any.
  unreadable_at: Option<usize>,
}

impl Relation {
  /// A relation in which nothing is found yet.
  const NEW: Self = Self {
    uninit: None,
    disjoint: None,
    partial: None,
    marks: Marks::NONE,
    unspecified: None,
    unreadable: Marks::NONE,
    unreadable_at: None,
  };

  /// Records `pos` as a part that is not proven in every layout, if none is recorded yet.
  const fn unspecified(&mut self, pos: usize) {
    if self.unspecified.is_none() {
      self.unspecified = Some(pos);
    }
  }

  /// Marks the leaf `leaf`, which is at `pos`, to be checked at run time.
  ///
  /// `pos` is recorded as `partial` if nothing is recorded there yet.
  const fn mark(&mut self, leaf: usize, pos: usize) {
    self.marks.set(leaf);
    if self.partial.is_none() {
      self.partial = Some(pos);
    }
  }

  /// Records `pos` as a byte read from padding, if none is recorded yet.
  const fn uninit(&mut self, pos: usize) {
    if self.uninit.is_none() {
      self.uninit = Some(pos);
    }
  }

  /// Records that a check of any of the `count` leaves from `leaf` would read the byte at `pos`,
  /// which may be uninitialized in the source.
  const fn unreadable(&mut self, leaf: usize, count: usize, pos: usize) {
    let mut i = 0;
    while i < count {
      self.unreadable.set(leaf + i);
      // The leaves past the capacity of a set are all in it, or none is.
      if leaf + i >= Marks::CAPACITY {
        break;
      }
      i += 1;
    }
    if self.unreadable_at.is_none() {
      self.unreadable_at = Some(pos);
    }
  }

  /// Records a byte read from padding if a leaf that is marked must not be read.
  ///
  /// This is the last step of an analysis whose marks tell the validation what to check. Before
  /// it, a leaf may still be marked.
  const fn settle(&mut self) {
    if let Some(pos) = self.unreadable_at
      && self.marks.meets(&self.unreadable)
    {
      self.uninit(pos);
    }
  }

  /// Returns whether every value of the source is a valid value of the target, with nothing to
  /// check.
  pub(crate) const fn infallible(&self) -> bool {
    self.uninit.is_none()
      && self.disjoint.is_none()
      && self.partial.is_none()
      && self.unspecified.is_none()
  }
}

/// Relates the first `end` bytes of `a` and `b`.
///
/// `end` is `usize::MAX` for all the bytes, however many. `checked` tells which rules of `b` may
/// be checked at run time. The bytes that these rules read must be initialized.
pub const fn relate(
  a: &'static Layout,
  b: &'static Layout,
  end: usize,
  checked: Checked,
) -> Relation {
  relate_at(a, 0, b, 0, 0, end, checked)
}

/// Relates the bytes `start..end` of `a`, which starts at `base_a`, and of `b`, which starts at
/// `base_b`.
///
/// `end` may be `usize::MAX`. Offsets count from the start of the outermost of the two layouts,
/// the offsets of the returned [`Relation`] included. This relates a target to the end of a
/// source (`b` at `size(a) - size(b)`), and the part of a source that a `&mut` covers to that
/// target (`a` there).
///
/// Nothing else changes with where the layouts are. The analysis is made of what each run of
/// `b` has under it. Periods are found from the strides of both, because what lies under a run of
/// `b` a period later is the same, whatever the phase of the repetitions of `a` and `b`. An enum is
/// proven only by an enum at the same offset.
///
/// # Panics
///
/// Panics if `checked` is not [`Checked::No`] and `base_a` is not 0. `a` must start at 0 then,
/// since [`Cursor::padding_is_read`] looks for what `a` has at the offsets of `b`.
pub const fn relate_at(
  a: &'static Layout,
  base_a: usize,
  b: &'static Layout,
  base_b: usize,
  start: usize,
  end: usize,
  checked: Checked,
) -> Relation {
  relate_with(a, base_a, b, base_b, start, end, checked, &Known::NONE)
}

/// Relates `a` and `b` as [`relate_at`] does, knowing what the source holds at some bytes.
///
/// A string that the target ends with and that starts at `end` is empty, and the range does not
/// reach it. No value of the source is valid for it if its automaton does not accept the empty
/// string.
#[expect(clippy::too_many_arguments, reason = "the arguments of `relate_at`, and what is known")]
const fn relate_with(
  a: &'static Layout,
  base_a: usize,
  b: &'static Layout,
  base_b: usize,
  start: usize,
  end: usize,
  checked: Checked,
  known: &Known,
) -> Relation {
  assert!(base_a == 0 || matches!(checked, Checked::No), "internal error: shifted source");
  let mut relation = Relation::NEW;
  if !sizes_kept(a, b) {
    relation.unspecified(start);
    return relation;
  }
  // No value of a source that has values is a value of a target without values, whatever their
  // sizes. A source that holds a union none of whose fields has values may exist without being
  // made, as may such a target: it is refused only if the target cannot exist.
  if !b.is_inhabited() && (a.is_inhabited() || a.may_exist() && !b.may_exist()) {
    relation.disjoint = Some(start);
    return relation;
  }
  // Nor of a target with an element without values. `is_inhabited` goes through every sized part,
  // not the elements of a slice. Nothing is assumed of the source, which may hold a value even
  // where none can be made (a union none of whose fields has values).
  if let Some((elem, at)) = tail_slice(b, base_b)
    && elem.size > 0
    && !elem.is_inhabited()
    && (end == usize::MAX || end >= at + elem.size)
    && !matches!(tail_slice(a, base_a), Some((source, from)) if from == at && equal(source, elem))
  {
    relation.disjoint = Some(at);
    return relation;
  }
  let mut a = Cursor::new(a, base_a, false, Checked::No);
  let mut b = Cursor::new(b, base_b, true, checked);
  b.known = *known;
  relate_range(&mut a, &mut b, start, end, &mut relation);
  // The empty string at `end`. The root of a dynamic layout has no end, so the cursor reaches it.
  if end != usize::MAX
    && b.stack[0].layout.is_dynamic()
    && end == base_b + b.stack[0].layout.size
    && let Run { kind: RunKind::Pattern(dfa), leaf, .. } = b.run_at(end)
    && !dfa.accepts(1)
  {
    relation.mark(leaf, end);
    if relation.disjoint.is_none() {
      relation.disjoint = Some(end);
    }
  }
  relation
}

/// Returns the offset of the first byte in `start..end` that a marked check of `target` reads,
/// and that the compiler may move in `source`.
///
/// `target` is at `base_b`, with its first leaf `leaf`, and `source` is at `base_a`. The checks are
/// those of a part of `target` that `marks` has, rules included ([`analyze`], [`analyze_tail`]). A
/// byte may move if it is in a part of `source` that is not ordered ([`moves`]), and in another
/// layout the check would read another part of the same value, and might answer otherwise.
///
/// The elements of an array share their leaves, so each one is checked if one is. Where the
/// source has elements of the same size at the same places, what the first element reads tells
/// of every one. Otherwise any byte under the array counts, which may find a byte that no check
/// reads. Within a variant of an enum, only the values of the source that have its `tag` are read
/// ([`moves`]).
///
/// The rule is general. This function does not look for the puns where every order would give
/// the same answer, such as `(u8, u8)` reinterpreted as `[bool; 2]`, and reports their bytes too.
#[expect(clippy::too_many_arguments, reason = "two layouts where they are, the bytes, the tag")]
const fn reads_moved(
  source: &'static Layout,
  base_a: usize,
  target: &'static Layout,
  base_b: usize,
  leaf: usize,
  marks: &Marks,
  start: usize,
  end: usize,
  tag: Option<Tag>,
) -> Option<usize> {
  let (lo, hi) = (max(start, base_b), min(end, extent_end(target, base_b)));
  if lo >= hi || !marks.any(leaf, target.leaves) {
    return None;
  }
  match target.kind {
    Kind::Struct(fields, rules) => {
      // Its rules, the last of its leaves, read any of its bytes.
      let rules_read = !rules.is_empty() && marks.any(leaf + target.leaves - 1, 1);
      if rules_read && moves(source, base_a, lo, hi, tag) {
        return Some(lo);
      }
      let mut i = 0;
      while i < fields.len() {
        let (field, at) = (fields[i].layout, base_b + fields[i].offset);
        let leaf = leaf + fields[i].leaf;
        let found = reads_moved(source, base_a, field, at, leaf, marks, start, end, tag);
        if found.is_some() {
          return found;
        }
        i += 1;
      }
      None
    }
    // Its tag, then the fields of the variant that the tag selects. Where the source has an enum
    // whose tag is at the same place, these count only for the values of the source whose tag is
    // that variant's.
    Kind::Enum(layout) if !layout.union => {
      let Kind::Scalar(spec) = layout.tag.layout.kind else { panic!() };
      let at = base_b + layout.tag.offset;
      if moves(source, base_a, max(lo, at), min(hi, at + spec.size), tag) {
        return Some(max(lo, at));
      }
      // Every leaf of the variant: the check of an enum checks the whole of it.
      let mut i = 0;
      while i < layout.variants.len() {
        let variant = &layout.variants[i];
        i += 1;
        let tag = Some(Tag { at, size: spec.size, endian: spec.endian, tags: variant.tags() });
        let found =
          reads_moved(source, base_a, variant.layout, base_b, 0, &Marks::ALL, lo, hi, tag);
        if found.is_some() {
          return found;
        }
      }
      None
    }
    Kind::Array(elem, _) | Kind::Slice(elem)
      if elem.size > 0 && same_elements(source, base_b - base_a, target) =>
    {
      reads_moved(source, base_a, elem, base_b, leaf, marks, start, end, tag)
    }
    // A scalar, a union (each of its fields), a pattern, or every element of an array.
    _ => {
      if moves(source, base_a, lo, hi, tag) {
        Some(lo)
      } else {
        None
      }
    }
  }
}

/// A tag of an enum of a target, and the values of it for which a check reads further.
#[derive(Copy, Clone)]
struct Tag {
  /// The offset of the tag from the start of the outermost of the source and the target.
  at: usize,
  /// The size of the tag.
  size: usize,
  /// The byte order of the tag.
  endian: Endian,
  /// The values of the tag of the variant that the check reads.
  tags: Tags,
}

/// Returns whether a byte in `start..end` of `source`, at `base`, is in a part that the compiler
/// may move.
///
/// A part may move if it is not [`ordered`](crate::layout::Stable::ordered). Where `tag` is at the
/// place of an enum tag in `source`, only the variants whose tags meet those of `tag` count, since
/// a check reads no further for the others. With the `unstable` feature, nothing moves, so the
/// result is `false`.
const fn moves(source: &Layout, base: usize, start: usize, end: usize, tag: Option<Tag>) -> bool {
  let (lo, hi) = (max(start, base), min(end, extent_end(source, base)));
  if !STRICT || lo >= hi {
    return false;
  }
  if !source.stable.ordered {
    return true;
  }
  match source.kind {
    Kind::Struct(fields, _) => {
      let mut i = 0;
      while i < fields.len() {
        if moves(fields[i].layout, base + fields[i].offset, lo, hi, tag) {
          return true;
        }
        i += 1;
      }
      false
    }
    Kind::Enum(layout) => {
      let Kind::Scalar(spec) = layout.tag.layout.kind else { panic!() };
      let only = match tag {
        Some(tag)
          if !layout.union
            && tag.at == base + layout.tag.offset
            && tag.size == spec.size
            && tag.endian.equals(spec.endian) =>
        {
          Some(tag.tags)
        }
        _ => None,
      };
      let mut i = 0;
      while i < layout.variants.len() {
        let variant = &layout.variants[i];
        i += 1;
        if let Some(tags) = only
          && !variant.tags().meets(&tags)
        {
          continue;
        }
        if moves(variant.layout, base, lo, hi, tag) {
          return true;
        }
      }
      false
    }
    // Every element is the same, so the first and the last element that the bytes overlap decide,
    // with one whole element between them.
    Kind::Array(elem, _) | Kind::Slice(elem) if elem.size > 0 => {
      let (first, last) = ((lo - base) / elem.size, (hi - 1 - base) / elem.size);
      if last - first >= 2 {
        let whole = base + (first + 1) * elem.size;
        return moves(elem, whole, whole, whole + elem.size, tag);
      }
      moves(elem, base + first * elem.size, lo, hi, tag)
        || moves(elem, base + last * elem.size, lo, hi, tag)
    }
    _ => false,
  }
}

/// Returns the element of the slice that `layout`, at `base`, ends with, and where the slice
/// starts, if `layout` ends with one.
///
/// The slice is the last field of a struct, or of a struct that is the last field, at any depth.
/// A string is not a slice: its bytes are not elements.
const fn tail_slice(layout: &'static Layout, base: usize) -> Option<(&'static Layout, usize)> {
  match layout.kind {
    Kind::Slice(elem) => Some((elem, base)),
    Kind::Struct(fields, _) if layout.unsized_ && !fields.is_empty() => {
      let last = &fields[fields.len() - 1];
      tail_slice(last.layout, base + last.offset)
    }
    _ => None,
  }
}

/// Returns one past the last byte of `layout` at `base`, or `usize::MAX` if it is unsized.
const fn extent_end(layout: &Layout, base: usize) -> usize {
  if layout.unsized_ { usize::MAX } else { base + layout.size }
}

/// Returns whether the comparison of the sizes of `a` and `b` is the same in every layout that
/// Rust allows.
///
/// It is if both sizes are the same in every layout, or if `a` and `b` are the same type. With
/// the `unstable` feature, the result is `true`.
const fn sizes_kept(a: &'static Layout, b: &'static Layout) -> bool {
  !STRICT || (a.stable.size && b.stable.size) || same_type(a, b)
}

/// Returns whether `a` and `b` are the layouts of the same type.
///
/// A type has one layout in a program. Its identity and its layout must both be equal.
const fn same_type(a: &Layout, b: &Layout) -> bool {
  a.id.equals(&b.id) && a.equals(b)
}

/// Returns whether `a` and `b` are the same layout in every layout that Rust allows.
///
/// They must be the same as `rustc` chose them. Without the `unstable` feature, what Rust leaves
/// to the compiler in them must also be of the same type. Two types of the same fields may be
/// arranged alike by one compiler and otherwise by the next.
const fn equal(a: &Layout, b: &Layout) -> bool {
  a.equals(b) && (!STRICT || alike(a, b))
}

/// Returns whether what Rust leaves to the compiler in `a` and `b` is of the same type.
///
/// `a` and `b` are two layouts that are the same as `rustc` chose them.
const fn alike(a: &Layout, b: &Layout) -> bool {
  if a.stable.deep && b.stable.deep {
    return true;
  }
  if !a.stable.fixed || !b.stable.fixed {
    return a.id.equals(&b.id);
  }
  match (a.kind, b.kind) {
    (Kind::Array(a, _), Kind::Array(b, _)) | (Kind::Slice(a), Kind::Slice(b)) => alike(a, b),
    (Kind::Struct(a, _), Kind::Struct(b, _)) => {
      let mut i = 0;
      while i < a.len() && i < b.len() {
        if !alike(a[i].layout, b[i].layout) {
          return false;
        }
        i += 1;
      }
      true
    }
    (Kind::Enum(a), Kind::Enum(b)) => {
      let mut i = 0;
      while i < a.variants.len() && i < b.variants.len() {
        if !alike(a.variants[i].layout, b.variants[i].layout) {
          return false;
        }
        i += 1;
      }
      true
    }
    _ => true,
  }
}

/// Relates `a` and `b` over `pos..end`, and records what it finds in `out`.
///
/// Returns `true` if this settles everything after `end` too, because what follows repeats forever,
/// or a pattern goes on until the end.
const fn relate_range(
  a: &mut Cursor,
  b: &mut Cursor,
  mut pos: usize,
  end: usize,
  out: &mut Relation,
) -> bool {
  while pos < end {
    let (run_a, run_b) = (a.run_at(pos), b.run_at(pos));
    // Patterns go on until the end.
    if let RunKind::Pattern(dfa) = run_b.kind {
      pattern_target(a, dfa, run_b.leaf, pos, end, out);
      return true;
    }
    if let RunKind::Pattern(_) = run_a.kind {
      // The bytes of a pattern are initialized: enough for a target without constraint.
      let free = matches!(run_b.kind, RunKind::Pad | RunKind::Any) && run_b.end >= end;
      if !free || b.enumeration().is_some() {
        // Otherwise nothing is proven, and everything is checked at run time.
        out.marks = Marks::ALL;
        if out.partial.is_none() {
          out.partial = Some(pos);
        }
        // No check accepts a scalar that a pun never makes.
        if b.stack[0].layout.holds_never() && out.disjoint.is_none() {
          out.disjoint = Some(pos);
        }
        // A part whose layout Rust leaves to the compiler is recorded too, since a check would read
        // it where `rustc` put it.
        if !b.stack[0].layout.checks_kept() {
          out.unspecified(pos);
        }
      }
      return true;
    }
    match find_skip(a, b, pos, end, &run_a, &run_b) {
      Skip::No => {}
      Skip::To { period, next } => {
        relate_range(a, b, pos, pos + period, out);
        pos = next;
        continue;
      }
      Skip::Forever { period } => {
        relate_range(a, b, pos, pos + period, out);
        return true;
      }
    }
    // A part whose layout Rust leaves to the compiler (after the repetitions: an array of it is one
    // period).
    if let RunKind::Opaque(target) = run_b.kind {
      let stop = min(run_b.end, end);
      let whole = run_b.start == pos && run_b.end <= end;
      // Not a part that holds a scalar that a pun never makes, which nothing proves, not even the
      // same type.
      let same = matches!(run_a.kind, RunKind::Opaque(source) if same_type(source, target))
        && run_a.start == pos
        && whole
        && !target.holds_never();
      let fields = if same || !whole { (false, None) } else { relate_fields(a, b, target, pos) };
      let proven = same || fields.0;
      // Proven, but what a check of it would read may be uninitialized in the source.
      if same && !checkable(target) {
        out.unreadable(run_b.leaf, target.leaves, pos);
      } else if let (true, Some(at)) = fields {
        out.unreadable(run_b.leaf, target.leaves, at);
      } else if !proven
        && target.leaves > 0
        && let Some(at) = uninit_from(a, pos, stop, &b.known)
      {
        // Not proven, yet checked where a union that a later field proves tries it, or where the
        // enum of a variant that holds it is checked as a whole. The check reads its fields where
        // `rustc` put them, so any byte under it counts.
        out.unreadable(run_b.leaf, target.leaves, at);
      }
      if !proven && target.holds_never() {
        // Refused in every layout, so not for the layout.
        if out.disjoint.is_none() {
          out.disjoint = Some(pos);
        }
      } else if !proven {
        out.unspecified(pos);
      }
      pos = stop;
      continue;
    }
    // What an enum requires of its fields depends on its tag: it is proven as a whole, by an
    // enum, or not at all.
    if let Some((enum_b, base_b, leaf)) = b.enumeration() {
      if let Some((enum_a, base_a, _)) = a.enumeration()
        && base_a == pos
        && base_b == pos
        && relate_enums(enum_a, enum_b, pos, leaf, out)
      {
        pos += enum_b.size;
        continue;
      }
      // A union: one of its fields.
      if base_b == pos && pos + enum_b.size <= end && relate_union_from(a, enum_b, pos, leaf, out) {
        pos += enum_b.size;
        continue;
      }
      // From something else, variant by variant: the ones whose tags the source can hold there.
      if base_b == pos
        && pos + enum_b.size <= end
        && relate_enum_from(a, enum_b, pos, leaf, &b.known, out)
      {
        pos += enum_b.size;
        continue;
      }
      // Otherwise the enum is marked to be checked as a whole at run time, which cannot be done if
      // the check would read a part whose layout Rust leaves to the compiler, where `rustc` put it.
      out.mark(leaf, pos);
      if !enum_b.checks_kept() {
        out.unspecified(pos);
      }
    }
    let stop = min(run_b.end, end);
    match run_b.kind {
      // Padding that a rule reads accepts everything but padding.
      RunKind::Pad if b.padding_is_read(a.stack[0].layout) => {
        if run_a.kind.may_be_uninit() && !b.known.covers(pos) {
          out.uninit(pos);
        }
        pos = min(run_a.end, stop);
      }
      // Padding accepts everything.
      RunKind::Pad => pos = stop,
      // Unconstrained bytes accept everything but padding.
      RunKind::Any => {
        if run_a.kind.may_be_uninit() && !b.known.covers(pos) {
          out.uninit(pos);
          // An enum here is marked above, and its check may read the byte: a tag that accepts
          // every value, or a field of its variant. A constrained tag is recorded as an atom.
          if let Some((_, _, leaf)) = b.enumeration() {
            out.unreadable(leaf, 1, pos);
          }
        }
        pos = min(run_a.end, stop);
      }
      // Only some bytes of the scalar are related, and what the others hold is not known: `b` is
      // the source of a `&mut` that covers a part of it.
      RunKind::Atom(_) if run_b.start != pos || run_b.end > end => {
        out.mark(run_b.leaf, pos);
        pos = stop;
      }
      RunKind::Atom(spec) => {
        // The tag, whose values may be known.
        let relation = if let Some(known) = b.known.of(run_b.start, run_b.end, spec.endian) {
          let Valid::In(ranges) = spec.valid else { panic!("not an atom") };
          Some(relate_ranges(known, ranges))
        } else {
          let (pieces, len) = gather(a, run_b.start, run_b.end);
          atom_relation(&pieces, len, &spec)
        };
        match relation {
          // Nothing proves it, and its check would read what may be uninitialized.
          None => {
            out.uninit(pos);
            out.unreadable(run_b.leaf, 1, pos);
          }
          Some(Rel::Subset) => {}
          Some(rel) => {
            out.mark(run_b.leaf, pos);
            if matches!(rel, Rel::Disjoint) && out.disjoint.is_none() {
              out.disjoint = Some(pos);
            }
          }
        }
        pos = run_b.end;
      }
      RunKind::Pattern(_) | RunKind::Opaque(_) => unreachable!(),
    }
  }
  false
}

/// The most places at which [`relate_fields`] relates the fields of a struct with a source.
///
/// Past them, the struct only proves itself.
const PLACES: usize = 1024;

/// Returns whether the source `a` proves `target` at `pos`, in every order of the fields of
/// `target`.
///
/// `target` is a struct or a tuple of the `Rust` representation, whose size Rust specifies but
/// not the order of its fields ([`RunKind::Opaque`]). A field may be anywhere, so it must be
/// proven everywhere it may be.
///
/// A struct is valid if each of its fields is valid at its place. So the source proves the
/// struct in every order of its fields if, and only if, it proves each field at each offset that
/// the field has in some order. This is exact, and no order is tried. The offsets taken are all
/// those that the field may have, the multiples of its alignment where it fits. Where no order
/// needs padding, because every size is a multiple of every alignment, they are the multiples of
/// the greatest common divisor of the sizes, since every sum of sizes is one. Some of these offsets
/// are in no order, such as the second byte of a `(bool, [u8; 3])`. More is then required, never
/// less.
///
/// Nothing is left to check at run time, since a check would read where the compiler may not have
/// put the field. A field that the source does not prove somewhere fails the relation. So does a
/// rule, which names fields. Past [`PLACES`] places, the relation fails too.
///
/// Also returns the offset of a byte that a check of a field would read, and that may be
/// uninitialized in the source, if there is one ([`Relation::unreadable`]).
const fn relate_fields(
  a: &Cursor,
  b: &Cursor,
  target: &'static Layout,
  pos: usize,
) -> (bool, Option<usize>) {
  let Kind::Struct(fields, _) = target.kind else { return (false, None) };
  if !target.stable.size || target.has_rules {
    return (false, None);
  }
  // The greatest common divisor of the sizes.
  let (mut step, mut i) = (0, 0);
  while i < fields.len() {
    let field = fields[i].layout;
    if field.size > 0 {
      step = if step == 0 { field.size } else { gcd(step, field.size) };
    }
    i += 1;
  }
  let tiled = target.tiled();
  let (mut places, mut proven, mut unreadable, mut i) = (0, true, None, 0);
  while i < fields.len() {
    let field = fields[i].layout;
    i += 1;
    if field.size == 0 {
      continue;
    }
    let by = if tiled { step } else { min(field.align, target.align) };
    let mut offset = 0;
    while offset + field.size <= target.size {
      places += 1;
      if places > PLACES {
        return (false, None);
      }
      let start = pos + offset;
      let (mut from, mut to) = (*a, Cursor::new(field, start, true, b.checked));
      to.known = b.known;
      let mut relation = Relation::NEW;
      relate_range(&mut from, &mut to, start, start + field.size, &mut relation);
      proven &= relation.infallible();
      if unreadable.is_none() {
        unreadable = relation.unreadable_at;
      }
      offset += by;
    }
  }
  (proven, unreadable)
}

/// Relates the enum `a` with the enum `b`, both at `pos`, variant by variant.
///
/// Each variant of `a` is related with the variants of `b` that its tags are tags of, knowing
/// that the tag is one of both. A variant of `a` is proven if all its tags are of one variant of
/// `b`, which it proves. If a variant of `a` is not proven, the enum is marked, and checked as a
/// whole at run time. Returns `true` at once if `a` and `b` are the same layout ([`equal`]) that
/// holds no scalar that a pun never makes, which nothing proves, not even the same layout.
///
/// Returns `false`, having done nothing, if either is a union, if `a` is smaller than `b`, or if
/// their tags differ in place, size or byte order.
const fn relate_enums(
  a: &'static Layout,
  b: &'static Layout,
  pos: usize,
  leaf: usize,
  out: &mut Relation,
) -> bool {
  if equal(a, b) && !b.holds_never() {
    // Proven without reading anything. A check of it would read what the same type does not
    // promise to be initialized, such as the bytes of a union.
    if !checkable(b) {
      out.unreadable(leaf, 1, pos);
    }
    return true;
  }
  let (Kind::Enum(enum_a), Kind::Enum(enum_b)) = (a.kind, b.kind) else { panic!() };
  // A union is not told by a tag: one only proves the same union.
  if enum_a.union || enum_b.union {
    return false;
  }
  let (Kind::Scalar(tag_a), Kind::Scalar(tag_b)) = (enum_a.tag.layout.kind, enum_b.tag.layout.kind)
  else {
    panic!()
  };
  if a.size < b.size
    || enum_a.tag.offset != enum_b.tag.offset
    || tag_a.size != tag_b.size
    || !tag_a.endian.equals(tag_b.endian)
  {
    return false;
  }
  // Whether some variants of `a` are valid for `b`, and whether all are.
  let (mut some, mut all, mut i) = (false, true, 0);
  // A byte that the check of a variant would read, and that may be uninitialized in the source.
  let mut unreadable = None;
  let (tag, size) = (enum_b.tag.offset, tag_b.size);
  while i < enum_a.variants.len() {
    let from = &enum_a.variants[i];
    i += 1;
    // A variant that no value is, under any rule for unions.
    if !from.may_exist() {
      continue;
    }
    // Proven if all of its tags are of one variant of `b`, which it proves.
    let (tags, mut proven, mut j) = (from.tags(), false, 0);
    while j < enum_b.variants.len() {
      let to = &enum_b.variants[j];
      j += 1;
      if !tags.meets(&to.tags()) {
        continue;
      }
      // What the tag holds in `to`: the tags of both.
      let known = Known::both(tag, size, tag_b.endian, &tags, &to.tags());
      let mut relation = relate_with(from.layout, 0, to.layout, 0, 0, b.size, Checked::All, &known);
      mark_rules(from.layout, to.layout, 0, 0, &mut relation);
      unreadable_rules(from.layout, 0, to.layout, 0, 0, b.size, &known, &mut relation);
      if let Some(offset) = relation.uninit {
        out.uninit(pos + offset);
      }
      // The check of the enum would read it where `rustc` put it.
      if let Some(offset) = relation.unspecified {
        out.unspecified(pos + offset);
      }
      if let (None, Some(offset)) = (unreadable, relation.unreadable_at) {
        unreadable = Some(pos + offset);
      }
      some |= relation.disjoint.is_none();
      proven |= relation.infallible() && tags.within(&to.tags());
    }
    // Else some of its tags are not of `b`, or of several variants, or the fields of one are
    // not proven.
    all &= proven;
  }
  if !all {
    out.mark(leaf, pos);
  }
  // The check of the enum checks every part of the variant that the tag selects.
  if let Some(at) = unreadable {
    out.unreadable(leaf, 1, at);
  }
  if !some && out.disjoint.is_none() {
    out.disjoint = Some(pos);
  }
  true
}

/// Relates what the source `a` holds at `pos` with the enum `b` there, which an enum of the
/// source did not prove.
///
/// The tag of `b` is related as a scalar ([`tag_values`]). Each variant whose tags the source may
/// hold there is related with what the source holds over the bytes of the enum, knowing that
/// the tag is one of those tags. A niche of the variant is then proven by them.
///
/// The enum is proven if every value that the source may hold at the tag is the tag of a variant
/// that has values, and each such variant is proven. Otherwise the enum is marked, and checked
/// as a whole at run time. A byte that a variant requires to be initialized must be initialized
/// in the source, if the source may be that variant. If the tag may be padding in the source, that
/// is recorded, since no value of the source is then this enum.
///
/// Returns `false`, having done nothing, if `b` is a union.
const fn relate_enum_from(
  a: &Cursor,
  b: &'static Layout,
  pos: usize,
  leaf: usize,
  known: &Known,
  out: &mut Relation,
) -> bool {
  let Kind::Enum(layout) = b.kind else { panic!() };
  let Kind::Scalar(tag) = layout.tag.layout.kind else { panic!() };
  if layout.union {
    return false;
  }
  let start = pos + layout.tag.offset;
  // What the source holds at the tag.
  let Some(values) = tag_values(a, start, &tag, known) else {
    // Padding where the tag is: nothing is ever this enum, and its check would read the tag.
    out.uninit(start);
    out.unreadable(leaf, 1, start);
    return true;
  };
  let (mut some, mut all, mut i) = (false, true, 0);
  // A byte that the check of a variant would read, and that may be uninitialized in the source.
  let mut unreadable = None;
  let (source, base) = (a.stack[0].layout, a.stack[0].base);
  while i < layout.variants.len() {
    let variant = &layout.variants[i];
    i += 1;
    if !variant.is_present() || !values.meets(&variant.tags()) {
      continue;
    }
    let known = Known::both(start, tag.size, tag.endian, &values, &variant.tags());
    let mut relation = Relation::NEW;
    let (mut from, mut to) = (*a, Cursor::new(variant.layout, pos, true, Checked::All));
    to.known = known;
    relate_range(&mut from, &mut to, pos, pos + b.size, &mut relation);
    mark_rules(source, variant.layout, pos, 0, &mut relation);
    unreadable_rules(source, base, variant.layout, pos, 0, pos + b.size, &known, &mut relation);
    if let Some(offset) = relation.uninit {
      out.uninit(offset);
    }
    if let Some(offset) = relation.unspecified {
      out.unspecified(offset);
    }
    if unreadable.is_none() {
      unreadable = relation.unreadable_at;
    }
    some |= relation.disjoint.is_none();
    all &= relation.infallible();
  }
  if !all || !all_tags(&values, layout.variants) {
    out.mark(leaf, pos);
  }
  // The check of the enum checks every part of the variant that the tag selects.
  if let Some(at) = unreadable {
    out.unreadable(leaf, 1, at);
  }
  if !some && out.disjoint.is_none() {
    out.disjoint = Some(pos);
  }
  true
}

/// Relates what the source `a` holds at `pos` with the union `b` there, which the same union of
/// the source did not prove.
///
/// The union is proven if the source proves one of its fields. Otherwise it is checked at run
/// time, field by field, if no field's check would read a byte that may be padding in the
/// source. Otherwise it is refused, with that byte recorded as read from padding.
///
/// Returns `false`, having done nothing, if `b` is not a union.
const fn relate_union_from(
  a: &Cursor,
  b: &'static Layout,
  pos: usize,
  leaf: usize,
  out: &mut Relation,
) -> bool {
  let Kind::Enum(layout) = b.kind else { panic!() };
  if !layout.union {
    return false;
  }
  let (mut proven, mut some, mut uninit, mut i) = (false, false, None, 0);
  let mut unspecified = None;
  // A byte that a check of the union would read, and that may be uninitialized in the source. The
  // check tries the fields in order, and stops at the first that the bytes are, so it reads no
  // field after the first that the source proves. This is less than what a field requires to be
  // initialized: a byte of any value must be initialized, and no check reads it.
  let mut unreadable = None;
  let (source, base) = (a.stack[0].layout, a.stack[0].base);
  while i < layout.variants.len() {
    let field = &layout.variants[i];
    i += 1;
    if !field.is_present() {
      continue;
    }
    let mut relation = Relation::NEW;
    let (mut from, mut to) = (*a, Cursor::new(field.layout, pos, true, Checked::All));
    relate_range(&mut from, &mut to, pos, pos + b.size, &mut relation);
    mark_rules(source, field.layout, pos, 0, &mut relation);
    unreadable_rules(source, base, field.layout, pos, 0, pos + b.size, &Known::NONE, &mut relation);
    if unspecified.is_none() {
      unspecified = relation.unspecified;
    }
    if !proven && unreadable.is_none() {
      unreadable = relation.unreadable_at;
    }
    if relation.uninit.is_none() {
      proven |= relation.infallible();
      some |= relation.disjoint.is_none();
    } else if uninit.is_none() {
      // Its check would read padding: it cannot be checked.
      uninit = relation.uninit;
    }
  }
  // A check would try the fields before the one that proves the union, and that one, or every
  // field if none does.
  if let Some(at) = unreadable {
    out.unreadable(leaf, 1, at);
  }
  if proven {
    return true;
  }
  // Checked field by field, each where `rustc` put what it holds.
  if let Some(offset) = unspecified {
    out.unspecified(offset);
  }
  if let Some(offset) = uninit {
    out.uninit(offset);
  } else {
    out.mark(leaf, pos);
    if !some && out.disjoint.is_none() {
      out.disjoint = Some(pos);
    }
  }
  true
}

/// Returns the values that the source `a` may hold at `start`, where a target has the scalar
/// `tag`, as an integer of the byte order of `tag`.
///
/// These are the first of the following that applies:
///
/// - The values `known` there, as one range that holds them. They are the tag of an enum that
///   holds this one in a niche.
/// - What a scalar of the source of the same size and byte order is known to be.
/// - Any value.
///
/// # Errors
///
/// Returns `None` if some of these bytes may be uninitialized.
const fn tag_values(a: &Cursor, start: usize, tag: &Spec, known: &Known) -> Option<Tags> {
  if let Some(ranges) = known.of(start, start + tag.size, tag.endian) {
    // Their hull, which holds them.
    return Some(Tags::range(ranges[0].0, ranges[ranges.len() - 1].1));
  }
  let mut from = *a;
  let (pieces, len) = gather(&mut from, start, start + tag.size);
  let mut i = 0;
  while i < len {
    if pieces[i].kind.may_be_uninit() {
      return None;
    }
    i += 1;
  }
  if len == 1
    && let RunKind::Atom(Spec { size, endian, known: Valid::In(ranges), .. }) = pieces[0].kind
    && pieces[0].skip == 0
    && size == tag.size
    && (size == 1 || endian.equals(tag.endian))
  {
    return Some(Tags::of(ranges));
  }
  Some(Tags::range(0, max_value(tag.size)))
}

/// Returns whether every value of `values` is a tag of one of `variants` that has values.
const fn all_tags(values: &Tags, variants: &[crate::layout::Variant]) -> bool {
  let mut r = 0;
  while r < values.len() {
    let (mut next, hi) = values.get(r);
    r += 1;
    loop {
      // The range of the tags of a variant that holds `next`, if any.
      let (mut found, mut i) = (None, 0);
      while i < variants.len() && found.is_none() {
        let tags = variants[i].tags();
        let mut j = 0;
        while variants[i].is_present() && j < tags.len() {
          let (lo, up) = tags.get(j);
          if lo <= next && next <= up {
            found = Some(up);
          }
          j += 1;
        }
        i += 1;
      }
      match found {
        None => return false,
        Some(up) if up >= hi => break,
        Some(up) => next = up + 1,
      }
    }
  }
  true
}

/// No repetition, to fill arrays of [`Repeat`].
const NO: Repeat = Repeat { stride: 0, end: 0, run: false, id: (0, 0) };

/// What the analysis may skip of repeated bytes.
enum Skip {
  No,
  /// What happens for the next `period` bytes repeats until `next`.
  To {
    period: usize,
    next: usize,
  },
  /// What happens for the next `period` bytes repeats forever.
  Forever {
    period: usize,
  },
}

/// Returns a period of both layouts at `pos`, if there is one.
///
/// `b` must be at the start of an element of a repetition. The analysis is made of what happens
/// to each run of `b`, so a period must not cut one. Nor may it cut an enum or a union of `b`,
/// which is related as a whole ([`relate_enums`], [`relate_union_from`], [`relate_enum_from`]).
/// So a run of padding or of bytes of any value in one, such as a tag that accepts every value, is
/// not a repetition, while an element of an array of `b` holds the enums in it whole. `a` only has
/// to be inside a repetition, because what lies under a run of `b` a period later is the same, even
/// if the period cuts runs of `a`. A period is taken if it repeats forever or at least twice before
/// `end`. The outermost repetitions are tried first, since they skip the most.
///
/// What is known of a tag ([`Cursor::known`]) holds of its own bytes, not of the bytes a period
/// later. So bytes that hold it are not a period: what they prove, the next ones may not. The
/// first element of an array that holds a niche is such bytes.
const fn find_skip(
  a: &Cursor,
  b: &Cursor,
  pos: usize,
  end: usize,
  run_a: &Run,
  run_b: &Run,
) -> Skip {
  let (mut repeats_a, mut repeats_b) = ([NO; MAX_DEPTH + 1], [NO; MAX_DEPTH + 1]);
  let mut len_b = b.repeats(pos, run_b, true, &mut repeats_b);
  // A run of an enum, the last repetition.
  if len_b > 0 && repeats_b[len_b - 1].run && b.enumeration().is_some() {
    len_b -= 1;
  }
  if len_b == 0 {
    return Skip::No;
  }
  let len_a = a.repeats(pos, run_a, false, &mut repeats_a);
  // Outermost repetitions first: they skip the most.
  let mut i = 0;
  while i < len_b {
    let mut j = 0;
    while j < len_a {
      let (ra, rb) = (repeats_a[j], repeats_b[i]);
      j += 1;
      // Two runs are stepped over at once by `relate_range`.
      if ra.run && rb.run {
        continue;
      }
      let Some(period) = lcm(ra.stride, rb.stride) else { continue };
      if b.known.meets(pos, pos.saturating_add(period)) {
        continue;
      }
      let until = min(min(ra.end, rb.end), end);
      if until == usize::MAX {
        return Skip::Forever { period };
      }
      let count = (until - pos) / period;
      if count >= 2 {
        return Skip::To { period, next: pos + count * period };
      }
    }
    i += 1;
  }
  Skip::No
}

/// Returns the greater of `a` and `b`.
const fn max(a: usize, b: usize) -> usize {
  if a > b { a } else { b }
}

/// Returns the lesser of `a` and `b`.
const fn min(a: usize, b: usize) -> usize {
  if a < b { a } else { b }
}

/// Returns the greatest common divisor of `a` and `b`, or the other if one is 0.
const fn gcd(a: usize, b: usize) -> usize {
  if b == 0 { a } else { gcd(b, a % b) }
}

/// Returns the least common multiple of `a` and `b`, or `None` if it overflows.
///
/// # Panics
///
/// Panics if both `a` and `b` are 0.
const fn lcm(a: usize, b: usize) -> Option<usize> {
  (a / gcd(a, b)).checked_mul(b)
}

// -------------------------------------------------------------------------------------------------
// Relation of what lies under a constrained scalar with that scalar

/// The relation of a set of values `A` with a set of values `B`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Rel {
  /// `A ⊆ B`.
  Subset,
  /// `A` meets `B`, and is not a subset of it.
  Overlap,
  /// `A ∩ B = ∅`.
  Disjoint,
}

/// What the source `a` has under some bytes of a scalar of the target `b`.
#[derive(Copy, Clone)]
struct Piece {
  /// The kind of the bytes.
  kind: RunKind,
  /// The first byte, as an offset in the scalar of `b`.
  from: usize,
  /// One past the last byte, as an offset in the scalar of `b`.
  to: usize,
  /// For an atom of `a`, the offset in that atom of its first byte under the scalar of `b`.
  skip: usize,
}

/// Returns the pieces of what `a` has over `start..end`, and their number.
///
/// An opaque part of the source is what every layout of it has ([`RunKind::seen`]).
///
/// # Panics
///
/// Panics if there are more than 16 pieces, which cannot happen if `end - start` is at most 16,
/// the size of the largest scalar.
const fn gather(a: &mut Cursor, start: usize, end: usize) -> ([Piece; 16], usize) {
  let mut pieces = [Piece { kind: RunKind::Any, from: 0, to: 0, skip: 0 }; 16];
  let (mut len, mut pos) = (0, start);
  while pos < end {
    let mut run = a.run_at(pos);
    run.kind = run.kind.seen();
    let to = min(run.end, end);
    let skip = if matches!(run.kind, RunKind::Atom(_)) { pos - run.start } else { 0 };
    pieces[len] = Piece { kind: run.kind, from: pos - start, to: to - start, skip };
    (len, pos) = (len + 1, to);
  }
  (pieces, len)
}

/// Returns the relation of the values that the first `len` `pieces` may hold with the values that
/// `spec` accepts.
///
/// The answer is exact, whatever the alignment of the pieces and their byte orders. Past a budget
/// of computation that is not reached in practice, the answer is [`Rel::Overlap`]. A restriction
/// that the type system does not enforce (`valid(..)`) only proves a restriction of the same
/// kind.
///
/// # Errors
///
/// Returns `None` if one of the pieces may be uninitialized, unless `spec` is a scalar that a pun
/// never makes, which no source is, whatever its pieces hold.
const fn atom_relation(pieces: &[Piece; 16], len: usize, spec: &Spec) -> Option<Rel> {
  let ranges = match spec.valid {
    Valid::In(ranges) => ranges,
    // No value of the source is one, whatever it holds.
    Valid::Never => return Some(Rel::Disjoint),
    Valid::Any | Valid::Uninit => panic!("not an atom"),
  };
  // What is known of the pieces: a restriction that the type system does not enforce only
  // proves a restriction of the same kind.
  let (mut known, soft) = (*pieces, spec.is_soft());
  let (mut i, mut all_any) = (0, true);
  while i < len {
    match known[i].kind {
      RunKind::Pad | RunKind::Opaque(_) => return None,
      // Whatever a pattern accepts is initialized, and that is all that is made of it here.
      RunKind::Any | RunKind::Pattern(_) => {}
      // Required to be some values, but a source may hold anything there, padding included.
      RunKind::Atom(from) if from.may_be_uninit() => return None,
      RunKind::Atom(from) => match if soft { from.valid } else { from.known } {
        valid @ Valid::In(_) => {
          known[i].kind = RunKind::Atom(Spec { valid, ..from });
          all_any = false;
        }
        _ => known[i].kind = RunKind::Any,
      },
    }
    i += 1;
  }
  let pieces = &known;
  // The scalar is not full, and everything may come.
  if all_any {
    return Some(Rel::Overlap);
  }
  // The usual case: a scalar over a scalar of the same size and byte order.
  if len == 1
    && let RunKind::Atom(Spec { size, endian, valid: Valid::In(from), .. }) = pieces[0].kind
    && pieces[0].skip == 0
    && size == spec.size
    && (size == 1 || endian.equals(spec.endian))
  {
    return Some(relate_ranges(from, ranges));
  }
  // The general case.
  let mut boxes =
    Boxes { pieces, len, spec, ranges, escapes: false, meets: false, budget: 1 << 14 };
  boxes.explore(0, &mut [0; 16], &mut [0xff; 16]);
  Some(match (boxes.escapes, boxes.meets) {
    (false, _) => Rel::Subset,
    (true, false) => Rel::Disjoint,
    (true, true) => Rel::Overlap,
  })
}

/// Returns the relation of two sorted lists of disjoint and non-adjacent ranges.
pub const fn relate_ranges(a: &[(u128, u128)], b: &[(u128, u128)]) -> Rel {
  let (mut subset, mut meets, mut i) = (true, false, 0);
  while i < a.len() {
    let ((lo, hi), mut inside, mut j) = (a[i], false, 0);
    while j < b.len() {
      meets |= lo <= b[j].1 && b[j].0 <= hi;
      // The ranges of `b` are not adjacent: a range of `a` is in `b` if it is in one range of it.
      inside |= b[j].0 <= lo && hi <= b[j].1;
      j += 1;
    }
    subset &= inside;
    i += 1;
  }
  match (subset, meets) {
    (true, _) => Rel::Subset,
    (false, true) => Rel::Overlap,
    (false, false) => Rel::Disjoint,
  }
}

/// The values that the pieces may hold, as a union of boxes, related to the values of a scalar.
///
/// A box is a set of byte strings given by a range for each byte, which does not depend on any
/// byte order. A range of integers splits into at most `2 * size - 1` boxes ([`digit_box`]).
/// What a box has in common with a range of integers of the scalar is found without enumerating
/// anything ([`box_meets`]).
struct Boxes<'a> {
  /// The pieces of the source, of which the first `len` are used.
  pieces: &'a [Piece; 16],
  /// The number of pieces.
  len: usize,
  /// The scalar of the target.
  spec: &'a Spec,
  /// The valid values of the scalar.
  ranges: &'a [(u128, u128)],
  /// Whether some values of the pieces are not valid for the scalar.
  escapes: bool,
  /// Whether some values of the pieces are valid for the scalar.
  meets: bool,
  /// The number of boxes left to check. Past it, both `escapes` and `meets` are set.
  budget: usize,
}

impl Boxes<'_> {
  /// Checks each box of the pieces from the `index`th on, given a box of the pieces before.
  ///
  /// The box `lo..=hi` is a range for each byte of the scalar, in memory order. The search stops
  /// once both `escapes` and `meets` are set.
  const fn explore(&mut self, index: usize, lo: &mut [u8; 16], hi: &mut [u8; 16]) {
    if self.escapes && self.meets {
      return;
    }
    if index == self.len {
      return self.check(lo, hi);
    }
    let piece = self.pieces[index];
    let RunKind::Atom(Spec { size, endian, valid: Valid::In(ranges), .. }) = piece.kind else {
      // Unconstrained bytes.
      let mut i = piece.from;
      while i < piece.to {
        (lo[i], hi[i]) = (0, 0xff);
        i += 1;
      }
      return self.explore(index + 1, lo, hi);
    };
    let mut r = 0;
    while r < ranges.len() {
      let (mut t, count) = (0, box_count(ranges[r].0, ranges[r].1, size));
      while t < count {
        if let Some((digits_lo, digits_hi)) = digit_box(ranges[r].0, ranges[r].1, size, t) {
          // Project the box on the bytes of the piece.
          let mut i = piece.from;
          while i < piece.to {
            let byte = piece.skip + (i - piece.from);
            let digit = if endian.is_little() { size - 1 - byte } else { byte };
            (lo[i], hi[i]) = (digits_lo[digit], digits_hi[digit]);
            i += 1;
          }
          self.explore(index + 1, lo, hi);
        }
        t += 1;
      }
      r += 1;
    }
  }

  /// Updates `escapes` and `meets` with the box `lo..=hi` of the pieces.
  const fn check(&mut self, lo: &[u8; 16], hi: &[u8; 16]) {
    if self.budget == 0 {
      (self.escapes, self.meets) = (true, true);
      return;
    }
    self.budget -= 1;
    // Most significant digit of the scalar first.
    let (size, little) = (self.spec.size, self.spec.endian.is_little());
    let (mut digits_lo, mut digits_hi, mut i) = ([0; 16], [0; 16], 0);
    while i < size {
      let byte = if little { size - 1 - i } else { i };
      (digits_lo[i], digits_hi[i]) = (lo[byte], hi[byte]);
      i += 1;
    }
    // `gap` is the first value after the previous range.
    let (mut i, mut gap) = (0, Some(0));
    while i < self.ranges.len() {
      let (start, end) = self.ranges[i];
      self.meets = self.meets || box_meets(&digits_lo, &digits_hi, size, start, end);
      if let Some(gap) = gap
        && gap < start
      {
        self.escapes = self.escapes || box_meets(&digits_lo, &digits_hi, size, gap, start - 1);
      }
      gap = if end < max_value(size) { Some(end + 1) } else { None };
      i += 1;
    }
    if let Some(gap) = gap {
      self.escapes = self.escapes || box_meets(&digits_lo, &digits_hi, size, gap, max_value(size));
    }
  }
}

/// Returns the `index`th digit, in base 256 and most significant first, of a number of `size`
/// digits.
const fn digit(value: u128, size: usize, index: usize) -> u8 {
  (value >> (8 * (size - 1 - index))) as u8
}

/// Returns the index of the first digit that differs between `lo` and `hi`, or `size` if none
/// does.
const fn first_difference(lo: u128, hi: u128, size: usize) -> usize {
  let mut i = 0;
  while i < size && digit(lo, size, i) == digit(hi, size, i) {
    i += 1;
  }
  i
}

/// Returns the number of boxes that `lo..=hi` splits into ([`digit_box`]).
const fn box_count(lo: u128, hi: u128, size: usize) -> usize {
  let p = first_difference(lo, hi, size);
  if p + 1 >= size { 1 } else { 1 + 2 * (size - 1 - p) }
}

/// Returns the `index`th box of the split of `lo..=hi`, or `None` if that box is empty.
///
/// A box is a range for each digit, most significant first. With `p` the first digit that
/// differs between `lo` and `hi`, the numbers of `lo..=hi` are the following:
///
/// - The numbers whose digit `p` is strictly between those of `lo` and `hi`: the first box.
/// - The numbers that start like `lo` up to a digit `q > p`, which is greater: a box for each `q`.
/// - The numbers that start like `hi` up to a digit `q > p`, which is less: a box for each `q`.
/// - `lo` and `hi` themselves, which are in the boxes of the last digit.
const fn digit_box(lo: u128, hi: u128, size: usize, index: usize) -> Option<([u8; 16], [u8; 16])> {
  let p = first_difference(lo, hi, size);
  let (mut box_lo, mut box_hi) = ([0; 16], [0xff; 16]);
  // The digits before `q` are those of `prefix`, the digits after are free.
  let (prefix, q, range);
  if p + 1 >= size {
    // `lo..=hi` is a box: only the last digit differs, if any.
    (prefix, q) = (lo, size - 1);
    range = (digit(lo, size, q), digit(hi, size, q));
  } else if index == 0 {
    (prefix, q) = (lo, p);
    if digit(hi, size, p) - digit(lo, size, p) < 2 {
      return None;
    }
    range = (digit(lo, size, p) + 1, digit(hi, size, p) - 1);
  } else if index <= size - 1 - p {
    (prefix, q) = (lo, p + index);
    let digit = digit(lo, size, q);
    if q == size - 1 {
      range = (digit, 0xff);
    } else if digit == 0xff {
      return None;
    } else {
      range = (digit + 1, 0xff);
    }
  } else {
    (prefix, q) = (hi, p + index - (size - 1 - p));
    let digit = digit(hi, size, q);
    if q == size - 1 {
      range = (0, digit);
    } else if digit == 0 {
      return None;
    } else {
      range = (0, digit - 1);
    }
  }
  let mut i = 0;
  while i < q {
    (box_lo[i], box_hi[i]) = (digit(prefix, size, i), digit(prefix, size, i));
    i += 1;
  }
  (box_lo[q], box_hi[q]) = range;
  Some((box_lo, box_hi))
}

/// Returns whether a number of `start..=end` is in the box `lo..=hi`, a range for each digit,
/// most significant first.
const fn box_meets(lo: &[u8; 16], hi: &[u8; 16], size: usize, start: u128, end: u128) -> bool {
  // Look for the least number of the box that is not less than `start`: it starts like `start`
  // for as long as the box allows it.
  let mut i = 0;
  while i < size && lo[i] <= digit(start, size, i) && digit(start, size, i) <= hi[i] {
    i += 1;
  }
  // Whether the digit at `i` is raised. If it is not, it is the least digit of the box, as are the
  // digits after it.

  let bump = if i == size {
    // `start` is in the box.
    return true;
  } else if digit(start, size, i) < lo[i] {
    // Any digit of the box is greater there.
    false
  } else {
    // Any digit of the box is lesser there: a previous digit has to be greater than in `start`.
    loop {
      if i == 0 {
        return false;
      }
      i -= 1;
      if digit(start, size, i) < hi[i] {
        break;
      }
    }
    true
  };
  let (mut least, mut k) = (0u128, 0);
  while k < size {
    let digit = match () {
      () if k < i => digit(start, size, k),
      () if k == i && bump => digit(start, size, k) + 1,
      () => lo[k],
    };
    least = (least << 8) | digit as u128;
    k += 1;
  }
  least <= end
}

// -------------------------------------------------------------------------------------------------
// Patterns

/// Relates what `a` has from `pos` with the pattern `dfa` of the target, whose leaf is `leaf`.
///
/// The automaton runs over the runs of `a`, with a set of states. The rest of an array or a slice
/// of `a` is skipped as soon as one of its elements leaves the set of states as it found it. The
/// pattern is marked if a string of the source may not be accepted, or past a budget of runs. A
/// byte that may be padding under the pattern is recorded as read from padding.
///
/// The source may end wherever its slice may, and from the end of its sized part where it ends with
/// bytes of any value, which are one run without elements: the automaton must accept there, before
/// the first byte as after each one.
const fn pattern_target(
  a: &mut Cursor,
  dfa: &'static Dfa,
  leaf: usize,
  mut pos: usize,
  end: usize,
  out: &mut Relation,
) {
  // The automaton is in one of the states of `set`.
  let mut set = 1u64;
  // The `set` at the start of the previous element of the repetitions of `a`.
  let mut memo = [((0, 0), 0u64); MAX_DEPTH + 1];
  let mut memo_len = 0;
  let mut budget = 1usize << 12;
  'runs: while pos < end {
    if budget == 0 {
      return out.mark(leaf, pos);
    }
    budget -= 1;
    let run = a.run_at(pos);
    let mut repeats = [NO; MAX_DEPTH + 1];
    let (len, mut i) = (a.repeats(pos, &run, true, &mut repeats), 0);
    while i < len {
      let repeat = repeats[i];
      i += 1;
      if repeat.run {
        continue;
      }
      // A slice can end before any of its elements.
      if repeat.end == usize::MAX && !dfa.accepts(set) {
        return out.mark(leaf, pos);
      }
      let mut j = 0;
      while j < memo_len && !(memo[j].0.0 == repeat.id.0 && memo[j].0.1 == repeat.id.1) {
        j += 1;
      }
      if j < memo_len && memo[j].1 == set {
        // An element changes nothing: neither will the next ones.
        pos = min(repeat.end, end);
        continue 'runs;
      }
      memo[j] = (repeat.id, set);
      if j == memo_len {
        memo_len += 1;
      }
    }
    let stop = min(run.end, end);
    match run.kind.seen() {
      // The check of the pattern would read what may be uninitialized.
      RunKind::Pad | RunKind::Opaque(_) => {
        out.unreadable(leaf, 1, pos);
        return out.uninit(pos);
      }
      RunKind::Any => {
        // A source that ends with these bytes may end before the first of them, from the end of
        // its sized part.
        let root = &a.stack[0];
        if stop == usize::MAX && pos >= root.base + root.layout.size && !dfa.accepts(set) {
          return out.mark(leaf, pos);
        }
        // Any byte, `stop - pos` times, or until it changes nothing.
        let mut steps = 0;
        while steps < stop - pos {
          let (next, total) = dfa.step(set, 0, 0xff);
          // The source can end anywhere if nothing bounds the analysis.
          if !total || steps > 256 || (end == usize::MAX && !dfa.accepts(next)) {
            return out.mark(leaf, pos);
          }
          if next == set {
            break;
          }
          (set, steps) = (next, steps + 1);
        }
      }
      RunKind::Atom(spec) if spec.may_be_uninit() => {
        out.unreadable(leaf, 1, pos);
        return out.uninit(pos);
      }
      RunKind::Atom(spec) => {
        // Each box of the scalar goes through the automaton on its own. Only what the type
        // system enforces is known of it.
        let ranges: &[(u128, u128)] = match spec.known {
          Valid::In(ranges) => ranges,
          _ => &[(0, max_value(spec.size))],
        };
        let (mut result, mut r) = (0, 0);
        while r < ranges.len() {
          let (mut t, count) = (0, box_count(ranges[r].0, ranges[r].1, spec.size));
          while t < count {
            if let Some((lo, hi)) = digit_box(ranges[r].0, ranges[r].1, spec.size, t) {
              let (mut states, mut byte) = (set, pos - run.start);
              while byte < stop - run.start {
                let digit = if spec.endian.is_little() { spec.size - 1 - byte } else { byte };
                let (next, total) = dfa.step(states, lo[digit], hi[digit]);
                if !total {
                  return out.mark(leaf, pos);
                }
                (states, byte) = (next, byte + 1);
              }
              result |= states;
            }
            t += 1;
          }
          r += 1;
        }
        set = result;
      }
      RunKind::Pattern(source) => {
        // The source may be anywhere in its own pattern if it started before.
        let from = if run.start == pos { 1 } else { source.reachable() };
        if !pattern_product(source, from, dfa, set) {
          out.mark(leaf, pos);
        }
        return;
      }
    }
    pos = stop;
  }
  // The source ends here.
  if end != usize::MAX && !dfa.accepts(set) {
    out.mark(leaf, pos);
  }
}

/// Returns whether every string that the automaton `a` accepts from one of the states `from_a`
/// is accepted by the automaton `b` started in the set of states `from_b`.
const fn pattern_product(a: &Dfa, from_a: u64, b: &Dfa, from_b: u64) -> bool {
  // For each state of `a`, the states of `b` it can be paired with.
  let (mut seen, mut done) = ([0u64; 64], [0u64; 64]);
  let mut state = 0;
  while state < a.states.len() {
    if from_a & (1 << state) != 0 {
      seen[state] = from_b;
    }
    state += 1;
  }
  loop {
    let (mut changed, mut state) = (false, 0);
    while state < a.states.len() {
      let todo = seen[state] & !done[state];
      if todo != 0 {
        (done[state], changed) = (done[state] | todo, true);
        if a.states[state].accept && !b.accepts(todo) {
          return false;
        }
        let (edges, mut i) = (a.states[state].edges, 0);
        while i < edges.len() {
          let (next, total) = b.step(todo, edges[i].0, edges[i].1);
          if !total {
            return false;
          }
          seen[edges[i].2 as usize] |= next;
          i += 1;
        }
      }
      state += 1;
    }
    if !changed {
      return true;
    }
  }
}

// -------------------------------------------------------------------------------------------------
// Rules

/// Marks the rules of the structs of `target` that `source` does not prove.
///
/// `target` is at `base` from the start of `source`, and its first leaf is `leaf`. A rule is not
/// a property of bytes. It is proven only by the same rule, at the same place of the source, in
/// a struct of the same extent ([`proves_rules`]). The elements of an array share their leaves,
/// so their rules are proven all at once or not at all. They are proven where the source has as
/// many elements of the same size at the same place ([`same_elements`]).
const fn mark_rules(
  source: &'static Layout,
  target: &'static Layout,
  base: usize,
  leaf: usize,
  out: &mut Relation,
) {
  if !target.has_rules || covered(source, base, target) {
    return;
  }
  match target.kind {
    Kind::Struct(fields, rules) => {
      if !rules.is_empty() && !proves_rules(source, base, target) {
        out.mark(leaf + target.leaves - 1, base);
      }
      let mut i = 0;
      while i < fields.len() {
        mark_rules(source, fields[i].layout, base + fields[i].offset, leaf + fields[i].leaf, out);
        i += 1;
      }
    }
    // Elements share their leaves, so either all of them are proven at once, or none is. They are
    // proven when the source has as many elements of the same size at the same place, since each
    // element is then to the source what the first one is.
    Kind::Array(elem, _) | Kind::Slice(elem) => {
      if same_elements(source, base, target) {
        mark_rules(source, elem, base, leaf, out);
      } else {
        mark_all_rules(elem, base, leaf, out);
      }
    }
    Kind::Scalar(_) | Kind::Pattern(_) | Kind::Enum(_) => {}
  }
}

/// Marks the rules of every struct of `target`, which is at `base` and whose first leaf is
/// `leaf`.
const fn mark_all_rules(target: &'static Layout, base: usize, leaf: usize, out: &mut Relation) {
  if !target.has_rules {
    return;
  }
  match target.kind {
    Kind::Struct(fields, rules) => {
      if !rules.is_empty() {
        out.mark(leaf + target.leaves - 1, base);
      }
      let mut i = 0;
      while i < fields.len() {
        mark_all_rules(fields[i].layout, base + fields[i].offset, leaf + fields[i].leaf, out);
        i += 1;
      }
    }
    Kind::Array(elem, _) | Kind::Slice(elem) => mark_all_rules(elem, base, leaf, out),
    Kind::Scalar(_) | Kind::Pattern(_) | Kind::Enum(_) => {}
  }
}

/// Returns whether `source` has, at `offset`, a struct whose rules are those of the struct
/// `target`.
///
/// The rules must be the same function of the same bytes ([`Layout::same_rules`]). They then
/// hold of the target if they hold of the source. The fields are not compared, since those of the
/// target are related to the source as any other. Without the `unstable` feature, both structs
/// must have a layout that Rust specifies, or be the same type. Nothing is looked for inside a
/// part of the source whose layout Rust leaves to the compiler.
const fn proves_rules(source: &'static Layout, offset: usize, target: &'static Layout) -> bool {
  // The same rules of the same bytes: of the same fields at the same offsets, which Rust specifies
  // for both, or which are those of the same type.
  let kept = !STRICT || (source.stable.fixed && target.stable.fixed) || same_type(source, target);
  if offset == 0 && kept && source.same_rules(target) {
    return true;
  }
  // Rust leaves to the compiler where the fields of the source are, so nothing is looked for in it.
  if STRICT && !source.stable.fixed {
    return false;
  }
  match source.kind {
    // A struct at the same offset may be its first field.
    Kind::Struct(fields, _) => {
      let mut i = 0;
      while i < fields.len() {
        let field = &fields[i];
        let inside = field.layout.is_dynamic() || offset < field.offset + field.layout.size;
        if offset >= field.offset
          && inside
          && proves_rules(field.layout, offset - field.offset, target)
        {
          return true;
        }
        i += 1;
      }
      false
    }
    Kind::Array(elem, len) => {
      elem.size > 0 && offset / elem.size < len && proves_rules(elem, offset % elem.size, target)
    }
    Kind::Slice(elem) => elem.size > 0 && proves_rules(elem, offset % elem.size, target),
    Kind::Scalar(_) | Kind::Pattern(_) | Kind::Enum(_) => false,
  }
}

/// Returns whether `source` has, at `offset`, at least as many elements as the array or slice
/// `target`, of the same element size, starting where an element of an array or a slice starts.
///
/// Each element of the target is then at the same place in an element of the source as the first
/// one is. Without the `unstable` feature, nothing is looked for inside a part of the source whose
/// layout Rust leaves to the compiler.
const fn same_elements(source: &'static Layout, offset: usize, target: &'static Layout) -> bool {
  // Rust leaves to the compiler where the parts of the source are, so nothing is looked for in it.
  if STRICT && !source.stable.fixed {
    return false;
  }
  let same = match (source.kind, target.kind) {
    (Kind::Array(a, n), Kind::Array(b, m)) => {
      a.size == b.size && a.size > 0 && offset.is_multiple_of(a.size) && offset / a.size + m <= n
    }
    (Kind::Slice(a), Kind::Slice(b) | Kind::Array(b, _)) => {
      a.size == b.size && (offset == 0 || a.size > 0 && offset.is_multiple_of(a.size))
    }
    _ => false,
  };
  if same {
    return true;
  }
  match source.kind {
    Kind::Struct(fields, _) => {
      let mut i = 0;
      while i < fields.len() {
        let field = &fields[i];
        let inside = field.layout.is_dynamic() || offset < field.offset + field.layout.size;
        if offset >= field.offset && inside {
          return same_elements(field.layout, offset - field.offset, target);
        }
        i += 1;
      }
      false
    }
    Kind::Array(elem, len) => {
      elem.size > 0 && offset / elem.size < len && same_elements(elem, offset % elem.size, target)
    }
    Kind::Slice(elem) => elem.size > 0 && same_elements(elem, offset % elem.size, target),
    Kind::Scalar(_) | Kind::Pattern(_) | Kind::Enum(_) => false,
  }
}

/// Returns whether `source` has, at `offset`, the very thing that `target` is.
///
/// This is the same layout ([`equal`]), or at least as many elements of the same layout.
const fn covered(source: &'static Layout, offset: usize, target: &'static Layout) -> bool {
  if offset == 0 {
    if equal(source, target) {
      return true;
    }
    // As many of the same elements, or more.
    let same = match (source.kind, target.kind) {
      (Kind::Array(a, n), Kind::Array(b, m)) => n >= m && equal(a, b),
      (Kind::Slice(a), Kind::Slice(b) | Kind::Array(b, _)) => equal(a, b),
      _ => false,
    };
    if same {
      return true;
    }
  }
  // Rust leaves to the compiler where the fields of the source are, so nothing is looked for in it.
  if STRICT && !source.stable.fixed {
    return false;
  }
  match source.kind {
    Kind::Struct(fields, _) => {
      let mut i = 0;
      while i < fields.len() {
        let field = &fields[i];
        let inside = field.layout.is_dynamic() || offset < field.offset + field.layout.size;
        if offset >= field.offset && inside {
          return covered(field.layout, offset - field.offset, target);
        }
        i += 1;
      }
      false
    }
    Kind::Array(elem, len) => {
      elem.size > 0 && offset / elem.size < len && covered(elem, offset % elem.size, target)
    }
    Kind::Slice(elem) => elem.size > 0 && covered(elem, offset % elem.size, target),
    Kind::Scalar(_) | Kind::Pattern(_) | Kind::Enum(_) => false,
  }
}

// -------------------------------------------------------------------------------------------------
// What a check reads

/// Returns whether a check of every part of a value of `layout` reads only bytes that are
/// initialized in every value of `layout`.
///
/// The same type proves a part without reading it. This tells whether the part could be checked
/// as well, where something else marks its leaf. It could not if the check would read:
///
/// - The bytes of a union, of which nothing is known. The check of a union tries its fields in
///   order, and reads nothing where its first field has nothing to check. Any other union counts
///   as read.
/// - The padding of a struct, which a rule that reads its bytes reads.
/// - A scalar that may be uninitialized ([`Spec::maybe_uninit`]).
const fn checkable(layout: &'static Layout) -> bool {
  if layout.leaves == 0 {
    return true;
  }
  match layout.kind {
    Kind::Scalar(spec) => !(spec.may_be_uninit() && matches!(spec.valid, Valid::In(_))),
    Kind::Pattern(_) => true,
    Kind::Array(elem, _) | Kind::Slice(elem) => checkable(elem),
    Kind::Struct(fields, _) => {
      let end = extent_end(layout, 0);
      if layout.rules_read_bytes() && uninit_in(layout, 0, 0, end, &Known::NONE).is_some() {
        return false;
      }
      let mut i = 0;
      while i < fields.len() {
        if !checkable(fields[i].layout) {
          return false;
        }
        i += 1;
      }
      true
    }
    // The first field is tried first. If it has nothing to check, the bytes are it, and no other
    // field is tried.
    Kind::Enum(enumeration) if enumeration.union => {
      enumeration.variants.is_empty() || enumeration.variants[0].layout.leaves == 0
    }
    Kind::Enum(enumeration) => {
      let mut i = 0;
      while i < enumeration.variants.len() {
        let variant = &enumeration.variants[i];
        i += 1;
        // No value of the type is a variant that has no value, so its fields are not checked.
        if variant.is_present() && !checkable(variant.layout) {
          return false;
        }
      }
      true
    }
  }
}

/// Returns the offset of the first byte of `start..end` that may be uninitialized in a value of
/// `source`, which is at `base`.
///
/// The bytes that `known` covers are initialized. A part of the source is what the analysis reads
/// of it ([`Cursor::run_at`], [`RunKind::seen`]).
const fn uninit_in(
  source: &'static Layout,
  base: usize,
  start: usize,
  end: usize,
  known: &Known,
) -> Option<usize> {
  let mut cursor = Cursor::new(source, base, false, Checked::No);
  uninit_from(&mut cursor, max(start, base), end, known)
}

/// Returns [`uninit_in`] of the layout of `a`, from `pos`.
///
/// An element of an array or a slice whose bytes are all initialized tells of the elements after
/// it, so they are not read one by one. What is known is of its own bytes, not of those of the
/// next element, so it is left out of what an element tells.
const fn uninit_from(a: &mut Cursor, mut pos: usize, end: usize, known: &Known) -> Option<usize> {
  let end = min(end, a.stack[0].end());
  while pos < end {
    let run = a.run_at(pos);
    let mut repeats = [NO; MAX_DEPTH + 1];
    let (len, mut i, mut skipped) = (a.repeats(pos, &run, true, &mut repeats), 0, false);
    while i < len && !skipped {
      let repeat = repeats[i];
      i += 1;
      let until = min(repeat.end, end);
      // Not a run of the same bytes, which is one step below, and at least two elements.
      if repeat.run || (until - pos) / repeat.stride < 2 {
        continue;
      }
      let mut element = *a;
      if uninit_from(&mut element, pos, pos + repeat.stride, &Known::NONE).is_some() {
        continue;
      }
      if until == usize::MAX {
        return None;
      }
      pos += (until - pos) / repeat.stride * repeat.stride;
      skipped = true;
    }
    if skipped {
      continue;
    }
    if run.kind.seen().may_be_uninit() && !known.covers(pos) {
      return Some(pos);
    }
    pos = known.step(pos, min(run.end, end));
  }
  None
}

/// Records the rules of the structs of `target` that must not be checked, because they read the
/// bytes of their struct, and one of these may be uninitialized in `source`
/// ([`Relation::unreadable`]).
///
/// `source` is at `base_a`. `target` is at `base`, and its first leaf is `leaf`. Only the bytes
/// before `end` are read. The bytes that `known` covers are initialized.
///
/// A rule that reads the bytes of its struct reads all of them: its padding, and what the source
/// has under a part that is proven without being read, such as an enum. So every byte of the
/// struct must be initialized in the source for the rule to be checked. The elements of an array
/// share their leaves. Where the source has elements of the same size at the same place
/// ([`same_elements`]), the first element tells of each. Otherwise each element is looked at, up to
/// [`CUT_ELEMENTS`] of them, and past that every byte under the array counts.
#[expect(clippy::too_many_arguments, reason = "two layouts where they are, the bytes, the tag")]
const fn unreadable_rules(
  source: &'static Layout,
  base_a: usize,
  target: &'static Layout,
  base: usize,
  leaf: usize,
  end: usize,
  known: &Known,
  out: &mut Relation,
) {
  if !target.has_rules || base >= end {
    return;
  }
  let stop = min(extent_end(target, base), end);
  match target.kind {
    Kind::Struct(fields, _) => {
      if target.rules_read_bytes()
        && let Some(pos) = uninit_in(source, base_a, base, stop, known)
      {
        out.unreadable(leaf + target.leaves - 1, 1, pos);
      }
      let mut i = 0;
      while i < fields.len() {
        let (field, at) = (fields[i].layout, base + fields[i].offset);
        unreadable_rules(source, base_a, field, at, leaf + fields[i].leaf, end, known, out);
        i += 1;
      }
    }
    Kind::Array(elem, _) | Kind::Slice(elem) => {
      if elem.size == 0 {
        return;
      }
      // Each element is to the source what the first one is. What is known is of the bytes of one
      // element only, so it is left out. Nothing is lost: where the source has the same elements,
      // it has an element at these bytes, not the padding of a tag.
      if same_elements(source, base - base_a, target) {
        return unreadable_rules(source, base_a, elem, base, leaf, end, &Known::NONE, out);
      }
      let count = if stop == usize::MAX { usize::MAX } else { (stop - base) / elem.size };
      if count > CUT_ELEMENTS {
        if let Some(pos) = uninit_in(source, base_a, base, stop, known) {
          unreadable_all_rules(elem, leaf, pos, out);
        }
        return;
      }
      let mut k = 0;
      while k < count {
        unreadable_rules(source, base_a, elem, base + k * elem.size, leaf, end, known, out);
        k += 1;
      }
    }
    Kind::Scalar(_) | Kind::Pattern(_) | Kind::Enum(_) => {}
  }
}

/// Records, as rules that must not be checked for the byte at `pos`, the rules of every struct of
/// `target` that read the bytes of their struct. The first leaf of `target` is `leaf`.
const fn unreadable_all_rules(
  target: &'static Layout,
  leaf: usize,
  pos: usize,
  out: &mut Relation,
) {
  if !target.has_rules {
    return;
  }
  match target.kind {
    Kind::Struct(fields, _) => {
      if target.rules_read_bytes() {
        out.unreadable(leaf + target.leaves - 1, 1, pos);
      }
      let mut i = 0;
      while i < fields.len() {
        unreadable_all_rules(fields[i].layout, leaf + fields[i].leaf, pos, out);
        i += 1;
      }
    }
    Kind::Array(elem, _) | Kind::Slice(elem) => unreadable_all_rules(elem, leaf, pos, out),
    Kind::Scalar(_) | Kind::Pattern(_) | Kind::Enum(_) => {}
  }
}

// -------------------------------------------------------------------------------------------------
// `Fit`

/// Whether a source and a target have the same extent.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Exact {
  /// For every source.
  Always,
  /// For some lengths of the slice tail of the source.
  Sometimes,
  /// For no source, for the reason given.
  Never(&'static str),
}

/// How the extent of a source maps to the extent of a target.
///
/// The *extent* of a value is its size without the trailing padding of an unsized type (the offset
/// of its slice tail plus the size of that slice).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Fit {
  /// Whether the extents are the same.
  pub(crate) exact: Exact,
  /// Whether every source is at least as large as the smallest target.
  pub(crate) shrink: bool,
  /// The number of leading bytes to relate, or `usize::MAX` if there is no bound.
  pub(crate) end: usize,
}

/// The reason of [`Exact::Never`] when the source is smaller than the target.
pub const TOO_SMALL: &str = "the source is smaller than the target";
/// The reason of [`Exact::Never`] when the source is larger than the target.
pub const TOO_LARGE: &str = "the source is larger than the target";
/// The reason of [`Exact::Never`] when no number of elements in the tail of the target makes it the
/// size of the source.
pub const UNEVEN: &str =
  "the size of the source cannot be reached with elements of the tail slice of the target";

/// Returns how the extent of a source of layout `a` maps to the extent of a target of layout `b`.
pub const fn fit(a: &Layout, b: &Layout) -> Fit {
  let ((sa, ya), (sb, yb)) = ((a.size, a.stride()), (b.size, b.stride()));
  match (ya != 0, yb != 0) {
    // Both have a fixed size.
    (false, false) => Fit {
      exact: match () {
        () if sa < sb => Exact::Never(TOO_SMALL),
        () if sa > sb => Exact::Never(TOO_LARGE),
        () => Exact::Always,
      },
      shrink: sa >= sb,
      end: sb,
    },
    // The target takes as many elements as the source has room for.
    (false, true) => {
      let elems = if sa >= sb { (sa - sb) / yb } else { 0 };
      Fit {
        exact: match () {
          () if sa < sb => Exact::Never(TOO_SMALL),
          () if (sa - sb) % yb != 0 => Exact::Never(UNEVEN),
          () => Exact::Always,
        },
        shrink: sa >= sb,
        end: sb + elems * yb,
      }
    }
    // The source must have the very number of elements that makes it the size of the target.
    (true, false) => Fit {
      exact: match () {
        () if sa > sb => Exact::Never(TOO_LARGE),
        () if (sb - sa) % ya != 0 => Exact::Never(UNEVEN),
        () => Exact::Sometimes,
      },
      shrink: sa >= sb,
      end: sb,
    },
    (true, true) => Fit {
      exact: match () {
        // No `sa + n * ya == sb + m * yb`.
        () if sa.abs_diff(sb) % gcd(ya, yb) != 0 => Exact::Never(UNEVEN),
        // One for every `n`.
        () if sa >= sb && (sa - sb) % yb == 0 && ya % yb == 0 => Exact::Always,
        () => Exact::Sometimes,
      },
      shrink: sa >= sb,
      end: usize::MAX,
    },
  }
}

// -------------------------------------------------------------------------------------------------
// `Analysis`

/// The result of the analysis of a pun.
#[derive(Copy, Clone)]
pub struct Analysis {
  /// How the extents of the source and the target map.
  pub(crate) fit: Fit,
  /// How the values of the source relate to the target.
  ///
  /// Its offsets are from the start of the source. The target is at `shift`.
  pub(crate) relation: Relation,
  /// Whether the target is aligned wherever the source is.
  pub(crate) aligned: bool,
  /// The offset of the target in the source.
  ///
  /// It is 0, or, for the last bytes of the source (`tail`), the size of the source minus the
  /// size of the target.
  pub(crate) shift: usize,
}

/// Analyzes a pun from the first bytes of a source of layout `a` to a target of layout `b`.
///
/// If no source fits, even truncated, nothing is related. Without the `unstable` feature, a
/// marked check that would read a byte that the compiler may move is recorded as `unspecified`
/// ([`reads_moved`]).
pub const fn analyze(a: &'static Layout, b: &'static Layout) -> Analysis {
  let fit = fit(a, b);
  // No source fits, even truncated: there is nothing to relate.
  let hopeless = matches!(fit.exact, Exact::Never(_)) && !fit.shrink;
  let mut relation = if hopeless { Relation::NEW } else { relate(a, b, fit.end, Checked::Marked) };
  if !hopeless {
    mark_rules(a, b, 0, 0, &mut relation);
    unreadable_rules(a, 0, b, 0, 0, fit.end, &Known::NONE, &mut relation);
    relation.settle();
    if let Some(pos) = reads_moved(a, 0, b, 0, 0, &relation.marks, 0, fit.end, None) {
      relation.unspecified(pos);
    }
  }
  Analysis { fit, relation, aligned: b.align <= a.align, shift: 0 }
}

/// The reason of [`Exact::Never`] for the last bytes of an unsized source, where they start
/// depends on its length.
pub const TAIL_UNSIZED_SOURCE: &str = "the source is unsized: where its last bytes start depends \
                                       on its length, which is only known at run time.\nUse \
                                       `from_tail` to take a value from the end of bytes";
/// The reason of [`Exact::Never`] for the last bytes of a source as an unsized target, whose
/// extent cannot be told from the end.
pub const TAIL_UNSIZED_TARGET: &str =
  "the target is unsized: its extent cannot be found from the end of the source";

/// Analyzes a pun from the last bytes of a source of layout `a` to a target of layout `b`.
///
/// The target is at `shift = size(a) - size(b)` in the source. Both must be sized, and the source
/// must be at least as large as the target. Otherwise the fit says why, and nothing is related.
///
/// It checks what [`analyze`] checks of the first bytes: padding under a byte that the target
/// requires to be initialized, scalars, patterns, enums and rules. An enum only proves an enum at
/// the same offset, and a rule only the same rule at the same offset ([`mark_rules`] from
/// `shift`). The target is aligned if it needs no more alignment than the source, and `shift` is
/// a multiple of its alignment.
pub const fn analyze_tail(a: &'static Layout, b: &'static Layout) -> Analysis {
  if a.is_unsized() || b.is_unsized() || a.size < b.size {
    let exact = Exact::Never(match () {
      () if a.is_unsized() => TAIL_UNSIZED_SOURCE,
      () if b.is_unsized() => TAIL_UNSIZED_TARGET,
      () => TOO_SMALL,
    });
    let fit = Fit { exact, shrink: false, end: 0 };
    return Analysis { fit, relation: Relation::NEW, aligned: false, shift: 0 };
  }
  let shift = a.size - b.size;
  let exact = if shift == 0 { Exact::Always } else { Exact::Never(TOO_LARGE) };
  let fit = Fit { exact, shrink: true, end: b.size };
  let mut relation = relate_at(a, 0, b, shift, shift, a.size, Checked::Marked);
  mark_rules(a, b, shift, 0, &mut relation);
  unreadable_rules(a, 0, b, shift, 0, a.size, &Known::NONE, &mut relation);
  relation.settle();
  if let Some(pos) = reads_moved(a, 0, b, shift, 0, &relation.marks, shift, a.size, None) {
    relation.unspecified(pos);
  }
  // The source is at a multiple of its alignment: the target is at a multiple of its own if both
  // the alignment of the source and the shift are.
  let aligned = b.align <= a.align && shift.is_multiple_of(b.align);
  Analysis { fit, relation, aligned, shift }
}

/// Returns whether [`mutable`] is false because of a layout that Rust leaves to the compiler.
///
/// This happens only without the `unstable` feature. What may be written through the `&mut` to
/// `b` is then not known to be a valid `a` in every layout that Rust allows.
pub const fn mutable_unspecified(a: &'static Layout, b: &'static Layout, fit: &Fit) -> bool {
  STRICT
    && !matches!(fit.exact, Exact::Never(_))
    && relate(b, a, fit.end, Checked::No).unspecified.is_some()
}

/// Returns whether every value written through a `&mut` to the target `b` leaves a valid source
/// `a`.
///
/// Every value of `b`, padding included, must be a valid `a`, which means that the reverse relation
/// must be infallible. It is never so if the extents never match.
pub const fn mutable(a: &'static Layout, b: &'static Layout, fit: &Fit) -> bool {
  !matches!(fit.exact, Exact::Never(_)) && relate(b, a, fit.end, Checked::No).infallible()
}

/// Why a `&mut` is refused when some values of the target are not valid for the source.
pub const MUTABLE: &str = "some values of the target are not valid for the source (or are padding \
                           where the source is not), and could be written through the `&mut`";
/// Why a `&mut` to a part is refused when it covers a part of the source whose layout Rust leaves
/// to the compiler, without being of the same type.
pub const PART_UNSPECIFIED: &str = "the `&mut` covers a part of the source whose layout Rust \
                                    leaves to the compiler (a struct, a tuple or an enum of the \
                                    `Rust` representation), which only a `&mut` \
                                    to the same type may write.\nAdd `#[repr(C)]` (or \
                                    `#[repr(u8)]` to an enum), or enable the `unstable` feature \
                                    of `puns`";
/// Why a `&mut` to a part is refused when a struct of the source that has a rule overlaps it.
pub const PART_RULE: &str = "a struct of the source that has a rule (a size, a length, an \
                             assertion, a region) overlaps the bytes that the `&mut` covers, and \
                             a write could break the rule";
/// Why a `&mut` to a part is refused when it covers only a part of an enum of the source.
pub const PART_ENUM: &str = "an enum of the source is only partly covered by the `&mut`: what its \
                             tag says of the bytes on one side depends on the other";
/// Why a `&mut` to a part is refused when it covers only a part of a string of the source.
pub const PART_PATTERN: &str = "a string of the source is only partly covered by the `&mut`: what \
                                is written must be valid with what follows it";

/// Returns why a value written through a `&mut` to the target `b` may leave an invalid source
/// `a`, or `None` if no value does.
///
/// The `&mut` covers the bytes of `a` from `shift`, as many as `fit` says. Any value of `b` may be
/// written, so every value of `b` must be valid for what `a` has over those bytes. This is the
/// reverse relation, which [`mutable`] requires of the whole. And nothing of `a` outside those
/// bytes may depend on them ([`cut`]): a rule of a struct that holds over them, an enum of which
/// they are only a part (its tag on one side, fields on the other), or a string that goes on after
/// them. This is conservative, since such a source is refused even if no value of `b` would break
/// it.
///
/// # Errors
///
/// Returns [`MUTABLE`] if a source may be smaller than `b`, or if a value of `b` is not valid for
/// `a`. Returns the reason of [`cut`] if something of `a` depends on the bytes. Returns
/// [`PART_ENUM`] if a target that takes as many elements as fit may end within an enum or a string
/// of `a`.
pub const fn mutable_part(
  a: &'static Layout,
  b: &'static Layout,
  fit: &Fit,
  shift: usize,
) -> Option<&'static str> {
  if !fit.shrink {
    // Refused by the analysis of the reference already.
    return Some(MUTABLE);
  }
  let end = shift.saturating_add(fit.end);
  if let Some(why) = cut(a, 0, b, shift, end) {
    return Some(why);
  }
  // A target that takes as many elements as fit may end anywhere in the source: within an enum
  // or a string, which `cut` cannot see without an end.
  if end == usize::MAX && !matches!(fit.exact, Exact::Always) && is_tagged(a) {
    return Some(PART_ENUM);
  }
  if relate_at(b, shift, a, 0, shift, end, Checked::No).infallible() { None } else { Some(MUTABLE) }
}

/// The most elements with rules that are looked at one by one: by [`cut`], in an array that the
/// bytes of a `&mut` cover in part, and by [`unreadable_rules`], in an array whose elements the
/// source does not match.
///
/// Past it, `cut` refuses, and `unreadable_rules` counts every byte under the array. A period, as
/// in the analysis, would lift this limit.
const CUT_ELEMENTS: usize = 64;

/// Returns why something of `layout`, at `base`, depends on the bytes `lo..hi` without lying
/// within them, or `None` if nothing does.
///
/// `hi` may be `usize::MAX`. The target `b`, which is written over the bytes, starts at `lo`. See
/// [`mutable_part`].
///
/// A struct with rules that lies within the bytes is not cut if the target has, at the same place,
/// a struct with the same rules over the same extent ([`proves_rules`], the other way). What is
/// written then meets the rules of the source if it meets those of the target. The same holds for
/// elements with rules, if the target has elements of the same size at the same place. Without the
/// `unstable` feature, a part whose layout Rust leaves to the compiler is cut unless the target has
/// the same layout over the whole of it ([`covered`]).
const fn cut(
  layout: &'static Layout,
  base: usize,
  b: &'static Layout,
  lo: usize,
  hi: usize,
) -> Option<&'static str> {
  let end = if layout.is_dynamic() { usize::MAX } else { base + layout.size };
  if end <= lo || base >= hi || base == end {
    return None;
  }
  // Within the bytes, and sized: the target is there too.
  let within = !layout.is_dynamic() && base >= lo && end <= hi;
  // A part whose layout Rust leaves to the compiler is written whole, by the same type: nothing
  // else tells what of it the bytes cover.
  if layout.is_opaque() {
    return if within && covered(b, base - lo, layout) { None } else { Some(PART_UNSPECIFIED) };
  }
  match layout.kind {
    Kind::Scalar(_) => None,
    Kind::Struct(fields, rules) => {
      let proven = within && proves_rules(b, base - lo, layout);
      if !rules.is_empty() && !proven {
        return Some(PART_RULE);
      }
      let mut i = 0;
      while i < fields.len() {
        if let Some(why) = cut(fields[i].layout, base + fields[i].offset, b, lo, hi) {
          return Some(why);
        }
        i += 1;
      }
      None
    }
    Kind::Enum(enumeration) => {
      if base < lo || end > hi {
        return Some(PART_ENUM);
      }
      // Within the bytes: a variant is written whole, but some may have rules.
      let mut i = 0;
      while i < enumeration.variants.len() {
        if has_rules(enumeration.variants[i].layout) {
          return Some(PART_RULE);
        }
        i += 1;
      }
      None
    }
    Kind::Pattern(_) => {
      if base < lo || hi != usize::MAX {
        Some(PART_PATTERN)
      } else {
        None
      }
    }
    Kind::Array(elem, _) | Kind::Slice(elem) => {
      if elem.size == 0 {
        return None;
      }
      if has_rules(elem) {
        // Every element is within the bytes, and the target has elements of the same size there, so
        // each element is to the target what the first one is.
        if within && same_elements(b, base - lo, layout) {
          return cut(elem, base, b, lo, hi);
        }
        // Else each element that the bytes cover, if there are not many: one that they cover in
        // part is cut, as one whose rules the target does not have over it.
        let count = match layout.kind {
          Kind::Array(_, count) => count,
          _ => usize::MAX,
        };
        let first = if lo > base { (lo - base) / elem.size } else { 0 };
        if hi == usize::MAX || count == usize::MAX {
          return Some(PART_RULE);
        }
        let last = (hi - 1 - base) / elem.size;
        let last = if last < count { last } else { count - 1 };
        if last - first >= CUT_ELEMENTS {
          return Some(PART_RULE);
        }
        let mut k = first;
        while k <= last {
          if let Some(why) = cut(elem, base + k * elem.size, b, lo, hi) {
            return Some(why);
          }
          k += 1;
        }
        return None;
      }
      if !is_tagged(elem) {
        return None;
      }
      // Only the elements at either end of the bytes may be cut.
      let count = match layout.kind {
        Kind::Array(_, count) => count,
        _ => usize::MAX,
      };
      let first = if lo > base { (lo - base) / elem.size } else { 0 };
      if first < count
        && let Some(why) = cut(elem, base + first * elem.size, b, lo, hi)
      {
        return Some(why);
      }
      if hi != usize::MAX {
        let last = (hi - 1 - base) / elem.size;
        if last < count
          && last != first
          && let Some(why) = cut(elem, base + last * elem.size, b, lo, hi)
        {
          return Some(why);
        }
      }
      None
    }
  }
}

/// Returns whether a struct in `layout` has rules, in the variants of its enums too.
const fn has_rules(layout: &'static Layout) -> bool {
  match layout.kind {
    _ if layout.has_rules => true,
    Kind::Struct(fields, _) => {
      let mut i = 0;
      while i < fields.len() {
        if has_rules(fields[i].layout) {
          return true;
        }
        i += 1;
      }
      false
    }
    Kind::Array(elem, _) | Kind::Slice(elem) => has_rules(elem),
    Kind::Enum(enumeration) => {
      let mut i = 0;
      while i < enumeration.variants.len() {
        if has_rules(enumeration.variants[i].layout) {
          return true;
        }
        i += 1;
      }
      false
    }
    Kind::Scalar(_) | Kind::Pattern(_) => false,
  }
}

/// Returns whether `layout` holds an enum with fields, a union or a pattern (bytes whose validity
/// depends on other bytes).
const fn is_tagged(layout: &'static Layout) -> bool {
  match layout.kind {
    Kind::Enum(_) | Kind::Pattern(_) => true,
    Kind::Struct(fields, _) => {
      let mut i = 0;
      while i < fields.len() {
        if is_tagged(fields[i].layout) {
          return true;
        }
        i += 1;
      }
      false
    }
    Kind::Array(elem, _) | Kind::Slice(elem) => is_tagged(elem),
    Kind::Scalar(_) => false,
  }
}
