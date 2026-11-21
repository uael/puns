//! Every verdict of `puns` without the `unstable` feature, on pairs of types of which Rust keeps
//! some layouts and not others, and which bytes each type accepts: the same whatever layout
//! `rustc` chooses where Rust lets it choose. Not the message of a refusal, which may name what
//! `rustc` chose (the cases of `compile-fail-kept` hold messages, of the layout of this build).
//!
//! The record is compared with `seeds.txt`, which CI requires under `-Zrandomize-layout` with
//! several seeds, and without (`PUNS_BLESS=1` writes it). It is the record of `x86_64`: the
//! alignment of `u16` and the width of `usize` are the ones of the target.
//!
//! Without the `unstable` feature only (`cargo test -p puns --features verdict --test seeds`):
//! with it, `puns` takes the layout that `rustc` chose, which is what the seeds change. It needs
//! the `verdict` feature (`hook`), as the model check does.
//!
//! Not for Miri: it runs no unsafe code of its own, and what it reaches of the library is what
//! the other tests of `puns` reach.

#![cfg(not(miri))]
#![cfg(not(feature = "unstable"))]

use core::{any::type_name, fmt::Write as _, marker::PhantomData, num::NonZero};

use puns::{
  Pun, SelfRelative, hook,
  layout::{Endian, Pointer},
};

/// A pointer to a `T`, or none: the distance from its own address, an `i32`, or zero.
#[repr(transparent)]
struct Opt<T>(i32, PhantomData<T>);

// SAFETY: the type is its `i32`; no value of it is made here.
unsafe impl<T: Pun> SelfRelative for Opt<T> {
  type Target = T;
  const POINTER: Pointer = Pointer::one::<T>("seeds", 4, Endian::NATIVE).or_null();
}

puns::self_relative!(impl[T: Pun] Opt<T>);

/// A struct of the `Rust` representation of which Rust keeps the size and that its bytes are
/// initialized, not where its `bool` is.
#[derive(Pun, Clone, Copy)]
struct Mixed {
  a: u8,
  b: bool,
}

/// A struct of the `Rust` representation of which Rust keeps every byte: two of the same.
#[derive(Pun, Clone, Copy)]
struct Twins {
  a: bool,
  b: bool,
}

/// A struct of the `Rust` representation with a rule: bytes of any value, but the rule tells
/// which is which.
#[derive(Pun, Clone, Copy)]
#[pun(assert(a < b))]
struct Ordered {
  a: u8,
  b: u8,
}

/// A `repr(C)` struct with a rule: its fields are where `repr(C)` puts them.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(a < b))]
struct Ruled {
  a: u8,
  b: u8,
}

/// A struct of the `Rust` representation that holds one with a rule, at 0 or at 1.
#[derive(Pun, Clone, Copy)]
struct Carries {
  ruled: Ruled,
  flag: bool,
}

/// The same fields, where `repr(C)` puts them: the rule of `Ruled` is at 0.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct InOrder {
  ruled: Ruled,
  flag: bool,
}

/// An enum of a representation that Rust specifies, which holds a struct that it does not keep.
#[derive(Pun, Clone, Copy)]
#[repr(u8)]
#[expect(dead_code, reason = "a type of which only the layout matters")]
enum Wraps {
  A(Mixed),
  B(u8),
}

/// A union that holds a struct that Rust does not keep.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Either {
  mixed: Mixed,
  bytes: [u8; 2],
}

/// A `repr(C)` struct that holds a struct whose bytes Rust does not keep.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Holds {
  mixed: Mixed,
  word: u16,
}

/// An enum with fields and without `repr`: nothing is kept of its tag.
#[derive(Pun, Clone, Copy)]
enum Tagged {
  A(u8),
  B(bool),
}

/// An enum without fields and without `repr`: nothing is kept of its values.
#[derive(Pun, Clone, Copy)]
enum Plain {
  X,
  Y,
}

/// The same enum, `repr(u8)`: it is kept.
#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Kept {
  X,
  Y,
}

/// The values a byte takes in the patterns: the edges of `bool`, of `NonZero` and of the tags.
const BYTES: [u8; 5] = [0, 1, 2, 0x7f, 0xff];

/// Bytes aligned for every type here.
#[repr(align(16))]
struct Aligned([u8; 16]);

/// The verdicts of every function on a pun from `S` to `D`.
fn line<S: Pun, D: Pun>() -> String {
  let v = hook::verdict::<S, D>();
  let p = hook::parts::<S, D>();
  let flags = [
    v.pun,
    v.head,
    v.try_pun,
    v.pun_ref,
    v.head_ref,
    v.try_pun_ref,
    v.pun_mut,
    v.try_pun_mut,
    // The parts, by `&mut` and from the end.
    p.head_mut,
    p.tail,
    p.tail_ref,
    p.tail_mut,
    // With what the pointers aim at.
    hook::views::<S, D>(1),
    hook::tries_view::<S, D>(1),
  ];
  let flags: String = flags.iter().map(|&ok| if ok { '+' } else { '-' }).collect();
  format!("{} -> {}: {flags}\n", type_name::<S>(), type_name::<D>())
}

/// Which byte patterns of the size of `D` it accepts from bytes (`try_pun_ref`): how many of
/// them, and a hash of which.
fn accepts<D: Pun>() -> String {
  let name = type_name::<D>();
  if !hook::tries_view::<[u8], D>(0) {
    return format!("[u8] -> {name}: refused\n");
  }
  let size = size_of::<D>();
  let total = BYTES.len().pow(u32::try_from(size).expect("a small type"));
  let (mut count, mut hash) = (0u64, 0xcbf2_9ce4_8422_2325_u64);
  for index in 0..total {
    let mut buffer = Aligned([0; 16]);
    let mut rest = index;
    for byte in &mut buffer.0[..size] {
      *byte = BYTES[rest % BYTES.len()];
      rest /= BYTES.len();
    }
    let accepted = matches!(hook::try_view_ref::<[u8], D, 0>(&buffer.0[..size]), Some(Ok(_)));
    if accepted {
      count += 1;
      hash = (hash ^ index as u64).wrapping_mul(0x0000_0100_0000_01b3);
    }
  }
  format!("[u8] -> {name}: {count} of {total} ({hash:016x})\n")
}

/// The record of every pair of the types, then of what each accepts.
macro_rules! record {
  ($($t:ty),* $(,)?) => {{
    let mut out = String::new();
    record!(@rows out [$($t),*] $($t),*);
    $(out.push_str(&accepts::<$t>());)*
    out
  }};
  (@rows $out:ident $all:tt $($s:ty),*) => {
    $(record!(@row $out $s; $all);)*
  };
  (@row $out:ident $s:ty; [$($d:ty),*]) => {
    $($out.push_str(&line::<$s, $d>());)*
  };
}

#[test]
fn the_same_whatever_the_layout() {
  let record = record!(
    u8,
    bool,
    [u8; 2],
    [u8; 4],
    [bool; 2],
    [bool; 4],
    NonZero<u16>,
    [NonZero<u16>; 2],
    (NonZero<u16>, u16),
    (bool, [u8; 3]),
    (u8, u8),
    (bool, bool),
    (bool, u8),
    (u8, u16),
    (u8, u16, u8),
    Mixed,
    Ordered,
    Twins,
    Holds,
    Ruled,
    Carries,
    InOrder,
    Wraps,
    Either,
    [Mixed; 2],
    Opt<[u8; 2]>,
    Opt<[bool; 2]>,
    Opt<(bool, u8)>,
    Tagged,
    Plain,
    Kept,
    Option<bool>,
    Option<NonZero<u16>>,
    Result<NonZero<u8>, ()>,
  );
  let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/seeds.txt");
  if std::env::var_os("PUNS_BLESS").is_some() {
    std::fs::write(path, &record).expect("seeds.txt is written");
    return;
  }
  let expected = std::fs::read_to_string(path).expect("seeds.txt: run with `PUNS_BLESS=1`");
  let mut diff = String::new();
  for (line, (got, want)) in record.lines().zip(expected.lines()).enumerate() {
    if got != want {
      writeln!(diff, "line {}:\n  got  {got}\n  want {want}", line + 1).unwrap();
    }
  }
  if record.lines().count() != expected.lines().count() {
    writeln!(diff, "{} lines, {} expected", record.lines().count(), expected.lines().count())
      .unwrap();
  }
  assert!(diff.is_empty(), "the verdicts are not the ones of `seeds.txt`:\n{diff}");
}
