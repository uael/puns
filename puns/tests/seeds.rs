//! Tests that, without the `unstable` feature, the verdicts of `puns` do not depend on the layout
//! that `rustc` chooses where Rust leaves the layout to the compiler.
//!
//! The record holds the verdict of every function on each pair of a set of types, and which byte
//! patterns each type accepts. Rust specifies some layouts of these types and leaves others to
//! the compiler. The record holds no message of a refusal, because a message may name what
//! `rustc` chose. The cases of `compile-fail-kept` hold such messages, for the layout of this
//! build.
//!
//! The record must equal `seeds.txt`. CI checks this without `-Zrandomize-layout` and with it,
//! under several seeds. `PUNS_BLESS=1` writes the record to `seeds.txt`. The record is the one of
//! `x86_64`, because the alignment of `u16` and the width of `usize` depend on the target.
//!
//! The test runs only without the `unstable` feature, with
//! `cargo test -p puns --features verdict --test seeds`. With the `unstable` feature, `puns` uses
//! the layout that `rustc` chose, which is what the seeds change. The test needs the `verdict`
//! feature for `puns::hook`, as the model check does.
//!
//! This file is not built under Miri. It runs no unsafe code of its own, and the other tests of
//! `puns` reach the same code of the library.

#![cfg(not(miri))]
#![cfg(not(feature = "unstable"))]

use core::{any::type_name, fmt::Write as _, num::NonZero};

use puns::{Pun, Scalar, hook, layout::Spec};

/// An opaque scalar, which a pun reads as any initialized bytes but never makes, as an offset of
/// another crate would be.
#[derive(Clone, Copy)]
#[repr(transparent)]
struct Sealed(u32);

// SAFETY: `Sealed` is `repr(transparent)` over a `u32`, so it has its size, and every value is 4
// initialized bytes, each a valid `Sealed` in Rust's sense, which `Spec::any` accepts. Whether it is
// correct is not in its bytes: `SPEC` is opaque, and not a number, so that no rule reads it. It
// holds no `UnsafeCell`.
unsafe impl Scalar for Sealed {
  const SPEC: Spec = Spec::any::<Sealed>().opaque().not_a_number();
}

/// A struct of the `Rust` representation. Rust specifies its size and that its bytes are
/// initialized, but leaves to the compiler which byte is its `bool`.
#[derive(Pun, Clone, Copy)]
struct Mixed {
  a: u8,
  b: bool,
}

/// A struct of the `Rust` representation with two fields of the same type. Every byte is a `bool`,
/// whichever order the compiler chooses.
#[derive(Pun, Clone, Copy)]
struct Twins {
  a: bool,
  b: bool,
}

/// A struct of the `Rust` representation with a rule. Its bytes can hold any value, but the rule
/// depends on which field each byte is.
#[derive(Pun, Clone, Copy)]
#[pun(assert(a < b))]
struct Ordered {
  a: u8,
  b: u8,
}

/// A `repr(C)` struct with a rule, whose fields are where `repr(C)` puts them.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(a < b))]
struct Ruled {
  a: u8,
  b: u8,
}

/// A struct of the `Rust` representation that holds a struct with a rule, at offset 0 or 1.
#[derive(Pun, Clone, Copy)]
struct Carries {
  ruled: Ruled,
  flag: bool,
}

/// A `repr(C)` struct with the fields of `Carries`. The rule of `Ruled` is at offset 0.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct InOrder {
  ruled: Ruled,
  flag: bool,
}

/// An enum whose representation Rust specifies, which holds a struct whose layout Rust leaves to
/// the compiler.
#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Wraps {
  A(Mixed),
  B(u8),
}

/// A union that holds a struct whose layout Rust leaves to the compiler.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Either {
  mixed: Mixed,
  bytes: [u8; 2],
}

/// A `repr(C)` struct that holds a struct whose layout Rust leaves to the compiler.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Holds {
  mixed: Mixed,
  word: u16,
}

/// An enum with fields, of the `Rust` representation. Rust leaves its tag to the compiler.
#[derive(Pun, Clone, Copy)]
enum Tagged {
  A(u8),
  B(bool),
}

/// An enum without fields, of the `Rust` representation. Rust leaves the bit patterns of its
/// variants to the compiler.
#[derive(Pun, Clone, Copy)]
enum Plain {
  X,
  Y,
}

/// The enum `Plain` with `repr(u8)`, whose layout Rust specifies.
#[derive(Pun, Clone, Copy)]
#[repr(u8)]
enum Kept {
  X,
  Y,
}

/// The values that a byte takes in the patterns. They are the bounds of `bool`, of `NonZero` and
/// of the tags.
const BYTES: [u8; 5] = [0, 1, 2, 0x7f, 0xff];

/// Bytes aligned for every type of the record.
#[repr(align(16))]
struct Aligned([u8; 16]);

/// Returns the line of the record with the verdict of every function on a pun from `S` to `D`.
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
  ];
  let flags: String = flags.iter().map(|&ok| if ok { '+' } else { '-' }).collect();
  format!("{} -> {}: {flags}\n", type_name::<S>(), type_name::<D>())
}

/// Returns the line of the record that tells which byte patterns of the size of `D` it accepts
/// from bytes (`try_pun_ref`).
///
/// The line holds how many patterns `D` accepts and a hash of which. If `puns` refuses to read a
/// `D` from bytes, the line says so.
fn accepts<D: Pun>() -> String {
  let name = type_name::<D>();
  if !hook::verdict::<[u8], D>().try_pun_ref {
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
    let tried = hook::try_pun_ref::<[u8], D>(&buffer.0[..size]);
    let accepted = matches!(tried, Some(Ok(_)));
    if accepted {
      count += 1;
      hash = (hash ^ index as u64).wrapping_mul(0x0000_0100_0000_01b3);
    }
  }
  format!("[u8] -> {name}: {count} of {total} ({hash:016x})\n")
}

/// Returns the record: a line for every ordered pair of the types, then a line for what each type
/// accepts.
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
    Sealed,
    (u8, Sealed),
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
