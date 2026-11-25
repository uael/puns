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
- [`Scalar`](src/lib.rs) states the contract of a scalar declared by hand, and the crate
  documentation ("Aims") what `puns` guarantees a type that holds an eager aim, such as a
  self-relative pointer, which derives `Pun` with `#[pun(pointer(..))]`.

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
  [`std_enums`](src/std_enums.rs) applies it to `Option` and `Result`. [`assume`](src/assume.rs)
  computes the layout that `rustc` gives such an enum from the layouts of its fields, where that is
  the same in every layout that Rust allows, and compares it with what the experiments found.
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
  `layout::Stable` records what is the same in every layout that Rust allows. The layout of an
  enum of the `Rust` representation that `assume` gives counts as such, because it is a function
  of the layouts of the fields, and a `const` assertion compares it with what `rustc` chose. The
  scope of the assumption admits only enums whose layout that function gives the same way in every
  layout of their fields, and nothing outside it is assumed. Where `Spec::validity` is false, a
  scalar gives no niche that `assume` knows from its own values. It is set only for the closed list
  of types whose `Spec` is what Rust allows (`bool`, `char`, `NonZero`, derived enums without
  fields), since only `core` and `std` make niches. A scalar declared by hand gives the niche of
  the type that its bytes are, if its `Spec` declares one (`Spec::stored_as`). `Layout::assumed`
  returns the layout that the experiments found, never the predicted one, so a false declaration
  only fails the build.
- A requirement (`valid(..)`, a rule) never proves a guarantee of another type. Two layouts are
  equal only if they require and guarantee the same values. Each rule of a struct is a leaf of its
  own (`Layout::rule_leaf`), proven by the same rule of a struct of the same extent at the same
  place, after the same aims at the cursor if it is one, and checked on its own otherwise.
- An identity (`layout::Id`, `Pun::ID`) names a type by its declaration and by every parameter but
  its lifetimes, so two types with one identity are one type. It has two uses only: proving a part
  that is not `fixed` by the same type at the same place, and naming a type without its layout.
  `ManuallyDrop<T>` is `T` only as the whole of a part (`Id::equals`). An identity is made of the
  identities of the parameters, never of their layouts, so that a type that holds itself through a
  generic type has a layout. The derive gives a generic type one only where each type parameter is
  bounded by `Pun` (`identity` in `puns-derive`).
- The validation of the `try_` and `from_` functions compares the rules of what the derive or a
  hand-written `Pun` lists with the layout (`validate::listed`) and the place of each field
  (`F::Path`), and relies on the `# Safety` section of `Pun` for the other lists.
- What a layout is made of depends on no path that the deriving crate can redefine, though the
  generated code names `::core` and the root of `puns` ("What the derive trusts" in the
  documentation of `#[derive(Pun)]`). `Layout::PIN` fixes the types whose functions make a
  layout, and `Layout::PIN_AIMS` those of the rule of an aim, so a new type whose functions the
  derive calls goes there. `fakecore` in `puns-model`
  tests that the derive meets it for `::core`, and the cases `crate_path_*` of
  `tests/compile-fail` for the root of `puns`.
- The derive writes the type of each field once, in a trait of its generated code (`homes` in
  `puns-derive`), and a function that is never called fails to compile unless the item as
  compiled has those fields and variants, of those types. The offsets of the fields are measured
  on the item as compiled (`Layout::offsets`, the samples of the variants), except where
  "What the derive trusts" says that the `repr` that the derive read gives them.
- The self-relative pointer of the tests (`Near`, in
  [`tests/support/aims.rs`](tests/support/aims.rs)) derives `Pun` with an eager aim, as another
  crate would declare it.
- An aim (`layout::Aim`) is where its pointee is (`Start`: stored, or the cursor of the tail),
  how large it is (`Extent`: what the pointee stores, a length, a size in bytes, or the rest of the
  tail of the struct), and when it is checked (`Check`), and every combination is checked and read.
  The size of a pointee is the size of its value with its metadata, as `size_of_val` gives. An
  eager aim is checked with the struct that holds it, its pointee within the bytes that the check
  reads (`validate::Input`), and code reads the pointee without a check, through a safe method.
  So no function makes a value that holds one (`Plan::eager`), gives a `&mut` to or from one
  (`Plan::eager_mut`), or checks one from a source whose bytes may be padding, only the same aim at
  the same place proves one (`Aim::equals`, with a known identity of the pointee), and a union does
  not hold one (`Layout::union`). The derive gives an eager aim only to a struct whose field of the
  aim (the offset of a pointer, or the bytes of eager regions) is an `InPlace<T>`, which no code
  makes and which is not `Pun`, and checks that the field is the `InPlace<T>` of `puns` for its
  own `T` through `Layout::in_place` (`item` in `homes`), which no crate path replaces, so that no
  safe code makes a value of the struct, nor a reference to one but through `puns`: the
  documentation of `InPlace` states this argument, and the obligation of unsafe code that makes
  such values, as `nearest` does, and the `# Safety` of `Pun` the property that the reads rely
  on. An `InPlace<T>` is a `T` in the layout. A lazy aim at the cursor requires of the struct only
  that its pointee fits, and a lazy aim at a stored place nothing, so its type counts in a proof
  only where the pointee stores its length.
- Both kinds of start are one rule (`Rule::Aim`), checked in one place (`validate::check_aim`),
  which places the pointee by one step of the cursor (`validate::cursor_step`) or at a stored place
  (`validate::stored_at`), written by the derive in one place (`aim_rule` in `puns-derive`), which
  `pointer(..)` and `regions(..)` enter. What reads a pointee finds it as the check does: the same
  steps of the cursor, which truncate where the check fails (`validate::region_at`), or the same
  expression (`validate::pointer_at`, `validate::value_of`, whose arithmetic is checked). A struct
  may have both, and the pointee of a region may hold eager aims, which its read looks for in the
  bytes of the struct (`tests/aims.rs`).
- The read of the pointee of an eager aim (`name`, from `&self`) is safe: the derive writes a call
  of `aim_of`, a safe function of `puns`, with no `unsafe` block, so a crate path that names
  another function there calls safe code. `aim_of` relies on the `# Safety` of `Pun`, which the
  `InPlace` of the aim meets, and on the reference having the provenance of the bytes of the
  pointee, which tree borrows gives and stacked borrows does not. `aim` and `aim_mut` take a
  pointer, so that a caller under stacked borrows gives the provenance of the checked bytes, as the
  tests that Miri runs with stacked borrows do (`tests/aims.rs`); the tests of the reads of
  pointees outside the value through a reference (`tests/aim_reads.rs`) run under tree borrows
  only. They name an aim by its name, a field of the constant `AIMS` that the derive writes
  (`AimOf`). The derive writes no method that writes a pointee: such a method would be `unsafe`,
  and would call a function that the crate path names. The length of a pointee that stores it is
  read from its sized fields or before its value only, by the check and by the read
  (`Layout::stores_length`, `validate::own_length`), so the read never meets a length that the
  check accepted and it cannot read.
- A layout cannot hold itself, so an aim holds the size, the alignment and the identity of its
  pointee, and the pointee is reached through `Pun::Rules` (`Rules::Pointee`), so that the type of
  a pointee is written in one place of the derive. A read names the item of its aim by where it is
  in the list of its struct (`Item`: `First`, `After`, hidden, which the derive counts for
  `AimOf`), never by the item, so the pointee that it reads is the one that the check of the struct
  found, whatever its identity. `Aim::new` reads no layout of the pointee for this reason; what it
  cannot assert, `validate::pointee_checked` asserts where the aim is checked or read. The identity
  of a type is `Pun::ID`, made of the identities of its parameters and never of a layout, which its
  layout holds too. The pointee is a `Pointee`, a
  sealed trait: a `Pun` type, or a `Thin<T, M>`, the metadata of a pointer to `T` stored before it
  as an `M` that ends with the length, at which the pointer points, and nothing for a sized `T`
  (`Metadata::Stored`). `Pun::Metadata` is a sealed `Metadata`, `()` exactly when the layout is
  sized (`Layout::measured` asserts it of every struct).
- The functions that check in place (`from_head` and the others) check a pointee in place, nested
  in the check of the struct whose aim leads to it, as a check without loops does, and in one pass
  hand a pointee whose type holds an eager aim and has an identity over to a loop of its type
  (`validate::run`, `validate::check_value`): where its parts are not left (`AIM_PARTS`), and where
  it is a link of a chain nested in a value of its type past `IN_PLACE` parts, so that a `const`
  check of a chain stays within 128 frames. A link of a chain is checked in place with
  `validate::InPlace`, which tells that a link of its type is nested in it; any other pointee with
  `All`, as before. So data that the check in place takes is checked as before, at its speed. In the value of a loop, every such
  pointee waits for a loop: the first in a register, the others in 64 entries in the frame of the
  first loop, one above another, and one whose type has a loop lower on the path is put below the
  entries of that loop (`Loops::wait_below`). So the frames are one for each type on the path,
  whatever the data. What a loop does not take (its values, the pointees without an eager aim or
  an identity, those past the entries) is checked in place, a recursion that the parts of the
  layouts bound (`AIM_PARTS` again in the loops, `Layout::parts`), and `Error::Limit` is where that
  nests too deep. Leaves are checked with `All` (`validate::check_whole`), so that they have one
  instance. Whether a loop runs is a constant of the checks (`Checks::LOOP`), so that a check in
  place holds no code of the loops, and the first loop is cold and never inlined
  (`validate::start`), and takes the budget of its pointee itself, so that a check that takes no
  loop holds no entry and reads nothing more: before that, the read of the budget for a handover
  that seldom happens took 8% more cycles on an `Expr` of 4 levels. `Input` is `repr(C)` with
  `budget` and `parts` apart, since one load of both waits for the two stores that wrote them.
  `benches/checks.rs` times it, with wide and deep data. A loop takes two pointees for one type
  where a proof of an aim does (`PointeeType`, `Aim::equals`): an identity, a size and an alignment.
  So the stack does not depend on the bytes, and stays within what an embedded target has:
  `AIM_PARTS` holds the one statement of the measured figures and how they were measured. Each
  part nests a few calls of the validation, and `tests/aims.rs` runs data of each shape past
  `AIM_PARTS` on a thread of 64 KiB, and a list of 1,000 in a `const`. A change of the validation
  that makes its frames larger is measured again. The functions on that path hold no copy of a
  layout, a field or a rule, and what reads one, or computes before the recursion, is a function
  of its own in a build without optimizations (`cfg_attr(not(debug_assertions),
  inline(always))`: "The stack of a check" in the documentation of `validate`), so that the frames
  that wait hold little. Optimized, all of it is inlined, as `examples/codegen.rs` checks. An
  assertion of a type that a check makes in `const` is an associated constant of its own
  (`validate::AimOf`, `FieldAt`, `RulesOf`), so that a refusal is reported once, whatever
  instances check it.
- `check` and `check_compact` check with `validate::Work`: every pointee of an eager aim waits in
  the worklist (`Pending`, a stage and where it applies), and `validate::drive`, one function for
  every type (its steps are function pointers, the worklist a trait object), runs them depth
  first, reversing those that each step adds, so that the parts of each value are checked before
  its pointees, in the order that `nearest` 0.8 checked an image. Once an element of a slice put a
  pointee in the worklist, the rest of the slice waits in one entry (`validate_elems`), so the
  worklist grows with the depth of the data. Before a step, the driver grows the worklist to the
  most entries that the step may add (`Layout::defers`), so no step is made twice, and a worklist
  that cannot grow stops the check where an entry does not fit. Where the check fails is recorded
  as it runs (`Input::fail`). A compact check claims each pointee where `check_stored` checks it
  (`Input::claim`), so in the order of a copy. Alignment is by offset from a base alignment there
  (`Input::aligned`), since the bytes are checked before they are copied. `Worklist` is an
  `unsafe` trait: the driver keeps its entries in the memory that it gives.

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
  (`Layout::same_extent`, `Layout::has_rule`), could be skipped at once, and the validation would
  then report which rule failed. But every check would carry that report, where it is now a `bool`
  that the optimizer folds.
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
- **More enums of the `Rust` representation assumed.** [`assume`](src/assume.rs) leaves out a
  variant whose fields have different alignments (the width of the tag follows the field that
  `rustc` puts first), a niche that ties with another one (`rustc` chooses by where they are),
  and a niche in a part whose fields `rustc` may reorder. Each would need the choice of `rustc`
  measured and stated as part of the assumption.
- **Patterns other than `str` as types.** The derive cannot make a type that guarantees a
  pattern, since safe code can write its fields. A rule `matches(DFA, start, end)` (`Op::Matches`)
  requires one instead. Two automata are equal only if their states and edges are: their `id`,
  which a user can choose, only tells different ones apart early.
