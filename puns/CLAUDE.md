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
  equal only if they require and guarantee the same values.
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
  layout, so a new type whose functions the derive calls goes there. `fakecore` in `puns-model`
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
- An aim (`layout::Aim`) is where its pointee starts (`Start`: stored, or the cursor of the
  tail), how large it is (`Extent`: its own size, a given size, or the rest), and when it is
  checked (`Check`), and every combination is checked and read. An eager aim is checked with the
  struct that holds it, its pointee within the bytes that the check reads (`validate::Input`), and
  code reads the pointee without a check. So no function makes a value that holds one
  (`Plan::eager`), gives a `&mut` to or from one (`Plan::eager_mut`), or checks one from a source
  whose bytes may be padding, and only the same aim at the same place proves one (`Aim::equals`,
  with a known identity of the pointee). The derive gives an eager aim only to a struct whose field
  of the aim (the offset of a pointer, or the bytes of eager regions) is an `InPlace<T>`, which no
  code makes and which is not `Pun`, and checks that the field has this type (`item` in `homes`),
  so that no safe code makes a value of the struct either. An `InPlace<T>` is a `T` in the layout.
  A lazy aim requires of the struct only that its pointee fits, so its type counts in a proof only
  where the size is its own.
- Both kinds of start are one rule (`Rule::Aim`), checked in one place (`validate::check_aim`),
  which places the pointee by one step of the cursor (`validate::cursor_step`) or at a stored place
  (`validate::stored`), written by the derive in one place (`aim_rule` in `puns-derive`), which
  `pointer(..)` and `regions(..)` enter. What reads a pointee finds it as the check does: the same
  steps of the cursor, which truncate where the check fails (`validate::region_at`), or the same
  expression (`validate::pointer_at`). A struct may have both, and the pointee of a region may hold
  eager aims, which its read looks for in the bytes of the struct (`tests/aims.rs`).
- The read of the pointee of an eager aim (`aim`, `aim_mut`, the methods of
  `pointer(name: ..)` and of eager regions) is `unsafe` and stays so: the derive is safe, so the
  type of a value does not tell that `puns` checked it. Its `# Safety` is the contract of a writer
  such as `nearest`. The read of a pointer takes a pointer, not a reference, so that a caller under
  stacked borrows can give the provenance of the checked bytes. The length of a pointee of its own
  size is read from its sized fields only, by the check and by the read (`validate::stores_length`,
  `validate::thin`), so the read never meets a length that the check accepted and it cannot read.
- A layout cannot hold itself, so an aim holds the size, the alignment and the identity of its
  pointee, and the pointee is reached through `Pun::Rules` (`Rules::Pointee`), whose item a read
  names, so that the type of a pointee is written in one place of the derive. The identity of a
  type is `Pun::ID`, made of the identities of its parameters and never of a layout, which its
  layout holds too. The pointee is a `Pointee`, a sealed trait: a `Pun` type, or a `Thin<T, M>`,
  the format of `nearest` for a list or a text, a `T` right after an `M` that ends with its
  length, at which the pointer points. `Pun::Metadata` is a sealed `Metadata`, `()` exactly when
  the layout is sized (`Layout::measured` asserts it of every struct).
- The check of eager aims recurses once for each level, up to `AIM_DEPTH`, which is set so that
  the deepest check of the types of `tests/support/aims.rs` fits in 1 MiB of stack in a build
  without optimizations (`tests/aims.rs` runs it on a thread of 1 MiB). A change of the validation
  that makes its frames larger is measured again, and the bound lowered if it no longer fits. What
  a function on that path computes before it recurses is in a function of its own in such a build
  (`check_next`, `find_stored`, `fits_stored`), so that the frames that wait hold little.
- `check` goes deeper with the same code: `Input` has a worklist, in which `check_aim` puts a
  `Pending` (the struct and `check_stored` for its aim) instead of recursing, and
  `validate::drive` runs the entries depth first, reversing those that each step adds. A step that
  fills the worklist is made again from a copy of `Input` once `Worklist::grow` made room. A
  compact check claims each pointee where `check_stored` checks it (`Input::claim`), so in the
  order of a copy. Where a check fails is recorded only by `validate::Locate`, a `Checks` whose
  `LOCATE` is true, and `check` makes the check again with it after a failure, so the check that
  succeeds compiles as before. Alignment is by offset from a base alignment there
  (`Input::aligned`), since the bytes are checked before they are copied.

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
- **More enums of the `Rust` representation assumed.** [`assume`](src/assume.rs) leaves out a
  variant whose fields have different alignments (the width of the tag follows the field that
  `rustc` puts first), a niche that ties with another one (`rustc` chooses by where they are),
  and a niche in a part whose fields `rustc` may reorder. Each would need the choice of `rustc`
  measured and stated as part of the assumption.
- **Patterns other than `str` as types.** The derive cannot make a type that guarantees a
  pattern, since safe code can write its fields. A rule `matches(DFA, start, end)` (`Op::Matches`)
  requires one instead. Two automata are equal only if their states and edges are: their `id`,
  which a user can choose, only tells different ones apart early.
- **The depth of the check of aims.** It recurses once for each level of eager aims, up to
  `AIM_DEPTH`, with 12 to 15 KiB of stack for each level in a build without optimizations, and
  less than 100 bytes in an optimized one. A stack of pending targets, as `nearest` has, would
  lift the bound, but needs memory that `const fn` code cannot allocate, or a bound of its own.
