//! Checks that the files of the parts of a check cut along its sources keep each source once
//! between them.
//!
//! A check of `tests/support/NAME.rs` runs in the files `NAME_1.rs` to `NAME_n.rs`, each of which
//! calls its `pairs!` with `n` marks (`puns_model::sources!`). The file `NAME_k.rs` must mark the
//! `k`-th source of every `n`, and none other, so that the `n` files keep every source once: a
//! missing file, a file too many, or marks that keep a source twice or never fail here.
//!
//! This file needs the `full` feature, which `cargo test` does not turn on. It runs with
//! `cargo test --profile model -p puns-model --features full`, every night and on demand
//! (`.github/workflows/model.yml`).

use std::{collections::BTreeMap, fs, path::Path};

#[test]
#[cfg_attr(miri, ignore = "reads the files of the tests, which Miri isolates")]
fn every_source_once() {
  let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
  let mut parts: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
  for entry in fs::read_dir(tests.join("support")).unwrap() {
    let path = entry.unwrap().path();
    parts.insert(path.file_stem().unwrap().to_str().unwrap().to_owned(), Vec::new());
  }
  for entry in fs::read_dir(&tests).unwrap() {
    let path = entry.unwrap().path();
    let text = fs::read_to_string(&path).unwrap_or_default();
    let Some((_, after)) = text.split_once("#[path = \"support/") else {
      continue;
    };
    let name = after.split_once(".rs\"]").unwrap().0;
    let (_, call) = text.split_once("pairs!(").unwrap();
    let marks = call.split_once(')').unwrap().0;
    let file = path.file_stem().unwrap().to_str().unwrap().to_owned();
    parts.get_mut(name).unwrap().push((file, marks.to_owned()));
  }
  for (name, files) in parts {
    let n = files.len();
    assert!(n > 0, "no file runs the check of `support/{name}.rs`");
    for (file, marks) in files {
      let k: usize = file
        .strip_prefix(&format!("{name}_"))
        .and_then(|k| k.parse().ok())
        .unwrap_or_else(|| panic!("`{file}.rs` runs `support/{name}.rs` but is not `{name}_k.rs`"));
      let expected: Vec<&str> = (1..=n).map(|i| if i == k { "x" } else { "_" }).collect();
      assert_eq!(
        marks.split_whitespace().collect::<Vec<_>>(),
        expected,
        "`{file}.rs`, one of {n} files of `support/{name}.rs`, keeps the source {k} of every {n}"
      );
    }
  }
}
