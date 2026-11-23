//! # Limitations
//!
//! ## Limits
//!
//! - The derive bounds its implementation by "the type of each field that depends on a parameter is
//!   `Pun`", except for the fields that name the type itself. Such a bound would make the type
//!   `Pun` if its field is, and the field `Pun` if the type is, and the compiler does not resolve
//!   that cycle. So a parameter of a recursive type is declared with whatever its fields need. Two
//!   generic types that hold each other would have their bounds found in a cycle (`overflow
//!   evaluating the requirement`). When the bounds that the types declare are enough, write
//!   `#[pun(bound())]` on both.
//! - The types of the fields of a struct must be at least as visible as the struct, because the
//!   list of fields is an associated type, and the privacy rules of Rust do not let it name a less
//!   visible type.
//! - A struct with more than about a hundred fields may need a higher `#![recursion_limit]` in the
//!   crate that validates it, and so may an enum with fields and more than about 120 variants,
//!   because validation recurses over the list of variants.
//! - The analysis follows arrays, slices and structs nested at most 32 deep, and refuses a pun of a
//!   type nested deeper ("type is nested too deeply").
//! - Some proofs search a bounded number of cases. Past the bound, what is not proven is checked at
//!   run time, or the pun is refused where it cannot be checked, and nothing is proven wrongly. A
//!   struct of the `Rust` representation is proven by another source only if its fields have at
//!   most 1,024 offsets in all, counting each offset that a field may have in some order. Otherwise
//!   only the same struct proves it. The values of a scalar made of parts of the source are
//!   compared in at most 16,384 steps, and a pattern over at most 4,096 runs of the source. A
//!   `&mut` to a part of an array whose elements have rules looks at no more than 64 elements, and
//!   is refused past them.
//! - A packed struct whose unsized field has trailing padding of its own is refused.
//! - No function returns a new value of an unsized type, such as a struct that ends with a slice,
//!   because Rust cannot return an unsized value by value, and `puns` never allocates. To send
//!   such a value, write its bytes.
//! - The build time of a view enum whose variants overlap grows with the square of the number of
//!   variants, or faster.
//! - `PhantomData<T>` is a [`Scalar`] for any `T`, and deriving `Pun` exposes private fields. [What
//!   deriving `Pun` means](super::soundness#what-deriving-pun-means) says which types must not
//!   derive `Pun`.
//! - The evaluator of rules has a stack of 16 values and 8 variables. An expression has at most 256
//!   operations and at most 4 nested walks, and the derive says when an expression needs more, and
//!   how much more. A walk takes at most `8 * size + 128` steps, where `size` is the size of the
//!   value, or, for a `size` or `len` rule, the number of bytes from the start of the value to the
//!   end of the input.
//! - Unions must be `repr(C)`, since the fields of other unions are not known to be at 0. A union
//!   must not hold a scalar that a pun never makes, and as a source nothing is known about it (see
//!   [Unions](super::soundness#unions)).
//! - An enum of the `Rust` representation is refused if the sample of one of its variants needs a
//!   value of a scalar that a pun never makes, since `puns` cannot make one. A sample holds the
//!   least value of each field, and of an enum its first variant that has values. The direct tag of
//!   such an enum, which is all the bytes before the first field of any variant, must be 1, 2, 4, 8
//!   or 16 bytes, since `rustc` makes no other size, or the enum is refused. If `rustc` places the
//!   fields of a variant without values before the fields of the other variants, the build fails,
//!   because `puns` cannot observe where those fields are and does not guess.
//! - In an enum with fields and a primitive or `C` representation, a variant that holds a scalar
//!   that a pun never makes, or a union none of whose fields has values, has no sample from which
//!   `puns` reads its tag. Its discriminant must hold no macro and no attribute, or it must have
//!   none after a variant whose tag is known, or the enum is refused ([What the derive
//!   trusts](macro@Pun#what-the-derive-trusts)).
//!
//! ## Not done yet
//!
//! - **`Vec`, `String` and maps, through their contents.** A pun could reinterpret what they hold,
//!   as it reinterprets a slice, and keep the structure that holds it. The structure itself is not
//!   data: its bytes are a pointer and lengths, so reading it as bytes would not read the contents,
//!   and bytes made into one would hold a pointer to no allocation.
//! - **Atomics, `Cell`, `UnsafeCell`, `Wrapping` and `Saturating`.** They have the layout of what
//!   they hold, but `puns` implements `Pun` for none of them.
//! - **Precise mutable parts.** [`head_mut`] and [`tail_mut`] refuse any source in which a struct
//!   that has a rule overlaps the part, unless the target has the same struct with the same rules
//!   there, even if the rule reads other fields. They also refuse any enum that the part covers
//!   only partly, even one whose variants all accept what is written. A [`pun_mut`] of a whole
//!   value does not require this of rules, so a `&mut Plain` made from a `&mut S`, where `S` has a
//!   rule that `Plain` does not, can write a value that does not meet that rule, just as the fields
//!   of `S` can, since a rule is a [requirement](super::soundness#guarantees-and-requirements).
//! - **Ranges from conditions.** A counter that only a comparison bounds, such as `i` in the walk
//!   condition `|i| i < count`, is computed on 128 bits, unless the expression also reads the value
//!   at that offset (`be(i, 1)`). The state of a walk inside two other walks is given no range.
//! - **Rules implied by other rules.** A rule is proven by the same rule, not by other rules that
//!   imply it. For example, `v % 6 == 0` implies `v % 3 == 0`, but does not prove it. Nor is a rule
//!   proven by the same rule in a struct of another extent, even when the rule does not read the
//!   extent, such as `a != b` in a struct of 2 bytes and in one of 3. The pun is then checked.
//! - **A rule that two variants of a view share, where neither variant accepts every value of the
//!   other.** For bytes that fail the rule, each variant computes it. A variant declared after them
//!   that accepts every value of both computes it once, and shares what succeeds as well.
//!   `Frame<Ipv4>` after two kinds of IPv4 packet is such a variant.
//! - **Checked parts of a typed value** (`try_head` and so on). A part is either proven or refused.
//!   For bytes, there are [`from_head`] and [`from_tail`].
//! - **Precise checks of an enum whose source is another type.** When only some of its variants are
//!   proven, or some values of the source are not a tag, the whole enum is checked at run time, its
//!   tag first, then the fields of its variant.
//! - **A checked pun from an enum to a type that is not an enum.** Take `try_pun` from the `Shape`
//!   of [Enums: what is refused](super::errors#enums-what-is-refused) to a struct that requires an
//!   initialized byte where one variant of `Shape` has padding. It is refused because of that
//!   padding byte, even if the check of the tag would exclude that variant, because as a source an
//!   enum is known by the bytes that every variant initializes, not variant by variant.
//! - **Checking some fields of a union.** When the check of one field would read a byte that may be
//!   padding, and no field is proven, the pun is refused.
//! - **Checking only what is not proven, among parts that are checked together.** The elements of
//!   an array share their check, and an enum is checked as a whole. A source that ends with a `str`
//!   where the target does not has every part of the target checked. The parts to check of a target
//!   past its first 1,024 are checked all together. A part that is proven there without being read
//!   (a union, an enum that holds one, a struct whose rule reads its padding) is then checked too.
//!   If that check would read a byte that may be uninitialized, the pun is refused, though the part
//!   needs no check.
//! - **Patterns other than `str` as types.** The engine accepts any automaton of at most 64 states
//!   as a type that guarantees it, but no such type can come from the derive, since safe code can
//!   write the fields of a type that derives `Pun`. A struct can require a pattern of its bytes
//!   with the rule `matches(DFA, start, end)`, which, like any rule, is proven only by the same
//!   rule.
//! - **A byte order that a value gives**, such as `EI_DATA` of ELF or the byte order mark of TIFF.
//!   A type has a byte order of its own (`u32be`, `i16le`, `f64be`, and so on), so no type reads
//!   its fields in the byte order that a value gives. A view enum chooses between a type of each
//!   byte order instead.
//! - **`default()`.** No function builds a value from the defaults of its fields (literals, sizes).
//! - **Fields of fields in rules.** A rule names the integer fields of its struct, not a field of a
//!   field (`header.length`) or an element of an array (`magic[0]`), which it reads with `be` or
//!   `le` at their offsets instead.
//! - **Signed and floating-point fields in rules.** Rules compute on unsigned integers. A signed or
//!   floating-point field is refused, and is read as bits with `le` or `be` instead. A getter reads
//!   fields, not bytes, and a value that only the bytes hold, such as a variable-length integer, is
//!   decoded by the user's code.
//! - **A size that is not in the value**, such as "to the end of whatever holds it" (an MP4 box of
//!   size 0).
//! - **Sums of types in sequences, and recursion**, such as `OPTIONAL` and `CHOICE` of ASN.1, and
//!   the containers of CBOR and MessagePack. A chain of types (`Then<A, B>`) is one sequence of
//!   types.
//! - **An arbitrary function in a rule.** A getter may call any `const fn`, but a rule may not. The
//!   validator is `const fn` code, generic over the type it validates, and on stable Rust 1.100
//!   such code cannot call a function that the type chooses. A call through a function pointer is
//!   refused ("function pointer calls are not allowed in constant functions"), and so is a call of
//!   a trait method (E0015, "cannot call non-const associated function"). Const traits are unstable
//!   (E0658). A user function could only be called if the validator stopped being `const`, or
//!   behind a check that runs only outside of `const`, which stable Rust cannot express. So what a
//!   rule computes is built in, and described by data where it varies: walks, variable-length
//!   integers, CRCs by their parameters, and sums. The analysis does not reason about that data,
//!   since a rule is proven only by the same rule, and a checksum only makes a check stricter.

// The items of the crate, which the links of this chapter name. Only rustdoc reads them.
#[cfg(doc)]
use crate::*;
