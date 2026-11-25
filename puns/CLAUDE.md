# `puns`

The library: `#![no_std]`, no dependency but the derive.

## Where the design is written

- Each chapter of the reference is a file of [`docs/`](../docs), included by
  `src/reference/<chapter>.rs`, which only documents and imports the crate under `#[cfg(doc)]` for
  its links.
- The derive page, on `pub use puns_derive::Pun` in [`src/lib.rs`](src/lib.rs), states each
  attribute, [The language of rules](../docs/rules.md) what a rule means and reads, and
  [What the derive trusts](../docs/trust.md) what the derive trusts.
- `Scalar`, in [`src/lib.rs`](src/lib.rs), states the contract of a scalar declared by hand, and
  [Aims](../docs/aims.md) what `puns` guarantees a type that holds an eager aim, such as a
  self-relative pointer, which derives `Pun` with `#[pun(pointer(..))]`.
- [Unspecified layouts](../docs/layouts.md) states the design of the `unstable` feature and what
  `puns` assumes, and [What Rust documents](../docs/rust.md#what-rust-documents) lists what Rust
  documents and where.
- The invariants that cross modules are in [Soundness](../docs/soundness.md),
  [Layout independence](../docs/layouts.md#layout-independence),
  [Requirements](../docs/concepts.md#requirements), `Scalar`, the "Safety" of `Pun`, and
  [What the derive trusts](../docs/trust.md).
- What is not done yet is in [Not yet](../docs/limits.md#not-yet).

`cargo doc --no-deps -p puns --document-private-items` renders the private modules, but `view` is
`#[doc(hidden)]`, so read its documentation in the source.

- [`analysis`](src/analysis.rs) compares two layouts at compile time: whether a pun fits, which
  values are valid, what must be checked, and whether a `&mut` is allowed, a section each.
- [`validate`](src/validate.rs) checks at run time what the marks of the analysis name. It recurses
  over the types, not the values, and each operation of a rule is a constant of a function of its
  own, applied to `layout::Machine`, so no interpreter chooses operations at run time.
- [`narrow`](src/narrow.rs) computes the range of every value of a rule, so that each operation
  runs in a register where it fits, with the same results as on `u128`.
- [`view`](src/view.rs) decides each variant of a view enum once, from its base: the first later
  variant to which `pun_ref` of the variant is proven, and from which `try_pun_ref` to it compiles.
- [`discover`](src/discover.rs) finds the layouts of enums of the `Rust` representation by
  experiments, and names the facts of the compiler that they rely on.
  [`std_enums`](src/std_enums.rs) applies it to `Option` and `Result`. [`assume`](src/assume.rs)
  computes the layout that `rustc` gives such an enum from the layouts of its fields, where that is
  the same in every layout that Rust allows, and compares it with what the experiments found.
- [`ptr`](src/ptr.rs) splits and builds wide pointers in `const`, verified for each type.

## Invariants of the assumed layouts and of aims

The other invariants that cross modules are in the chapters that the first section names.

- The layout of an enum of the `Rust` representation that `assume` gives counts as the same in
  every layout that Rust allows (`layout::Stable`), because it is a function of the layouts of the
  fields, and a `const` assertion compares it with what `rustc` chose. The scope of the assumption
  admits only enums whose layout that function gives the same way in every layout of their
  fields, and nothing outside it is assumed. Where `Spec::validity` is false, a scalar gives no
  niche that `assume` knows from its own values. It is set only for the closed list of types whose
  `Spec` is what Rust allows (`bool`, `char`, `NonZero`, derived enums without fields), since only
  `core` and `std` make niches. A scalar declared by hand gives the niche of the type that its
  bytes are, if its `Spec` declares one (`Spec::stored_as`). `Layout::assumed` returns the layout
  that the experiments found, never the predicted one, so a false declaration only fails the
  build.
- Each rule of a struct is a leaf of its own (`Layout::rule_leaf`), proven by the same rule of a
  struct of the same extent at the same place, after the same aims at the cursor if it is one, and
  checked on its own otherwise.
- An identity (`layout::Id`, `Pun::ID`) has two uses only: proving a part that is not `fixed` by
  the same type at the same place, and naming a type without its layout.
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
- The functions that check in place (`from_head` and the others) check a pointee within the check
  of the struct whose aim leads to it, a recursion that the parts of the layouts of the pointees
  bound (`AIM_PARTS`, `Layout::parts`), so that their stack does not depend on the bytes, and stays
  within what an embedded target has: `AIM_PARTS` holds the one statement of the measured figures
  and how they were measured. Each part nests a few calls of the validation, and `tests/aims.rs`
  runs the deepest checks of the shapes that nest the most on a thread of 64 KiB. A change of the
  validation that makes its frames larger is measured again. The functions on that path hold no
  copy of a layout, a field or a rule, and what reads one, or computes before the recursion, is a
  function of its own in a build without optimizations (`cfg_attr(not(debug_assertions),
  inline(always))`: "The stack of a check in place" in the documentation of `validate`), so that
  the frames that wait hold little. Optimized, all of it is inlined, as `examples/codegen.rs`
  checks.
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

## Commands

The commands that run the tests both ways, the record of verdicts under a seed, and Miri on each
target are in [Commands](../docs/testing.md#commands). These are the others:

```sh
cargo test --test api                # Run only one test file of tests/ (also: bytes, parts, enums, ethernet, str, aims)
cargo test --test api TESTNAME       # Run a single test
cargo test --test compile_fail       # Compile-fail tests (trybuild); TRYBUILD=overwrite to regenerate .stderr
cargo run --example ethernet         # The Ethernet example; it asserts its results and prints `ok`
PUNS_BLESS=1 cargo test -p puns --features verdict --test seeds  # Write the record of verdicts again (`puns/tests/seeds.txt`)
cargo build -p puns --target thumbv7em-none-eabihf  # A target without std (CI also builds msp430-none-elf, see docs/target.md)
cargo rustc --release --features unstable --example codegen -- --emit asm -C llvm-args=-x86-asm-syntax=intel  # Machine code, see examples/codegen.rs
cargo bench -p puns --features unstable --bench checks   # Time of the run-time checks, next to the same checks by hand (not in CI)
cargo bench -p puns --features unstable --bench checks -- ipv6  # Only the cases whose name has `ipv6`
```

## Tests

Most of what the tests and CI run is without the `unstable` feature, the default: every test
that builds without it (`tests/seeds.rs` needs `verdict`), the cases of `compile-fail-kept`
included. Only what is about the feature runs with it: the targets of
`required-features = ["unstable"]`, the files with tests under `#[cfg(feature = "unstable")]`
(`.github/tests.sh` lists both), and the cases of `compile-fail-unstable`. A test that does not
compile without the feature is one of these, or relies on an unspecified layout. A change of the
analysis that changes a verdict of `tests/seeds.txt` is written again with `PUNS_BLESS=1` and read,
since the record must be the same under every seed (CI runs three).

The trybuild test has passing cases (`tests/compile-pass/` and `tests/compile-pass-kept/`), so that
trybuild builds instead of checking. Miri ignores it. Generate its `.stderr` files with the pinned
stable.

Each Miri run of the tests is six jobs in CI. Without the feature, there are four parts of about
the same time: three name their files, and the fourth takes all the remaining files. With the
feature, the files that test it are split in two.

What `unsafe` code assumes of the target is run under Miri on a target where the assumption does
not hold ([Miri](../docs/testing.md#miri)). A new `unsafe` block that reads a type at an address
comes with the test that stresses it (an over-aligned type, an integer wider than `isize`, the
other byte order).

`Layout::PIN` fixes the types whose functions make a layout, and `Layout::PIN_AIMS` those of the
rule of an aim, so a new type whose functions the derive calls goes there. The cases `crate_path_*` of `tests/compile-fail` test that no path of
`#[pun(crate = ..)]` replaces one, and `fakecore` in `puns-model` that `::core` does not.

The self-relative pointer of the tests (`Near`, in `tests/support/aims.rs`) derives `Pun` with an
eager aim through the public API only, as another crate would declare it. The compile-fail cases
include it by `#[path]`.

## Measurements

Do not let the machine code of `examples/codegen.rs` regress: compare it with the table of its
module documentation before and after a change of the library.

Run `benches/checks.rs` on the tree before and the tree after a change of the evaluator or the
validators, one right after the other. The machine should be idle, since its load changes the
times. The benchmark takes minutes to build (release, LTO), and is not a check of CI. With
`PUNS_BENCH_RUNS=1000` and `PUNS_BENCH_SIDE=puns` (or `hand`), it runs a case that many times
instead, for `valgrind --tool=callgrind --toggle-collect='*checks*runs*'`, which gives a count of
instructions per check that a loaded machine does not change.
