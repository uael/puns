# `puns-derive`

`#[derive(Pun)]`, re-exported as `puns::Pun`.

`puns` documents it, on `pub use puns_derive::Pun` in [`puns/src/lib.rs`](../puns/src/lib.rs): what
it implements, each attribute and its conditions, and where each error is reported, which
[Errors of the derive](../docs/errors.md#errors-of-the-derive) shows. Read that page before changing
the derive, and keep it true. What the code that the derive generates trusts, and what it does not,
is in [What the derive trusts](../docs/trust.md).

The derive is tested from the other crates: by the tests and the compile-fail cases of `puns`, by
the model check, and against a `core` whose items give wrong answers and an attribute that changes
the item after the derive has read it ([`puns-model/CLAUDE.md`](../puns-model/CLAUDE.md)).
