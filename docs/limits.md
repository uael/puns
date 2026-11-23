# Limits, and what is not done yet

This chapter lists what `puns` does not do, and why: limits of Rust, bounds that keep the
analysis within them, and what is planned. The conditions under which the derive refuses a type
are on [its page][macro@Pun], and [Compile-time errors][errors] gives each message of a refusal.

## Limits of Rust

- **Const evaluation is bounded.** The deny-by-default lint [`long_running_const_eval`] stops it
  after 2,000,000 basic blocks ([`LINT_TERMINATOR_LIMIT`] in `rustc`), with the error "constant
  evaluation is taking a long time". So the analysis cannot take a step for each byte of a type,
  nor a step or an instance for each element of an array, and its searches stop at the
  [bounds](#bounds-of-the-analysis) below.
- **No function that a type chooses.** The validator is `const fn` code, generic over the type
  it validates, and on stable Rust 1.100 such code cannot call a function that the type chooses.
  A call through a function pointer is refused ("function pointer calls are not allowed in
  constant functions"), a call of a trait method too (E0015, "cannot call non-const associated
  function"), and const traits are unstable (E0658). A user function could be called only if the
  validator stopped being `const`, or behind a check that runs only outside of `const`, which
  stable Rust cannot express. So a getter may call any `const fn`, but a rule may not call an
  arbitrary function, and what a rule computes is [built in][rules]. For the same reason, the
  implementations of standard traits, such as `From` and `Display`, are not `const`.
- **No unsized value by value.** Rust cannot return an unsized value by value, and `puns` never
  allocates, so no function returns a new value of an unsized type, such as a struct that ends
  with a slice. To send one, write its bytes.
- **The tag of an enum of the `Rust` representation is found on values.** Stable Rust tells
  neither where the tag is nor its values: `offset_of!` on variants, `mem::variant_count` and
  `discriminant_value` are unstable. So the derive reads them on a value of each variant, and
  refuses the enum where it cannot make one: where the value would hold an [opaque
  scalar][macro@Pun#enums-with-fields], or where the variant holds a union none of whose fields
  has values ("a variant holds a union none of whose fields has values: a value of it may exist,
  but it has no sample to find its tag: unsupported layout").

## Bounds of the analysis

Some proofs search a bounded number of cases. Past the bound, what is not proven is checked at
run time, or the pun is refused where it cannot be checked, and nothing is proven wrongly.

- The analysis follows arrays, slices and structs nested at most 32 deep, and refuses a pun of a
  type nested deeper ("type is nested too deeply").
- A struct of the `Rust` representation is proven by another source only if its fields have at
  most 1,024 offsets in all, counting each offset that a field may have in some order. Otherwise
  only the same struct proves it.
- `puns` tries every order of the fields of a struct of the `Rust` representation that has up to
  6 fields with a size or an alignment greater than 1. With more such fields, it takes the size
  to depend on the order, unless the fields need no padding in any order ([Puns without the
  feature][layouts#puns-without-the-feature]).
- The values of a scalar made of parts of the source are compared in at most 16,384 steps, and a
  pattern over at most 4,096 runs of the source.
- An automaton has at most 64 states, whether a rule requires it ([`matches`][rules#functions])
  or a type guarantees it. Only `str` and a type that implements `Pun` by hand with
  [`Layout::pattern`] guarantee a pattern: the derive cannot make one, since safe code can write
  the fields of a type that derives `Pun`.
- A `&mut` to a part of an array whose elements have rules looks at no more than 64 elements, and
  is refused past them.
- The parts of a target to check past its first 1,024 are checked all together.
- Validation recurses over the fields of a struct and the variants of an enum. With the default
  [`recursion_limit`] of 128, a struct of more than 127 fields, or an enum with fields and more
  than 128 variants, needs a higher `#![recursion_limit]` in the crate that calls the function
  that checks it ("reached the recursion limit while instantiating"). Each struct that holds the
  struct lowers its number of fields by 2.
- The evaluator of rules has [limits of its own][rules#limits-of-the-evaluator].

## Not yet

- **`Vec`, `String` and maps, through their contents.** A pun could reinterpret what they hold,
  as it does a slice, and keep the structure. The structure itself is not data: its bytes are a
  pointer and lengths, which are not the contents, and bytes made into one would point to no
  allocation.
- **Atomics, `Cell`, `UnsafeCell`, `Wrapping` and `Saturating`.** They have the layout of what
  they hold, but `puns` implements `Pun` for none of them.
- **Precise mutable parts.** [`head_mut`] and [`tail_mut`] refuse a source in which a struct
  that has a rule overlaps the part, unless the target has the same struct with the same rules
  there, even if the rule reads other fields. They also refuse an enum that the part covers only
  partly, even one whose variants all accept what is written. The operations of a rule show which
  fields and bytes it reads, so both could be allowed. A [`pun_mut`] of a whole value does not
  require this of rules. The two could be made consistent by allowing more of the parts, or by
  requiring more of `pun_mut`.
- **Ranges from conditions.** A counter that only a comparison bounds, such as `i` in the walk
  condition `|i| i < count`, is computed on 128 bits, unless the expression also reads the value
  at that offset (`be(i, 1)`). The state of a walk inside two other walks is given no range.
- **Rules implied by other rules.** A rule of the target is proven only by the same rule of the
  source, over the same extent at the same place, and is checked otherwise. `v % 6 == 0` implies
  `v % 3 == 0` but does not prove it, and `a != b` in a struct of 2 bytes does not prove it in a
  struct of 3, though it does not read the extent. The data that describes a walk, a
  variable-length integer, a CRC or a sum is not reasoned about either: a checksum only makes a
  check stricter.
- **A rule that two variants of a view share, where neither accepts every value of the other.**
  For bytes that fail the rule, each variant computes it. The later variants with a rule that
  failed over the same extent at the same place could be skipped, and the validation would report
  which rule failed, but every check would carry that report, where it is now a `bool` that the
  optimizer folds. A later variant that accepts every value of both computes the rule once ([How
  the derive reads a type][macro@Pun#how-the-derive-reads-a-type]).
- **Checked parts of a typed value** (`try_head` and so on). A part is either proven or refused.
  For bytes, there are [`from_head`] and [`from_tail`].
- **Precise checks of an enum whose source is another type.** When only some of its variants
  are proven, or some values of the source are not a tag, the whole enum is checked at run time,
  its tag first, then the fields of its variant, rather than only what was not proven.
- **A checked pun from an enum to a type that is not an enum.** As a source, an enum is known by
  the bytes that every variant initializes, not variant by variant. So `try_pun` from the
  `Shape` of [Values and padding][errors#values-and-padding] to a struct that requires an
  initialized byte where one variant has padding is refused, even if the check of the tag would
  exclude that variant. Relating the target to each variant whose tag it may accept would make
  the pun checked.
- **Checking some fields of a union.** When the check of one field would read a byte that may be
  padding, and no field is proven, the pun is refused. Checking only the fields that read no
  padding would be sound, but the analysis cannot tell the validation which fields to skip.
- **Checking only what is not proven, among parts that are checked together.** The elements of
  an array share their check ([What a check may read][layouts#what-a-check-may-read]), an enum
  is checked as a whole, as are the parts to check past the [first
  1,024](#bounds-of-the-analysis). A source that ends with a `str` where the target does not has
  every part of the target checked. A part that is proven there without being read (a union, an
  enum that holds one, a struct whose rule reads its padding) is then checked too, and the pun is
  refused if that check would read a byte that may be uninitialized, though the part needs no
  check. Marks that told the validation which elements and parts are proven would let it skip
  them.
- **The identity of a type that may hold uninitialized bytes.** A scalar of
  [`Spec::maybe_uninit`] equals no scalar, so a struct or an enum that holds one is not proven by
  itself where only the [same type][layouts#the-same-type] proves it.
- **A byte order that a value gives**, such as `EI_DATA` of ELF or the byte order mark of TIFF.
  A type has a byte order of its own (`u32be`, `i16le`, `f64be`, and so on), so no type reads
  its fields in the byte order that a value gives. A view enum chooses between a type of each
  byte order instead.
- **`default()`.** No function builds a value from the defaults of its fields (literals, sizes).
- **Fields of fields in rules.** A rule names the integer fields of its struct, not a field of a
  field (`header.length`) or an element of an array (`magic[0]`), which it reads with `be` or
  `le` at their offsets instead. Loading them would be sound if the offset were checked against
  the layout of the field, as it is for the fields of the struct.
- **Signed and floating-point fields in rules.** Rules compute on unsigned integers, so such a
  field is [refused][rules#fields], and read as bits with `le` or `be` instead.
- **Getters that read bytes.** A getter reads fields, not bytes, so a value that only the bytes
  hold, such as a variable-length integer, is decoded by the user's code. A rule reads one
  ([`leb128`][rules#functions] and others), but only to check a value or to give a size.
- **A size that is not in the value**, such as "to the end of whatever holds it" (an MP4 box of
  size 0).
- **Sums of types in sequences, and recursion**, such as `OPTIONAL` and `CHOICE` of ASN.1, and
  the containers of CBOR and MessagePack. A chain of types ([`Then<A, B>`]) is one sequence of
  types.

[`LINT_TERMINATOR_LIMIT`]: https://github.com/rust-lang/rust/blob/1.99.0/compiler/rustc_const_eval/src/const_eval/machine.rs
[`long_running_const_eval`]: https://doc.rust-lang.org/1.99.0/rustc/lints/listing/deny-by-default.html#long-running-const-eval
[`recursion_limit`]: https://doc.rust-lang.org/1.99.0/reference/attributes/limits.html#r-attributes.limits.recursion_limit
[`Then<A, B>`]: https://github.com/uael/puns/blob/v0.2.3/puns/examples/encodings.rs

[`from_head`]: https://docs.rs/puns/0.2.3/puns/fn.from_head.html
[`from_tail`]: https://docs.rs/puns/0.2.3/puns/fn.from_tail.html
[`head_mut`]: https://docs.rs/puns/0.2.3/puns/fn.head_mut.html
[`Layout::pattern`]: https://docs.rs/puns/0.2.3/puns/layout/struct.Layout.html#method.pattern
[`pun_mut`]: https://docs.rs/puns/0.2.3/puns/fn.pun_mut.html
[`Spec::maybe_uninit`]: https://docs.rs/puns/0.2.3/puns/layout/struct.Spec.html#method.maybe_uninit
[`tail_mut`]: https://docs.rs/puns/0.2.3/puns/fn.tail_mut.html
[errors]: errors.md
[errors#values-and-padding]: errors.md#values-and-padding
[layouts#puns-without-the-feature]: layouts.md#puns-without-the-feature
[layouts#the-same-type]: layouts.md#the-same-type
[layouts#what-a-check-may-read]: layouts.md#what-a-check-may-read
[macro@Pun]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html
[macro@Pun#enums-with-fields]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html#enums-with-fields
[macro@Pun#how-the-derive-reads-a-type]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html#how-the-derive-reads-a-type
[rules]: rules.md
[rules#fields]: rules.md#fields
[rules#functions]: rules.md#functions
[rules#limits-of-the-evaluator]: rules.md#limits-of-the-evaluator
