//! `cargo xtask build-time`: the times of the section "Build time" of `docs/performance.md`.
//!
//! Each case is a crate written in `target/build-time`, which depends on `puns` by path. It is
//! built once, then three times after its source is touched, and the best of the three is printed
//! in seconds. Only that crate is built again: `puns` and the derive are not. The builds are not
//! incremental, since an incremental build would reuse the results of the previous one and measure
//! almost nothing.

use std::{
  fmt::Write as _,
  fs::{self, File},
  path::Path,
  process::Command,
  time::{Duration, Instant, SystemTime},
};

use anyhow::{Context, Result, ensure};

/// What the crate of a case holds, for a number `n`.
#[derive(Clone, Copy)]
enum Case {
  /// `n` structs that derive `Pun`, and a function that reads none of them.
  Types,
  /// The same structs, and a function that reads each one from bytes: `n` pairs of types.
  Pairs,
  /// A view enum of `n` variants, each a struct that starts with a different `u8c`.
  Distinct,
  /// A view enum of `n` variants, the struct of variant `i` starting with a `u8in<0, i>`, so that
  /// each variant takes every value of the variants before it.
  Overlapping,
}

impl Case {
  const fn name(self) -> &'static str {
    match self {
      Self::Types => "types",
      Self::Pairs => "pairs",
      Self::Distinct => "distinct",
      Self::Overlapping => "overlapping",
    }
  }

  /// Returns the source of the crate of the case.
  fn source(self, n: usize) -> String {
    let mut source = String::from(
      "#![allow(unused_imports, unused_mut, unused_variables)]\n\
       use puns::{Pun, num::{u8c, u8in, u16le}};\n\
       #[derive(Pun, Clone, Copy)] #[repr(u8)] pub enum Mode { Off = 0, On = 1, Auto = 2 }\n",
    );
    match self {
      Self::Types | Self::Pairs => {
        for i in 0..n {
          let (tag, len) = (i % 256, 4 + i % 7);
          writeln!(
            source,
            "#[derive(Pun)] #[repr(C)] pub struct T{i} {{ pub tag: u8c<{tag}>, pub mode: Mode, \
             pub ok: bool, pub len: u16le, pub data: [u8; {len}] }}"
          )
          .unwrap();
        }
        source.push_str("pub fn run(b: &[u8]) -> usize {\n  let mut n = 0;\n");
        if matches!(self, Self::Pairs) {
          for i in 0..n {
            writeln!(source, "  n += usize::from(puns::from_bytes::<T{i}>(b).is_ok());").unwrap();
          }
        }
        source.push_str("  n\n}\n");
      }
      Self::Distinct | Self::Overlapping => {
        let mut variants = String::new();
        for i in 0..n {
          let first = if matches!(self, Self::Distinct) {
            format!("u8c<{i}>")
          } else {
            format!("u8in<0, {i}>")
          };
          writeln!(
            source,
            "#[derive(Pun)] #[repr(C)] pub struct V{i} {{ pub x: {first}, pub mode: Mode, \
             pub len: u16le, pub data: [u8] }}"
          )
          .unwrap();
          write!(variants, " A{i}(&'a V{i}),").unwrap();
        }
        writeln!(source, "#[derive(Pun)] pub enum View<'a> {{{variants} }}").unwrap();
        source
          .push_str("pub fn run(b: &[u8]) -> usize { usize::from(View::from_bytes(b).is_ok()) }\n");
      }
    }
    source
      .push_str("fn main() { println!(\"{}\", run(&std::env::args().count().to_ne_bytes())); }\n");
    source
  }
}

/// Times each case, in debug and in release, and prints one line for each.
pub fn run(root: &Path) -> Result<()> {
  let dir = root.join("target").join("build-time");
  fs::create_dir_all(dir.join("src"))?;
  // A literal string of TOML, which takes the path as it is, separators included.
  let puns = root.join("puns");
  fs::write(
    dir.join("Cargo.toml"),
    format!(
      "[package]\nname = \"build-time\"\nedition = \"2024\"\n\n[dependencies]\npuns = {{ path = \
       '{}' }}\n\n[workspace]\n",
      puns.display()
    ),
  )?;
  let mut cases = Vec::new();
  for case in [Case::Types, Case::Pairs] {
    cases.extend([100, 200].map(|n| (case, n)));
  }
  for case in [Case::Distinct, Case::Overlapping] {
    cases.extend([4, 8, 16, 32, 64].map(|n| (case, n)));
  }
  for release in [false, true] {
    let profile = if release { "release" } else { "debug" };
    for &(case, n) in &cases {
      let best = time(&dir, case, n, release)?;
      println!("{} {n} {profile} {:.2}", case.name(), best.as_secs_f64());
    }
  }
  Ok(())
}

/// Returns the best of three builds of the crate of `case`.
fn time(dir: &Path, case: Case, n: usize, release: bool) -> Result<Duration> {
  let main = dir.join("src").join("main.rs");
  fs::write(&main, case.source(n))?;
  build(dir, release)?;
  let mut best = Duration::MAX;
  for _ in 0..3 {
    // A newer modification time makes Cargo build the crate again, from the same source.
    File::options().write(true).open(&main)?.set_modified(SystemTime::now())?;
    let start = Instant::now();
    build(dir, release)?;
    best = best.min(start.elapsed());
  }
  Ok(best)
}

fn build(dir: &Path, release: bool) -> Result<()> {
  let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
  let mut command = Command::new(cargo);
  command.current_dir(dir).args(["build", "--quiet"]).env("CARGO_INCREMENTAL", "0");
  if release {
    command.arg("--release");
  }
  let status = command.status().context("cannot run cargo")?;
  ensure!(status.success(), "the build in {} failed", dir.display());
  Ok(())
}
