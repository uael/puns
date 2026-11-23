//! The links of `README.md` and of `docs/*.md`, pinned to the version of `puns`.
//!
//! The README is read in three places: on GitHub, on crates.io, and on docs.rs, where it is the top
//! of the crate documentation. Every link of it to docs.rs, to crates.io, or to a file of the
//! repository by its URL names the version of `puns` in `puns/Cargo.toml`, or its tag
//! `v{version}`. A link without a version would lead elsewhere: `docs.rs/puns/latest` is the
//! newest stable release, and `blob/main` is the code after it. The one exception is a branch of
//! [`BRANCHES`], which holds what no release contains, and is linked as it is.
//!
//! The chapters of the guide are files of `docs/`, which rustdoc includes as the guide's modules,
//! as it includes the README as the crate page. A link between chapters is relative, so that
//! GitHub stays in the revision being read. rustdoc does not resolve such a link, so the `.rs` file
//! that includes a Markdown file defines the same label before the include, as an intra-doc link:
//! of two definitions of a label, the first counts. A relative link of an included file is
//! therefore a definition (`[label]: target`), and the `.rs` file must define its label.
//!
//! The README has no relative link. crates.io resolves one in the directory of the package
//! (`puns/`, `puns-derive/`) on the default branch, where `docs/` is not, so the README names each
//! file of the repository by its URL at the tag, and `puns/src/lib.rs` defines the same label as
//! an intra-doc link for docs.rs.
//!
//! GitHub highlights a block of code only if its info string starts with a language, while
//! rustdoc takes a block with none for Rust. So each block of these files names its language:
//! `rust,compile_fail` rather than `compile_fail`, which rustdoc reads the same way.
//!
//! GitHub and rustdoc make the same anchor from a heading, except where rustdoc already uses that
//! `id` on the page (`fields`, `implementations` and others): it then adds `-1`, and a link that
//! works on GitHub lands at the top of the page on docs.rs. So each heading of an included file must
//! have its anchor as an `id` of the page that rustdoc wrote.
//!
//! `cargo xtask links` writes the version into the links. A release PR changes the version, and
//! the workflow `release-plz.yml` runs it on the branch of the PR. With `--check`, it fails if a
//! link is not pinned, or if its target is not there: a file of the repository, a heading of a
//! Markdown file, or a page and an `id` of the documentation that `cargo doc` wrote to
//! `target/doc`, which must be built first.

use std::{
  collections::BTreeSet,
  fs,
  path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

/// The files whose links are pinned.
const README: &str = "README.md";
const DOCS: &str = "docs";
/// The documentation that `cargo doc --no-deps` writes for `puns`.
const RUSTDOC: &str = "target/doc/puns";

/// A link that names a version: the text before the version, and the version in it, from the
/// version of `puns` (`{v}`).
///
/// The version follows the text, after a `/`, and is made of the characters of a version
/// ([`is_version`]). A link with nothing after the text (a `)` or a space) has none yet.
const PINNED: [(&str, &str); 5] = [
  ("https://docs.rs/puns", "{v}"),
  ("https://img.shields.io/docsrs/puns", "{v}"),
  ("https://crates.io/crates/puns", "{v}"),
  ("https://github.com/uael/puns/blob", "v{v}"),
  ("https://github.com/uael/puns/tree", "v{v}"),
];

/// The branches that a link names as they are, since no release contains them: `wip`, which keeps
/// the prototypes that `puns` comes from, for history.
const BRANCHES: [&str; 1] = ["wip"];

/// Images of the repository, which GitHub, crates.io and docs.rs all show from this host.
const RAW: (&str, &str) = ("https://raw.githubusercontent.com/uael/puns", "v{v}");

const fn is_version(c: char) -> bool {
  c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+')
}

pub fn run(root: &Path, check: bool) -> Result<()> {
  let version = version(root)?;
  let mut files = vec![PathBuf::from(README)];
  for entry in fs::read_dir(root.join(DOCS))? {
    let path = entry?.path();
    if path.extension().is_some_and(|e| e == "md") {
      files.push(path.strip_prefix(root)?.to_path_buf());
    }
  }
  files.sort();
  let mut faults = vec![];
  for file in &files {
    let text =
      fs::read_to_string(root.join(file)).with_context(|| format!("reading {}", file.display()))?;
    let pinned = pin(&text, &version);
    if check {
      if pinned != text {
        faults.push(format!(
          "{}: a link does not name {version}: run `cargo xtask links`",
          file.display()
        ));
      }
      targets(root, file, &text, &version, &mut faults)?;
      rendered(root, file, &text, &mut faults)?;
      for line in unnamed_blocks(&text) {
        faults.push(format!(
          "{}:{line}: a block of code names no language, so GitHub does not highlight it: \
           write `rust`, or `rust,` before what rustdoc reads (`rust,compile_fail`)",
          file.display()
        ));
      }
    } else if pinned != text {
      fs::write(root.join(file), pinned)?;
      println!("wrote {}", file.display());
    }
  }
  if !faults.is_empty() {
    bail!("{}", faults.join("\n"));
  }
  Ok(())
}

/// Returns the version of `puns`, from `puns/Cargo.toml`.
fn version(root: &Path) -> Result<String> {
  let manifest = fs::read_to_string(root.join("puns/Cargo.toml"))?;
  let line = manifest
    .lines()
    .find_map(|line| line.strip_prefix("version = \""))
    .context("puns/Cargo.toml has no `version`")?;
  Ok(line.trim_end_matches('"').to_string())
}

/// Returns `text` with the version of each link of [`PINNED`] and [`RAW`] set to `version`, except
/// for a link to a branch of [`BRANCHES`].
fn pin(text: &str, version: &str) -> String {
  let mut text = text.to_string();
  for (base, form) in PINNED.iter().chain([&RAW]) {
    let want = form.replace("{v}", version);
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(at) = rest.find(base) {
      let after = &rest[at + base.len()..];
      out.push_str(&rest[..at + base.len()]);
      match after.chars().next() {
        // A version, which this replaces.
        Some('/') => {
          let name = after[1..].split(|c| !is_version(c)).next().unwrap_or_default();
          if BRANCHES.contains(&name) {
            rest = after;
            continue;
          }
          out.push('/');
          out.push_str(&want);
          rest = &after[1 + name.len()..];
        }
        // None yet.
        None | Some(')' | ' ' | '>' | '\n') => {
          out.push('/');
          out.push_str(&want);
          rest = after;
        }
        // Another name, such as `puns-derive`.
        Some(_) => rest = after,
      }
    }
    out.push_str(rest);
    text = out;
  }
  text
}

/// Checks that the target of each link of `file` is there, and records each that is not in
/// `faults`.
fn targets(
  root: &Path,
  file: &Path,
  text: &str,
  version: &str,
  faults: &mut Vec<String>,
) -> Result<()> {
  let docs = format!("https://docs.rs/puns/{version}/puns/");
  let blob = format!("https://github.com/uael/puns/blob/v{version}/");
  let tree = format!("https://github.com/uael/puns/tree/v{version}/");
  let raw = format!("https://raw.githubusercontent.com/uael/puns/v{version}/");
  let dir = file.parent().unwrap_or_else(|| Path::new(""));
  let rustdoc = includer(root, file);
  for (label, link) in links(text) {
    let fault = |what: &str| format!("{}: {link}: {what}", file.display());
    let (path, anchor) = link.split_once('#').map_or((link.as_str(), None), |(p, a)| (p, Some(a)));
    if let Some(page) = path.strip_prefix(&docs) {
      let page = if page.is_empty() { "index.html" } else { page };
      let html = root.join(RUSTDOC).join(page);
      if !root.join(RUSTDOC).is_dir() {
        bail!("{RUSTDOC} is not there: run `cargo doc --no-deps -p puns` first");
      }
      match fs::read_to_string(&html) {
        Ok(html) => {
          if let Some(anchor) = anchor
            && !html.contains(&format!("id=\"{anchor}\""))
          {
            faults.push(fault("no such section in the documentation"));
          }
        }
        Err(_) => faults.push(fault("no such page in the documentation")),
      }
    } else if let Some(path) = path.strip_prefix(&blob).or_else(|| path.strip_prefix(&raw)) {
      repository(root, Path::new(path), anchor, Kind::File, &fault, faults);
    } else if let Some(path) = path.strip_prefix(&tree) {
      repository(root, Path::new(path), anchor, Kind::Directory, &fault, faults);
    } else if let Some(anchor) = link.strip_prefix('#') {
      // A section of the same file: on docs.rs, the README is the crate page.
      if !headings(text).contains(anchor) {
        faults.push(fault("no such heading"));
      }
      if file == Path::new(README) {
        let html = fs::read_to_string(root.join(RUSTDOC).join("index.html"))
          .with_context(|| format!("{RUSTDOC} is not there: run `cargo doc --no-deps -p puns`"))?;
        if !html.contains(&format!("id=\"{anchor}\"")) {
          faults.push(fault("no such section in the documentation"));
        }
      }
    } else if !link.contains("://") && !link.starts_with("mailto:") {
      if file == Path::new(README) {
        faults.push(fault(&format!(
          "a relative link of the README leads nowhere on crates.io: write it as {blob}{path}"
        )));
      }
      if let Some((rs, defined)) = &rustdoc {
        match &label {
          None => faults.push(fault(&format!(
            "a relative link of a file that {} includes must be a definition, `[label]: target`, \
             whose label it defines for rustdoc",
            rs.display()
          ))),
          Some(label) if !defined.contains(&key(label)) => faults.push(fault(&format!(
            "rustdoc does not resolve it: define `[{label}]` in {} before the include",
            rs.display()
          ))),
          Some(_) => {}
        }
      }
      repository(root, &dir.join(path), anchor, Kind::Either, &fault, faults);
    }
  }
  Ok(())
}

/// The kind of a link to the repository: GitHub names a file `blob` and a directory `tree`, and a
/// relative link may be either.
#[derive(Clone, Copy)]
enum Kind {
  File,
  Directory,
  Either,
}

/// Checks that `path` is a file or a directory of the repository, as `kind` says, and that
/// `anchor` is a heading of it if it is Markdown.
fn repository(
  root: &Path,
  path: &Path,
  anchor: Option<&str>,
  kind: Kind,
  fault: &dyn Fn(&str) -> String,
  faults: &mut Vec<String>,
) {
  let full = root.join(path);
  let there = match kind {
    Kind::File => full.is_file(),
    Kind::Directory => full.is_dir(),
    Kind::Either => full.exists(),
  };
  if !there {
    faults.push(fault("no such file in the repository"));
    return;
  }
  if let Some(anchor) = anchor
    && path.extension().is_some_and(|e| e == "md")
  {
    let text = fs::read_to_string(&full).unwrap_or_default();
    if !headings(&text).contains(anchor) {
      faults.push(fault("no such heading"));
    }
  }
}

/// Records in `faults` each heading of `file` whose anchor is not an `id` of the page that rustdoc
/// wrote from it, if a `.rs` file includes it.
fn rendered(root: &Path, file: &Path, text: &str, faults: &mut Vec<String>) -> Result<()> {
  let page = if file == Path::new(README) {
    PathBuf::from("index.html")
  } else if file == Path::new(DOCS).join(README) {
    PathBuf::from("guide/index.html")
  } else if includer(root, file).is_some() {
    let name = file.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    PathBuf::from(format!("guide/{name}/index.html"))
  } else {
    return Ok(());
  };
  let html = fs::read_to_string(root.join(RUSTDOC).join(&page)).with_context(|| {
    format!("{RUSTDOC}/{} is not there: run `cargo doc --no-deps -p puns`", page.display())
  })?;
  for anchor in headings(text) {
    if !html.contains(&format!("id=\"{anchor}\"")) {
      faults.push(format!(
        "{}: the heading `#{anchor}` has another id on docs.rs, since rustdoc uses `{anchor}` on \
         the page: rename the heading",
        file.display()
      ));
    }
  }
  Ok(())
}

/// Returns the `.rs` file that includes `file` as documentation, and the labels that it defines
/// before the include, or `None` if no file includes it: the README is included by
/// `puns/src/lib.rs`, `docs/README.md` by `puns/src/guide.rs`, and `docs/{name}.md` by
/// `puns/src/guide/{name}.rs`.
fn includer(root: &Path, file: &Path) -> Option<(PathBuf, BTreeSet<String>)> {
  let rs = if file == Path::new(README) {
    PathBuf::from("puns/src/lib.rs")
  } else if file == Path::new(DOCS).join(README) {
    PathBuf::from("puns/src/guide.rs")
  } else {
    let name = file.file_stem()?.to_str()?;
    PathBuf::from(format!("puns/src/guide/{name}.rs"))
  };
  let source = fs::read_to_string(root.join(&rs)).ok()?;
  let name = file.file_name()?.to_str()?;
  // The include: `include_str!("../README.md")`, `include_str!("../../docs/{name}")`.
  let at = source.find(&format!("/{name}\")"))?;
  let defined = source[..at]
    .lines()
    .filter_map(|line| line.strip_prefix("//! ["))
    .filter_map(|line| line.split_once("]: "))
    .map(|(label, _)| key(label))
    .collect();
  Some((rs, defined))
}

/// A label as `CommonMark` matches it: case and runs of white space do not count.
fn key(label: &str) -> String {
  label.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Returns the links of a Markdown text, outside blocks of code: inline (`[text](target)`, images
/// too), with no label, and by reference (`[label]: target`), with their label.
fn links(text: &str) -> Vec<(Option<String>, String)> {
  let mut out = vec![];
  let mut code = false;
  for line in text.lines() {
    if line.trim_start().starts_with("```") {
      code = !code;
      continue;
    }
    if code {
      continue;
    }
    // A definition is a label and its target. A line that starts with a link and a colon, such as
    // `[text][label]: and so on`, is not one.
    if let Some(line) = line.strip_prefix('[')
      && let Some((label, target)) = line.split_once("]: ")
      && !label.contains(['[', ']'])
    {
      out.push((Some(label.to_string()), target.trim().to_string()));
      continue;
    }
    let mut rest = line;
    while let Some(at) = rest.find("](") {
      let after = &rest[at + 2..];
      let end = after.find(')').unwrap_or(after.len());
      out.push((None, after[..end].to_string()));
      rest = &after[end..];
    }
  }
  out
}

/// Returns the lines (from 1) of the blocks of code of a Markdown text whose info string does not
/// start with a language: none, or what only rustdoc reads, such as `compile_fail`.
fn unnamed_blocks(text: &str) -> Vec<usize> {
  const RUSTDOC: [&str; 6] = ["compile_fail", "ignore", "no_run", "should_panic", "edition", "E0"];
  let mut out = vec![];
  let mut code = false;
  for (at, line) in text.lines().enumerate() {
    let Some(info) = line.trim_start().strip_prefix("```") else { continue };
    if !code {
      let first = info.split(',').next().unwrap_or_default().trim();
      if first.is_empty() || RUSTDOC.iter().any(|word| first.starts_with(word)) {
        out.push(at + 1);
      }
    }
    code = !code;
  }
  out
}

/// Returns the anchors of the headings of a Markdown text, as GitHub makes them: lowercase,
/// without punctuation, a space made a `-`. rustdoc makes the same from the same heading, unless
/// the page already has that `id` ([`rendered`]).
fn headings(text: &str) -> BTreeSet<String> {
  let mut out = BTreeSet::new();
  let mut code = false;
  for line in text.lines() {
    if line.trim_start().starts_with("```") {
      code = !code;
      continue;
    }
    if code || !line.starts_with('#') {
      continue;
    }
    let title = line.trim_start_matches('#').trim();
    let anchor: String = title
      .chars()
      .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
      .map(|c| if c == ' ' { '-' } else { c.to_ascii_lowercase() })
      .collect();
    out.insert(anchor);
  }
  out
}

#[cfg(test)]
mod tests {
  use super::{pin, unnamed_blocks};

  #[test]
  fn finds_blocks_without_a_language() {
    let text = "```rust\nlet a = 1;\n```\n\n```\nlet b = 2;\n```\n\n```compile_fail,E0080\n\
                x\n```\n\n```text\nan error\n```\n\n```rust,compile_fail\ny\n```\n";
    assert_eq!(unnamed_blocks(text), [5, 9]);
  }

  #[test]
  fn pins_each_form() {
    let text = "[a]: https://docs.rs/puns/latest/puns/#soundness\n\
                [b](https://docs.rs/puns) [c](https://crates.io/crates/puns)\n\
                [d]: https://github.com/uael/puns/blob/main/docs/testing.md\n\
                [e]: https://github.com/uael/puns/tree/v0.1.0/puns/examples\n\
                ![f](https://raw.githubusercontent.com/uael/puns/main/docs/charts/checks.svg)\n\
                [g]: https://docs.rs/puns-derive/latest/puns_derive/\n\
                [h]: https://github.com/uael/puns/issues\n\
                [i]: https://github.com/uael/puns/tree/wip\n";
    let want = "[a]: https://docs.rs/puns/1.2.3-beta.4/puns/#soundness\n\
                [b](https://docs.rs/puns/1.2.3-beta.4) \
                [c](https://crates.io/crates/puns/1.2.3-beta.4)\n\
                [d]: https://github.com/uael/puns/blob/v1.2.3-beta.4/docs/testing.md\n\
                [e]: https://github.com/uael/puns/tree/v1.2.3-beta.4/puns/examples\n\
                ![f](https://raw.githubusercontent.com/uael/puns/v1.2.3-beta.4/docs/charts/checks.svg)\n\
                [g]: https://docs.rs/puns-derive/latest/puns_derive/\n\
                [h]: https://github.com/uael/puns/issues\n\
                [i]: https://github.com/uael/puns/tree/wip\n";
    assert_eq!(pin(text, "1.2.3-beta.4"), want);
    assert_eq!(pin(want, "1.2.3-beta.4"), want);
  }
}
