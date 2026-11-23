# The guide

The [crate page][crate] shows what `puns` is for. This guide explains how it works, in an order in
which each chapter uses only what the earlier ones define.

To use `puns`, read the first four chapters, then the next three when you need them:

1. [How `puns` decides][concepts]: sources and targets, layouts and rules, and why a pun is
   proven, checked or refused.
2. [Choosing a function][functions]: which function to call for a value or for bytes, and the
   conditions under which each one compiles.
3. [Describing a format][formats]: the attributes of the derive, from a header and its data to a
   tag that selects a payload.
4. [Compile-time errors]: how to read a refusal, and how to fix each one.
5. [Unspecified layouts and the `unstable` feature][layouts]: what Rust leaves to the compiler,
   and what `puns` refuses or accepts because of it.
6. [The language of rules][rules]: the exact meaning of the expressions that the attributes take.
7. [Limits, and what is not done yet][limits]: the limits of Rust and of the analysis, and what is
   planned.

[Measured performance][performance] gives the machine code and the time of the checks, and the
build time of the analysis.

To audit `puns`, read the first chapter, then these:

1. [Soundness]: what a pun guarantees, and what `puns` assumes to guarantee it.
2. [What the derive trusts][trust]: what the generated code relies on in the crate that derives
   it, where safe code can redefine any path.
3. [What `puns` relies on][rust]: what Rust documents of layouts, and the behaviors of `rustc` that
   no documentation states.
4. [Testing]: the tests, and what each one checks.
5. [Rust version and targets][target]: why `puns` needs Rust 1.99, and what it assumes of a target.

[crate]: ../README.md
[concepts]: concepts.md
[functions]: functions.md
[formats]: formats.md
[Compile-time errors]: errors.md
[layouts]: layouts.md
[rules]: rules.md
[limits]: limits.md
[performance]: performance.md
[Soundness]: soundness.md
[trust]: trust.md
[rust]: rust.md
[Testing]: testing.md
[target]: target.md
