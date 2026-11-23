# Limits, and what is not done yet

This chapter lists what `puns` does not do, and why: limits of Rust, bounds that keep the
analysis within them, and what is planned. The conditions under which the derive does not compile
a type are on [its page][macro@Pun], and [Compile-time errors][errors] gives the message of each
[refused][kinds] pun.

## Limits of Rust

- **Const evaluation is bounded.** The deny-by-default lint [`long_running_const_eval`] stops it
  after 2,000,000 basic blocks ([`LINT_TERMINATOR_LIMIT`] in `rustc`), with the error "constant
  evaluation is taking a long time". So the analysis cannot take a step for each byte of a type,
  nor a step or an instance for each element of an array, and its searches stop at the
  [bounds](#bounds-of-the-analysis) below.
- **No function that a type chooses.** The check of a [rule][rules] is `const fn` code, generic
  over the type it checks, and stable Rust cannot call there a function that the type chooses. A
  call through a function pointer is refused ("function pointer calls are not allowed in constant
  functions"), and so is a call of a trait method (E0015, "cannot call non-const associated
  function"), while const traits are unstable (E0658, "const trait impls are experimental"), with
  the same messages on Rust 1.99.0 and 1.100.0. A user function could be called only if the check
  stopped being `const`, or behind a check that runs only outside of `const`, which stable Rust
  cannot express. So a rule may not call an arbitrary function, and what it computes is built in.
  A [getter][macro@Pun#get] is a `const fn` that the derive writes for its own type, not generic
  code, so it may call any `const fn`. Since const traits are unstable, the implementations of
  `Display` for [`Error`] and the types of [`num`], and of `From` between these types and their
  integers, are not `const`.
- **No comparison of types in `const`.** `puns` tells types apart by an
  [identity][layouts#the-same-type], which a generic type that derives `Pun`, a `PhantomData` and a
  [`Scalar`] implemented by hand do not have. Without the `unstable` feature, a generic struct or
  enum of the `Rust` representation that derives `Pun` therefore does not prove itself, unless it
  is proven field by field. `pun` of an `enum E<T> { A(T), B(u16) }` as itself is refused, with
  the message of [Unspecified layouts][errors#unspecified-layouts].
- **No unsized value by value.** Rust cannot return an unsized value by value, and `puns` never
  allocates, so no function returns a new value of an unsized type, such as a struct that ends
  with a slice. To make such a value, write its bytes, and read them with [`from_bytes`].
- **The tag of an enum of the `Rust` representation is found on values.** Stable Rust tells
  neither where the tag is nor its values: `offset_of!` on variants, `mem::variant_count` and
  `discriminant_value` are unstable. So the derive reads them on a value of each variant, and does
  not compile the enum where it cannot make one: where the value would hold an [opaque scalar]
  ([conditions][macro@Pun#enums-with-fields]), or where the variant holds a union none of whose
  fields has values ("a variant holds a union none of whose fields has values: a value of it may
  exist, but it has no sample to find its tag: unsupported layout").
- **The recursion limit.** The check of a struct recurses over its fields, and that of an enum
  over its variants. With the default [`recursion_limit`] of 128, on Rust 1.100.0, [`from_bytes`]
  of a `repr(C)` struct of 127 `bool`s compiles and of 128 fails ("reached the recursion limit
  while instantiating"), and of a `repr(u16)` enum of 128 variants that each hold a `bool`
  compiles and of 129 fails. A struct that holds the struct lowers that number of fields by 1,
  and by 1 more for each field before it. A larger type needs a higher `#![recursion_limit]` in
  the crate that calls the function.

## Bounds of the analysis

The analysis bounds its searches to stay within the limit of const evaluation. Past a bound, what
is not proven is checked at run time, or the pun is refused where it cannot be checked, and
nothing is proven wrongly. The limits of rules, of their walks and of their automata are in [Limits
of the evaluator][rules#limits-of-the-evaluator] and [Functions][rules#functions].

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
  combinations of the ranges of values of those fields. Past them, it is checked at run time.
- **A string over the source.** A `str` of the target, or another type that guarantees what an
  automaton accepts, is proven by running the automaton over at most 4,096 stretches of the
  source, each of padding, of bytes of any value, or of a scalar with restricted values. Past
  them, it is checked at run time.
- **Elements under a `&mut`.** A `&mut` to a part of an array whose elements have rules looks at
  no more than 64 of its elements, and is refused past them.
- **What a check can single out.** The analysis tells apart the first 1,024 scalars, strings,
  enums, unions and rules of a target. If one after them needs a check, the check covers all of
  them after it.

## Not yet

- **`Vec`, `String` and maps, through their contents.** Today, reinterpret what they hold:
  [`pun_ref`] of `v.as_slice()`. The bytes of a `Vec` itself are a pointer and lengths, so
  reading them would not read its contents. Planned: a pun that reinterprets the contents and
  keeps the structure that holds them.
- **`Wrapping` and `Saturating`.** They are `repr(transparent)` around their integer, and `puns`
  does not implement `Pun` for them yet.
- **Atomics, `Cell` and `UnsafeCell`.** An atomic integer has the size and the bit validity of
  its integer, but `core` makes its alignment "always equal to its size, even on targets where
  `T` has alignment less than its size" ([`Atomic`]). `Cell<T>` and `UnsafeCell<T>` have "the same
  in-memory representation as" `T` ([`UnsafeCell`]). All of them allow writes through a shared
  reference, while the functions that return a `&` rely on a [`Pun`] type holding no
  `UnsafeCell`. Whether to support them by value and through `&mut` only is not decided.
- **Precise `head_mut` and `tail_mut`.** [`head_mut`] and [`tail_mut`] refuse a source in which a struct
  that has a rule overlaps the bytes that they cover, unless the target has the same struct with
  the same rules there, even if the rule reads other fields. They also refuse an enum that they
  cover only partly, even one whose variants all accept what is written. Planned: reading which
  fields and bytes a rule reads, from its operations, to allow both. A [`pun_mut`] of a whole
  value does not require this of rules ([Writing through `&mut`][functions#writing-through-mut]).
  Whether to make the two agree, by allowing more to `head_mut` and `tail_mut` or by requiring
  more of `pun_mut`, is not decided.
- **Ranges from conditions.** A value is computed in a register of its range where the range is
  known, and a comparison that held does not narrow it ([Walks][rules#walks]). So a counter that
  only a comparison bounds, such as `i` in the walk condition `|i| i < count`, is computed on 128
  bits, unless the expression also reads the value at that offset (`be(i, 1)`), and so is the
  state of a walk inside two other walks. The check is then longer. Planned: ranges from the
  conditions.
- **Rules implied by other rules.** A rule of the target is proven only as [Why the pair
  decides][concepts#why-the-pair-decides] says, by the same rules in the same order, over the
  same sized part and trailing slice. Otherwise it is checked: `v % 6 == 0` does not prove
  `v % 3 == 0`, and `a != b` in a struct of 2 bytes does not prove it in a struct of 3, though it
  does not read the [extent][rules#extent]. Nor does the analysis reason about the data that
  describes a walk, a variable-length integer, a CRC or a sum, since a checksum only makes a check
  stricter. Planned: proving a rule from rules that imply it.
- **A rule that two variants of a [view][macro@Pun#views] share, where neither accepts every
  value of the other.** For bytes that fail the rule, each variant computes it. A later variant
  that accepts every value of both computes it once ([How the derive reads a
  type][macro@Pun#how-the-derive-reads-a-type]). Skipping the later variants whose same rule
  failed, over the same extent at the same place, would need the check to report which rule
  failed, while it now returns a `bool` that the optimizer folds. Whether that cost is worth it is
  not decided.
- **Checked front and back of a typed value** (`try_head` and so on). [`head`], [`tail`] and
  their forms prove or are refused. For bytes, [`from_head`] and [`from_tail`] check.
- **Precise checks of an enum whose source is another type.** When only some of its variants
  are proven, or some values of the source are not a tag, the whole enum is checked at run time,
  its tag first, then the fields of its variant. Planned: checking only what was not proven.
- **A checked pun from an enum to a type that is not an enum.** `puns` knows an enum as a source
  only by the bytes that every variant initializes, not variant by variant. So `try_pun` from the
  `Shape` of [Values and padding][errors#values-and-padding] to a struct that requires an
  initialized byte where one variant has padding is refused, even if the check of the tag would
  exclude that variant. Planned: relating the target to each variant whose tag it may accept, so
  that the pun is checked.
- **Checking some fields of a union.** Where some field of a target union might read padding and
  no field is proven, the pun is refused ([Unions][soundness#unions]). Planned: checking only the
  fields that read no padding.
- **Checking only what is not proven.** Some parts are checked together: the elements of an array
  ([What a check may read][layouts#what-a-check-may-read]), the fields of an enum checked as a
  whole, everything past the [first 1,024](#bounds-of-the-analysis) things that a check can single
  out, and every part of the target where the source ends with a `str` and the target does not. A
  union that is proven there, or an enum that holds one, or a struct whose rule reads its padding,
  is then checked too, and the pun is refused if that check would read a byte that may be
  uninitialized, though the part needs no check. Planned: checking only the parts that are not
  proven.
- **A byte order that a value gives**, such as `EI_DATA` of ELF or the byte order mark of TIFF.
  A type has a byte order of its own (`u32be`, `i16le`, `f64be`, and so on), so no type reads
  its fields in the byte order that a value gives. A view chooses between a type of each byte
  order instead.
- **`default()`.** No function builds a value from the defaults of its fields (literals, sizes).
- **Fields of fields in rules.** A rule names the integer fields of its struct, not a field of a
  field (`header.length`) or an element of an array (`magic[0]`), which it reads with `be` or
  `le` at their offsets instead. Naming them would be sound if their offset were checked against
  the layout of the field, as a scalar of that size there, as it is for the fields of the struct.
- **Signed and floating-point fields in rules.** Rules compute on unsigned integers, so such a
  field does [not compile][rules#fields] in a rule, and is read as bits with `le` or `be`
  instead.
- **Getters that read bytes.** A getter reads fields, not bytes, so a value that only the bytes
  hold, such as a variable-length integer, is decoded by the user's code. A rule reads one
  ([`leb128`][rules#functions] and others), but only to check a value or to give a size.
- **A size to the end of what holds the value.** A rule cannot name the end of the bytes that
  hold the value, which an MP4 box of size 0 extends to.
- **Choices and recursion in a sequence of types.** A struct, or a chain of types such as
  `Then<A, B>` of the example [`encodings.rs`], reads one fixed sequence of types. A choice among
  types at one place of it (`OPTIONAL` and `CHOICE` of ASN.1), and a type that holds itself (the
  containers of CBOR and MessagePack), are not supported.

[`Atomic`]: https://doc.rust-lang.org/1.100.0/core/sync/atomic/struct.Atomic.html
[`encodings.rs`]: https://github.com/uael/puns/blob/v0.2.3/puns/examples/encodings.rs
[`LINT_TERMINATOR_LIMIT`]: https://github.com/rust-lang/rust/blob/1.99.0/compiler/rustc_const_eval/src/const_eval/machine.rs
[`long_running_const_eval`]: https://doc.rust-lang.org/1.99.0/rustc/lints/listing/deny-by-default.html#long-running-const-eval
[`recursion_limit`]: https://doc.rust-lang.org/1.99.0/reference/attributes/limits.html#r-attributes.limits.recursion_limit
[`UnsafeCell`]: https://doc.rust-lang.org/1.100.0/core/cell/struct.UnsafeCell.html#memory-layout

[`Error`]: https://docs.rs/puns/0.2.3/puns/enum.Error.html
[`from_bytes`]: https://docs.rs/puns/0.2.3/puns/fn.from_bytes.html
[`from_head`]: https://docs.rs/puns/0.2.3/puns/fn.from_head.html
[`from_tail`]: https://docs.rs/puns/0.2.3/puns/fn.from_tail.html
[`head_mut`]: https://docs.rs/puns/0.2.3/puns/fn.head_mut.html
[`head`]: https://docs.rs/puns/0.2.3/puns/fn.head.html
[`num`]: https://docs.rs/puns/0.2.3/puns/num/index.html
[`Pun`]: https://docs.rs/puns/0.2.3/puns/trait.Pun.html#safety
[`pun_mut`]: https://docs.rs/puns/0.2.3/puns/fn.pun_mut.html
[`pun_ref`]: https://docs.rs/puns/0.2.3/puns/fn.pun_ref.html
[`Scalar`]: https://docs.rs/puns/0.2.3/puns/trait.Scalar.html
[`tail_mut`]: https://docs.rs/puns/0.2.3/puns/fn.tail_mut.html
[`tail`]: https://docs.rs/puns/0.2.3/puns/fn.tail.html
[concepts#why-the-pair-decides]: concepts.md#why-the-pair-decides
[errors]: errors.md
[errors#unspecified-layouts]: errors.md#unspecified-layouts
[errors#values-and-padding]: errors.md#values-and-padding
[functions#writing-through-mut]: functions.md#writing-through-mut
[kinds]: concepts.md#proven-checked-and-refused
[layouts#the-same-type]: layouts.md#the-same-type
[layouts#what-a-check-may-read]: layouts.md#what-a-check-may-read
[macro@Pun]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html
[macro@Pun#enums-with-fields]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html#enums-with-fields
[macro@Pun#get]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html#get
[macro@Pun#how-the-derive-reads-a-type]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html#how-the-derive-reads-a-type
[macro@Pun#views]: https://docs.rs/puns/0.2.3/puns/derive.Pun.html#views
[opaque scalar]: concepts.md#unspecified-layouts-opaque-scalars-and-uninhabited-types
[rules]: rules.md
[rules#extent]: rules.md#extent
[rules#fields]: rules.md#fields
[rules#functions]: rules.md#functions
[rules#limits-of-the-evaluator]: rules.md#limits-of-the-evaluator
[rules#walks]: rules.md#walks
[soundness#unions]: soundness.md#unions
