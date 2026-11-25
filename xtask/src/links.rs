//! The links of `README.md` and of `docs/*.md`, pinned to the version of `puns`.
//!
//! The README is read in three places: on GitHub, on crates.io, and on docs.rs, where it is the top
//! of the crate documentation. Every link of it to docs.rs, to crates.io, or to a file of the
//! repository by its URL names the version of `puns` in `puns/Cargo.toml`, or its tag
//! `v{version}`. A link without a version would lead elsewhere: `docs.rs/puns/latest` is the
//! newest stable release, and `blob/main` is the code after it. The one exception is a branch of
//! [`BRANCHES`], which holds what no release contains, and is linked as it is.
//!
//! The chapters of the reference are files of `docs/`, which rustdoc includes as the reference's
//! modules, as it includes the README as the crate page. A link between chapters is relative, so
//! that GitHub stays in the revision being read. rustdoc does not resolve such a link, so the `.rs`
//! file that includes a Markdown file defines the same label before the include, as an intra-doc
//! link: of two definitions of a label, the first counts. A relative link of an included file is
//! therefore a definition (`[label]: target`), and the `.rs` file must define its label, for the
//! same item. Labels match without case, so two labels that differ only by case are one label, and
//! rustdoc gives both the first target.
//!
//! The README links to the chapters by URL, pinned like its other links to the repository, since
//! crates.io resolves a relative link of the README on the default branch, in the directory of the
//! package, `puns/`, where `docs` is a symbolic link that GitHub does not follow. `cargo xtask
//! links` writes a relative link of a definition of the README as such a URL.
//!
//! GitHub highlights a block of code only if its info string starts with a language, while
//! rustdoc takes a block with none for Rust. So each block of these files names its language:
//! `rust,compile_fail` rather than `compile_fail`, which rustdoc reads the same way.
//!
//! GitHub and rustdoc make the same anchor from a heading, except where rustdoc already uses that
//! `id` on the page (`fields`, `implementations` and others): it then adds `-1`, and a link that
//! works on GitHub lands at the top of the page on docs.rs. So each heading of an included file
//! must have its anchor as an `id` of the page that rustdoc wrote.
//!
//! `cargo xtask links` writes the version into the links. A release PR changes the version, and
//! the workflow `release-plz.yml` runs it on the branch of the PR. With `--check`, it fails if a
//! link is not pinned, if its target is not there (a file of the repository, a heading of a
//! Markdown file, or a page and an `id` of the documentation that `cargo doc` wrote to
//! `target/doc`, which must be built first), if a label has two definitions, or if the `.rs` file
//! and the Markdown file that it includes define a label for different items.

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
    let absolute =
      if file == Path::new(README) { absolute(root, &text, &version) } else { text.clone() };
    let pinned = pin(&absolute, &version);
    if check {
      if pinned != text {
        faults.push(format!(
          "{}: a link is relative or does not name {version}: run `cargo xtask links`",
          file.display()
        ));
      }
      targets(root, file, &text, &version, &mut faults)?;
      labels(file, &text, &version, includer(root, file).as_ref(), &mut faults);
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

/// Returns the README `text` with the target of each definition that is relative, a file or a
/// directory of the repository, written as its URL on GitHub at the tag of `version`.
fn absolute(root: &Path, text: &str, version: &str) -> String {
  let mut out = String::with_capacity(text.len());
  let mut code = false;
  for line in text.split_inclusive('\n') {
    if line.trim_start().starts_with("```") {
      code = !code;
    }
    match definition(line.trim_end_matches('\n')) {
      Some((label, target)) if !code && is_relative(target) => {
        let path = target.split_once('#').map_or(target, |(path, _)| path);
        let kind = if root.join(path).is_dir() { "tree" } else { "blob" };
        let defined = format!("[{label}]: https://github.com/uael/puns/{kind}/v{version}/{target}");
        out.push_str(&defined);
        out.push_str(&line[line.trim_end_matches('\n').len()..]);
      }
      _ => out.push_str(line),
    }
  }
  out
}

/// Whether `target` is a link relative to the file: neither a URL nor a section of the same file.
fn is_relative(target: &str) -> bool {
  !target.contains("://") && !target.starts_with('#') && !target.starts_with("mailto:")
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
    } else if is_relative(&link) {
      if let Some((rs, defined)) = &rustdoc {
        match &label {
          None => faults.push(fault(&format!(
            "a relative link of a file that {} includes must be a definition, `[label]: target`, \
             whose label it defines for rustdoc",
            rs.display()
          ))),
          Some(label) if !defined.iter().any(|(rs, _)| key(rs) == key(label)) => {
            faults.push(fault(&format!(
              "rustdoc does not resolve it: define `[{label}]` in {} before the include",
              rs.display()
            )));
          }
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
  } else if includer(root, file).is_some() {
    let name = file.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    PathBuf::from(format!("reference/{name}/index.html"))
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

/// Returns the `.rs` file that includes `file` as documentation, and the definitions of labels
/// (label and target) that it makes before the include, or `None` if no file includes it: the
/// README is included by `puns/src/lib.rs`, and `docs/{name}.md` by `puns/src/reference/{name}.rs`.
fn includer(root: &Path, file: &Path) -> Option<(PathBuf, Vec<(String, String)>)> {
  let rs = if file == Path::new(README) {
    PathBuf::from("puns/src/lib.rs")
  } else {
    let name = file.file_stem()?.to_str()?;
    PathBuf::from(format!("puns/src/reference/{name}.rs"))
  };
  let source = fs::read_to_string(root.join(&rs)).ok()?;
  let name = file.file_name()?.to_str()?;
  // The include: `include_str!("../README.md")`, `include_str!("../../docs/{name}")`.
  let at = source.find(&format!("/{name}\")"))?;
  let defined = source[..at]
    .lines()
    .filter_map(|line| definition(line.strip_prefix("//! ")?))
    .map(|(label, target)| (label.to_string(), target.to_string()))
    .collect();
  Some((rs, defined))
}

/// Records in `faults` each label that `file` and the `.rs` file that includes it (`rustdoc`)
/// define twice, which one page of rustdoc reads as one label, and each label that both define for
/// different items.
fn labels(
  file: &Path,
  text: &str,
  version: &str,
  rustdoc: Option<&(PathBuf, Vec<(String, String)>)>,
  faults: &mut Vec<String>,
) {
  let markdown: Vec<_> =
    links(text).into_iter().filter_map(|(label, link)| Some((label?, link))).collect();
  twice(&file.display().to_string(), &markdown, faults);
  let Some((rs, defined)) = rustdoc else { return };
  twice(&rs.display().to_string(), defined, faults);
  let chapter = file != Path::new(README);
  for (label, target) in defined {
    for (other, link) in &markdown {
      if key(label) != key(other) {
        continue;
      }
      let place = format!("{} and {}: `[{label}]`", rs.display(), file.display());
      if label != other {
        faults.push(format!(
          "{place} and `[{other}]` are one label, since case does not count: rustdoc gives both \
           `{target}`, so give one another label"
        ));
        continue;
      }
      match (rustdoc_item(target, chapter), markdown_item(link, chapter, version)) {
        (Some(a), Some(b)) if a.same(&b) => {}
        (Some(_), Some(_)) => faults.push(format!(
          "{place} names `{target}` on docs.rs and `{link}` on GitHub, which are different \
           items: name an item by the path of its page, not of a re-export"
        )),
        _ => faults.push(format!(
          "{place}: cannot tell whether `{target}` and `{link}` are the same item: write the \
           target as an intra-doc path, and the link as docs.rs, docs of `core` or a chapter"
        )),
      }
    }
  }
}

/// Records in `faults` each two definitions of `place` whose labels match without case.
fn twice(place: &str, definitions: &[(String, String)], faults: &mut Vec<String>) {
  for (at, (label, _)) in definitions.iter().enumerate() {
    if let Some((other, _)) = definitions[..at].iter().find(|(other, _)| key(other) == key(label)) {
      let what = if other == label {
        format!("`[{label}]` has two definitions")
      } else {
        format!("`[{other}]` and `[{label}]` are one label, since case does not count")
      };
      faults.push(format!("{place}: {what}, and only the first counts"));
    }
  }
}

/// What a link names in the documentation: a path from the root of `puns` (a crate name first
/// for another crate), the kind of the item if the link says it, and a section of its page.
#[derive(Debug)]
struct Item {
  path: Vec<String>,
  kind: Option<&'static str>,
  section: Option<String>,
}

impl Item {
  /// Whether two links name the same item: a kind that only one of them says does not count.
  fn same(&self, other: &Self) -> bool {
    self.path == other.path
      && self.section == other.section
      && (self.kind.is_none() || other.kind.is_none() || self.kind == other.kind)
  }
}

/// The kind that a disambiguator of an intra-doc link (`trait@`) or the prefix of a page of
/// rustdoc (`trait.`) names, or `None` if it names none.
fn kind(name: &str) -> Option<&'static str> {
  Some(match name {
    "fn" | "function" | "method" => "fn",
    "macro" | "derive" => "macro",
    "mod" | "module" => "mod",
    "const" | "constant" => "constant",
    "prim" | "primitive" => "primitive",
    "trait" => "trait",
    "struct" => "struct",
    "enum" => "enum",
    "union" => "union",
    "type" | "tyalias" => "type",
    "static" => "static",
    _ => return None,
  })
}

/// Returns the item that an intra-doc link names, from a module of `puns::reference` if `chapter`,
/// whose `super` is `reference`, or from the crate root, and which imports the crate either way.
fn rustdoc_item(target: &str, chapter: bool) -> Option<Item> {
  let (target, section) =
    target.split_once('#').map_or((target, None), |(t, s)| (t, Some(s.to_string())));
  let (kind, target) = match target.split_once('@') {
    Some((name, rest)) => (Some(self::kind(name)?), rest),
    None => (None, target),
  };
  let mut path: Vec<String> = vec![];
  for (at, segment) in target.split("::").enumerate() {
    match segment {
      "crate" if at == 0 => {}
      "super" if at == 0 && chapter => path.push("reference".into()),
      "super" | "self" | "" => return None,
      _ => path.push(segment.into()),
    }
  }
  Some(Item { path, kind, section })
}

/// Returns the item that a link of a Markdown file names: a page of `puns` or of `core` on docs.rs
/// or the documentation of Rust, a chapter by its file, or the README, which is the crate page.
/// `chapter` says whether the file is a chapter, in `docs/`, or the README.
fn markdown_item(link: &str, chapter: bool, version: &str) -> Option<Item> {
  let (page, section) = link.split_once('#').map_or((link, None), |(p, s)| (p, Some(s)));
  let section = section.map(str::to_string);
  let docs = format!("https://docs.rs/puns/{version}/puns/");
  let repository = [
    format!("https://github.com/uael/puns/blob/v{version}/"),
    format!("https://github.com/uael/puns/tree/v{version}/"),
  ];
  if let Some(page) = page.strip_prefix(&docs) {
    return rustdoc_page(vec![], page, section);
  }
  if let Some(rest) = page.strip_prefix("https://doc.rust-lang.org/") {
    // A channel or a version, then the crate.
    let (_, page) = rest.split_once('/')?;
    return rustdoc_page(vec![], page, section);
  }
  let file = match repository.iter().find_map(|base| page.strip_prefix(base.as_str())) {
    Some(file) => file.to_string(),
    None if page.contains("://") => return None,
    None if chapter => match page {
      "../README.md" => String::from("README.md"),
      _ => format!("docs/{page}"),
    },
    None => page.to_string(),
  };
  let path = match file.as_str() {
    "README.md" => vec![],
    "docs" => vec!["reference".to_string()],
    _ => {
      let name = file.strip_prefix("docs/")?.strip_suffix(".md")?;
      vec!["reference".to_string(), name.to_string()]
    }
  };
  Some(Item { path, kind: Some("mod"), section })
}

/// Returns the item of a page of rustdoc (`layout/struct.Spec.html`) below the module `path`, with
/// a `section` of the page, which names a part of the item (`method.stored_as`) or a heading.
fn rustdoc_page(mut path: Vec<String>, page: &str, section: Option<String>) -> Option<Item> {
  let mut segments: Vec<&str> = page.split('/').collect();
  let last = segments.pop()?;
  path.extend(segments.iter().map(|s| (*s).to_string()));
  let kind = if last.is_empty() || last == "index.html" {
    Some("mod")
  } else {
    let (prefix, name) = last.strip_suffix(".html")?.split_once('.')?;
    path.push(name.to_string());
    Some(kind(prefix)?)
  };
  let part = ["method.", "tymethod.", "variant.", "structfield.", "associatedconstant."];
  match section.as_deref().and_then(|s| part.iter().find_map(|p| s.strip_prefix(p))) {
    Some(name) => {
      path.push(name.to_string());
      Some(Item { path, kind: None, section: None })
    }
    None => Some(Item { path, kind, section }),
  }
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
    if let Some((label, target)) = definition(line) {
      out.push((Some(label.to_string()), target.to_string()));
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

/// Returns the label and the target of a line that defines a label (`[label]: target`). A line that
/// starts with a link and a colon, such as `[text][label]: and so on`, is not one.
fn definition(line: &str) -> Option<(&str, &str)> {
  let (label, target) = line.strip_prefix('[')?.split_once("]: ")?;
  (!label.contains(['[', ']'])).then(|| (label, target.trim()))
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
  use std::path::{Path, PathBuf};

  use super::{absolute, labels, pin, unnamed_blocks};

  /// The faults that [`labels`] records for the chapter `docs/x.md` (or the README) and the
  /// definitions of the `.rs` file that includes it.
  fn faults(file: &str, markdown: &str, rs: &[(&str, &str)]) -> Vec<String> {
    let rs = rs.iter().map(|(l, t)| ((*l).to_string(), (*t).to_string())).collect();
    let mut faults = vec![];
    labels(Path::new(file), markdown, "1.0.0", Some(&(PathBuf::from("x.rs"), rs)), &mut faults);
    faults
  }

  #[test]
  fn finds_labels_that_collide() {
    // Two labels that differ by case, in the Markdown file, in the `.rs` file, or one in each.
    let pun = "[`Pun`]: https://docs.rs/puns/1.0.0/puns/trait.Pun.html\n\
               [`pun`]: https://docs.rs/puns/1.0.0/puns/fn.pun.html\n";
    assert_eq!(faults("docs/x.md", pun, &[]).len(), 1);
    assert_eq!(faults("docs/x.md", "", &[("`Pun`", "trait@Pun"), ("`pun`", "pun")]).len(), 1);
    let aim = "[`aim`]: https://docs.rs/puns/1.0.0/puns/fn.aim.html\n";
    let found = faults("docs/x.md", aim, &[("`Aim`", "layout::Aim"), ("`aim`", "aim")]);
    assert!(found.iter().any(|f| f.contains("`[`Aim`]` and `[`aim`]` are one label")), "{found:?}");
    // A label defined twice.
    let miri =
      "[Miri]: https://github.com/rust-lang/miri\n[Miri]: https://github.com/rust-lang/miri\n";
    assert_eq!(faults("docs/x.md", miri, &[]).len(), 1);
    // Different labels, and a definition inside a block of code, which is none.
    let code = "[a]: https://a.org\n```text\n[A]: https://a.org\n```\n[b]: https://b.org\n";
    assert_eq!(faults("docs/x.md", code, &[]), Vec::<String>::new());
  }

  #[test]
  fn compares_the_items_of_a_label() {
    let docs = "https://docs.rs/puns/1.0.0/puns";
    let same = [
      ("`pun`", "pun", format!("{docs}/fn.pun.html")),
      ("`Pun`", "trait@Pun", format!("{docs}/trait.Pun.html")),
      ("`Pun`", "trait@Pun#safety", format!("{docs}/trait.Pun.html#safety")),
      ("d", "macro@Pun#views", format!("{docs}/derive.Pun.html#views")),
      ("`Error::Limit`", "Error::Limit", format!("{docs}/enum.Error.html#variant.Limit")),
      ("s", "layout::Spec::stored_as", format!("{docs}/layout/struct.Spec.html#method.stored_as")),
      ("`num`", "num", format!("{docs}/num/index.html")),
      ("`AIM_PARTS`", "AIM_PARTS", format!("{docs}/constant.AIM_PARTS.html")),
      (
        "t",
        "core::mem::transmute",
        "https://doc.rust-lang.org/1.99.0/core/mem/fn.transmute.html".into(),
      ),
      (
        "p",
        "core::ptr#alignment",
        "https://doc.rust-lang.org/1.99.0/core/ptr/index.html#alignment".into(),
      ),
      ("aims", "super::aims", "aims.md".into()),
      ("r", "super::rules#walks", "rules.md#walks".into()),
      ("crate#x", "crate#x", "../README.md#x".into()),
    ];
    for (label, target, link) in &same {
      let markdown = format!("[{label}]: {link}\n");
      assert_eq!(
        faults("docs/x.md", &markdown, &[(label, target)]),
        Vec::<String>::new(),
        "{label}"
      );
    }
    let different = [
      ("`pun`", "trait@Pun", format!("{docs}/fn.pun.html")),
      ("`Aim`", "layout::Aim", format!("{docs}/fn.aim.html")),
      ("c", "fn@crc32", format!("{docs}/layout/fn.crc32.html")),
      ("r", "super::rules#walks", "rules.md#regions".into()),
      ("e", "Error::Limit", format!("{docs}/enum.Error.html#variant.Size")),
    ];
    for (label, target, link) in &different {
      let markdown = format!("[{label}]: {link}\n");
      let found = faults("docs/x.md", &markdown, &[(label, target)]);
      assert!(found.len() == 1 && found[0].contains("different items"), "{label}: {found:?}");
    }
    // The README names the chapters by their URL on GitHub, and the `.rs` file by their module.
    let readme = "[c]: https://github.com/uael/puns/blob/v1.0.0/docs/\
                  how_puns_decides.md#requirements\n\
                  [r]: https://github.com/uael/puns/tree/v1.0.0/docs\n";
    let rs = [("c", "reference::how_puns_decides#requirements"), ("r", "mod@reference")];
    assert_eq!(faults("README.md", readme, &rs), Vec::<String>::new());
    let found = faults("README.md", readme, &[("c", "reference::rules#requirements")]);
    assert_eq!(found.len(), 1, "{found:?}");
    // A link whose item the check cannot tell.
    let found = faults("docs/x.md", "[w]: https://example.org\n", &[("w", "pun")]);
    assert!(found.len() == 1 && found[0].contains("cannot tell"), "{found:?}");
  }

  #[test]
  fn writes_relative_links_of_the_readme_as_urls() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let text = "[a]: docs/aims.md#x\n[b]: docs\n[c]: #license\n[d]: https://crates.io/crates/puns\n\
                ```text\n[e]: docs/aims.md\n```\n";
    let want = "[a]: https://github.com/uael/puns/blob/v1.0.0/docs/aims.md#x\n\
                [b]: https://github.com/uael/puns/tree/v1.0.0/docs\n[c]: #license\n\
                [d]: https://crates.io/crates/puns\n```text\n[e]: docs/aims.md\n```\n";
    assert_eq!(absolute(root, text, "1.0.0"), want);
  }

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
