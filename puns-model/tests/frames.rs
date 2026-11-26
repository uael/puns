//! The frames of evaluation of a check in a `const`, as `rustc` counts them, against what the
//! validation counts (`puns::hook::frames`).
//!
//! Each constant of the crate `frames` (`frames/src/lib.rs`) checks data of a shape and a depth
//! after a number of frames of its own, its pad. The test builds that crate with pads, in a target
//! directory of its own, and reads which constants fail for the frames: from the pads that do not
//! fail, the most that each constant takes, so the frames of its check.
//!
//! This file is not built under Miri: it runs `cargo`.
#![cfg(not(miri))]

use std::{collections::BTreeMap, path::PathBuf, process::Command};

use frames::{Array, Chain, Fields, Nest, Prefixed, Region, Ruled, Variants, Worst};
use puns::{Thin, hook::Frames};

/// The frames that Rust allows a `const` by default.
const LIMIT: usize = 128;

/// The frames of a constant of `frames` before its check: the constant, its function, and the
/// first call of its pad.
const OWN: usize = 3;

/// A constant of `frames`: its name, its shape, its depth, and its line in the source.
struct Case {
  name: String,
  shape: String,
  depth: usize,
  line: usize,
}

/// The constants of `frames`, in the order of their pads.
fn cases() -> Vec<Case> {
  let source = include_str!("../frames/src/lib.rs");
  let mut cases = vec![];
  for (i, line) in source.lines().enumerate() {
    let Some(rest) = line.strip_prefix("pub const ") else { continue };
    let (name, call) = rest.split_once(": () = ").expect("a constant of a check");
    let (shape, args) = call.split_once('(').expect("a call");
    let (case, depth) = args.trim_end_matches(");").split_once(", ").expect("two arguments");
    assert_eq!(case.parse::<usize>().unwrap(), cases.len(), "{name} is not in order");
    let depth = depth.parse().unwrap();
    cases.push(Case { name: name.into(), shape: shape.into(), depth, line: i + 1 });
  }
  cases
}

/// Builds `frames` with `pads` and returns which constants fail for the frames, by index.
fn failing(cases: &[Case], pads: &[usize]) -> Vec<bool> {
  let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
  let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
  let pads = pads.iter().map(ToString::to_string).collect::<Vec<_>>().join(",");
  let output = Command::new(cargo)
    .current_dir(&root)
    .args(["check", "--quiet", "--color", "never", "-p", "frames", "--message-format", "short"])
    .arg("--target-dir")
    .arg(root.join("target/frames"))
    .env("PUNS_FRAMES_PADS", &pads)
    .output()
    .expect("cargo runs");
  let stderr = String::from_utf8_lossy(&output.stderr);
  let lines = cases.iter().enumerate().map(|(i, case)| (case.line, i)).collect::<BTreeMap<_, _>>();
  let mut fails = vec![false; cases.len()];
  for message in stderr.lines() {
    if message.starts_with("error: could not compile") || !message.contains("error") {
      continue;
    }
    let at = message.split_once("frames/src/lib.rs:").and_then(|(_, rest)| {
      rest.split(':').next()?.parse::<usize>().ok().and_then(|line| lines.get(&line))
    });
    match at {
      Some(&i) if message.contains("stack frames") => fails[i] = true,
      _ => panic!("pads {pads}: {stderr}"),
    }
  }
  assert_eq!(output.status.success(), !fails.contains(&true), "pads {pads}: {stderr}");
  fails
}

/// What the validation counts for the pointee of each shape.
fn model(shape: &str) -> Frames {
  use puns::hook::frames;
  match shape {
    "nest" => frames::<[Nest]>(),
    "fields" => frames::<[Fields]>(),
    "variants" => frames::<[Variants]>(),
    "array" => frames::<[Array]>(),
    "prefixed" => frames::<Thin<[Prefixed], u8>>(),
    "ruled" => frames::<[Ruled]>(),
    "chain" => frames::<Chain>(),
    "region" => frames::<[Region]>(),
    "worst" => frames::<[Worst]>(),
    _ => panic!("no shape {shape}"),
  }
}

#[test]
fn the_frames_of_a_check_are_those_that_the_validation_counts() {
  let cases = cases();
  // The most pads that do not fail, between `low` and `high`, by bisection.
  let at_zero = failing(&cases, &vec![0; cases.len()]);
  let (mut low, mut high) = (vec![0; cases.len()], vec![LIMIT - OWN; cases.len()]);
  while low.iter().zip(&high).any(|(low, high)| low < high) {
    let pads =
      low.iter().zip(&high).map(|(low, high)| (low + high).div_ceil(2)).collect::<Vec<_>>();
    for (i, fails) in failing(&cases, &pads).into_iter().enumerate() {
      if low[i] < high[i] {
        if fails { high[i] = pads[i] - 1 } else { low[i] = pads[i] }
      }
    }
  }
  // The frames that each check takes, from `from_head`, by shape and depth.
  let mut taken = BTreeMap::<&str, BTreeMap<usize, usize>>::new();
  let (mut table, mut wrong, mut most) = (String::new(), vec![], 0);
  for (i, case) in cases.iter().enumerate() {
    let frames = LIMIT - OWN - low[i];
    let shown = if at_zero[i] { "fails".into() } else { frames.to_string() };
    table += &format!("{} {shown}\n", case.name);
    if at_zero[i] {
      wrong.push(format!("{} passes the frames of a `const`", case.name));
    }
    taken.entry(&case.shape).or_default().insert(case.depth, frames);
  }
  for (shape, taken) in &taken {
    let Frames { level, looped, deepest, nested, reserved, caller, .. } = model(shape);
    table += &format!("{shape}: {:?}\n", model(shape));
    // Below the value at the top, each value in place takes `level` frames more than the one that
    // points at it, the value of the loop `looped`, and those after it none, since the loop checks
    // them one after another.
    let start = |depth: usize| match depth {
      _ if depth <= nested => depth * level,
      _ => nested * level + looped,
    };
    for (&depth, &frames) in taken.range(2..) {
      let expected = taken[&1] + start(depth) - start(1);
      if frames != expected {
        wrong.push(format!("{shape} at {depth}: {frames} frames, {expected} counted"));
      }
    }
    // What the check takes past what `in_place` counts at most, within `RESERVED` with the frames
    // of its caller.
    let past = taken[&40].saturating_sub(nested * level + looped + deepest);
    if past + caller > reserved {
      wrong.push(format!("{shape}: {past} frames past those counted, {reserved} reserved"));
    }
    most = most.max(past);
  }
  let reserved = model("nest").reserved - model("nest").caller;
  if most != reserved {
    wrong.push(format!("at most {most} frames past those counted, {reserved} reserved"));
  }
  assert!(wrong.is_empty(), "{}\n{table}", wrong.join("\n"));
}
