# The design of `puns`

The design is in the rustdoc, and this file says where each part is written and states the
invariants that cross modules. Read the module documentation of what you change before you change
it, and keep it true. `cargo doc --no-deps -p puns --document-private-items` renders the private
modules, but the module `view` is `#[doc(hidden)]`, so read its documentation in the source.

## What `puns` promises

- The crate documentation ([`src/lib.rs`](src/lib.rs), after the README) is the public design:
  the table of functions ("Reference"), what a valid value is and what the analysis relies on
  ("Soundness"), the `unstable` feature, the minimum Rust version, portability, and
  "Limitations".
- The documentation of `#[derive(Pun)]` (on `pub use puns_derive::Pun` in
  [`src/lib.rs`](src/lib.rs)) says what a rule means, what it reads, and what the derive trusts
  ("What the derive trusts").
- [`Scalar`](src/lib.rs) states the contract of a scalar declared by hand, a scalar that a pun
  never makes (`Spec::never`) included, and the crate documentation ("Scalars that a pun never
  makes") what `puns` guarantees its crate in return.

## How it works

- **A layout is a value.** [`layout`](src/layout.rs) ("Nodes") describes a type as a `const` tree
  of scalars, structs, enums, unions, arrays and patterns.
- **The analysis compares two layouts at compile time.** [`analysis`](src/analysis.rs) decides
  whether a pun fits, which values are valid, what must be checked, and whether a `&mut` is allowed.
  Its module documentation has a section for each.
- **Validation checks only the marks.** [`validate`](src/validate.rs) recurses over the types and
  runs no interpreter. Rules are evaluated by `layout::Machine`.
- **Narrowing.** [`narrow`](src/narrow.rs) computes the range of every value of a rule, so that
  each operation runs in a register where it fits.
- **Views.** [`view`](src/view.rs) decides each variant of a view enum once, from its base.
- **Enums of the `Rust` representation.** [`discover`](src/discover.rs) finds their layouts by
  experiments, and names the facts of the compiler that they rely on.
  [`std_enums`](src/std_enums.rs) applies it to `Option` and `Result`.
- **Pointers to unsized types.** [`ptr`](src/ptr.rs) splits and builds wide pointers in `const`,
  verified for each type.
- **Measurements.** [`examples/codegen.rs`](examples/codegen.rs) records the machine code of most of
  its functions. [`benches/checks.rs`](benches/checks.rs) times each check next to the same check
  written by hand.
- **The model check.** [`puns-model`](../puns-model/src/lib.rs) compares every verdict with
  oracles that do not use the analysis.

## Invariants that cross modules

- A pun gives a value only from bytes that are valid for it, proven by `analysis` or checked by
  `validate`. A check reads only what the marks of the analysis name.
- Without the `unstable` feature, nothing depends on a layout that Rust leaves to the compiler.
  `analysis` does not look into such a node, and no check replaces a proof in it. A check that
  reads it all the same gives the same answer in every layout, and reads only initialized bytes.
  `layout::Stable` records what is the same in every layout that Rust allows.
- A requirement (`valid(..)`, a rule) never proves a guarantee of another type. Two layouts are
  equal only if they require and guarantee the same values.
- No function makes a value that holds a scalar that a pun never makes (`Spec::never`), and no
  function gives a `&mut` to one. As a target, such a scalar is disjoint from every source
  (`atom_relation`), and a part of a target that holds one is proven by nothing, not even by
  itself (`Layout::holds_never`). Only `validate_but_never` accepts one, unread, and makes no value.
  `Option` of one is one of the same size (`std_enums::never_option`), found without a sample, which
  `puns` has none of: its size and the bytes of `None` are checked in const evaluation.
- The validation of the `try_` and `from_` functions compares the rules of what the derive or a
  hand-written `Pun` lists with the layout (`validate::listed`) and the place of each field
  (`F::Path`), and relies on the `# Safety` section of `Pun` for the other lists.
- What a layout is made of depends on no path that the deriving crate can redefine, though the
  generated code names `::core` and the root of `puns` ("What the derive trusts" in the
  documentation of `#[derive(Pun)]`). `fakecore` in `puns-model` tests that the derive meets it.
- The derive writes the type of each field once, in a trait of its generated code (`homes` in
  `puns-derive`), and a function that is never called fails to compile unless the item as
  compiled has those fields and variants, of those types. The order of the fields and the `repr`
  are taken from the tokens that the derive read ("What the derive trusts").
- Every scalar that a pun never makes in the repository is declared through the public API only,
  as another crate would declare it, with `Scalar` and `Spec::never`. Most of them share
  [`tests/support/never.rs`](tests/support/never.rs).

## Not done yet

The limits that users of the crate face are in "Limitations" of the crate documentation. This
section lists changes that could lift some of them, and what each would cost.

- **Precise mutable parts.** The operations of a rule show which fields and bytes it reads, so
  `head_mut` and `tail_mut` could allow a rule over other fields only, and an enum all of whose
  variants accept what is written. A `pun_mut` of a whole value does not require this of rules. The
  parts and `pun_mut` could be made consistent, by allowing more of the parts or by requiring more
  of `pun_mut`.
- **Ranges from conditions.** Narrowing uses how a value is computed and the reads that succeeded.
  It does not use a comparison that held, so the condition of a walk does not narrow its step, and
  the condition of an `if` does not narrow its branches. The walks of `Font` in `examples/images.rs`
  show the first case: their states are a power of two and its exponent, bounded only by their
  conditions.
- **Rules implied by other rules.** The analysis proves a rule of the target from the source only
  where the source has the same rule, over the same extent at the same place. A rule that other
  rules of the source imply is checked all the same. This is sound, and the model check does not
  count it as imprecise.
- **A rule that two variants of a view share, where neither variant accepts every value of the
  other.** The later variants that have a rule that failed, over the same extent at the same place
  (`Layout::same_rules`), could be skipped at once, and the validation would then report which rule
  failed. But every check would carry that report, where it is now a `bool` that the optimizer
  folds.
- **Precise checks of an enum whose source is another type.** Only what was not proven could be
  checked, rather than the whole enum.
- **A checked pun from an enum to a type that is not an enum.** Relating the target to each variant
  of the source whose tag it may accept would make the pun checked rather than refused.
- **Checking some fields of a union.** Checking only the fields that read no padding would be
  sound, but the marks of the analysis cannot tell the validation which fields to skip.
- **Checking only what is not proven, among parts that are checked together.** The elements of an
  array share their leaves, an enum is checked as a whole, a `str` of the source under something
  else marks every leaf, and the leaves past the first 1,024 are marked all together. A part that
  is proven there without being read is checked too, and the pun is refused if that check would
  read a byte that may be uninitialized (`Relation::unreadable`). Marks that told the validation
  which elements and which parts are proven would let it skip them.
- **The identity of an enum whose bytes may be uninitialized.** A scalar made by
  `Spec::maybe_uninit` equals no scalar, so a struct or an enum that holds one does not prove itself
  by `Layout::equals`. The library makes no such enum today, since the tags that it discovers are
  exact.
- **Fields of fields in rules.** Loading `header.length` or `magic[0]` would be sound if the
  offset were checked against the layout of the field, as a scalar of that size there, as it is
  for the fields of the struct.
- **Patterns other than `str` as types.** The derive cannot make a type that guarantees a
  pattern, since safe code can write its fields. A rule `matches(DFA, start, end)` (`Op::Matches`)
  requires one instead. Two automata are equal only if their states and edges are: their `id`,
  which a user can choose, only tells different ones apart early.
