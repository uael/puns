//! Checks parts without values, unions that may hold anything, and niches in arrays, against the
//! oracles.
//!
//! The pairs put each of these shapes where the library has to see it: a part without values in a
//! struct, a variant, an array and a union; a union without values in a source; a niche in an
//! array; and a part that a check need not read next to a part that it checks, over bytes that may
//! be uninitialized. Natively, the oracles judge the verdicts. Under Miri, the checks run on values
//! whose bytes are uninitialized wherever the type allows, and Miri reports a check that reads one:
//! no oracle sees a read whose value does not change the result. The full run has every pair of
//! these types (`size*.rs`, `parts*.rs`).
//!
//! Each shape is a test of its own, so that one runs alone under Miri, which stops at the first
//! read of an uninitialized byte.

use core::num::NonZero;

use puns_model::types::*;

/// The number of values of each source converted at run time.
///
/// Under Miri, the first values: the enumeration varies the last bytes first, so they hold the
/// first tag with every byte after it uninitialized, initialized, or a requirement broken, which
/// is what reaches the reads of each shape. Natively, every value.
const LIMIT: usize = if cfg!(miri) { 64 } else { usize::MAX };

/// A niche in an array, in its first element, which only the `unstable` feature looks into.
#[test]
fn niche_in_array() {
  puns_model::pairs!(check_pair, LIMIT;
    ((bool, u8), FlagsArr), ([u8; 2], FlagsArr), ([bool; 2], FlagsArr),
    ([u8; 2], Option<[NonZero<u8>; 2]>),
  );
  puns_model::finish("a niche in an array");
}

/// A union that a check need not read, next to a part that it checks: the same union in an array,
/// and a union proven by one of its fields in an enum checked as a whole.
#[test]
fn union_next_to_a_check() {
  puns_model::pairs!(check_pair, LIMIT;
    (UnionBytes, [FirstU; 2]), (TagFlag, UnionIn), (FirstU, FirstU), ([u8; 2], FirstU),
  );
  // The parts natively only: under Miri, `parts.rs` reaches their unsafe paths.
  #[cfg(not(miri))]
  puns_model::pairs!(check_part, LIMIT; (UnionBytes, FirstU));
  puns_model::finish("a union next to a check");
}

/// A variant, and a field of a union, without values, over a byte that may be uninitialized.
#[test]
fn without_values_unread() {
  puns_model::pairs!(check_pair, LIMIT;
    (ByteZero, HalfNever), (u16, HalfNever), (ByteZero, VoidFirstU), ([u8; 2], VoidFirstU),
  );
  puns_model::finish("without values, unread");
}

/// A union without values in a source, in a variant and in a struct.
#[test]
fn union_without_values() {
  puns_model::pairs!(check_pair, LIMIT;
    (MaybeHollow, NotHollow), (MaybeHollow, [u8; 2]), (HollowByte, Void1), (HollowByte, u8),
  );
  // The parts natively only: under Miri, `parts.rs` reaches their unsafe paths.
  #[cfg(not(miri))]
  puns_model::pairs!(check_part, LIMIT; (MaybeHollow, u8), (HollowByte, u8));
  puns_model::finish("a union without values");
}

/// Parts without values of size 0 and above, in a struct and in an array.
#[test]
fn parts_without_values() {
  puns_model::pairs!(check_pair, LIMIT;
    ([u8; 1], Void1), (u8, [Void1; 1]), ([u8; 2], [Void1; 2]), ([u8; 2], VoidBools),
  );
  puns_model::finish("parts without values");
}

/// An array of small elements under an enum and a union of the target, and an enum whose tag
/// accepts every value of 2 bytes.
#[test]
fn array_under_an_enum() {
  puns_model::pairs!(check_pair, LIMIT;
    (UnitsArray, HoldsUnits), (UnitsFields, HoldsUnits), ([bool; 2], Option<NonZero<u16>>),
    ([ByteUnit; 2], OptOrUnits),
  );
  // The parts natively only: under Miri, `parts.rs` reaches their unsafe paths.
  #[cfg(not(miri))]
  puns_model::pairs!(check_part, LIMIT; (UnitsArray, [ByteUnit; 2]));
  puns_model::finish("an array under an enum");
}

/// A part of the target whose layout Rust leaves to the compiler, a tuple, in a field of a union
/// and in a variant with a scalar that a pun never makes, over bytes that may be uninitialized.
/// Without the `unstable` feature, the analysis does not look into it.
#[test]
fn layout_left_to_the_compiler() {
  puns_model::pairs!(check_pair, LIMIT;
    (TagUnits, HoldsTuple), (TagUnits, TupleNever), ([u8; 3], HoldsTuple), ([u8; 3], TupleNever),
  );
  puns_model::finish("a layout left to the compiler");
}

/// A union whose first field has a rule that loads a field, proven by its second field, in an
/// enum that is checked, over a byte that may be uninitialized.
#[test]
fn rule_in_a_union() {
  puns_model::pairs!(check_pair, LIMIT;
    (TagUnit, HoldsRule), ([u8; 2], HoldsRule), (RuleOrUnit, RuleOrUnit),
  );
  puns_model::finish("a rule in a union");
}

/// A union whose first field is proven by a requirement of the source (`valid(..)`), which safe
/// code can break, in an enum that is checked: a source that breaks it has its union checked.
#[test]
fn union_proven_by_a_requirement() {
  puns_model::pairs!(check_pair, LIMIT;
    (TagSoft, HoldsSoft), ([u8; 3], HoldsSoft), (SoftOrFlags, SoftOrFlags),
  );
  puns_model::finish("a union proven by a requirement");
}
