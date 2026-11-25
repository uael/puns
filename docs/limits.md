# Limits, and what is not done yet

The conditions under which the derive does not compile a type are on [its page][macro@Pun], and
the message of each [refused][kinds] pun is in [Compile-time errors][errors].

## Limits of Rust

- **Const evaluation is bounded.** The deny-by-default lint [`long_running_const_eval`] stops it
  after 2,000,000 basic blocks ([`LINT_TERMINATOR_LIMIT`] in `rustc`), with the error "constant
  evaluation is taking a long time". So the analysis cannot take a step for each byte of a type,
  nor a step or an instance for each element of an array, and its searches stop at the
  [bounds](#bounds-of-the-analysis) below.
- **No function that a type chooses.** The check of a [rule][rules] is `const fn` code in `puns`,
  generic over the type it checks, and stable Rust cannot call there a function that the type
  chooses. A call through a function pointer is refused ("function pointer calls are not allowed in
  constant functions"). A call of a trait method or of an associated function of a trait is
  refused too (E0015, "cannot call non-const method" or "cannot call non-const associated
  function"), since const traits are unstable (E0658, "const trait impls are
  experimental"). The messages are the same on Rust 1.99.0 and 1.100.0. A function of the user could
  be called only if the check stopped being `const`, or behind a check that runs only outside of
  `const`, which stable Rust cannot express. So a rule calls only the functions of rules. A
  [getter][macro@Pun#get] may call any `const fn`, because the derive writes its code in the
  deriving crate, where the function is named directly. Since const traits are unstable, the
  implementations of standard traits, such as `Display` for [`Error`] and the types of [`num`], and
  `From` between these types and their integers, are not `const`.
- **No comparison of types in `const`.** So `puns` tells types apart by an
  [identity][layouts#the-same-type], which some types lack.
- **No unsized value by value.** Rust cannot return an unsized value by value, and `puns` never
  allocates, so no function returns a new value of an unsized type, such as a struct that ends
  with a slice. To make such a value, write its bytes, and read them with [`from_bytes`].
- **The tag of an enum of the `Rust` representation is found on values.** Stable Rust tells
  neither where the tag is nor its values: `offset_of!` on variants, `mem::variant_count` and
  `discriminant_value` are unstable. So the derive reads them on a value of each variant. It does
  not compile the enum where it cannot make such a value
  ([conditions][macro@Pun#enums-with-fields]), as where the variant holds a union none of whose
  fields has values. The message is then "a variant holds a union none of whose fields has
  values: a value of it may exist, but it has no sample to find its tag: unsupported layout".
- **The recursion limit.** The check of a struct recurses over its fields, and that of an enum over
  its variants. With the default [`recursion_limit`] of 128, on Rust 1.100.0, [`from_bytes`] of a
  `repr(C)` struct of 127 `bool`s compiles, and of 128 fails ("reached the recursion limit while
  instantiating"). [`from_bytes`] of a `repr(u16)` enum of 128 variants that each hold a `bool`
  compiles, and of 129 fails. A struct that holds the struct lowers that number of fields by 1, and
  by 1 more for each field before it. A larger type needs a higher `#![recursion_limit]` in the
  crate that calls the function.

## Bounds of the analysis

Past a bound, what is not proven is checked at run time, or the pun is refused where it cannot be
checked, and nothing is proven wrongly. The limits of rules are in [Limits of the
evaluator][rules#limits-of-the-evaluator] and [Functions][rules#functions].

- **Nesting.** The analysis follows at most 31 levels of arrays, slices and structs around a
  scalar: `try_pun` of `[u8; 1]` as 31 nested arrays around a `bool` compiles, and as 32 is
  refused ("type is nested too deeply").
- **Offsets of fields.** Without the `unstable` feature, a struct of the `Rust` representation is
  proven by another type only if its fields have at most 1,024 offsets in all, counting each
  offset that a field may have in some order. Otherwise only the same struct proves it.
- **Orders of fields.** `puns` tries every order of the fields of a struct of the `Rust`
  representation that has up to 6 fields with a size or an alignment greater than 1. With more
  such fields, it takes the size to depend on the order, unless the fields need no padding in any
  order. Without the feature, a pun of the struct then compiles only to itself.
- **A scalar over several fields.** A scalar of the target that covers several fields of the
  source, such as a `u16` over two `u8`s, is proven only if its values are found within 16,384
  combinations of the ranges of values of those fields.
- **A string over the source.** A `str` of the target, or another type that guarantees what an
  automaton accepts, is proven by running the automaton over the source. The analysis runs it
  over at most 4,096 stretches of the source, each of padding, of bytes of any value, or of a
  scalar with restricted values. Over a
  stretch of bytes of any value, the analysis follows the automaton for at most 257 bytes: if the
  states that it may be in still change after them, the string is checked.
- **The values of a tag.** Where a variant of an enum of the target is related to the source, the
  analysis knows which values the source holds where the tag is, as at most 8 ranges. Past them,
  it knows nothing of these values, so what only they prove, such as a niche of the target over
  padding of the source variant, is not proven.
- **Elements under a `&mut`.** A `&mut` to a part of an array whose elements have rules looks at
  no more than 64 of its elements, and is refused past them.
- **What a check can single out.** The analysis tells apart the first 1,024 parts of a target that
  a check may test: its scalars with restricted values, strings, enums, unions and rules. Those
  after them share one mark, so if one of them needs a check, all of them are checked.

## Bounds of the checks

- **Pointees within pointees.** The `try_` and `from_` functions check the pointees of eager [aims]
  in place, one within another. They return [`Error::Limit`] past the bound that it states:
  [`AIM_PARTS`] parts of the layouts of the pointees, one within another, or pointees of twice as
  many bytes as the function reads. [`check`] and [`check_compact`] check deeper data, with a stack
  that does not grow with it.

## Not yet

What is planned, each with what it would allow.

- **`Vec`, `String` and maps, through their contents.** Today, reinterpret what they hold:
  [`pun_ref`] of `v.as_slice()`. The bytes of a `Vec` itself are a pointer and lengths, so
  reading them would not read its contents. Planned: a pun that reinterprets the contents and
  keeps the structure that holds them.
- **Precise `head_mut` and `tail_mut`.** [`head_mut`] and [`tail_mut`] refuse a struct that has a
  rule even if the rule reads other fields, and an enum that they cover only partly even if all
  its variants accept what is written ([Writes leave a valid
  source][api#writes-leave-a-valid-source]). Planned: reading which fields and bytes a rule
  reads, from its operations, to allow both. [`pun_mut`] lets a write break a rule ([Writing
  through `&mut`][api#writing-through-mut]). Whether to make the two agree, by allowing more
  to `head_mut` and `tail_mut` or by requiring more of `pun_mut`, is not decided.
- **Ranges from conditions.** A comparison that held does not narrow the range of a value
  ([Walks][rules#walks]). So a counter that only a comparison bounds, such as `i` in the walk
  condition `|i| i < count`, is computed on 128 bits, unless the expression also reads the value
  at that offset (`be(i, 1)`). So is the state of a walk inside two other walks. The check is
  then longer. Planned: ranges from the conditions.
- **Rules implied by other rules.** A rule of the target is proven only by the same rule ([Why the
  pair decides][how_puns_decides#why-the-pair-decides]), and is otherwise checked. So
  `v % 6 == 0` does not prove `v % 3 == 0`. Likewise, `a != b` in a struct of 2 bytes does not
  prove it in a struct of 3, though it does not read the
  [extent][how_puns_decides#layouts-and-rules]. Nor does the analysis
  reason about the data that describes a walk, a variable-length integer, a CRC or a sum, since a
  checksum only makes a check stricter. Planned: proving a rule from rules that imply it.
- **A checked pun from an enum to a type that is not an enum.** `puns` knows an enum as a source
  only by the bytes that every variant initializes, not variant by variant. So `try_pun` from the
  `Shape` of [Values and padding][errors#values-and-padding] to a struct that requires an
  initialized byte where one variant has padding is refused, even if the check of the tag would
  exclude that variant. Planned: relating the target to each variant whose tag it may accept, so
  that the pun is checked.
- **Checking only what is not proven.** Some parts of a target are checked together, so a proven
  part among them is checked with the others:

  - An enum, when only some of its variants are proven from a source that is another type, or some
    values of the source are not a tag. Its tag is checked, then the fields of its variant.
  - The elements of an array ([What a check may read][layouts#what-a-check-may-read]).
  - The parts past the [first 1,024](#bounds-of-the-analysis).
  - Every part of the target, where the source ends with a `str` and the target does not.

  A proven union, an enum that holds one, or a struct whose rule reads its padding may then read a
  byte that is uninitialized in the source, and the pun is refused, though the part needs no
  check. Planned: checking only the parts that are not proven, and only the fields of a target
  union that require no byte that may be padding to be initialized, where [Unions][soundness#unions]
  refuses the pun.
- **Atomics, `Cell` and `UnsafeCell`.** An atomic integer has the size and the bit validity of
  its integer, but `core` makes its alignment "always equal to its size, even on targets where
  `T` has alignment less than its size" ([`Atomic`]). `Cell<T>` and `UnsafeCell<T>` have "the same
  in-memory representation as" `T` ([`UnsafeCell`]). All of them allow writes through a shared
  reference, while the functions that return a `&` rely on a [`Pun`] type holding no
  `UnsafeCell`. Planned: supporting them by value and through `&mut`, where no shared reference
  can write them.

## Not decided

What could be done, where whether to do it is not decided.

- **More enums of the `Rust` representation assumed.** `puns` does not
  [assume][layouts#what-puns-assumes] the layout of an enum in three cases. One is an enum with a
  variant whose fields have different alignments, since the width of the tag follows the field
  that `rustc` puts first. Another is an enum with a niche that ties with another, since `rustc`
  chooses by where they are. The third is an enum with a niche in a part whose fields `rustc` may
  reorder. Each would need the choice of `rustc` measured
  and stated as part of the assumption.
- **`Wrapping` and `Saturating`.** They are `repr(transparent)` around their integer, and `puns`
  does not implement `Pun` for them.
- **`repr(transparent)` enums with fields.** Rust gives such an enum the layout of its only field
  of nonzero size ([`r-layout.repr.transparent.layout-abi`]), but the derive reads it as an enum
  of the `Rust` representation ([Enums with fields][macro@Pun#enums-with-fields]). So without the
  `unstable` feature, a pun that reads its field compiles only where `puns`
  [assumes][layouts#what-puns-assumes] the layout of the enum, as it does for a variant of one
  field, and is refused for an unspecified layout otherwise.
- **A rule that two variants of a [view][macro@Pun#views] share, where neither accepts every
  value of the other.** For bytes that fail the rule, each variant computes it, unless a later
  variant computes it once for both ([What the view functions
  check][macro@Pun#what-the-view-functions-check]). Skipping the later variants whose same rule
  failed, over the same extent at the same place, would need the check to report which rule
  failed, while it now returns a `bool` that the optimizer folds. Whether that cost is worth it is
  not decided.
- **Checked front and back of a typed value** (`try_head` and so on). [`head`], [`tail`] and
  their forms prove or are refused. For bytes, [`from_head`] and [`from_tail`] check.
- **A byte order that a value gives.** A type has a byte order of its own, so no type reads its
  fields in the byte order that a value gives. A view chooses between a type of each byte order
  instead ([A byte order that the value gives][in_practice#a-byte-order-that-the-value-gives]).
- **A value from what its type fixes.** No function builds a value of a struct whose fields of a
  constant type of [`num`], such as `u8c<7>`, hold their value, and whose fields that a `size` or
  `len` rule reads hold the length of the value. Each field is written by hand, as
  `Default::default()` for a field of a constant type.
- **Fields of fields, and floating-point fields, in rules.** A rule reads them with `le` or `be` at
  their offsets, and a signed field under a cast ([Fields in a rule][rules#fields-in-a-rule]).
  Naming a field of a field (`header.length`) or an element of an array (`magic[0]`) would be sound
  if its offset were checked against the layout of the field, as a scalar of that size there, as it
  is for the fields of the struct.
- **The functions of rules in getters.** A getter has none of the functions of rules, so a value
  that only the bytes hold, such as a variable-length integer, is decoded by a `const fn` of the
  user. A rule reads one ([`leb128`][rules#functions] and others), but only to check a value or to
  give a size.
- **A size to the end of what holds the value.** A rule cannot name the end of the bytes that
  hold the value, which an MP4 box of size 0 extends to.
- **Choices and recursion in a sequence of types.** A struct, or a chain of types such as
  `Then<A, B>` of the example [`encodings.rs`], reads one fixed sequence of types. A choice among
  types at one place of it (`OPTIONAL` and `CHOICE` of ASN.1), and a type that holds itself (the
  containers of CBOR and MessagePack), are not supported.

[`Atomic`]: https://doc.rust-lang.org/1.99.0/core/sync/atomic/struct.Atomic.html
[`encodings.rs`]: https://github.com/uael/puns/blob/v0.2.4/puns/examples/encodings.rs
[`LINT_TERMINATOR_LIMIT`]: https://github.com/rust-lang/rust/blob/1.99.0/compiler/rustc_const_eval/src/const_eval/machine.rs
[`long_running_const_eval`]: https://doc.rust-lang.org/1.99.0/rustc/lints/listing/deny-by-default.html#long-running-const-eval
[`recursion_limit`]: https://doc.rust-lang.org/1.99.0/reference/attributes/limits.html#r-attributes.limits.recursion_limit
[`r-layout.repr.transparent.layout-abi`]: https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.transparent.layout-abi
[`UnsafeCell`]: https://doc.rust-lang.org/1.99.0/core/cell/struct.UnsafeCell.html#memory-layout

[`AIM_PARTS`]: https://docs.rs/puns/0.2.4/puns/constant.AIM_PARTS.html
[`check_compact`]: https://docs.rs/puns/0.2.4/puns/fn.check_compact.html
[`check`]: https://docs.rs/puns/0.2.4/puns/fn.check.html
[`Error::Limit`]: https://docs.rs/puns/0.2.4/puns/enum.Error.html#variant.Limit
[`Error`]: https://docs.rs/puns/0.2.4/puns/enum.Error.html
[`from_bytes`]: https://docs.rs/puns/0.2.4/puns/fn.from_bytes.html
[`from_head`]: https://docs.rs/puns/0.2.4/puns/fn.from_head.html
[`from_tail`]: https://docs.rs/puns/0.2.4/puns/fn.from_tail.html
[`head_mut`]: https://docs.rs/puns/0.2.4/puns/fn.head_mut.html
[`head`]: https://docs.rs/puns/0.2.4/puns/fn.head.html
[`num`]: https://docs.rs/puns/0.2.4/puns/num/index.html
[`Pun`]: https://docs.rs/puns/0.2.4/puns/trait.Pun.html#safety
[`pun_mut`]: https://docs.rs/puns/0.2.4/puns/fn.pun_mut.html
[`pun_ref`]: https://docs.rs/puns/0.2.4/puns/fn.pun_ref.html
[`tail_mut`]: https://docs.rs/puns/0.2.4/puns/fn.tail_mut.html
[`tail`]: https://docs.rs/puns/0.2.4/puns/fn.tail.html
[aims]: aims.md
[how_puns_decides#why-the-pair-decides]: how_puns_decides.md#why-the-pair-decides
[errors]: errors.md
[errors#values-and-padding]: errors.md#values-and-padding
[in_practice#a-byte-order-that-the-value-gives]: in_practice.md#a-byte-order-that-the-value-gives
[api#writing-through-mut]: api.md#writing-through-mut
[api#writes-leave-a-valid-source]: api.md#writes-leave-a-valid-source
[kinds]: how_puns_decides.md#proven-checked-and-refused
[layouts#the-same-type]: layouts.md#the-same-type
[layouts#what-puns-assumes]: layouts.md#what-puns-assumes
[layouts#what-a-check-may-read]: layouts.md#what-a-check-may-read
[macro@Pun]: https://docs.rs/puns/0.2.4/puns/derive.Pun.html
[macro@Pun#enums-with-fields]: https://docs.rs/puns/0.2.4/puns/derive.Pun.html#enums-with-fields
[macro@Pun#get]: https://docs.rs/puns/0.2.4/puns/derive.Pun.html#get
[macro@Pun#what-the-view-functions-check]: https://docs.rs/puns/0.2.4/puns/derive.Pun.html#what-the-view-functions-check
[macro@Pun#views]: https://docs.rs/puns/0.2.4/puns/derive.Pun.html#views
[rules]: rules.md
[how_puns_decides#layouts-and-rules]: how_puns_decides.md#layouts-and-rules
[rules#fields-in-a-rule]: rules.md#fields-in-a-rule
[rules#functions]: rules.md#functions
[rules#limits-of-the-evaluator]: rules.md#limits-of-the-evaluator
[rules#walks]: rules.md#walks
[soundness#unions]: soundness.md#unions
