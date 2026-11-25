//! The derive macro `#[derive(Pun)]` of the `puns` crate, which documents it.

use std::{
  any::TypeId,
  ffi::{OsStr, OsString},
  hash::{DefaultHasher, Hash, Hasher},
  sync::{
    OnceLock,
    atomic::{AtomicUsize, Ordering},
  },
};

mod args;

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, format_ident, quote, quote_spanned};
use syn::{
  Attribute, BinOp, Data, DataEnum, DataStruct, DeriveInput, Error, Expr, ExprLit, ExprRange,
  Fields, GenericParam, Ident, Index, Lit, Member, Path, RangeLimits, Result, Token, Type, UnOp,
  Visibility,
  parse::{Parse, ParseStream},
  parse_macro_input, parse_quote,
  punctuated::Punctuated,
  spanned::Spanned,
  visit_mut::{self, VisitMut},
};

// The link names the version of this crate, which is that of `puns` (`version_group` in
// `release-plz.toml`).
#[doc = concat!(
  "Implements `Pun` as [`puns::Pun`](https://docs.rs/puns/",
  env!("CARGO_PKG_VERSION"),
  "/puns/derive.Pun.html) documents."
)]
#[proc_macro_derive(Pun, attributes(pun))]
pub fn derive_pun(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
  let tokens = TokenStream::from(input.clone());
  let input = parse_macro_input!(input as DeriveInput);
  // The generated items are named `__Pun..`. A type of the input with such a name would be taken
  // for one of them where the generated code names it.
  if let Some(ident) = find_reserved(tokens) {
    return Error::new(
      ident.span(),
      "names that start with `__Pun` are reserved by `#[derive(Pun)]`",
    )
    .into_compile_error()
    .into();
  }
  let expanded = match &input.data {
    Data::Struct(data) => derive_struct(&input, data),
    Data::Enum(data) => derive_enum(&input, data),
    Data::Union(data) => derive_union(&input, data),
  };
  expanded
    .unwrap_or_else(|error| {
      let mut tokens = error.into_compile_error();
      tokens.extend(fallback(&input));
      tokens
    })
    .into()
}

/// Returns the types of the variants of a view as a balanced tree (`puns::view::List`): the first
/// half, then the second.
fn view_tree(types: &[&Type], root: &TokenStream) -> TokenStream {
  if let [ty] = types {
    return quote!(#root::view::Leaf<#ty>);
  }
  let (first, second) = types.split_at(types.len() / 2);
  let (first, second) = (view_tree(first, root), view_tree(second, root));
  quote!(#root::view::Pair<#first, #second>)
}

/// Returns the place of the variant `index` of `count` in the tree of [`view_tree`], as a type.
fn view_place(index: usize, count: usize, root: &TokenStream) -> TokenStream {
  if count == 1 {
    return quote!(#root::view::Here);
  }
  let half = count / 2;
  if index < half {
    let inner = view_place(index, half, root);
    quote!(#root::view::First<#inner>)
  } else {
    let inner = view_place(index - half, count - half, root);
    quote!(#root::view::Second<#inner>)
  }
}

/// Returns what stands for the implementations of the derive when it fails.
///
/// The error of the derive is then the only one, rather than one at each use of the type. The
/// fallback is never used, since the crate does not compile.
fn fallback(input: &DeriveInput) -> TokenStream {
  let Ok(options) = parse_options(&input.attrs, "the type", &ALL_OPTIONS) else {
    return TokenStream::new();
  };
  let views = matches!(&input.data, Data::Enum(data) if data.variants.iter().all(|variant| {
    matches!(&variant.fields, Fields::Unnamed(fields) if fields.unnamed.len() == 1
      && matches!(fields.unnamed[0].ty, Type::Reference(_)))
  }));
  if views || matches!(input.data, Data::Union(_)) {
    return TokenStream::new();
  }
  let root = options.root.unwrap_or_else(|| quote!(::puns));
  let ident = &input.ident;
  let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
  let tags = options.tags.iter().map(|TagOption { key, tag }| {
    quote! {
      impl #impl_generics #root::Tag<#key> for #ident #ty_generics #where_clause {
        type Tag = #tag;
      }
    }
  });
  quote! {
    // SAFETY: This implementation is emitted only next to the compile error of the derive, so
    // the crate that holds it does not compile, and no code uses it. Its `PROBE` and `LAYOUT`
    // panic, so any evaluation of them is a compile error too.
    unsafe impl #impl_generics #root::Pun for #ident #ty_generics #where_clause {
      type Elem = ();
      type Fields = ();
      type Variants = ();
      type Rules = ();
      type Metadata = ();
      const PROBE: *const Self = ::core::panic!("`#[derive(Pun)]` failed");
      const LAYOUT: #root::layout::Layout = ::core::panic!("`#[derive(Pun)]` failed");
    }

    #(#tags)*
  }
}

/// Returns the first identifier of `tokens` that starts with `__Pun`, if any.
fn find_reserved(tokens: TokenStream) -> Option<Ident> {
  tokens.into_iter().find_map(|token| match token {
    proc_macro2::TokenTree::Ident(ident) if ident.to_string().starts_with("__Pun") => Some(ident),
    proc_macro2::TokenTree::Group(group) => find_reserved(group.stream()),
    _ => None,
  })
}

// -------------------------------------------------------------------------------------------------
// Attributes

/// Returns the identity of the type of `input` (`puns::layout::Id`): the crate being compiled
/// (`compiled_crate`), the name of the type, the number of this derive in that compilation, this
/// copy of the derive (`this_derive`), and the identity of each of its type and const parameters,
/// in order. `generics` are the generics of its implementation of `Pun`.
///
/// The name is a literal that the derive writes, passed to `Id::new`, an associated function of the
/// type that `Layout::PIN` fixes: no path chooses it, neither the crate path nor `::core`, so no
/// crate can give a type the identity of another. No two crates of a program have one name, each
/// derive in a compilation takes the next number of its copy, and two copies of the derive have two
/// names, so two types that derive `Pun` have two identities. A type has none (`Id::NONE`) where
/// the arguments of the process do not name its crate.
///
/// Two instances of a generic type are other types, which `rustc` may lay out differently, so the
/// identity of an instance has those of its parameters. The identity of a type parameter is the
/// `ID` of its `Pun` implementation, so the type has one only if each type parameter is bounded by
/// `Pun` where the derive sees it: in its bounds, in the `where` clause, in `#[pun(bound(..))]`, or
/// in the bounds that the derive adds for a field or a region of that type. Otherwise it has none
/// (`Id::NONE`), since the derive cannot leave out a parameter, nor name its identity. A bound is
/// read by its name, so a parameter bounded by another trait named `Pun` does not compile. Lifetime
/// parameters do not count, since `rustc` lays out a type whatever its lifetimes.
fn identity(input: &DeriveInput, generics: &syn::Generics, root: &TokenStream) -> TokenStream {
  // One crate is expanded by one process, in which each derive takes the next number of its copy.
  static DERIVES: AtomicUsize = AtomicUsize::new(0);
  let Some(krate) = compiled_crate() else {
    return quote!(#root::layout::Id::NONE);
  };
  let mut args = vec![];
  for param in &input.generics.params {
    match param {
      GenericParam::Lifetime(_) => {}
      GenericParam::Type(param) if has_pun(generics, &param.ident.to_string()) => {
        let ty = &param.ident;
        args.push(quote!(<#ty as #root::Pun>::ID));
      }
      GenericParam::Type(_) => return quote!(#root::layout::Id::NONE),
      GenericParam::Const(param) => {
        let value = &param.ident;
        // A const parameter is an integer, a `bool` or a `char`, which `as` casts to the `i128` of
        // `Id::value` without naming a type.
        args.push(quote!(#root::layout::Id::value(#value as _)));
      }
    }
  }
  let n = DERIVES.fetch_add(1, Ordering::Relaxed);
  let name = format!("{krate}::{}#{n}@{:016x}", input.ident, this_derive());
  let name = proc_macro2::Literal::string(&name);
  // The identities of the parameters are values of the instance, which a `const` block makes
  // `'static`.
  let args = if args.is_empty() { quote!(&[]) } else { quote!(const { &[#(#args),*] }) };
  // Of the type that `Layout::PIN` fixes, and that `Layout::identified`, `Pun::ID` and `Scalar::ID`
  // take, whatever type the path names, so `Id::new` is its function.
  quote!({
    let id: #root::layout::Id = #root::layout::Id::new(#name, #args);
    id
  })
}

/// Returns the crate that the `rustc` process running the derive compiles, named as `rustc` tells
/// it apart from the other crates of a program (`crate_of`), or `None` where the derive cannot read
/// the arguments of the process as `rustc` does (`args::process`, `crate_of`).
fn compiled_crate() -> Option<&'static str> {
  static CRATE: OnceLock<Option<String>> = OnceLock::new();
  CRATE
    .get_or_init(|| {
      let forced = std::env::var_os("RUSTC_FORCE_RUSTC_VERSION");
      crate_of(args::process()?.into_iter().skip(1), forced.as_deref())
    })
    .as_deref()
}

/// The options of `rustc` 1.100, as `rustc -Z unstable-options --help -v` lists them: the short
/// ones that take a value and those that take none, then the long ones of each kind. An option of
/// no list is an error of `rustc`, unless another version of `rustc` has it, which the derive cannot
/// read: either way, it names no crate. The test `the_options_are_those_of_rustc` compares them
/// with what the `rustc` of the build lists.
const SHORT_WITH_VALUE: &str = "LloAWDFCjZ";
const SHORT_FLAGS: &str = "hgOVv";
const LONG_WITH_VALUE: &[&str] = &[
  "cfg",
  "check-cfg",
  "crate-type",
  "crate-name",
  "edition",
  "emit",
  "print",
  "out-dir",
  "explain",
  "target",
  "allow",
  "warn",
  "force-warn",
  "deny",
  "forbid",
  "cap-lints",
  "codegen",
  "jobs",
  "jobs-frontend",
  "jobs-backend",
  "jobs-linker",
  "extern",
  "sysroot",
  "error-format",
  "json",
  "color",
  "diagnostic-width",
  "remap-path-prefix",
  "remap-path-scope",
];
const LONG_FLAGS: &[&str] = &["help", "test", "version", "verbose"];

/// The values of `#![crate_type]` that `rustc` 1.100 knows, and whether each is an executable.
/// `rustc` ignores another value where `unknown_crate_types` is allowed, and refuses it otherwise.
const CRATE_TYPES: &[(&str, bool)] = &[
  ("bin", true),
  ("lib", false),
  ("rlib", false),
  ("dylib", false),
  ("cdylib", false),
  ("staticlib", false),
  ("proc-macro", false),
  ("sdylib", false),
];

/// Returns the crate that `rustc` compiles with the arguments `args` (without the name of the
/// program), as the inputs of its `StableCrateId` name it: the name of the crate, whether it is an
/// executable, the values of `-C metadata`, and `forced`, the value of
/// `RUSTC_FORCE_RUSTC_VERSION`, which replaces the version of `rustc` there. `rustc` refuses two
/// crates of one `StableCrateId` in a program (E0519 for the crate it compiles and a dependency,
/// "colliding `StableCrateId` values" for two dependencies), so two crates of a program that give
/// one string differ in none of these, and are one crate. Each part is behind its length, so that
/// two lists of parts give two strings.
///
/// The arguments are read as `rustc` reads them: an `@file` is its lines (an `@shell:` file after
/// `-Z shell-argfiles` is not read), an option takes its value from the same argument or the next
/// one, `--` ends the options, and the argument that no option takes is the root file of the crate.
/// A crate is an executable with `--test`, which makes one whatever its types, or with `bin` among
/// the types of `--crate-type`. Without either option, its types are those of the
/// `#![crate_type]` attributes of its root file (`attributed_types`), and it is an executable
/// without them, as Cargo compiles a test or a bench without a harness.
///
/// Returns `None` where the derive cannot read the arguments as `rustc` does: not exactly one
/// `--crate-name`, an argument that is not UTF-8 (which `rustc` refuses), an `@file` that cannot be
/// read, an `@shell:` file after `-Z shell-argfiles`, an option of no list above, or, for a crate
/// without `--crate-type` nor `--test`, a root file whose attributes cannot be read
/// (`attributed_types`) or `-Z crate-attr`, which adds attributes.
fn crate_of(args: impl Iterator<Item = OsString>, forced: Option<&OsStr>) -> Option<String> {
  // As `rustc_driver_impl::args::Expander`, which sees `-Z shell-argfiles` as it expands.
  let mut expanded = vec![];
  let shell = |expanded: &[String]| {
    let mut options = expanded.iter().map(String::as_str);
    let mut shell = false;
    while let Some(arg) = options.next() {
      let option = if arg == "-Z" { options.next() } else { arg.strip_prefix("-Z") };
      shell |= option == Some("shell-argfiles");
    }
    shell
  };
  for arg in args {
    let arg = arg.into_string().ok()?;
    match arg.strip_prefix('@') {
      Some(file) if file.starts_with("shell:") && shell(&expanded) => return None,
      Some(file) => expanded.extend(std::fs::read_to_string(file).ok()?.lines().map(String::from)),
      None => expanded.push(arg),
    }
  }
  let (mut names, mut types, mut test, mut metadata) = (vec![], vec![], false, vec![]);
  let (mut inputs, mut crate_attr) = (vec![], false);
  let mut args = expanded.iter().map(String::as_str);
  while let Some(arg) = args.next() {
    let (option, value) = if arg == "--" {
      inputs.extend(args.by_ref());
      break;
    } else if let Some(long) = arg.strip_prefix("--") {
      let (option, value) = long.split_once('=').map_or((long, None), |(o, v)| (o, Some(v)));
      if LONG_FLAGS.contains(&long) {
        test |= long == "test";
        continue;
      }
      if !LONG_WITH_VALUE.contains(&option) {
        return None;
      }
      (
        option,
        match value {
          Some(value) => value,
          None => args.next()?,
        },
      )
    } else if let Some(short) = arg.strip_prefix('-').filter(|short| !short.is_empty()) {
      // Flags, then at most one option that takes the rest, or the next argument, as its value.
      let Some((at, option)) = short.char_indices().find(|&(_, c)| !SHORT_FLAGS.contains(c)) else {
        continue;
      };
      if !SHORT_WITH_VALUE.contains(option) {
        return None;
      }
      let (option, rest) = short[at..].split_at(option.len_utf8());
      (option, if rest.is_empty() { args.next()? } else { rest })
    } else {
      inputs.push(arg);
      continue;
    };
    match option {
      "crate-name" => names.push(value),
      "crate-type" => types.extend(value.split(',')),
      // A list, which `rustc` splits at white space.
      "codegen" | "C" => {
        metadata
          .extend(value.strip_prefix("metadata=").into_iter().flat_map(str::split_whitespace));
      }
      "Z" => crate_attr |= value.starts_with("crate-attr"),
      _ => {}
    }
  }
  let [name] = names[..] else { return None };
  let executable = if test {
    true
  } else if !types.is_empty() {
    types.contains(&"bin")
  } else {
    // `rustc` reads one root file; with another argument, such as the path of `rustc` that a
    // driver such as `clippy-driver` is given, the derive cannot tell which.
    let [input] = inputs[..] else { return None };
    if crate_attr {
      return None;
    }
    let types = attributed_types(input)?;
    types.is_empty() || types.contains(&true)
  };
  // As `StableCrateId::new`: the values of `-C metadata` in order, each once.
  metadata.sort_unstable();
  metadata.dedup();
  let kind = if executable { "bin" } else { "lib" };
  let mut parts = vec![format!("{}:{name}", name.len()), kind.to_owned()];
  parts.extend(metadata.iter().map(|value| format!("{}:{value}", value.len())));
  if let Some(forced) = forced {
    let forced = forced.to_str()?;
    parts.push(format!("version {}:{forced}", forced.len()));
  }
  Some(parts.join("/"))
}

/// Returns whether each `#![crate_type]` attribute of the root file `path` makes an executable, as
/// `rustc` reads them where the arguments give no type: the inner attributes at the start of the
/// file, after a byte order mark and a first line `#!` that starts no attribute. `rustc` forbids
/// `crate_type` in `cfg_attr`, so these are all the attributes that give a type.
///
/// Returns `None` where the derive cannot read them as `rustc` does: the file is standard input
/// (`-`) or cannot be read, its attributes do not parse, or a `crate_type` is not a string of
/// `CRATE_TYPES`.
fn attributed_types(path: &str) -> Option<Vec<bool>> {
  if path == "-" {
    return None;
  }
  let source = std::fs::read_to_string(path).ok()?;
  let mut source = source.strip_prefix('\u{feff}').unwrap_or(&source);
  // As `rustc_lexer::strip_shebang`: `#!` starts an attribute if the next token, past white space
  // and comments, is `[`. A comment there gives `None` rather than a lexer of comments.
  if let Some(tail) = source.strip_prefix("#!") {
    let next = tail.trim_start();
    if next.starts_with("//") || next.starts_with("/*") {
      return None;
    }
    if !next.starts_with('[') {
      source = tail.find('\n').map_or("", |at| &tail[at..]);
    }
  }
  let inner = |input: ParseStream| {
    let attrs = input.call(Attribute::parse_inner)?;
    input.parse::<TokenStream>()?;
    Ok(attrs)
  };
  let mut types = vec![];
  for attr in syn::parse::Parser::parse_str(inner, source).ok()? {
    if !attr.path().is_ident("crate_type") {
      continue;
    }
    let syn::Meta::NameValue(syn::MetaNameValue {
      value: Expr::Lit(ExprLit { lit: Lit::Str(value), .. }),
      ..
    }) = &attr.meta
    else {
      return None;
    };
    let value = value.value();
    let &(_, executable) = CRATE_TYPES.iter().find(|(name, _)| *name == value)?;
    types.push(executable);
  }
  Some(types)
}

/// Returns a name of this copy of the derive: a hash of the `TypeId` of a type of it, which names
/// the crate that `rustc` compiled it as.
///
/// Each copy of the derive that a compilation loads counts its own derives, so two copies, such as
/// a `puns-derive` of the registry and one of a git repository that a crate names both, give the
/// same numbers. Two copies are two crates of the build, with two `TypeId`s here, and two hashes,
/// unless the 64 bits of two hashes collide.
fn this_derive() -> u64 {
  struct ThisDerive;
  let mut hasher = DefaultHasher::new();
  TypeId::of::<ThisDerive>().hash(&mut hasher);
  hasher.finish()
}

/// The `repr` of a type, as far as the derive reads it.
#[derive(Default)]
struct Repr {
  /// Whether it has `repr(C)`.
  c: bool,
  /// Whether it has `repr(transparent)`.
  transparent: bool,
  /// The `N` of `packed(N)`, which is 1 for `packed`.
  packed: Option<TokenStream>,
  /// The integer of `repr(u8)`, `repr(i32)` or another primitive representation.
  int: Option<Ident>,
}

/// Returns the `repr` of the type of `attrs`.
///
/// `align(N)` is read and ignored.
///
/// # Errors
///
/// Returns an error if a `repr` attribute does not parse.
fn parse_repr(attrs: &[Attribute]) -> Result<Repr> {
  let mut repr = Repr::default();
  for attr in attrs.iter().filter(|attr| attr.path().is_ident("repr")) {
    attr.parse_nested_meta(|meta| {
      if meta.path.is_ident("C") {
        repr.c = true;
      } else if meta.path.is_ident("transparent") {
        repr.transparent = true;
      } else if meta.path.is_ident("packed") {
        repr.packed = Some(if meta.input.peek(syn::token::Paren) {
          let content;
          syn::parenthesized!(content in meta.input);
          content.parse::<Lit>()?.into_token_stream()
        } else {
          quote!(1)
        });
      } else if meta.input.peek(syn::token::Paren) {
        // `align(N)`
        let content;
        syn::parenthesized!(content in meta.input);
        content.parse::<TokenStream>()?;
      } else if let Some(ident) = meta.path.get_ident() {
        const INTS: [&str; 12] =
          ["u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize"];
        if INTS.iter().any(|int| ident == int) {
          repr.int = Some(ident.clone());
        }
      }
      Ok(())
    })?;
  }
  Ok(repr)
}

/// When the pointee of an aim is checked: what an aim of either option starts with, `eager` or
/// `lazy`, before its name.
///
/// A region without `eager` or `lazy` is checked when it is read ([`Prefix::is_eager`]). A pointer
/// is checked with the struct: it may say `eager`, and `lazy` is refused ([`AimOption`]).
struct Prefix {
  /// `Some(true)` for `eager`, `Some(false)` for `lazy`, `None` for neither.
  check: Option<bool>,
}

impl Parse for Prefix {
  fn parse(input: ParseStream) -> Result<Self> {
    let check = if keyword(input, "eager") {
      Some(true)
    } else if keyword(input, "lazy") {
      Some(false)
    } else {
      None
    };
    if check.is_some() {
      input.parse::<Ident>()?;
    }
    Ok(Self { check })
  }
}

impl Prefix {
  /// Returns whether the aim is eager: as it says, or as `by_default` says for its option.
  const fn is_eager(&self, by_default: bool) -> bool {
    match self.check {
      Some(eager) => eager,
      None => by_default,
    }
  }
}

/// A region of `#[pun(regions(..))]`: `name`, `name: Type`, `name = EXPR` or
/// `name: Type = EXPR`, after the [`Prefix`] of its aim.
struct Region {
  prefix: Prefix,
  name: Ident,
  ty: Option<Type>,
  size: Option<Expr>,
}

impl Parse for Region {
  fn parse(input: ParseStream) -> Result<Self> {
    let prefix = input.parse()?;
    let name = input.parse()?;
    let ty = if input.peek(Token![:]) {
      input.parse::<Token![:]>()?;
      Some(input.parse()?)
    } else {
      None
    };
    let size = if input.peek(Token![=]) {
      input.parse::<Token![=]>()?;
      Some(input.parse()?)
    } else {
      None
    };
    Ok(Self { prefix, name, ty, size })
  }
}

/// Whether a region is eager unless it says `lazy` or `eager`: no, it is checked when it is read.
const REGION_EAGER: bool = false;

impl Region {
  /// Returns whether the region is eager ([`REGION_EAGER`]).
  const fn eager(&self) -> bool {
    self.prefix.is_eager(REGION_EAGER)
  }
}

/// A tag of `#[pun(tag(..))]`: `Key = Type`.
struct TagOption {
  key: Type,
  tag: Type,
}

impl Parse for TagOption {
  fn parse(input: ParseStream) -> Result<Self> {
    let key = input.parse()?;
    input.parse::<Token![=]>()?;
    Ok(Self { key, tag: input.parse()? })
  }
}

/// The aim of `#[pun(pointer(..))]`: `name: Type = EXPR` or `Type = EXPR`, the name of the methods
/// that read the pointee, if any, its type and where it is, after `eager` if it says so, then
/// `, len = EXPR` for an unsized pointee of a given length, `, size = EXPR` for a pointee of a
/// given size in bytes, or `, size = ..` for one that takes the rest of the tail of the struct.
/// Without `= EXPR`, the pointee is as far from the start of the struct as the integer of the field
/// says, as its layout says (`start`). A pointer is eager: `lazy` is refused, since a pointee at a
/// stored place is checked with the struct (`layout::Start::Stored`).
struct AimOption {
  name: Option<Ident>,
  pointee: Type,
  start: Option<Expr>,
  size: AimSize,
}

impl AimOption {
  /// Returns where the pointee is from the start of the struct whose field `member` has the aim:
  /// its expression, or the integer of the field, as its layout says, under a cast that
  /// sign-extends a signed one. The cast has the span of the field, so that a refusal of the cast
  /// points at the field.
  fn start(&self, member: &Member) -> Expr {
    let span = member.span();
    self.start.clone().unwrap_or_else(|| syn::parse_quote_spanned!(span=> self.#member as u128))
  }
}

/// How large the pointee of an aim is, as `layout::Extent` says.
enum AimSize {
  /// What the pointee stores: nothing if it is sized, its length otherwise.
  Pointee,
  /// The length of its tail, the value of the expression.
  Length(Expr),
  /// The value of the expression, in bytes.
  Bytes(Expr),
  /// The rest of the tail of the struct.
  Rest,
}

/// Returns whether `expr` is `self`, the value whose fields an expression reads as `self.field`.
fn is_self(expr: &Expr) -> bool {
  matches!(expr, Expr::Path(path) if path.path.is_ident("self"))
}

/// Returns whether the identifier that `input` starts with is the keyword `word` before what
/// follows it, rather than a name, which a `:`, a `=`, a `,` or nothing follows, or the start of a
/// path, which a `::` follows: `eager [u8] = ..`, `lazy mut name: u8` and `eager <T as X>::P = ..`
/// start with a keyword, `lazy: u8 = ..` and `regions(eager, rest)` with a name.
fn keyword(input: ParseStream, word: &str) -> bool {
  let after = input.fork();
  after.parse::<Ident>().is_ok_and(|ident| ident == word)
    && !after.is_empty()
    && !after.peek(Token![:])
    && !after.peek(Token![=])
    && !after.peek(Token![,])
}

impl Parse for AimOption {
  fn parse(input: ParseStream) -> Result<Self> {
    if keyword(input, "lazy") {
      return Err(Error::new(
        input.span(),
        "a `pointer` is eager: its pointee is checked with the struct, since nothing but the \
         bytes that hold the struct bounds where it is. A region at the cursor may be `lazy`",
      ));
    }
    // `eager` says what a pointer is anyway.
    input.parse::<Prefix>()?;
    // A name is an identifier then one colon, where a type may start with an identifier then two.
    let name = if input.peek(Ident) && input.peek2(Token![:]) && !input.peek2(Token![::]) {
      let name = input.parse()?;
      input.parse::<Token![:]>()?;
      Some(name)
    } else {
      None
    };
    let pointee = input.parse()?;
    let start = if input.peek(Token![=]) {
      input.parse::<Token![=]>()?;
      Some(input.parse()?)
    } else {
      None
    };
    let size = if input.peek(Token![,]) {
      input.parse::<Token![,]>()?;
      let key: Ident = input.parse()?;
      input.parse::<Token![=]>()?;
      match key.to_string().as_str() {
        "len" => AimSize::Length(input.parse()?),
        "size" if input.peek(Token![..]) => {
          input.parse::<Token![..]>()?;
          AimSize::Rest
        }
        "size" => AimSize::Bytes(input.parse()?),
        _ => {
          return Err(Error::new(
            key.span(),
            "a `pointer` takes `len = EXPR`, `size = EXPR` or `size = ..` after it",
          ));
        }
      }
    } else {
      AimSize::Pointee
    };
    Ok(Self { name, pointee, start, size })
  }
}

/// A getter of `#[pun(get(..))]`: `name: Type = EXPR`.
struct Getter {
  name: Ident,
  ty: Type,
  expr: Expr,
}

impl Parse for Getter {
  fn parse(input: ParseStream) -> Result<Self> {
    let name = input.parse()?;
    input.parse::<Token![:]>()?;
    let ty = input.parse()?;
    input.parse::<Token![=]>()?;
    Ok(Self { name, ty, expr: input.parse()? })
  }
}

/// The bounds of `#[pun(bound(..))]`.
type Bounds = Punctuated<syn::WherePredicate, Token![,]>;

/// The options of `#[pun(..)]` of a type, a variant or a field.
#[derive(Default)]
struct Options {
  root: Option<TokenStream>,
  tags: Vec<TagOption>,
  getters: Vec<Getter>,
  size: Option<Expr>,
  asserts: Vec<Expr>,
  valid: Option<Punctuated<Expr, Token![,]>>,
  len: Option<Expr>,
  regions: Option<Punctuated<Region, Token![,]>>,
  pointer: Option<AimOption>,
  /// The bounds of the implementation, instead of the ones that the derive finds.
  bounds: Option<Bounds>,
}

/// Returns the options of `attrs`, which are those of `place`.
///
/// Only the options in `allowed` mean something there.
///
/// # Errors
///
/// Returns an error if an option is not in `allowed`, rather than ignore it, if an option of one
/// value is given twice, rather than keep the last, or if an option is unknown or does not parse.
fn parse_options(attrs: &[Attribute], place: &str, allowed: &[&str]) -> Result<Options> {
  let mut options = Options::default();
  let mut given = Vec::new();
  for attr in attrs.iter().filter(|attr| attr.path().is_ident("pun")) {
    attr.parse_nested_meta(|meta| {
      if let Some(name) = meta.path.get_ident().map(ToString::to_string)
        && ALL_OPTIONS.contains(&name.as_str())
      {
        if !allowed.contains(&name.as_str()) {
          let of = match name.as_str() {
            "crate" => "the type",
            "bound" => "a struct, an enum with fields that is not a view, or a union",
            "size" | "assert" | "tag" | "get" => "a struct",
            "valid" => "a field",
            "pointer" => "a field of a struct",
            _ => "the last field of a struct",
          };
          return Err(meta.error(format!("`{name}` is an option of {of}, not of {place}")));
        }
        // `bound`, `assert`, `tag` and `get` add to what they gave before.
        if !matches!(name.as_str(), "bound" | "assert" | "tag" | "get") && given.contains(&name) {
          return Err(meta.error(format!("`{name}` is given twice")));
        }
        given.push(name);
      }
      if meta.path.is_ident("crate") {
        options.root = Some(if meta.input.peek(Token![=]) {
          meta.value()?.parse::<Path>()?.into_token_stream()
        } else {
          quote!(crate)
        });
      } else if meta.path.is_ident("bound") {
        let content;
        syn::parenthesized!(content in meta.input);
        let bounds = Bounds::parse_terminated(&content)?;
        options.bounds.get_or_insert_default().extend(bounds);
      } else if meta.path.is_ident("size") {
        options.size = Some(meta.value()?.parse()?);
      } else if meta.path.is_ident("len") {
        options.len = Some(meta.value()?.parse()?);
      } else if meta.path.is_ident("assert") {
        let content;
        syn::parenthesized!(content in meta.input);
        options.asserts.push(content.parse()?);
      } else if meta.path.is_ident("tag") {
        let content;
        syn::parenthesized!(content in meta.input);
        options.tags.extend(Punctuated::<TagOption, Token![,]>::parse_terminated(&content)?);
      } else if meta.path.is_ident("get") {
        let content;
        syn::parenthesized!(content in meta.input);
        options.getters.extend(Punctuated::<Getter, Token![,]>::parse_terminated(&content)?);
      } else if meta.path.is_ident("valid") {
        let content;
        syn::parenthesized!(content in meta.input);
        options.valid = Some(Punctuated::parse_terminated(&content)?);
      } else if meta.path.is_ident("regions") {
        let content;
        syn::parenthesized!(content in meta.input);
        options.regions = Some(Punctuated::parse_terminated(&content)?);
      } else if meta.path.is_ident("pointer") {
        let content;
        syn::parenthesized!(content in meta.input);
        options.pointer = Some(content.parse()?);
      } else {
        return Err(meta.error("unknown `pun` option"));
      }
      Ok(())
    })?;
  }
  Ok(options)
}

/// Every option of `#[pun(..)]`.
const ALL_OPTIONS: [&str; 10] =
  ["crate", "bound", "size", "len", "assert", "tag", "get", "valid", "regions", "pointer"];

// -------------------------------------------------------------------------------------------------
// Expressions

/// A field of a struct, as the derive compiles it.
struct FieldInfo<'a> {
  member: Member,
  /// Its type, as written: what the bounds name, and what the derive reads of it. For the field
  /// of an eager aim, an `InPlace<T>`, it is `T`, the type of the field in the layout
  /// (`<F as InPlaceOf>::Inner`).
  ty: &'a Type,
  /// Its type, from its home (see [`homes`]): what the generated code names.
  home: TokenStream,
  vis: &'a Visibility,
  options: Options,
  /// Its offset in the struct, as a constant expression.
  offset: TokenStream,
}

/// The depth of the stack of the evaluator, which `layout::check_ops` checks again.
///
/// The validation applies each operation in an instance of a function of its own, down a tree of
/// the operations, so an expression also has at most [`MAX_OPS`] operations and nests at most
/// [`MAX_WALKS`] walks. It has at most [`VARIABLES`] variables.
const STACK: usize = 16;
/// The number of variables of an expression.
const VARIABLES: usize = 8;
/// The number of operations of an expression.
const MAX_OPS: usize = 256;
/// The depth of walks within walks of an expression.
const MAX_WALKS: usize = 4;

/// Returns `tokens` located at `span`, so that an error in what they compute is reported there.
///
/// What their names resolve to does not change.
fn located(tokens: TokenStream, span: Span) -> TokenStream {
  tokens
    .into_iter()
    .map(|mut token| {
      if let proc_macro2::TokenTree::Group(group) = &token {
        let mut located = proc_macro2::Group::new(group.delimiter(), located(group.stream(), span));
        located.set_span(group.span().located_at(span));
        token = proc_macro2::TokenTree::Group(located);
      } else {
        token.set_span(token.span().located_at(span));
      }
      token
    })
    .collect()
}

/// Returns the name of an operation: the first identifier after `Op::`.
fn op_name(op: &TokenStream) -> Option<String> {
  let tokens = op.clone().into_iter().collect::<Vec<_>>();
  tokens.windows(4).find_map(|window| match window {
    [
      proc_macro2::TokenTree::Ident(op),
      proc_macro2::TokenTree::Punct(_),
      proc_macro2::TokenTree::Punct(_),
      proc_macro2::TokenTree::Ident(name),
    ] if op == "Op" => Some(name.to_string()),
    _ => None,
  })
}

/// Returns how many values an operation takes off the stack of the evaluator, and how many it
/// puts on it.
fn effect(op: &TokenStream) -> (usize, usize) {
  match op_name(op).as_deref() {
    Some("Lit" | "load" | "load_as" | "Size" | "Get") => (0, 1),
    Some("at" | "Varint" | "Not" | "Walk" | "Shortcut" | "Then") => (1, 1),
    // After a branch, which it does not take off: see `layout::check_ops`.
    Some("Else") => (0, 0),
    Some("Select") => (3, 1),
    Some("CrcZeros") => (4, 1),
    Some("Set") => (1, 0),
    _ => (2, 1),
  }
}

/// Checks that `ops`, the operations of what `spanned` is, fit the stack of the evaluator, and
/// are not too many.
///
/// # Errors
///
/// Returns an error at `spanned`, with the numbers, if there are more than [`MAX_OPS`]
/// operations or they need more than [`STACK`] values.
fn limits(ops: &[TokenStream], spanned: &impl ToTokens) -> Result<()> {
  if ops.len() > MAX_OPS {
    return Err(Error::new_spanned(
      spanned,
      format!(
        "this expression has {} operations, and one has at most {MAX_OPS}: the validation applies \
         each in a function of its own. Split it into several `assert`s, or name what it \
         computes several times with `let`",
        ops.len()
      ),
    ));
  }
  let (mut depth, mut deepest) = (0usize, 0);
  for op in ops {
    let (pops, pushes) = effect(op);
    depth = depth.saturating_sub(pops) + pushes;
    deepest = deepest.max(depth);
  }
  if deepest > STACK {
    return Err(Error::new_spanned(
      spanned,
      format!(
        "this expression needs a stack of {deepest} values, and the one of the evaluator has \
         {STACK}: name what it computes with `let`, or compute it in another order \
         (`a + b + c` needs 2, `a + (b + c)` needs 3)"
      ),
    ));
  }
  Ok(())
}

/// The type of a value of an expression, as Rust types it, where every integer is a `u128`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ty {
  /// An integer.
  Int,
  /// A `bool`.
  Bool,
}

impl Ty {
  /// Returns the name of the type in a message.
  const fn name(self) -> &'static str {
    match self {
      Ty::Int => "an integer",
      Ty::Bool => "a `bool`",
    }
  }
}

/// Checks that `found` is `expected`.
///
/// # Errors
///
/// Returns an error at `spanned`, with a hint, if `found` is not `expected`.
fn expect(found: Ty, expected: Ty, spanned: &impl ToTokens) -> Result<()> {
  if found == expected {
    return Ok(());
  }
  let hint = match expected {
    Ty::Int => "`EXPR as u8` is `0` or `1`",
    Ty::Bool => "Rust does not convert an integer to a `bool`: write `EXPR != 0`",
  };
  Err(Error::new_spanned(
    spanned,
    format!("expected {}, found {} ({hint})", expected.name(), found.name()),
  ))
}

/// The variables of an expression: its `let`s, and the parameters of the closures of a `walk`.
#[derive(Default)]
struct Scope {
  /// Each variable in scope: its name, its index and its type.
  names: Vec<(Ident, u8, Ty)>,
  /// How many closures of `walk`s the expression is in.
  walking: usize,
  /// For each `walk` of the expression that is being compiled, in order (not those in another
  /// `walk`): its two expressions, as types.
  walks: Vec<(TokenStream, TokenStream)>,
  /// Whether the expression is a `size` or a `len`, which cannot be computed from
  /// `size_of_val(self)`.
  sizing: bool,
}

impl Scope {
  /// Declares the variable `name` of type `ty`, and returns its index.
  ///
  /// # Errors
  ///
  /// Returns an error if the expression already has [`VARIABLES`] variables in scope.
  fn declare(&mut self, name: &Ident, ty: Ty) -> Result<u8> {
    let index = self.names.len() as u8;
    if usize::from(index) >= VARIABLES {
      return Err(Error::new(
        name.span(),
        format!(
          "too many variables: an expression has at most {VARIABLES} at once (its `let`s, and the \
           parameters of its `walk`s)"
        ),
      ));
    }
    self.names.push((name.clone(), index, ty));
    Ok(index)
  }

  /// Returns the index and the type of the variable `name` in scope, the last one declared.
  fn get(&self, name: &Ident) -> Option<(u8, Ty)> {
    self
      .names
      .iter()
      .rev()
      .find(|(declared, ..)| declared == name)
      .map(|(_, index, ty)| (*index, *ty))
  }
}

/// Compiles the block `{ let name = EXPR; .. EXPR }` to operations, and returns its type.
///
/// # Errors
///
/// Returns an error if a statement is not `let name = EXPR;` or `let name: TYPE = EXPR;`, if the
/// block does not end with its value, or if an expression does not compile.
fn compile_block(
  block: &syn::Block,
  fields: &[FieldInfo],
  root: &TokenStream,
  ops: &mut Vec<TokenStream>,
  scope: &mut Scope,
) -> Result<Ty> {
  let (op, visible) = (quote!(#root::layout::Op), scope.names.len());
  let Some((last, statements)) = block.stmts.split_last() else {
    return Err(Error::new(block.span(), "a block has a value"));
  };
  for statement in statements {
    let syn::Stmt::Local(local) = statement else {
      return Err(Error::new(statement.span(), "expected `let name = EXPR;`"));
    };
    // `name`, or `name: TYPE`.
    let (name, annotation) = match &local.pat {
      syn::Pat::Ident(pat) => (&pat.ident, None),
      syn::Pat::Type(typed) => match &*typed.pat {
        syn::Pat::Ident(pat) => (&pat.ident, Some(&*typed.ty)),
        pat => return Err(Error::new(pat.span(), "expected a name")),
      },
      pat => return Err(Error::new(pat.span(), "expected a name")),
    };
    let Some(syn::LocalInit { expr, diverge: None, .. }) = &local.init else {
      return Err(Error::new(local.span(), "expected `let name = EXPR;`"));
    };
    let ty = compile(expr, fields, root, ops, scope)?;
    if let Some(annotation) = annotation {
      let named = match annotation {
        Type::Path(path) if path.path.is_ident("u128") => Some(Ty::Int),
        Type::Path(path) if path.path.is_ident("bool") => Some(Ty::Bool),
        _ => None,
      };
      if named != Some(ty) {
        return Err(Error::new_spanned(
          annotation,
          format!(
            "the values of a rule are `u128`s and `bool`s: this one is a `{}`",
            if ty == Ty::Int { "u128" } else { "bool" }
          ),
        ));
      }
    }
    // What a `let` binds is computed, as in Rust, whether it is used or not.
    let index = scope.declare(name, ty)?;
    ops.push(quote!(#op::Set(#index)));
  }
  let syn::Stmt::Expr(value, None) = last else {
    return Err(Error::new(last.span(), "a block ends with its value"));
  };
  let ty = compile(value, fields, root, ops, scope)?;
  scope.names.truncate(visible);
  Ok(ty)
}

/// Returns the number of operations in a part of an expression that may not be computed, as an
/// operation holds it (`layout::Op::Shortcut`).
///
/// # Errors
///
/// Returns an error if the number does not fit a `u16`.
fn part(len: usize) -> Result<u16> {
  u16::try_from(len).map_err(|_| Error::new(Span::call_site(), "this expression is too long"))
}

/// Compiles `expr` to the operations of a postfix expression, and returns its type.
///
/// # Errors
///
/// Returns an error at what Rust would not compile, or at what a rule does not compute.
fn compile(
  expr: &Expr,
  fields: &[FieldInfo],
  root: &TokenStream,
  ops: &mut Vec<TokenStream>,
  scope: &mut Scope,
) -> Result<Ty> {
  let op = quote!(#root::layout::Op);
  // A field, where it is named: if it is not an integer, the error is there.
  let load = |member: &Member, span: Span, ops: &mut Vec<TokenStream>| {
    let field = fields.iter().find(|field| &field.member == member)?;
    let (ty, offset) = (&field.home, &field.offset);
    ops.push(located(quote!(#op::load::<#ty>(#offset)), span));
    Some(())
  };
  // An integer, of what `expr` holds.
  let int = |expr: &Expr, ops: &mut Vec<TokenStream>, scope: &mut Scope| {
    let ty = compile(expr, fields, root, ops, scope)?;
    expect(ty, Ty::Int, expr)
  };
  Ok(match expr {
    Expr::Paren(expr) => compile(&expr.expr, fields, root, ops, scope)?,
    Expr::Group(expr) => compile(&expr.expr, fields, root, ops, scope)?,
    // `as` to an unsigned integer: the bits that it has, as in Rust.
    Expr::Cast(cast) => {
      let mask = cast_mask(&cast.ty, &op)?;
      // Of a constant: what Rust makes of it, `-1i8 as u8` or `'a' as u32`.
      if !mentions(cast.expr.to_token_stream(), fields, scope) {
        ops.push(quote_spanned!(expr.span()=> #op::Lit(#op::constant(&(#expr)))));
        return Ok(Ty::Int);
      }
      let ty = match named_field(&cast.expr, fields, scope) {
        // A field, signed or not: its value as `as` converts it, a signed one sign-extended.
        Some((field, span)) => {
          let (ty, offset) = (&field.home, &field.offset);
          ops.push(located(quote!(#op::load_as::<#ty>(#offset)), span));
          Ty::Int
        }
        None => compile(&cast.expr, fields, root, ops, scope)?,
      };
      if let (Some(mask), Ty::Int) = (mask, ty) {
        ops.push(quote!(#op::Lit(#mask)));
        ops.push(quote!(#op::And));
      }
      Ty::Int
    }
    Expr::Lit(ExprLit { lit: Lit::Bool(lit), .. }) => {
      let value = u128::from(lit.value);
      ops.push(quote!(#op::Lit(#value)));
      Ty::Bool
    }
    Expr::Lit(ExprLit { lit, .. }) => {
      let value = literal(lit)?;
      ops.push(quote!(#op::Lit(#value)));
      Ty::Int
    }
    Expr::Unary(unary) if matches!(unary.op, UnOp::Neg(_)) => {
      return Err(Error::new_spanned(
        unary,
        "a rule computes on unsigned integers: there is no negative number",
      ));
    }
    Expr::Block(block) => compile_block(&block.block, fields, root, ops, scope)?,
    // `if COND { EXPR } else { EXPR }`
    Expr::If(branch) => {
      let Some((_, otherwise)) = &branch.else_branch else {
        return Err(Error::new(branch.span(), "an `if` has an `else`"));
      };
      let cond = compile(&branch.cond, fields, root, ops, scope)?;
      expect(cond, Ty::Bool, &branch.cond)?;
      // Only the branch that is taken is computed: `Then` skips the first one and its `Else`,
      // `Else` the second one.
      let start = ops.len();
      let then = compile_block(&branch.then_branch, fields, root, ops, scope)?;
      let middle = ops.len();
      let other = compile(otherwise, fields, root, ops, scope)?;
      expect(other, then, otherwise)?;
      let (first, second) = (part(middle - start + 1)?, part(ops.len() - middle)?);
      ops.insert(middle, quote!(#op::Else(#second)));
      ops.insert(start, quote!(#op::Then(#first)));
      ops.push(quote!(#op::Select));
      then
    }
    // A variable.
    Expr::Path(path) if path.path.get_ident().is_some_and(|ident| scope.get(ident).is_some()) => {
      let Some((index, ty)) = path.path.get_ident().and_then(|ident| scope.get(ident)) else {
        unreachable!()
      };
      ops.push(quote!(#op::Get(#index)));
      ty
    }
    Expr::Path(path)
      if path
        .path
        .get_ident()
        .is_some_and(|ident| load(&Member::Named(ident.clone()), ident.span(), ops).is_some()) =>
    {
      Ty::Int
    }
    Expr::Field(field) if is_self(&field.base) => {
      load(&field.member, field.span(), ops)
        .ok_or_else(|| Error::new(field.member.span(), "no such field"))?;
      Ty::Int
    }
    // `!`: bitwise on an integer, logical on a `bool`.
    Expr::Unary(unary) if matches!(unary.op, UnOp::Not(_)) => {
      let ty = compile(&unary.expr, fields, root, ops, scope)?;
      match ty {
        Ty::Int => {
          ops.push(quote!(#op::Lit(0xffff_ffff_ffff_ffff_ffff_ffff_ffff_ffff)));
          ops.push(quote!(#op::Xor));
        }
        Ty::Bool => ops.push(quote!(#op::Not)),
      }
      ty
    }
    Expr::Binary(binary) => {
      // The name of the operation, the type of its operands (`None` for either type, if both are
      // the same), and whether it compares them.
      let (name, operands, compares) = match binary.op {
        BinOp::Add(_) => ("Add", Some(Ty::Int), false),
        BinOp::Sub(_) => ("Sub", Some(Ty::Int), false),
        BinOp::Mul(_) => ("Mul", Some(Ty::Int), false),
        BinOp::Div(_) => ("Div", Some(Ty::Int), false),
        BinOp::Rem(_) => ("Rem", Some(Ty::Int), false),
        BinOp::Shl(_) => ("Shl", Some(Ty::Int), false),
        BinOp::Shr(_) => ("Shr", Some(Ty::Int), false),
        BinOp::BitAnd(_) => ("And", None, false),
        BinOp::BitOr(_) => ("Or", None, false),
        BinOp::BitXor(_) => ("Xor", None, false),
        BinOp::Eq(_) => ("Eq", None, true),
        BinOp::Ne(_) => ("Ne", None, true),
        BinOp::Lt(_) => ("Lt", None, true),
        BinOp::Le(_) => ("Le", None, true),
        BinOp::Gt(_) => ("Gt", None, true),
        BinOp::Ge(_) => ("Ge", None, true),
        BinOp::And(_) => ("Both", Some(Ty::Bool), false),
        BinOp::Or(_) => ("Either", Some(Ty::Bool), false),
        _ => return Err(Error::new(binary.op.span(), "unsupported operator")),
      };
      let left = compile(&binary.left, fields, root, ops, scope)?;
      if let Some(ty) = operands {
        expect(left, ty, &binary.left)?;
      }
      let start = ops.len();
      let right = compile(&binary.right, fields, root, ops, scope)?;
      expect(right, left, &binary.right)?;
      // The right side of `&&` and `||` is only computed if the left one does not decide.
      if let BinOp::And(_) | BinOp::Or(_) = binary.op {
        let (either, len) = (matches!(binary.op, BinOp::Or(_)), part(ops.len() - start + 1)?);
        ops.insert(start, quote!(#op::Shortcut { either: #either, len: #len }));
      }
      let name = Ident::new(name, binary.op.span());
      ops.push(quote!(#op::#name));
      if compares { Ty::Bool } else { left }
    }
    // `size_of_val(self)`
    Expr::Call(call)
      if call.args.len() == 1
        && matches!(&*call.func, Expr::Path(path)
          if path.path.segments.last().is_some_and(|segment| segment.ident == "size_of_val"))
        && is_self(&call.args[0]) =>
    {
      if scope.sizing {
        return Err(Error::new_spanned(
          call,
          "a size or a length cannot be computed from `size_of_val(self)`, which it defines",
        ));
      }
      ops.push(quote!(#op::Size));
      Ty::Int
    }
    // `crc32(start, end)`, `sum16(start, end)`, `adler32(start, end)`...: over the bytes of the
    // value; a CRC can take a range as zeros.
    Expr::Call(call) if checksum(call, &op, root).is_some() => {
      let Some(checksum) = checksum(call, &op, root) else { unreachable!() };
      for arg in &call.args {
        int(arg, ops, scope)?;
      }
      ops.push(checksum?);
      Ty::Int
    }
    // `le(offset, size)` and `be(offset, size)`: an unsigned integer, anywhere in the value.
    Expr::Call(call) if call.args.len() == 2 && (is_call(call, "le") || is_call(call, "be")) => {
      let (size, endian) = (&call.args[1], if is_call(call, "be") { "Big" } else { "Little" });
      let endian = format_ident!("{endian}");
      int(&call.args[0], ops, scope)?;
      let endian = quote!(#root::layout::Endian::#endian);
      ops.push(located(quote!(#op::at((#size) as usize, #endian)), size.span()));
      Ty::Int
    }
    // `matches(DFA, start, end)`: whether an automaton accepts the bytes.
    Expr::Call(call) if call.args.len() == 3 && is_call(call, "matches") => {
      int(&call.args[1], ops, scope)?;
      int(&call.args[2], ops, scope)?;
      let dfa = &call.args[0];
      ops.push(located(quote!(#op::matches(#dfa)), dfa.span()));
      Ty::Bool
    }
    // `leb128(offset)`, `quic_len(offset)`...: a variable-length integer, or its length.
    Expr::Call(call) if call.args.len() == 1 && varint(call).is_some() => {
      let Some((form, length)) = varint(call) else { unreachable!() };
      int(&call.args[0], ops, scope)?;
      let form = Ident::new(form, call.func.span());
      ops.push(quote!(#op::Varint { form: #root::layout::Varint::#form, length: #length }));
      Ty::Int
    }
    // `walk(START, |at| COND, |at| NEXT)`: `START`, replaced by `NEXT` for as long as `COND`.
    Expr::Call(call) if call.args.len() == 3 && is_call(call, "walk") => {
      if scope.walking >= MAX_WALKS {
        return Err(Error::new_spanned(
          call,
          format!(
            "walks are nested {} deep here, and at most {MAX_WALKS}: the validation applies their \
             expressions in functions of their own",
            scope.walking + 1
          ),
        ));
      }
      int(&call.args[0], ops, scope)?;
      let (mut var, mut programs, mut types) = (0, vec![], vec![]);
      for (closure, ty) in [(&call.args[1], Ty::Bool), (&call.args[2], Ty::Int)] {
        let parameter = match closure {
          Expr::Closure(closure) if closure.inputs.len() == 1 => match &closure.inputs[0] {
            syn::Pat::Ident(pat) => Some((&pat.ident, &closure.body)),
            _ => None,
          },
          _ => None,
        };
        let Some((name, body)) = parameter else {
          return Err(Error::new(closure.span(), "expected `|name| EXPR`"));
        };
        let (visible, mut program) = (scope.names.len(), vec![]);
        var = scope.declare(name, Ty::Int)?;
        // The walks of this expression are its own.
        let outer = std::mem::take(&mut scope.walks);
        scope.walking += 1;
        let compiled = compile(body, fields, root, &mut program, scope);
        scope.walking -= 1;
        let inner = std::mem::replace(&mut scope.walks, outer);
        scope.names.truncate(visible);
        expect(compiled?, ty, body)?;
        limits(&program, body)?;
        types.push(steps(&program, inner, root));
        programs.push(program);
      }
      let (cond, next) = (&programs[0], &programs[1]);
      scope.walks.push((types[0].clone(), types[1].clone()));
      ops.push(quote!(#op::Walk {
        var: #var,
        cond: const { &[#(#cond),*] },
        next: const { &[#(#next),*] },
      }));
      Ty::Int
    }
    // Anything else must be a constant, which Rust computes.
    expr => {
      constant(expr, fields, scope)?;
      ops.push(quote_spanned!(expr.span()=> #op::Lit(#op::constant(&(#expr)))));
      Ty::Int
    }
  })
}

/// Returns the field that `expr` names, alone or in parentheses, and where it names it: `name`,
/// unless a variable of `scope` has that name, or `self.name`.
fn named_field<'f, 'a>(
  expr: &Expr,
  fields: &'f [FieldInfo<'a>],
  scope: &Scope,
) -> Option<(&'f FieldInfo<'a>, Span)> {
  match expr {
    Expr::Paren(inner) => named_field(&inner.expr, fields, scope),
    Expr::Group(inner) => named_field(&inner.expr, fields, scope),
    Expr::Path(path) => {
      let ident = path.path.get_ident().filter(|ident| scope.get(ident).is_none())?;
      let member = Member::Named(ident.clone());
      fields.iter().find(|field| field.member == member).map(|field| (field, ident.span()))
    }
    Expr::Field(field) if is_self(&field.base) => fields
      .iter()
      .find(|found| found.member == field.member)
      .map(|found| (found, field.member.span())),
    _ => None,
  }
}

/// The functions that a rule can call, as messages name them.
const FUNCTIONS: &str = "`size_of_val(self)`, `le`, `be`, `leb128`, `leb128_len`, `quic`, \
                         `quic_len`, `vlq`, `vlq_len`, `crc32`, `crc::<..>`, `sum16`, `sum`, \
                         `fletcher16`, `adler32`, `matches` and `walk`";

/// The functions of rules, and how many arguments each takes (`crc32` and `crc` also take 4).
const ARITIES: [(&str, usize); 16] = [
  ("le", 2),
  ("be", 2),
  ("leb128", 1),
  ("leb128_len", 1),
  ("quic", 1),
  ("quic_len", 1),
  ("vlq", 1),
  ("vlq_len", 1),
  ("crc32", 2),
  ("crc", 2),
  ("sum16", 2),
  ("sum", 2),
  ("fletcher16", 2),
  ("adler32", 2),
  ("matches", 3),
  ("walk", 3),
];

/// Returns the operation of the checksum that `call` is, if it is one with the right number of
/// arguments.
///
/// # Errors
///
/// Returns `Some` of an error if a call of `crc` lacks its 5 parameters.
fn checksum(
  call: &syn::ExprCall,
  op: &TokenStream,
  root: &TokenStream,
) -> Option<Result<TokenStream>> {
  let layout = quote!(#root::layout);
  let zeros = call.args.len() == 4;
  if call.args.len() == 2 || zeros {
    if is_call(call, "crc32") {
      return Some(Ok(if zeros {
        quote!(#op::CrcZeros(const { &#layout::Crc::CRC32 }))
      } else {
        quote!(#op::Crc32)
      }));
    }
    // `crc::<WIDTH, POLY, INIT, REFLECT, XOROUT>(..)`
    if let Expr::Path(path) = &*call.func
      && path.qself.is_none()
      && path.path.segments.len() == 1
      && path.path.segments[0].ident == "crc"
    {
      let syn::PathArguments::AngleBracketed(arguments) = &path.path.segments[0].arguments else {
        return Some(Err(Error::new_spanned(
          path,
          "a CRC has parameters: `crc::<WIDTH, POLY, INIT, REFLECT, XOROUT>(start, end)`",
        )));
      };
      if arguments.args.len() != 5 {
        return Some(Err(Error::new_spanned(
          arguments,
          "a CRC has 5 parameters: `crc::<WIDTH, POLY, INIT, REFLECT, XOROUT>(start, end)`",
        )));
      }
      let parameters = arguments.args.iter();
      let variant = if zeros { quote!(CrcZeros) } else { quote!(Crc) };
      // An error of the parameters is at the call.
      let crc = located(quote!(#layout::Crc::new(#(#parameters),*)), call.span());
      return Some(Ok(quote!(#op::#variant(const { &#crc }))));
    }
  }
  if call.args.len() == 2 {
    let checksum = [
      ("sum16", None),
      ("sum", Some("Sum")),
      ("fletcher16", Some("Fletcher16")),
      ("adler32", Some("Adler32")),
    ]
    .into_iter()
    .find(|(name, _)| is_call(call, name))?;
    return Some(Ok(checksum.1.map_or_else(
      || quote!(#op::Sum16),
      |name| {
        let name = Ident::new(name, call.func.span());
        quote!(#op::Checksum(#layout::Checksum::#name))
      },
    )));
  }
  None
}

/// Returns the form of the variable-length integer that `call` reads, and whether `call` reads
/// its length.
fn varint(call: &syn::ExprCall) -> Option<(&'static str, bool)> {
  [
    ("leb128", "Leb128", false),
    ("leb128_len", "Leb128", true),
    ("quic", "Quic", false),
    ("quic_len", "Quic", true),
    ("vlq", "Vlq", false),
    ("vlq_len", "Vlq", true),
  ]
  .into_iter()
  .find(|(name, ..)| is_call(call, name))
  .map(|(_, form, length)| (form, length))
}

/// Returns the value of a literal as a `u128`.
///
/// The literal is an integer or a byte. The suffix of an integer, if any, must be an unsigned
/// integer type.
///
/// # Errors
///
/// Returns an error if the literal is not an integer or a byte, or if its suffix is not an
/// unsigned integer type.
fn literal(lit: &Lit) -> Result<TokenStream> {
  let unsigned = "`#[pun(..)]` computes on unsigned integers";
  match lit {
    Lit::Int(int) => match int.suffix() {
      "" => {
        let value = int.base10_parse::<u128>()?;
        Ok(quote!(#value))
      }
      "u8" | "u16" | "u32" | "u64" | "u128" | "usize" => Ok(quote!((#int as u128))),
      suffix => Err(Error::new(lit.span(), format!("{unsigned}: `{suffix}` is not one"))),
    },
    Lit::Byte(byte) => Ok(quote!((#byte as u128))),
    Lit::Float(_) => Err(Error::new(
      lit.span(),
      format!("{unsigned}: a floating-point number is not one (scale both sides instead)"),
    )),
    Lit::Char(_) => Err(Error::new(
      lit.span(),
      format!("{unsigned}: a `char` is not one (write `b'a'`, or `'a' as u32`)"),
    )),
    _ => Err(Error::new(lit.span(), format!("{unsigned}: this literal is not one"))),
  }
}

/// Returns whether `tokens` name a field or a variable of the expression, or `self`, which a
/// constant cannot name.
fn mentions(tokens: TokenStream, fields: &[FieldInfo], scope: &Scope) -> bool {
  // After `.` or `::`, a name is a field or an item of something else.
  let mut qualified = false;
  let mut tokens = tokens.into_iter().peekable();
  while let Some(token) = tokens.next() {
    match token {
      proc_macro2::TokenTree::Ident(ident) => {
        // `self::` starts the path of an item of this module, such as `self::f(3)`, not the value.
        let path = matches!(tokens.peek(), Some(proc_macro2::TokenTree::Punct(punct)) if punct.as_char() == ':');
        let named = (ident == "self" && !path)
          || scope.get(&ident).is_some()
          || fields.iter().any(|field| field.member == Member::Named(ident.clone()));
        if named && !qualified {
          return true;
        }
        qualified = false;
      }
      proc_macro2::TokenTree::Punct(punct) => {
        qualified = matches!(punct.as_char(), '.' | ':');
      }
      proc_macro2::TokenTree::Group(group) => {
        if mentions(group.stream(), fields, scope) {
          return true;
        }
        qualified = false;
      }
      proc_macro2::TokenTree::Literal(_) => qualified = false,
    }
  }
  false
}

/// Checks that `expr`, which is not something that a rule computes, is a constant.
///
/// Rust computes a constant, which must be an unsigned integer. A macro (`offset_of!(Self,
/// field)`) is one.
///
/// # Errors
///
/// Returns an error, which says what a rule computes, if `expr` names a field, a variable or
/// `self`, or if it calls a function by its name alone: a function of rules with the wrong number
/// of arguments, or another function.
fn constant(expr: &Expr, fields: &[FieldInfo], scope: &Scope) -> Result<()> {
  let error = |message: String| Err(Error::new_spanned(expr, message));
  let bytes = "read its bytes with `le(offset, size)` or `be(offset, size)`";
  match expr {
    Expr::Macro(_) => Ok(()),
    Expr::Call(call) if is_crc(call) => error(
      "`crc::<WIDTH, POLY, INIT, REFLECT, XOROUT>` takes 2 arguments, or 4 (the last two a range \
       taken as zeros)"
        .into(),
    ),
    // A function of rules is called by its name alone.
    Expr::Call(call) if matches!(&*call.func, Expr::Path(path) if path.path.get_ident().is_some()) =>
    {
      let name = call.func.to_token_stream();
      if let Some((_, arity)) = ARITIES.iter().find(|(f, _)| is_call(call, f)) {
        let more = if is_call(call, "crc32") { " (or 4)" } else { "" };
        return error(format!("`{name}` takes {arity} arguments{more}"));
      }
      error(format!(
        "`{name}` is not a function of rules, which are {FUNCTIONS}; a constant can be computed \
         by a `const fn` named by its path (`self::{name}(..)`)"
      ))
    }
    _ if !mentions(expr.to_token_stream(), fields, scope) => Ok(()),
    Expr::Path(_) => error("`self` is only `size_of_val(self)` in a rule".into()),
    Expr::Index(_) => error(format!(
      "a rule reads the integer fields of its struct, not the elements of an array: {bytes}"
    )),
    Expr::Field(_) => error(format!(
      "a rule reads the integer fields of its struct, not the fields of a field: {bytes}"
    )),
    Expr::MethodCall(_) => error(format!(
      "a rule cannot call methods: it computes with operators, `if`, `let`, and {FUNCTIONS}"
    )),
    Expr::Call(_) => error(format!(
      "a rule cannot call this function: it computes with operators, `if`, `let`, and {FUNCTIONS}"
    )),
    _ => error(format!(
      "not something that a rule computes: it computes with operators, `if`, `let`, and \
       {FUNCTIONS}"
    )),
  }
}

/// Returns what `as TYPE` keeps of a `u128`: `None` for `u128`, else the mask of the bits of
/// another unsigned integer type.
///
/// # Errors
///
/// Returns an error if `ty` is not an unsigned integer type, since rules compute on those only.
fn cast_mask(ty: &Type, op: &TokenStream) -> Result<Option<TokenStream>> {
  let name = match ty {
    Type::Path(path) if path.qself.is_none() => path.path.get_ident().map(ToString::to_string),
    _ => None,
  };
  Ok(match name.as_deref() {
    Some("u128") => None,
    Some("u64") => Some(quote!(0xffff_ffff_ffff_ffff)),
    Some("u32") => Some(quote!(0xffff_ffff)),
    Some("u16") => Some(quote!(0xffff)),
    Some("u8") => Some(quote!(0xff)),
    Some("usize") => Some(quote!(#op::USIZE_MAX)),
    _ => {
      return Err(Error::new(
        ty.span(),
        "a rule computes on unsigned integers: `as` converts to `u8`, `u16`, `u32`, `u64`, \
         `u128` or `usize`",
      ));
    }
  })
}

/// Returns whether `call` is a call of `crc::<..>`.
fn is_crc(call: &syn::ExprCall) -> bool {
  matches!(&*call.func, Expr::Path(path) if path.qself.is_none()
    && path.path.segments.len() == 1 && path.path.segments[0].ident == "crc")
}

/// Returns whether `call` is a call of the function `name`.
fn is_call(call: &syn::ExprCall, name: &str) -> bool {
  matches!(&*call.func, Expr::Path(path) if path.path.is_ident(name))
}

/// Returns the expression of a getter as Rust, in which an integer field is its value as a
/// `u128`, wherever it is named, by its name or as `self.name`.
fn rust(expr: &Expr, fields: &[FieldInfo], root: &TokenStream, packed: bool) -> TokenStream {
  let mut expr = expr.clone();
  Fielder { fields, root, packed, shadowed: vec![] }.visit_expr_mut(&mut expr);
  expr.into_token_stream()
}

/// What replaces the fields of the expression of a getter.
struct Fielder<'a> {
  /// The fields of the struct.
  fields: &'a [FieldInfo<'a>],
  /// The path of the `puns` crate.
  root: &'a TokenStream,
  /// The fields of a packed struct cannot be borrowed: they are copied.
  packed: bool,
  /// The names that are not fields, where they are declared.
  shadowed: Vec<Ident>,
}

impl Fielder<'_> {
  /// Returns the value of `member` as a `u128`, read where `span` is, if it is a field.
  fn value(&self, member: &Member, span: Span) -> Option<Expr> {
    self.fields.iter().any(|field| &field.member == member).then(|| {
      let root = self.root;
      let value = if self.packed {
        quote!((#root::int(&{ self.#member })))
      } else {
        quote!((#root::int(&self.#member)))
      };
      // An error, if the field is not an unsigned integer, is there.
      Expr::Verbatim(located(value, span))
    })
  }
}

/// Appends to `names` the names that `pat` declares.
fn declared(pat: &syn::Pat, names: &mut Vec<Ident>) {
  match pat {
    syn::Pat::Ident(pat) => {
      names.push(pat.ident.clone());
      if let Some((_, pat)) = &pat.subpat {
        declared(pat, names);
      }
    }
    syn::Pat::Type(pat) => declared(&pat.pat, names),
    syn::Pat::Reference(pat) => declared(&pat.pat, names),
    syn::Pat::Paren(pat) => declared(&pat.pat, names),
    syn::Pat::Tuple(pat) => pat.elems.iter().for_each(|pat| declared(pat, names)),
    syn::Pat::TupleStruct(pat) => pat.elems.iter().for_each(|pat| declared(pat, names)),
    syn::Pat::Slice(pat) => pat.elems.iter().for_each(|pat| declared(pat, names)),
    syn::Pat::Or(pat) => pat.cases.iter().for_each(|pat| declared(pat, names)),
    syn::Pat::Struct(pat) => pat.fields.iter().for_each(|field| declared(&field.pat, names)),
    _ => {}
  }
}

impl VisitMut for Fielder<'_> {
  fn visit_expr_mut(&mut self, expr: &mut Expr) {
    let value = match expr {
      Expr::Path(path) if path.qself.is_none() => path
        .path
        .get_ident()
        .filter(|ident| !self.shadowed.contains(ident))
        .and_then(|ident| self.value(&Member::Named(ident.clone()), ident.span())),
      Expr::Field(access) if is_self(&access.base) => self.value(&access.member, access.span()),
      Expr::Closure(closure) => {
        let visible = self.shadowed.len();
        let mut names = vec![];
        closure.inputs.iter().for_each(|pat| declared(pat, &mut names));
        self.shadowed.extend(names);
        self.visit_expr_mut(&mut closure.body);
        self.shadowed.truncate(visible);
        return;
      }
      _ => None,
    };
    match value {
      Some(value) => *expr = value,
      None => visit_mut::visit_expr_mut(self, expr),
    }
  }

  fn visit_block_mut(&mut self, block: &mut syn::Block) {
    let visible = self.shadowed.len();
    for statement in &mut block.stmts {
      match statement {
        // What a `let` declares is not a field after it.
        syn::Stmt::Local(local) => {
          if let Some(init) = &mut local.init {
            self.visit_expr_mut(&mut init.expr);
            if let Some((_, diverge)) = &mut init.diverge {
              self.visit_expr_mut(diverge);
            }
          }
          let mut names = vec![];
          declared(&local.pat, &mut names);
          self.shadowed.extend(names);
        }
        statement => self.visit_stmt_mut(statement),
      }
    }
    self.shadowed.truncate(visible);
  }
}

/// A compiled expression.
struct Expression {
  /// The operations, as `&[Op, ...]`.
  ops: TokenStream,
  /// The same, as a type (`puns::Steps`).
  steps: TokenStream,
}

/// Compiles an expression whose value is a `ty`.
///
/// With `sizing`, the expression is a size or a length, which `size_of_val(self)` cannot be in.
///
/// # Errors
///
/// Returns an error if the expression does not compile, is not of type `ty`, or exceeds the
/// limits of the evaluator.
fn expression(
  expr: &Expr,
  ty: Ty,
  sizing: bool,
  fields: &[FieldInfo],
  root: &TokenStream,
) -> Result<Expression> {
  let (mut ops, mut scope) = (vec![], Scope { sizing, ..Scope::default() });
  let found = compile(expr, fields, root, &mut ops, &mut scope)?;
  expect(found, ty, expr)?;
  limits(&ops, expr)?;
  let steps = steps(&ops, std::mem::take(&mut scope.walks), root);
  Ok(Expression { ops: quote!(&[#(#ops),*]), steps })
}

/// Returns the operations `ops` as a type: a balanced tree (`puns::steps::Two`) whose leaves are
/// the operations.
///
/// Each leaf is `One<INDEX>`, or `Walk<INDEX, COND, NEXT>` for a walk, where `COND` and `NEXT` are
/// its expressions as types. `walks` holds them, in the order of the walks of `ops`. The
/// validation applies each operation in an instance of its own, down this tree. The tree is
/// balanced, so the recursion is as deep as the logarithm of the number of operations.
///
/// # Panics
///
/// Panics if `walks` has fewer entries than `ops` has walks.
fn steps(
  ops: &[TokenStream],
  walks: Vec<(TokenStream, TokenStream)>,
  root: &TokenStream,
) -> TokenStream {
  fn tree(leaves: &[TokenStream], root: &TokenStream) -> TokenStream {
    match leaves {
      [] => quote!(()),
      [leaf] => leaf.clone(),
      _ => {
        let (a, b) = leaves.split_at(leaves.len() / 2);
        let (a, b) = (tree(a, root), tree(b, root));
        quote!(#root::steps::Two<#a, #b>)
      }
    }
  }
  let mut walks = walks.into_iter();
  let leaves = ops
    .iter()
    .enumerate()
    .map(|(index, op)| {
      if op_name(op).as_deref() == Some("Walk") {
        let (cond, next) = walks.next().expect("a walk and its expressions");
        quote!(#root::steps::Walk<#index, #cond, #next>)
      } else {
        quote!(#root::steps::One<#index>)
      }
    })
    .collect::<Vec<_>>();
  tree(&leaves, root)
}

/// Returns the ranges of `#[pun(valid(..))]`, as an expression of type
/// `&'static [(u128, u128)]`.
///
/// # Errors
///
/// Returns an error if a bound is negative, or a literal that is not an unsigned integer.
fn valid_ranges(
  ranges: &Punctuated<Expr, Token![,]>,
  ty: &TokenStream,
  root: &TokenStream,
) -> Result<TokenStream> {
  // A bound: an unsigned integer, as the values of rules are.
  let value = |expr: &Expr| match expr {
    Expr::Lit(ExprLit { lit, .. }) => literal(lit),
    Expr::Unary(unary) if matches!(unary.op, UnOp::Neg(_)) => Err(Error::new_spanned(
      unary,
      "the values of a field are given as unsigned integers: there is no negative number",
    )),
    expr => Ok(quote_spanned!(expr.span()=> #root::layout::Op::constant(&(#expr)))),
  };
  let option = |expr: &Option<Box<Expr>>| -> Result<TokenStream> {
    Ok(match expr {
      None => quote!(::core::option::Option::None),
      Some(expr) => {
        let value = value(expr)?;
        quote!(::core::option::Option::Some(#value))
      }
    })
  };
  let count = ranges.len();
  let ranges = ranges
    .iter()
    .map(|range| {
      Ok(match range {
        Expr::Range(ExprRange { start, end, limits, .. }) => {
          let (start, end) = (option(start)?, option(end)?);
          let inclusive = matches!(limits, RangeLimits::Closed(_));
          quote!(#root::layout::Spec::range::<#ty>(#start, #end, #inclusive))
        }
        expr => {
          let value = value(expr)?;
          quote!((#value, #value))
        }
      })
    })
    .collect::<Result<Vec<_>>>()?;
  Ok(quote!({
    let merged: &'static ([(u128, u128); #count], usize) =
      const { &#root::layout::Spec::merge_ranges([#(#ranges),*]) };
    merged.0.split_at(merged.1).0
  }))
}

/// Adds `ty: Pun` to the bounds of `generics`, unless they hold it already.
///
/// A bound written twice is one that clippy sees twice, at the span of the one that the user
/// wrote.
fn bound(generics: &mut syn::Generics, ty: &Type, root: &TokenStream) {
  if !has_pun(generics, &ty.to_token_stream().to_string()) {
    generics.make_where_clause().predicates.push(parse_quote!(#ty: #root::Pun));
  }
}

/// Returns whether `generics` bound the type written `name` by `Pun`, as `<T: Pun>` or
/// `where T: Pun`.
///
/// The bound is read by its last name, as written: `Pun`, `puns::Pun` or `crate::Pun`.
fn has_pun(generics: &syn::Generics, name: &str) -> bool {
  let pun = |bounds: &Punctuated<syn::TypeParamBound, Token![+]>| {
    bounds.iter().any(|bound| {
      matches!(bound, syn::TypeParamBound::Trait(bound)
        if bound.path.segments.last().is_some_and(|segment| segment.ident == "Pun"))
    })
  };
  // `<T: Pun>`
  generics.type_params().any(|param| param.ident == name && pun(&param.bounds))
    // `where T: Pun`
    || generics.where_clause.iter().flat_map(|clause| &clause.predicates).any(|predicate| {
      matches!(predicate, syn::WherePredicate::Type(predicate)
        if predicate.bounded_ty.to_token_stream().to_string() == name && pun(&predicate.bounds))
    })
}

/// Returns the generics of `input`, with the bounds of its implementation of `Pun`: `bounds` as
/// `#[pun(bound(..))]` gives them, and otherwise, for each type that needs it ([`needs_bound`]),
/// `ty: Pun` for the types of its fields, and `ty: Pointee` for the pointees of its aims.
///
/// A bound of a type parameter joins the bounds that the parameter has, as `<T: ?Sized + Pun>` for
/// `struct Relaxed<T: ?Sized>` with `#[pun(bound(T: Pun))]` ([`join_bounds`]), so that no type is
/// bounded in two places. A bound that `bound(..)` repeats is written twice, as the user wrote it.
fn bounded<'a>(
  input: &DeriveInput,
  (types, pointees): (impl Iterator<Item = &'a Type>, impl Iterator<Item = &'a Type>),
  bounds: Option<&Bounds>,
  root: &TokenStream,
) -> syn::Generics {
  let mut generics = input.generics.clone();
  if let Some(bounds) = bounds {
    generics.make_where_clause().predicates.extend(bounds.iter().cloned());
  } else {
    types.filter(|ty| needs_bound(ty, input)).for_each(|ty| bound(&mut generics, ty, root));
    for ty in pointees.filter(|ty| needs_bound(ty, input)) {
      generics.make_where_clause().predicates.push(parse_quote!(#ty: #root::Pointee));
    }
  }
  join_bounds(&mut generics);
  generics
}

/// Moves each predicate of the `where` clause of `generics` that bounds a type parameter, written
/// as the parameter alone, without `for<..>`, into the bounds of the parameter, which a path of
/// one name without arguments names in the scope of the generics.
fn join_bounds(generics: &mut syn::Generics) {
  let Some(clause) = generics.where_clause.take() else { return };
  let mut kept = Punctuated::<syn::WherePredicate, Token![,]>::new();
  for predicate in clause.predicates {
    let param = match &predicate {
      syn::WherePredicate::Type(predicate) if predicate.lifetimes.is_none() => {
        match &predicate.bounded_ty {
          Type::Path(path) if path.qself.is_none() => path.path.get_ident().and_then(|ident| {
            generics
              .params
              .iter()
              .position(|param| matches!(param, GenericParam::Type(param) if param.ident == *ident))
          }),
          _ => None,
        }
      }
      _ => None,
    };
    match (param, predicate) {
      (Some(i), syn::WherePredicate::Type(predicate)) => {
        if let GenericParam::Type(param) = &mut generics.params[i] {
          param.colon_token.get_or_insert_default();
          param.bounds.extend(predicate.bounds);
        }
      }
      (_, predicate) => kept.push(predicate),
    }
  }
  if !kept.is_empty() {
    generics.where_clause =
      Some(syn::WhereClause { where_token: clause.where_token, predicates: kept });
  }
}

/// Returns the constant that pins the types whose functions the generated code calls to make a
/// layout to those of the crate, since a path can name another type (`Layout::PIN`).
fn pin(root: &TokenStream) -> TokenStream {
  let layout = quote!(#root::layout);
  quote! {
    const _: (
      fn(#layout::Layout),
      fn(#layout::Spec),
      fn(#layout::Id),
      fn(#layout::Rule),
      fn(#layout::Op),
      fn(#layout::Field),
      fn(#layout::Enum),
      fn(#layout::Variant),
      fn(#layout::Crc),
      fn(#layout::Checksum),
      fn(#layout::Varint),
    ) = #layout::Layout::PIN;
  }
}

// -------------------------------------------------------------------------------------------------
// The item as compiled

/// Returns the type `ty` of a field, named `name` in its home ([`homes`]), in the code generated
/// for `this`.
///
/// The tokens have the span of `ty`, so that an error about the type is reported at the field, as
/// an error in the item rather than in the code of the derive. Names resolve with that span as they
/// do in `ty`, in the same scope, where the generated items are visible (none of them is a local).
fn home(this: &TokenStream, name: &Ident, ty: &Type) -> TokenStream {
  fn respan(tokens: TokenStream, span: Span) -> TokenStream {
    tokens
      .into_iter()
      .map(|mut token| {
        if let proc_macro2::TokenTree::Group(group) = &token {
          let mut respanned =
            proc_macro2::Group::new(group.delimiter(), respan(group.stream(), span));
          respanned.set_span(span);
          token = proc_macro2::TokenTree::Group(respanned);
        } else {
          token.set_span(span);
        }
        token
      })
      .collect()
  }
  respan(quote!(<#this as __PunTypes>::#name), ty.span())
}

/// Returns the one home of the type of each field, and the function that ties it to the item as
/// `rustc` compiled it.
///
/// The derive reads the tokens of the item, and an item can name another type than its tokens
/// seem to: a macro in the type of a field expands in the code generated here, where `Self` and
/// the names `__Pun..` are other items than in the declaration, and an attribute expanded after
/// the derive can change the item. So the type of each field is written once, as the associated
/// type `name` of `__PunTypes`, in an implementation for the type, where `Self` is the type as in
/// its declaration. Every other place names it through [`home`], so they all name one type.
///
/// `check` is the body of `item`, which is never called. It compiles only if the item as compiled
/// has the fields and the variants that the derive read, each of the type of its home, and no
/// other: an exhaustive pattern, and a `&mut` to a pointer to each field, which no coercion
/// changes and in which the type is invariant, so that neither an unsized coercion nor a subtype
/// passes. The compiler type-checks it, `cargo check` included, for every type and every instance.
fn homes(input: &DeriveInput, fields: &[(Ident, &Type)], check: &TokenStream) -> TokenStream {
  let (ident, vis) = (&input.ident, &input.vis);
  let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
  let (names, types): (Vec<_>, Vec<_>) = fields.iter().map(|(name, ty)| (name, *ty)).unzip();
  quote! {
    // As visible as the type, so that the implementations of the lists can name its types.
    #vis trait __PunTypes {
      #(type #names: ?Sized;)*
      fn item(p: *mut Self);
    }

    impl #impl_generics __PunTypes for #ident #ty_generics #where_clause {
      #(type #names = #types;)*
      // Never called, and so, `#[inline]`, never compiled to machine code.
      #[inline]
      fn item(p: *mut Self) {
        // SAFETY: Never called: only its type-checking matters.
        unsafe { #check }
      }
    }
  }
}

/// Returns what `item` of [`homes`] checks of a field, `place`, whose home is `home`: its type is
/// `home`, exactly. An error is reported at `ty`, the type of the field as the derive read it.
///
/// A raw pointer made with `&raw mut` has the type of the place, and `*mut T` is invariant in `T`,
/// so a subtype is not a value of it. A `&mut` to that pointer coerces to no other type, since a
/// raw pointer has no unsized coercion to a pointer of another type, nor a `Deref`.
fn same_type(place: &TokenStream, home: &TokenStream, ty: &Type) -> TokenStream {
  located(quote!(let _: &mut *mut #home = &mut (&raw mut #place);), ty.span())
}

/// Returns [`homes`] for an enum, and the home of each field of each variant.
///
/// `item` matches on every variant, with no other arm, and binds each field with `ref mut`, whose
/// type is invariant in the type of the field: an enum has no packed field, which a reference
/// could not point to.
fn variant_homes(input: &DeriveInput, data: &DataEnum) -> (TokenStream, Vec<Vec<TokenStream>>) {
  let ident = &input.ident;
  let (_, ty_generics, _) = input.generics.split_for_impl();
  let this = quote!(#ident #ty_generics);
  let (mut named, mut paths, mut arms) = (vec![], vec![], vec![]);
  for (i, variant) in data.variants.iter().enumerate() {
    let (mut members, mut bindings, mut checks, mut row) = (vec![], vec![], vec![], vec![]);
    for (j, field) in variant.fields.iter().enumerate() {
      let name = format_ident!("V{}F{}", i, j);
      let path = home(&this, &name, &field.ty);
      let binding = format_ident!("__pun_field{}", j);
      members.push(
        field
          .ident
          .as_ref()
          .map_or_else(|| Member::Unnamed(Index::from(j)), |ident| Member::Named(ident.clone())),
      );
      checks.push(same_type(&quote!(*#binding), &path, &field.ty));
      named.push((name, &field.ty));
      bindings.push(binding);
      row.push(path);
    }
    let name = &variant.ident;
    arms.push(quote!(Self::#name { #(#members: ref mut #bindings),* } => { #(#checks)* }));
    paths.push(row);
  }
  (homes(input, &named, &quote!(match *p { #(#arms)* })), paths)
}

// -------------------------------------------------------------------------------------------------
// Structs

/// Returns the `T` of the type `ty` of the field of an eager aim, an `InPlace<T>`, however it is
/// spelled: `<ty as InPlaceOf>::Inner`. Rust names a trait only by a path, which a crate path may
/// redirect, so this projection alone does not make the field an `InPlace<T>`: `item` of [`homes`]
/// ties the field to the `InPlace<T>` of `puns` for this `T` (`Layout::in_place`).
fn in_place(ty: &Type, root: &TokenStream) -> Type {
  let span = ty.span();
  syn::parse_quote_spanned!(span=> <#ty as #root::InPlaceOf>::Inner)
}

/// Returns whether a field with `options` holds an eager aim, and is then an `InPlace<T>`: the
/// field of a `pointer`, which is eager, or the field of regions of which one is eager.
fn in_place_field(options: &Options) -> bool {
  options.pointer.is_some() || options.regions.iter().flatten().any(Region::eager)
}

/// Where the pointee of an aim is, as `layout::Start` says: at the place that an expression gives,
/// checked with the struct, or at the cursor, checked with the struct if `eager`.
#[derive(Clone, Copy)]
enum AimStart<'a> {
  /// `#[pun(pointer(..))]`.
  Stored(&'a Expr),
  /// `#[pun(regions(..))]`.
  Next { eager: bool },
}

/// Returns the rule of an aim at a `pointee` that a struct declares, where `start` says, as large
/// as `size` says, and the steps of its two expressions, where its pointee is and how large it is
/// ([`steps`]). `#[pun(pointer(..))]` gives a stored start, and `#[pun(regions(..))]` the cursor:
/// both are one rule, written here.
///
/// The type is written in the rule and in the list of the rules (`Pun::Rules`), in the one
/// implementation, where it names one type, and `validate::listed` compares the two.
///
/// # Errors
///
/// Returns an error if an expression does not compile.
fn aim_rule(
  pointee: &TokenStream,
  (start, size): (AimStart, &AimSize),
  fields: &[FieldInfo],
  root: &TokenStream,
) -> Result<(TokenStream, TokenStream, TokenStream)> {
  let layout = quote!(#root::layout);
  // An expression of the aim, in a block of its own: what a function takes is not extended to
  // `'static`.
  let expression = |expr| -> Result<_> {
    let Expression { ops, steps } = expression(expr, Ty::Int, false, fields, root)?;
    Ok((quote!(const { #ops }), steps))
  };
  let (start, start_steps) = match start {
    AimStart::Stored(start) => {
      let (ops, steps) = expression(start)?;
      (quote!(Stored(#ops)), steps)
    }
    AimStart::Next { eager: true } => (quote!(Next(#layout::Check::Eager)), quote!(())),
    AimStart::Next { eager: false } => (quote!(Next(#layout::Check::Lazy)), quote!(())),
  };
  let (extent, extent_steps) = match size {
    AimSize::Pointee => (quote!(Pointee), quote!(())),
    AimSize::Rest => (quote!(Rest), quote!(())),
    AimSize::Length(length) => {
      let (ops, steps) = expression(length)?;
      (quote!(Length(#ops)), steps)
    }
    AimSize::Bytes(bytes) => {
      let (ops, steps) = expression(bytes)?;
      (quote!(Bytes(#ops)), steps)
    }
  };
  let rule = quote!(#layout::Rule::Aim(#layout::Aim::new::<#pointee>(
    #layout::Start::#start,
    #layout::Extent::#extent,
  )));
  Ok((rule, start_steps, extent_steps))
}

/// How the pointee of an aim is read, which follows from when it is checked, and its type.
#[derive(Clone, Copy)]
enum Read {
  /// An eager aim: `name(&self)`, safe.
  Eager,
  /// A lazy region with a type: `name(&self)`, which checks the pointee in the bytes of the value.
  Region,
  /// A lazy region without a type: `name(&self)`, its bytes.
  Bytes,
}

/// Returns the methods that read the pointee of an aim, a `pointee` whose rule `item` names in the
/// list of the rules of `Self` (`puns::Item`), named `name` and as visible as `vis`, as `read`
/// says.
fn read_aim(
  (name, pointee, item): (&Ident, &TokenStream, &TokenStream),
  read: Read,
  vis: &Visibility,
  root: &TokenStream,
) -> TokenStream {
  // The value of the pointee, which a read returns: the pointee itself, or the `T` of a `Thin`.
  let value = quote!(<#pointee as #root::Pointee>::Value);
  let check = "\n\n# Compile-time errors\n\nA call compiles only if every byte of the struct, up \
               to the end of its tail, is initialized and, without the `unstable` feature of \
               `puns`, this does not depend on a layout that Rust leaves to the compiler.";
  match read {
    Read::Bytes => {
      let doc = format!(
        "Returns the bytes of the `{name}` region of the tail, truncated if the fields that size \
         the regions give more bytes than the tail holds. The region takes the rest of the tail \
         if its expression fails.{check}"
      );
      quote! {
        #[doc = #doc]
        #[inline]
        #vis const fn #name(&self) -> &[::core::primitive::u8] {
          #root::region::<Self, #item>(self)
        }
      }
    }
    Read::Region => {
      let doc = format!(
        "Returns the `{name}` region of the tail as the type it holds, checked as \
         `puns::from_bytes` checks bytes, with the pointees of its eager aims in the bytes of the \
         value. The region is truncated if the fields that size the regions give more bytes than \
         the tail holds, and takes the rest of the tail if its expression fails.\n\n# Errors\n\n\
         Returns the error of `puns::from_bytes` if the bytes of the region are not a value of \
         that type.{check} It also compiles only under the compile-time conditions of \
         `puns::from_bytes` for the type of the region."
      );
      quote! {
        #[doc = #doc]
        #[inline]
        #vis const fn #name(&self) -> ::core::result::Result<&#value, #root::Error> {
          #root::region_as::<Self, #item>(self)
        }
      }
    }
    Read::Eager => {
      let doc = format!(
        "Returns the pointee of the aim `{name}`, where the value points, which `puns` checked \
         with the value, or which unsafe code that made the value put there: no safe code makes \
         a value with an eager aim.\n\nThe pointee is read through the reference to the value, \
         which has the provenance of its bytes under the tree borrows model of Miri, the model \
         that `puns` follows for the reads of pointees.\n\n# Compile-time errors\n\nA call \
         compiles only if the pointee is sized, a `Thin`, or stores the length of its tail in its \
         sized fields, where its extent is what it stores."
      );
      // No `unsafe` block: the function that the path names is called as safe code, whatever it
      // is ("What the derive trusts" in the documentation of the derive).
      quote! {
        #[doc = #doc]
        #[inline]
        #vis const fn #name(&self) -> &#value {
          #root::aim_of::<Self, #item>(self)
        }
      }
    }
  }
}

/// Returns the constant `AIMS` of a struct whose eager aims `aims` have a name, with the item of
/// each in the list of the rules of the struct, and the visibility of its field: a value whose
/// fields, named as the aims, are their `puns::AimOf`, for `puns::aim_mut`. Nothing without such an
/// aim.
///
/// The constant is as visible as the struct, whatever the visibilities of its fields, and hidden
/// from the documentation, since its type has no name there: the documentation of the derive
/// describes it.
fn aims_constant(
  input: &DeriveInput,
  aims: &[(&Ident, TokenStream, &Visibility)],
  root: &TokenStream,
) -> Option<TokenStream> {
  if aims.is_empty() {
    return None;
  }
  let (ident, vis) = (&input.ident, &input.vis);
  let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
  let this = quote!(#ident #ty_generics);
  let fields = aims.iter().map(|(name, item, vis)| {
    let doc = format!("The aim `{name}`.");
    quote!(#[doc = #doc] #vis #name: #root::AimOf<#this, #item>)
  });
  let names = aims.iter().map(|(name, ..)| name);
  let doc = format!(
    "The eager aims of `{ident}` that have a name, for `puns::aim_mut`: the field `name` is the aim \
     `name`."
  );
  Some(quote! {
    #[doc = #doc]
    #vis struct __PunAims #impl_generics #where_clause {
      #(#fields),*
    }

    impl #impl_generics #this #where_clause {
      #[doc(hidden)]
      #vis const AIMS: __PunAims #ty_generics = __PunAims { #(#names: #root::AimOf::NEW),* };
    }
  })
}

/// Returns the constant that pins the types whose functions the generated code calls to make the
/// aims of a struct, as [`pin`] pins the others.
fn pin_aims(root: &TokenStream) -> TokenStream {
  let layout = quote!(#root::layout);
  quote! {
    const _: (
      fn(#layout::Aim),
      fn(#layout::Start),
      fn(#layout::Extent),
      fn(#layout::Check),
    ) = #layout::Layout::PIN_AIMS;
  }
}

/// Returns whether `ty` is unsized as written: a slice, `str` or a trait object, in parentheses or
/// not.
fn is_unsized(ty: &Type) -> bool {
  match ty {
    Type::Slice(_) | Type::TraitObject(_) => true,
    Type::Paren(inner) => is_unsized(&inner.elem),
    Type::Group(inner) => is_unsized(&inner.elem),
    Type::Path(path) => path.qself.is_none() && path.path.is_ident("str"),
    _ => false,
  }
}

fn derive_struct(input: &DeriveInput, data: &DataStruct) -> Result<TokenStream> {
  let repr = parse_repr(&input.attrs)?;
  let options =
    parse_options(&input.attrs, "a struct", &["crate", "bound", "size", "assert", "tag", "get"])?;
  let root = options.root.clone().unwrap_or_else(|| quote!(::puns));
  let ident = &input.ident;
  let (_, ty_generics, _) = input.generics.split_for_impl();
  let this = quote!(#ident #ty_generics);
  // The offsets of the fields of any other representation are found on a value, which an unsized
  // struct cannot be. Without this, the build fails in the generated code, where the struct is
  // required to be `Sized`. A tail that is unsized by a type of the user (another struct, a
  // parameter that is `?Sized`) is not seen here, and still fails there.
  if !(repr.c || repr.transparent)
    && let Some(last) = data.fields.iter().next_back()
    && is_unsized(&last.ty)
  {
    return Err(Error::new_spanned(
      &last.ty,
      "`Pun` is derived for a struct that ends with an unsized field only with `repr(C)` or \
       `repr(transparent)`, which say where that field is",
    ));
  }

  let fields = match &data.fields {
    Fields::Named(fields) => Some(&fields.named),
    Fields::Unnamed(fields) => Some(&fields.unnamed),
    Fields::Unit => None,
  };
  let declared = fields.into_iter().flatten().collect::<Vec<_>>();
  // The `T` of each field, were it the `InPlace<T>` of an eager aim.
  let inner = declared.iter().map(|field| in_place(&field.ty, &root)).collect::<Vec<_>>();
  let mut fields = declared
    .iter()
    .enumerate()
    .map(|(i, field)| {
      let options =
        parse_options(&field.attrs, "a field", &["valid", "len", "regions", "pointer"])?;
      if let (Some(pointer), true) =
        (&options.pointer, options.len.is_some() || options.regions.is_some())
      {
        return Err(Error::new_spanned(
          &pointer.pointee,
          "the field of a `pointer` is sized: it takes neither `len` nor `regions`",
        ));
      }
      // The field of an eager aim is an `InPlace<T>`, a `T` in the layout, whose home is apart.
      let (ty, name) = if in_place_field(&options) {
        (&inner[i], format_ident!("I{}", i))
      } else {
        (&field.ty, format_ident!("F{}", i))
      };
      Ok(FieldInfo {
        member: field
          .ident
          .as_ref()
          .map_or_else(|| Member::Unnamed(Index::from(i)), |ident| Member::Named(ident.clone())),
        ty,
        home: home(&this, &name, ty),
        vis: &field.vis,
        options,
        offset: TokenStream::new(),
      })
    })
    .collect::<Result<Vec<_>>>()?;
  let count = fields.len();

  // The offsets of the fields, measured on the struct as compiled by projecting a pointer to it
  // (`&raw const (*p).field`), which the language does, and not with `offset_of!`, which comes
  // from whatever crate `::core` names (`extern crate other as core;` makes it another crate).
  // With `repr(C)` and `repr(transparent)`, the struct may be unsized, so the pointer is into
  // uninitialized memory that holds it (`Layout::offsets`, `__PunOffsets`). Otherwise the struct is
  // `Sized`, and the pointer is to an uninitialized value.
  for (i, field) in fields.iter_mut().enumerate() {
    let member = &field.member;
    field.offset = match () {
      () if repr.c || repr.transparent => quote!(<Self as __PunOffsets>::OFFSETS[#i]),
      () => quote!({
        let value = #root::layout::Layout::uninit::<Self>();
        let start: *const Self = value.as_ptr();
        // SAFETY: `start` points to `value`, an uninitialized `Self` that is `Sized` (the
        // implementation requires it), so `(*start).#member` is a place in its allocation. `&raw
        // const` makes a pointer to it without reading it, which needs neither initialized
        // bytes nor alignment. That pointer and `start` are derived from the same allocation, and
        // the bytes between them are in it, as `byte_offset_from` requires [1]. The distance is
        // the offset of the field, which is not negative, so `as usize` keeps it.
        //
        // [1] Per https://doc.rust-lang.org/1.99.0/core/primitive.pointer.html#method.offset_from:
        //
        //     `self` and `origin` must either
        //
        //     - point to the same address, or
        //     - both be derived from a pointer to the same allocation, and the memory range
        //       between the two pointers must be in bounds of that object.
        unsafe { (&raw const (*start).#member).byte_offset_from(start) as usize }
      }),
    };
  }

  // Only the layout of `repr(C)` and `repr(transparent)` is known without the size of the fields.
  let sized = !(repr.c || repr.transparent);
  // The types of the fields, which are `Pun`, and the pointees that depend on a type parameter,
  // which are `Pointee`s, as every aim requires (`Aim::new`): a `Thin` is one, and every `Pun`
  // type. `#[pun(bound(..))]` gives all of them instead.
  let pointees = fields.iter().flat_map(|field| {
    let pointer = field.options.pointer.iter().map(|aim| &aim.pointee);
    pointer.chain(field.options.regions.iter().flatten().filter_map(|region| region.ty.as_ref()))
  });
  let types = (fields.iter().map(|field| field.ty), pointees);
  let mut generics = bounded(input, types, options.bounds.as_ref(), &root);
  if sized {
    generics.make_where_clause().predicates.push(parse_quote!(#this: ::core::marker::Sized));
  }
  // The bounds of the implementations, which `bounded` puts on the parameters where it can.
  let (impl_generics, _, where_clause) = generics.split_for_impl();

  // Layout of the fields.
  let offsets_trait = (repr.c || repr.transparent).then(|| {
    let measured = fields.iter().map(|field| {
      let member = &field.member;
      quote! {
        // SAFETY: `base` points to a `Self` in `memory`, all of whose places are in that
        // allocation (`Layout::measure_base`). `(*base).#member` is a place there, and `&raw const`
        // makes a pointer to it without reading it, which needs neither initialized bytes nor
        // alignment. That pointer and `base` are derived from the same allocation, and the bytes
        // between them are in it, as `byte_offset_from` requires [1]. The distance is the offset
        // of the field, which is not negative, so `as usize` keeps it.
        //
        // [1] Per https://doc.rust-lang.org/1.99.0/core/primitive.pointer.html#method.offset_from:
        //
        //     `self` and `origin` must either
        //
        //     - point to the same address, or
        //     - both be derived from a pointer to the same allocation, and the memory range
        //       between the two pointers must be in bounds of that object.
        unsafe { (&raw const (*base).#member).byte_offset_from(base) as ::core::primitive::usize }
      }
    });
    // The impl of `Offsets` for memory of any size.
    let mut sized_memory = generics.clone();
    sized_memory.params.push(parse_quote!(const __PunMemory: ::core::primitive::usize));
    let (memory_generics, _, _) = sized_memory.split_for_impl();
    quote! {
      trait __PunOffsets {
        const OFFSETS: [::core::primitive::usize; #count];
      }

      impl #impl_generics __PunOffsets for #this #where_clause {
        // SAFETY: `Self` is `Sized`, or its last field is unsized, so that it is a slice, `str` or
        // a struct whose last field is one of these, at any depth (`Pun`, section Safety), and its
        // probe is the probe of that field cast with `as` casts, as for `Layout::structure` below.
        // These are the requirements of `offsets`.
        const OFFSETS: [::core::primitive::usize; #count] = unsafe {
          #root::layout::Layout::offsets::<Self, #count>(<Self as #root::Pun>::PROBE)
        };
      }

      // SAFETY: `OFFSETS` projects a pointer to a `Self` onto each field that `Fields` lists, in
      // that order, so it holds the offset of each of these fields in the struct as compiled.
      #[allow(non_upper_case_globals)]
      unsafe impl #memory_generics #root::layout::Offsets<__PunMemory, #count> for #this
        #where_clause
      {
        #[allow(unused_variables)]
        const OFFSETS: [::core::primitive::usize; #count] = {
          let memory = #root::layout::Layout::uninit::<[::core::primitive::u8; __PunMemory]>();
          // SAFETY: As for `__PunOffsets`, these are the requirements of `measure_base`.
          let base = unsafe {
            #root::layout::Layout::measure_base::<Self, __PunMemory>(
              &memory,
              <Self as #root::Pun>::PROBE,
            )
          };
          [#(#measured),*]
        };
      }
    }
  });
  let mut layouts = vec![];
  for field in &fields {
    let (offset, ty) = (&field.offset, &field.home);
    layouts.push(match &field.options.valid {
      None => quote!(#root::layout::Field::new(#offset, &<#ty as #root::Pun>::LAYOUT)),
      Some(ranges) => {
        let ranges = valid_ranges(ranges, ty, &root)?;
        quote!(#root::layout::Field::restricted(
          #offset,
          const { &<#ty as #root::Pun>::LAYOUT.restricted(#ranges) },
        ))
      }
    });
  }

  // Rules.
  let rule = quote!(#root::layout::Rule);
  let mut rules = vec![];
  // The rules that have an expression or are aims: their index, the operations of each of their
  // expressions as a type, and the type of the pointee of an aim.
  let mut listed: Vec<(usize, TokenStream, Option<TokenStream>, TokenStream)> = vec![];
  // Where the next rule is in the list, which names its item for a read: `First`, in as many
  // `After` as there are items before it.
  let item = |position: usize| {
    (0..position).fold(quote!(#root::First), |item, _| quote!(#root::After<#item>))
  };
  if let Some(size) = &options.size {
    let Expression { ops, steps } = expression(size, Ty::Int, true, &fields, &root)?;
    listed.push((rules.len(), steps, None, quote!(())));
    rules.push(quote!(#rule::Size(#ops)));
  }
  for assert in &options.asserts {
    let Expression { ops, steps } = expression(assert, Ty::Bool, false, &fields, &root)?;
    listed.push((rules.len(), steps, None, quote!(())));
    rules.push(quote!(#rule::Assert(#ops)));
  }
  let mut accessors = vec![];
  let mut has_aims = false;
  // The eager aims that have a name, which `AIMS` names: the name, the item, the visibility.
  let mut named_aims = vec![];
  for (i, field) in fields.iter().enumerate() {
    let last = i + 1 == count;
    if let Some(pointer) = &field.options.pointer {
      let pointee = pointer.pointee.to_token_stream();
      let start = pointer.start(&field.member);
      let (aim, start, extent) =
        aim_rule(&pointee, (AimStart::Stored(&start), &pointer.size), &fields, &root)?;
      let index = rules.len();
      let read_item = item(listed.len());
      listed.push((index, start, Some(pointee.clone()), extent));
      rules.push(aim);
      has_aims = true;
      if let Some(name) = &pointer.name {
        named_aims.push((name, read_item.clone(), field.vis));
        accessors.push(read_aim((name, &pointee, &read_item), Read::Eager, field.vis, &root));
      }
    }
    if let Some(len) = &field.options.len {
      if !last {
        return Err(Error::new(len.span(), "`len` is an option of the last field"));
      }
      let Expression { ops, steps } = expression(len, Ty::Int, true, &fields, &root)?;
      listed.push((rules.len(), steps, None, quote!(())));
      rules.push(quote!(#rule::Len(#ops)));
    }
    if let Some(regions) = &field.options.regions {
      if !last {
        return Err(Error::new(field.member.span(), "`regions` is an option of the last field"));
      }
      for (index, region) in regions.iter().enumerate() {
        // A region without a type is its bytes.
        let pointee = region
          .ty
          .as_ref()
          .map_or_else(|| quote!([::core::primitive::u8]), ToTokens::to_token_stream);
        // A region is as many bytes as its expression says, what its type stores without one, or,
        // the last without one, the rest of the tail.
        let size = match &region.size {
          Some(size) => AimSize::Bytes(size.clone()),
          None if index + 1 == regions.len() => AimSize::Rest,
          None => AimSize::Pointee,
        };
        let eager = region.eager();
        let (aim, start, extent) =
          aim_rule(&pointee, (AimStart::Next { eager }, &size), &fields, &root)?;
        let read_item = item(listed.len());
        listed.push((rules.len(), start, Some(pointee.clone()), extent));
        rules.push(aim);
        has_aims = true;
        if eager {
          named_aims.push((&region.name, read_item.clone(), field.vis));
        }
        let read = match (eager, &region.ty) {
          (true, _) => Read::Eager,
          (false, Some(_)) => Read::Region,
          (false, None) => Read::Bytes,
        };
        accessors.push(read_aim((&region.name, &pointee, &read_item), read, field.vis, &root));
      }
    }
  }

  let aims_pin = has_aims.then(|| pin_aims(&root));
  let aims_constant = aims_constant(input, &named_aims, &root);

  // Getters.
  for getter in &options.getters {
    let (name, ty, vis) = (&getter.name, &getter.ty, &input.vis);
    let expr = rust(&getter.expr, &fields, &root, repr.packed.is_some());
    let doc = format!(
      "Returns `{}`, computed from the fields of the value, as a `{}`.\n\n# Panics\n\nPanics \
       where the expression, evaluated as Rust code, panics, such as on a division by zero.\n\n\
       # Compile-time errors\n\nA call compiles only if each field that the expression names is \
       an unsigned integer, or a sized struct whose only field with a size is one and is as large \
       as the struct.",
      getter.expr.to_token_stream(),
      ty.to_token_stream()
    );
    accessors.push(quote! {
      #[doc = #doc]
      #[inline]
      #[allow(unused_parens, clippy::all)]
      #vis const fn #name(&self) -> #ty {
        (#expr) as #ty
      }
    });
  }

  // The tags that the type can be carried next to.
  let tags = options.tags.iter().map(|TagOption { key, tag }| {
    quote! {
      impl #impl_generics #root::Tag<#key> for #this #where_clause {
        type Tag = #tag;
      }
    }
  });

  // The probe: a null pointer with the slice length of the probe of the last field, if any.
  let probe = match fields.last() {
    Some(last) if !sized => {
      let ty = &last.home;
      quote!(<#ty as #root::Pun>::PROBE as *const Self)
    }
    _ => quote!(::core::ptr::null()),
  };

  // The list of the fields, at the type level.
  let markers = (0..count).map(|i| format_ident!("__PunField{}", i)).collect::<Vec<_>>();
  let first = markers.first().map_or_else(|| quote!(()), |marker| quote!(#marker #ty_generics));
  let last = markers.last().map_or_else(|| quote!(()), |marker| quote!(#marker #ty_generics));
  let vis = &input.vis;
  let lists = fields.iter().enumerate().map(|(i, field)| {
    let (marker, ty) = (&markers[i], &field.home);
    let next = markers.get(i + 1).map_or_else(|| quote!(()), |marker| quote!(#marker #ty_generics));
    quote! {
      // As visible as the struct, so that it can be named by its implementation of `Pun`.
      #[allow(non_camel_case_types)]
      #vis struct #marker #impl_generics(::core::marker::PhantomData<#this>) #where_clause;

      // SAFETY: `FIELD` is the field of the layout of the struct whose declaration index is `#i`:
      // `layouts` holds the fields in the order of their declarations, each with the layout of
      // its type or a restriction of it, and `Layout::sort_fields` gives each its index. Its type
      // is `#ty`, the `T` of the `InPlace<T>` of an eager aim. `Next` is the field declared after
      // it, or `()` after the last, and `Last` is the last. So the list from the first marker is
      // the fields of the layout, in the order of their declarations, as the `# Safety` of
      // `Fields` requires.
      unsafe impl #impl_generics #root::Fields for #marker #ty_generics #where_clause {
        type Type = #ty;
        type Next = #next;
        type Last = #last;
        type Path<__P: #root::Path> = #root::At<__P, Self>;
        const FIELD: ::core::option::Option<#root::layout::Field> =
          ::core::option::Option::Some(*<#this as #root::Pun>::LAYOUT.field(#i));
      }
    }
  });

  // The metadata of a pointer to the struct is that of its last field, as in Rust.
  let metadata = fields.last().map_or_else(
    || quote!(()),
    |last| {
      let ty = &last.home;
      quote!(<#ty as #root::Pun>::Metadata)
    },
  );

  // The list of the rules that have an expression or are aims, at the type level, with their
  // operations and the pointees of the aims.
  let rule_list = listed.iter().rev().fold(quote!(()), |next, (index, steps, pointee, extent)| {
    pointee.as_ref().map_or_else(
      || quote!(#root::RuleAt<#index, #steps, #next>),
      |pointee| quote!(#root::RuleAt<#index, #steps, #next, #pointee, #extent>),
    )
  });

  let accessors = (!accessors.is_empty()).then(|| {
    quote! {
      impl #impl_generics #this #where_clause {
        #(#accessors)*
      }
    }
  });
  // The `Rust` representation: what every order of the fields has (`Layout::rust`).
  // Without a packing, `usize::MAX` is written `!0usize`: `::core::primitive::usize::MAX` is a
  // path, which `extern crate other as core;` can make another value of type `usize`.
  let rust = (!(repr.c || repr.transparent)).then(|| {
    let pack = repr.packed.clone().unwrap_or_else(|| quote!(!0usize));
    quote!(.rust(#pack))
  });
  let id = identity(input, &generics, &root);
  // A `repr(transparent)` struct around a type that `core` lays out in `Option` without a tag has
  // that guarantee too ("Representation" of `core::option`). Its other fields have size 0 and
  // alignment 1, which no such type has.
  let null_optimized = repr.transparent.then(|| {
    let types = fields.iter().map(|field| &field.home);
    quote!(const NULL_OPTIMIZED: bool = false #(|| <#types as #root::Pun>::NULL_OPTIMIZED)*;)
  });

  // The home of the type of each field, and of the `T` of the `InPlace<T>` of an eager aim, and
  // what ties the fields to the struct.
  let mut named = declared
    .iter()
    .enumerate()
    .map(|(i, field)| (format_ident!("F{}", i), &field.ty))
    .collect::<Vec<_>>();
  let aims = fields.iter().enumerate().filter(|(_, field)| in_place_field(&field.options));
  named.extend(aims.map(|(i, field)| (format_ident!("I{}", i), field.ty)));
  let members = fields.iter().map(|field| &field.member).collect::<Vec<_>>();
  let same = declared.iter().zip(&fields).enumerate().map(|(i, (declared, field))| {
    let member = &field.member;
    let written = home(&this, &format_ident!("F{}", i), &declared.ty);
    let same = same_type(&quote!((*p).#member), &written, &declared.ty);
    // The field of an eager aim is the `InPlace<T>` of `puns`, which no safe code makes, where `T`
    // is its type in the layout, tied by a function of `Layout`, which the type of `LAYOUT` fixes,
    // and not by a path, which could name another type `InPlace<T>` for some `T`.
    let home = &field.home;
    let in_place = in_place_field(&field.options).then(|| {
      located(
        quote!(#root::layout::Layout::in_place::<#home>(&mut (&raw mut (*p).#member));),
        declared.ty.span(),
      )
    });
    quote!(#same #in_place)
  });
  let homes = homes(
    input,
    &named,
    &quote! {
      let Self { #(#members: _),* } = *p;
      #(#same)*
    },
  );
  let pin = pin(&root);
  Ok(quote! {
    const _: () = {
      #pin

      #homes

      #(#lists)*

      #offsets_trait

      // SAFETY: For every struct that the derive accepts, the layout meets the `# Safety` of
      // `Pun`:
      // - `Layout::structure` takes the size and the alignment from the compiler. It asserts that
      //   the fields do not overlap and are within the size, and that the trailing padding of an
      //   unsized struct is the one it computes.
      // - The offset of each field is that of the compiler, measured on the struct as compiled:
      //   the offset of a place projected from a pointer to the struct, in the memory of
      //   `Layout::offsets` for `repr(C)` and `repr(transparent)`, which holds the struct
      //   whatever its size, and to an uninitialized value otherwise.
      // - The type of each field is its home in `__PunTypes`, which `item` requires to be the type
      //   of the field of that name in the struct as compiled, and the struct to have no other
      //   field. Each field has the layout of its type (`Pun`, section Safety), or a restriction
      //   of it (`Layout::restricted`), which guarantees the same values and accepts fewer. A
      //   struct is valid if each of its fields is valid [1], and a byte that no field covers is
      //   padding. The field of an eager aim is an `InPlace<T>`, which `item` requires too, and
      //   which is `repr(transparent)` over its `T` and has no other invariant, so its layout is
      //   that of `T`.
      //   Rules are requirements of `puns`, never of the language.
      // - An eager aim says that every value holds a valid pointee where its expression says. No
      //   code makes an `InPlace`, so no safe code makes a value of the struct, which exists only
      //   where `puns` checked its pointee, or where unsafe code put it.
      // - `Fields` lists the fields in order (see the markers above). `Rules` lists the rules that
      //   have an expression or are aims (the regions, whose pointee is `[u8]`, and the eager
      //   aims), which `validate::check_rules` asserts against the layout. `Metadata` is that of
      //   the last field, which holds the tail of the struct, or `()` without fields, as in Rust.
      //   The other lists are `()`. `NULL_OPTIMIZED` changes only what the analysis takes to be the
      //   same in every layout that Rust allows.
      // - Each field is `Pun`, so no byte is inside an `UnsafeCell`.
      // The functions called are those of the types that `Layout::PIN` fixes, which no path can
      // replace. A struct whose private fields keep an invariant must not derive `Pun` (crate
      // documentation, section Soundness): the layout describes only what the language requires of
      // its fields.
      //
      // [1] Per https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.struct:
      //
      //     A struct, tuple, and array requires all fields/elements to be valid at their
      //     respective type.
      unsafe impl #impl_generics #root::Pun for #this #where_clause {
        type Elem = ();
        type Fields = #first;
        type Variants = ();
        type Rules = #rule_list;
        type Metadata = #metadata;
        const PROBE: *const Self = #probe;
        // SAFETY: `Layout::structure` requires nothing of a `Sized` struct. An unsized struct has
        // an unsized last field, whose `Pun` type is a slice, `str`, or a struct whose last field
        // is one of these. Its probe is the probe of that field, `<[E] as Pun>::PROBE` cast with
        // `as` casts, cast again to `*const Self`. The size of the struct with 8 elements in its
        // tail must fit in an `isize`, which is not established. The derive relies on `rustc` to
        // report an error in const evaluation where it does not fit. No documentation promises this
        // behavior, which the crate documentation discloses (section Soundness).
        const LAYOUT: #root::layout::Layout = unsafe {
          #root::layout::Layout::structure::<Self>(
            Self::PROBE,
            const { &#root::layout::Layout::sort_fields([#(#layouts),*]) },
            const { &[#(#rules),*] },
          )
        }
        #rust
        .identified(Self::ID)
        .measured::<Self>();
        const ID: #root::layout::Id = #id;
        #null_optimized
      }

      #accessors
      #aims_constant
      #aims_pin

      #(#tags)*
    };
  })
}

// -------------------------------------------------------------------------------------------------
// Enums

fn derive_enum(input: &DeriveInput, data: &DataEnum) -> Result<TokenStream> {
  // Only an enum with fields that is not a view has bounds of its own: the others have no field
  // whose type needs one.
  let reference = |fields: &Fields| match fields {
    Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
      matches!(&fields.unnamed[0].ty, Type::Reference(reference) if reference.mutability.is_none())
    }
    _ => false,
  };
  let place = if data.variants.is_empty() {
    "an enum without variants"
  } else if data.variants.iter().all(|variant| reference(&variant.fields)) {
    "a view"
  } else if data.variants.iter().all(|variant| matches!(variant.fields, Fields::Unit)) {
    "an enum without fields"
  } else {
    "an enum"
  };
  let allowed: &[&str] = if place == "an enum" { &["crate", "bound"] } else { &["crate"] };
  let options = parse_options(&input.attrs, place, allowed)?;
  // No option means something on a variant, nor on the field of a view.
  for variant in &data.variants {
    parse_options(&variant.attrs, "a variant", &[])?;
    if place == "a view" {
      for field in &variant.fields {
        parse_options(&field.attrs, "a field of a view", &[])?;
      }
    }
  }
  let root = options.root.unwrap_or_else(|| quote!(::puns));
  let ident = &input.ident;
  let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
  let id = identity(input, &input.generics, &root);
  if data.variants.is_empty() {
    let (homes, _) = variant_homes(input, data);
    // No value: nothing is ever made of one, and nothing is valid for it.
    return Ok(quote! {
    const _: () = {
        #homes
      };

      // SAFETY: An enum without variants has no valid value [1], and `item` requires the enum as
      // compiled to have no variant. `Layout::uninhabited` has its size and alignment, and accepts
      // no value. So every value that the layout accepts is valid, and every value holds what it
      // says, since there are none. The lists are `()`, and it has no byte in an `UnsafeCell`.
      //
      // [1] Per https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.enum:
      //
      //     An enum must have a valid discriminant, and all fields of the variant indicated by
      //     that discriminant must be valid at their respective type.
      unsafe impl #impl_generics #root::Pun for #ident #ty_generics #where_clause {
        type Elem = ();
        type Fields = ();
        type Variants = ();
        type Rules = ();
        type Metadata = ();
        const PROBE: *const Self = ::core::ptr::null();
        const LAYOUT: #root::layout::Layout =
          #root::layout::Layout::uninhabited::<Self>().identified(Self::ID);
        const ID: #root::layout::Id = #id;
      }
    });
  }
  // An enum of references: a view of data that is one of several types.
  let references = data
    .variants
    .iter()
    .filter_map(|variant| match &variant.fields {
      Fields::Unnamed(fields) if fields.unnamed.len() == 1 => match &fields.unnamed[0].ty {
        Type::Reference(reference) if reference.mutability.is_none() => {
          Some((&variant.ident, reference))
        }
        _ => None,
      },
      _ => None,
    })
    .collect::<Vec<_>>();
  if references.len() == data.variants.len() {
    let lifetime = &references[0].1.lifetime;
    if lifetime.is_none() || references.iter().any(|(_, reference)| &reference.lifetime != lifetime)
    {
      return Err(Error::new(
        Span::call_site(),
        "the references of the variants must have the same lifetime",
      ));
    }
    let (vis, bytes) = (&input.vis, quote!(&#lifetime [::core::primitive::u8]));
    let (result, option) = (quote!(::core::result::Result), quote!(::core::option::Option));
    let count = references.len();
    let types: Vec<&Type> = references.iter().map(|(_, reference)| &*reference.elem).collect();
    // The types of the variants, as a balanced tree; the place of each in it, as a type.
    let list = view_tree(&types, &root);
    let index = |i: usize| view_place(i, count, &root);
    let decide = |i: usize| format_ident!("__pun_view_{}", i);
    let known = quote!(#root::view::Known<#lifetime, Self, #count, HEAD>);
    // Each variant is decided by a function of its own, which decides its base first: a later
    // variant, chosen at compile time (`puns::view`), whose function it calls.
    let deciders = types.iter().enumerate().map(|(i, ty)| {
      let (name, at) = (decide(i), index(i));
      let bases = (i + 1..count).map(|j| {
        let base = decide(j);
        quote!(#j => { let _ = Self::#base::<HEAD>(known); })
      });
      // The last variant has no later one to be decided from.
      let base = (i + 1 < count).then(|| {
        quote! {
          if #root::view::open::<Self, #at, #count, HEAD>(known) {
            match const { #root::view::base_of::<Self, #at, HEAD>() } {
              #(#bases)*
              _ => {}
            }
          }
        }
      });
      quote! {
        #[doc(hidden)]
        #[inline(always)]
        const fn #name<const HEAD: bool>(known: &mut #known) -> #option<&#lifetime #ty> {
          #base
          #root::view::decide::<Self, #at, #count, HEAD>(known)
        }
      }
    });
    let from_heads = references.iter().enumerate().map(|(i, (variant, _))| {
      let name = decide(i);
      quote! {
        if let #option::Some(value) = Self::#name::<true>(&mut known) {
          return #result::Ok((Self::#variant(value), known.rest(#i)));
        }
      }
    });
    let wholes = references.iter().enumerate().map(|(i, (variant, _))| {
      let name = decide(i);
      quote! {
        if let #option::Some(value) = Self::#name::<false>(&mut known) {
          return #result::Ok(Self::#variant(value));
        }
      }
    });
    return Ok(quote! {
      impl #impl_generics #root::view::Views for #ident #ty_generics #where_clause {
        type List = #list;
      }

      impl #impl_generics #ident #ty_generics #where_clause {
        #(#deciders)*

        /// Returns the first variant, in order, such that `bytes` start with a valid value of its
        /// type, and the bytes after that value.
        ///
        /// # Errors
        ///
        /// Returns `Err(Error::Validity)` if `bytes` start with a value of no variant's type.
        ///
        /// # Compile-time errors
        ///
        /// A call compiles only if the type of each variant meets the compile-time conditions of
        /// `puns::from_bytes`.
        #vis const fn from_head(bytes: #bytes) -> #result<(Self, #bytes), #root::Error> {
          let mut known = #root::view::Known::<Self, #count, true>::new(bytes);
          #(#from_heads)*
          #result::Err(#root::Error::Validity)
        }

        /// Returns the first variant, in order, such that `bytes` as a whole are a valid value of
        /// its type.
        ///
        /// # Errors
        ///
        /// Returns `Err(Error::Validity)` if `bytes` are a value of no variant's type.
        ///
        /// # Compile-time errors
        ///
        /// A call compiles only if the type of each variant meets the compile-time conditions of
        /// `puns::from_bytes`.
        #vis const fn from_bytes(bytes: #bytes) -> #result<Self, #root::Error> {
          let mut known = #root::view::Known::<Self, #count, false>::new(bytes);
          #(#wholes)*
          #result::Err(#root::Error::Validity)
        }
      }
    });
  }
  if data.variants.iter().any(|variant| !matches!(variant.fields, Fields::Unit)) {
    return derive_tagged(input, data, &root, options.bounds.as_ref());
  }
  let (mut variants, mut discriminants) = (vec![], vec![]);
  for variant in &data.variants {
    let variant = &variant.ident;
    // SAFETY (of the generated code): `bits_of` requires every byte of the value to be
    // initialized, which is not established: `align` may add padding after the discriminant, and
    // with the `Rust` representation, Rust does not say how the discriminant is stored. The call
    // is evaluated in the constants `SPEC` and `SPECIFIED`, where `rustc` reports an error instead
    // of reading an uninitialized byte. No documentation promises this behavior, which the crate documentation
    // discloses (section Soundness).
    variants.push(quote!(unsafe { #root::layout::Spec::bits_of(Self::#variant) }));
    discriminants.push(quote!(Self::#variant as ::core::primitive::i128));
  }
  let count = variants.len();
  let repr = parse_repr(&input.attrs)?;
  // With the `Rust` representation, Rust does not specify how a variant is stored, nor its size:
  // `puns` assumes it, and checks it against the bytes of each variant.
  let specified = if repr.c || repr.transparent || repr.int.is_some() {
    quote!(true)
  } else {
    quote!(#root::layout::Layout::fieldless::<Self>(
      &[#(#discriminants),*],
      &[#(#variants),*],
    ))
  };
  // No variant but those of `SPEC`.
  let (homes, _) = variant_homes(input, data);
  Ok(quote! {
    const _: () = {
      #homes
    };

    // SAFETY: `SPEC` has the size of `Self`, and the items of the `# Safety` of `Scalar` hold:
    // - A valid value of an enum without fields is one of its variants [1]. `SPEC` accepts the
    //   bytes of each variant, read from a value of it, as an unsigned integer in the byte order
    //   of the target. `item` requires the enum as compiled to have no other variant. An enum of
    //   size 0 has one variant, and no byte.
    // - Each value that `SPEC` accepts is the bytes of a variant, so a valid `Self`. The derive
    //   is told of no other invariant.
    // - An enum without fields holds no `UnsafeCell`.
    //
    // [1] Per https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.enum:
    //
    //     An enum must have a valid discriminant, and all fields of the variant indicated by
    //     that discriminant must be valid at their respective type.
    unsafe impl #impl_generics #root::Scalar for #ident #ty_generics #where_clause {
      const ID: #root::layout::Id = #id;
      const SPECIFIED: bool = #specified;
      const SPEC: #root::layout::Spec = if #root::layout::Spec::is_zero_sized::<Self>() {
        #root::layout::Spec::any::<Self>()
      } else {
        let merged: &'static ([(u128, u128); #count], usize) =
          const { &#root::layout::Spec::sorted_ranges([#(#variants),*]) };
        #root::layout::Spec::native_ranges::<Self>(merged.0.split_at(merged.1).0)
      };
    }
  })
}

/// Replaces `Self` with the enum `this` in a discriminant of an enum with fields, which the derive
/// writes again in the enum without fields that holds its tags, where `Self` is that enum.
///
/// It also finds whether the discriminant may have another value there. Only `Self` and a name
/// written by a macro can mean something else there: the items that the derive declares next to
/// that enum all have names that start with `__Pun`, which `find_reserved` refuses in the input. So
/// the discriminant is `opaque` if it holds a macro, an attribute (one on an item of a block may be
/// a macro), or a part that `syn` does not parse (`Verbatim`), which may hold one. In an item
/// declared in the discriminant, `Self` is that item, so it is kept, and `items` counts how deep in
/// items the visit is.
struct Discriminant<'a> {
  this: &'a Ident,
  items: usize,
  opaque: bool,
}

impl VisitMut for Discriminant<'_> {
  fn visit_ident_mut(&mut self, ident: &mut Ident) {
    if self.items == 0 && ident == "Self" {
      *ident = Ident::new(&self.this.to_string(), ident.span());
    }
  }

  fn visit_macro_mut(&mut self, _: &mut syn::Macro) {
    self.opaque = true;
  }

  fn visit_attribute_mut(&mut self, _: &mut Attribute) {
    self.opaque = true;
  }

  fn visit_expr_mut(&mut self, expr: &mut Expr) {
    self.opaque |= matches!(expr, Expr::Verbatim(_));
    visit_mut::visit_expr_mut(self, expr);
  }

  fn visit_type_mut(&mut self, ty: &mut Type) {
    self.opaque |= matches!(ty, Type::Verbatim(_));
    visit_mut::visit_type_mut(self, ty);
  }

  fn visit_pat_mut(&mut self, pat: &mut syn::Pat) {
    self.opaque |= matches!(pat, syn::Pat::Verbatim(_));
    visit_mut::visit_pat_mut(self, pat);
  }

  fn visit_type_param_bound_mut(&mut self, bound: &mut syn::TypeParamBound) {
    self.opaque |= matches!(bound, syn::TypeParamBound::Verbatim(_));
    visit_mut::visit_type_param_bound_mut(self, bound);
  }

  fn visit_item_mut(&mut self, item: &mut syn::Item) {
    self.opaque |= matches!(item, syn::Item::Verbatim(_));
    self.items += 1;
    visit_mut::visit_item_mut(self, item);
    self.items -= 1;
  }

  fn visit_impl_item_mut(&mut self, item: &mut syn::ImplItem) {
    self.opaque |= matches!(item, syn::ImplItem::Verbatim(_));
    visit_mut::visit_impl_item_mut(self, item);
  }

  fn visit_trait_item_mut(&mut self, item: &mut syn::TraitItem) {
    self.opaque |= matches!(item, syn::TraitItem::Verbatim(_));
    visit_mut::visit_trait_item_mut(self, item);
  }

  fn visit_foreign_item_mut(&mut self, item: &mut syn::ForeignItem) {
    self.opaque |= matches!(item, syn::ForeignItem::Verbatim(_));
    visit_mut::visit_foreign_item_mut(self, item);
  }
}

/// Returns whether the implementation of `Pun` for the type of `input` needs the bound `ty: Pun`,
/// for the type `ty` of a field.
///
/// Only a type that names a type parameter needs the bound: any other is `Pun` or is not,
/// whatever the parameters are. A type that names the type of `input` itself must not have it. A
/// type that holds itself through a generic field, such as an offset of another crate
/// (`next: Offset<Self>`), would be `Pun` if its field is, which is `Pun` if the type is, and the
/// compiler does not resolve such a cycle. What such a field needs of the parameters is then said
/// by their own bounds.
fn needs_bound(ty: &Type, input: &DeriveInput) -> bool {
  fn names(tokens: TokenStream, idents: &[String]) -> bool {
    tokens.into_iter().any(|token| match token {
      proc_macro2::TokenTree::Ident(ident) => idents.iter().any(|name| ident == name),
      proc_macro2::TokenTree::Group(group) => names(group.stream(), idents),
      _ => false,
    })
  }
  let parameters =
    input.generics.type_params().map(|param| param.ident.to_string()).collect::<Vec<_>>();
  let this = [input.ident.to_string(), "Self".to_owned()];
  names(ty.to_token_stream(), &parameters) && !names(ty.to_token_stream(), &this)
}

/// Derives `Pun` for an enum with fields, and passes one of the `Rust` representation to
/// [`derive_discovered`].
///
/// With `repr(C)` or an integer, Rust specifies the layout (RFC 2195): the offsets of the fields
/// are those of `repr(C)` structs, computed from the sizes and the alignments of the fields
/// (`Layout::repr_c`). `bounds` are those of `#[pun(bound(..))]`, if any.
fn derive_tagged(
  input: &DeriveInput,
  data: &DataEnum,
  root: &TokenStream,
  bounds: Option<&Bounds>,
) -> Result<TokenStream> {
  let repr = parse_repr(&input.attrs)?;
  if !repr.c && repr.int.is_none() {
    return derive_discovered(input, data, root, bounds);
  }
  let (ident, vis) = (&input.ident, &input.vis);
  let (_, ty_generics, _) = input.generics.split_for_impl();
  let this = quote!(#ident #ty_generics);
  let types = data.variants.iter().flat_map(|variant| &variant.fields).map(|field| &field.ty);
  let generics = bounded(input, (types, core::iter::empty()), bounds, root);
  let id = identity(input, &generics, root);
  // The bounds of the implementations, which `bounded` puts on the parameters where it can.
  let (impl_generics, _, where_clause) = generics.split_for_impl();

  // The tag: an enum without fields of the same variants and discriminants, with `Self` replaced
  // by the enum (`Discriminant`). It evaluates the discriminants again, here, so
  // `Variant::declared_tags` compares its tags with those of the enum.
  let tag = format_ident!("__PunTag");
  let tag_repr = repr.int.as_ref().map_or_else(|| quote!(C), |int| quote!(#int));
  let discriminants = data
    .variants
    .iter()
    .map(|variant| {
      variant.discriminant.as_ref().map(|(_, expr)| {
        let mut discriminant = Discriminant { this: ident, items: 0, opaque: false };
        let mut expr = expr.clone();
        discriminant.visit_expr_mut(&mut expr);
        (expr, discriminant.opaque)
      })
    })
    .collect::<Vec<_>>();
  let tags = data.variants.iter().zip(&discriminants).map(|(variant, discriminant)| {
    let name = &variant.ident;
    let discriminant = discriminant.as_ref().map(|(expr, _)| quote!(= #expr));
    quote!(#name #discriminant)
  });
  let names = data.variants.iter().map(|variant| &variant.ident).collect::<Vec<_>>();
  let count = names.len();
  // What the declaration of each variant tells of its tag, for a variant without a sample: a
  // discriminant has the same value here if all of it is read (`Discriminant`), and no
  // discriminant is the tag before it plus one.
  let option = quote!(::core::option::Option);
  let declared = discriminants.iter().map(|discriminant| match discriminant {
    None => quote!(#option::None),
    Some((_, opaque)) => {
      let known = !opaque;
      quote!(#option::Some(#known))
    }
  });
  // A value of each variant whose fields have samples, made as `derive_discovered` makes them,
  // whose first bytes are its tag.
  let layout = quote!(#root::layout::Layout);
  let (homes, home_types) = variant_homes(input, data);
  let (samples, exists): (Vec<_>, Vec<_>) = data
    .variants
    .iter()
    .zip(&home_types)
    .map(|(variant, types)| {
      let name = &variant.ident;
      let members = variant.fields.iter().enumerate().map(|(j, field)| {
        field
          .ident
          .as_ref()
          .map_or_else(|| Member::Unnamed(Index::from(j)), |ident| Member::Named(ident.clone()))
      });
      let layouts = quote!(&[#(&<#types as #root::Pun>::LAYOUT),*]);
      (
        quote! {
          if #layout::inhabited_all(#layouts) {
            #option::Some(#layout::wrap(Self::#name {
              // SAFETY (of the generated code): Each field type has values, which is checked
              // above, so `Layout::sample` makes a valid value of it, as `assume_init` requires.
              #(#members: unsafe { #layout::sample::<#types>().assume_init() }),*
            }))
          } else {
            #option::None
          }
        },
        quote!(#layout::may_exist_all(#layouts)),
      )
    })
    .unzip();

  // A `repr(C)` struct for each variant (RFC 2195), which starts with the tag unless the enum is
  // `repr(C)`: their layouts, from the sizes and alignments of the fields. Not measured from
  // structs declared here, which would hold a marker of the generic parameters, `PhantomData`,
  // named by a path that `extern crate other as core;` can make name another type.
  let shadows = (0..count).map(|i| format_ident!("__PunVariant{}", i)).collect::<Vec<_>>();
  let shapes = (0..count).map(|i| format_ident!("V{}", i)).collect::<Vec<_>>();
  let usize = quote!(::core::primitive::usize);
  let (shape_decls, shape_defs): (Vec<_>, Vec<_>) = home_types
    .iter()
    .zip(&shapes)
    .map(|(types, shape)| {
      let n = types.len();
      let fields = types.iter().map(|ty| quote!(#root::layout::Layout::size_align::<#ty>()));
      let start = if repr.c {
        quote!((0, 1))
      } else {
        quote!((<#tag as #root::Pun>::LAYOUT.size(), <#tag as #root::Pun>::LAYOUT.align()))
      };
      let ty = quote!(([#usize; #n], (#usize, #usize)));
      (
        quote!(const #shape: #ty;),
        quote!(const #shape: #ty = #root::layout::Layout::repr_c(#start, [#(#fields),*]);),
      )
    })
    .unzip();
  let sizes = shapes.iter().map(|shape| quote!(<Self as __PunShapes>::#shape.1));
  let sizes = quote!([#(#sizes),*]);
  let tag_layout = quote!(<#tag as #root::Pun>::LAYOUT);
  // Where the fields of the variants are.
  let fields =
    if repr.c { quote!(#root::layout::Enum::fields(&#tag_layout, &#sizes)) } else { quote!(0) };
  let (mut items, mut variants, mut offsets) = (vec![], vec![], vec![]);
  for (i, variant) in data.variants.iter().enumerate() {
    let (shadow, shape) = (&shadows[i], &shapes[i]);
    // The offsets of the fields of the variant, measured on its sample, or, if it has none, what
    // the representation that the derive read specifies.
    let n = variant.fields.len();
    let offsets_name = format_ident!("OFFSETS{}", i);
    let name = &variant.ident;
    let members = variant
      .fields
      .iter()
      .enumerate()
      .map(|(j, field)| {
        field
          .ident
          .as_ref()
          .map_or_else(|| Member::Unnamed(Index::from(j)), |ident| Member::Named(ident.clone()))
      })
      .collect::<Vec<_>>();
    let bindings = (0..n).map(|j| format_ident!("__pun_field{}", j)).collect::<Vec<_>>();
    let types = &home_types[i];
    let indices = (0..n).map(Index::from).collect::<Vec<_>>();
    let computed = quote!([#(#fields + <Self as __PunShapes>::#shape.0[#indices]),*]);
    offsets.push(quote! {
      const #offsets_name: [#usize; #n] = {
        // Bound by `let`, so that the samples live as long as the pointers into them.
        let samples = &<Self as __PunShapes>::SAMPLES;
        let base = #layout::sample_of(samples, #i);
        if base.is_null() {
          #computed
        } else {
          // SAFETY (of the generated code): The reference made from `base` meets the
          // requirements of a reference: it is not null, aligned and not dangling, because `base`
          // is `as_ptr` of a `MaybeUninit<Self>` in `SAMPLES`, and it points to a valid `Self`,
          // because the sample is the variant made of samples of its fields, which are valid.
          // Nothing writes `SAMPLES` while it lives.
          match unsafe { &*base } {
            Self::#name { #(#members: #bindings),* } => [#(
              // SAFETY (of the generated code): Each binding is a reference to a field of the
              // value at `base`, made from `base`. So both pointers are derived from the same
              // allocation, and the bytes between them are in it, as `byte_offset_from`
              // requires. The distance is the offset of the field, not negative.
              unsafe { (#bindings as *const #types).byte_offset_from(base) } as #usize
            ),*],
            // Not the variant: a sample is made as its variant, so this is not reached.
            #[allow(unreachable_patterns)]
            _ => #computed,
          }
        }
      };
    });
    let markers = (0..variant.fields.len())
      .map(|j| format_ident!("__PunVariant{}Field{}", i, j))
      .collect::<Vec<_>>();
    let mut layouts = vec![];
    for (j, field) in variant.fields.iter().enumerate() {
      let options = parse_options(&field.attrs, "a field of an enum", &["valid"])?;
      let ty = &home_types[i][j];
      let offset = quote!(<Self as __PunShapes>::#offsets_name[#j]);
      layouts.push(match &options.valid {
        None => quote!(#root::layout::Field::new(#offset, &<#ty as #root::Pun>::LAYOUT)),
        Some(ranges) => {
          let ranges = valid_ranges(ranges, ty, root)?;
          quote!(#root::layout::Field::restricted(
            #offset,
            const { &<#ty as #root::Pun>::LAYOUT.restricted(#ranges) },
          ))
        }
      });
      let (marker, next) = (
        &markers[j],
        markers.get(j + 1).map_or_else(|| quote!(()), |marker| quote!(#marker #ty_generics)),
      );
      items.push(quote! {
        #[allow(non_camel_case_types)]
        #vis struct #marker #impl_generics(::core::marker::PhantomData<#this>) #where_clause;

        // SAFETY: `FIELD` is the field `#j` of the variant `#i` in the layout of the enum, of
        // type `#ty`: `layouts` holds the fields of the variant in the order of their
        // declarations. `Next` is the field after it, or `()` after the last. So the list is
        // the fields of the variant in the layout, in order, as the `# Safety` of `Fields`
        // requires.
        unsafe impl #impl_generics #root::Fields for #marker #ty_generics #where_clause {
          type Type = #ty;
          type Next = #next;
          type Last = ();
          type Path<__P: #root::Path> = #root::At<__P, Self>;
          const FIELD: ::core::option::Option<#root::layout::Field> =
            ::core::option::Option::Some(*<#this as #root::Pun>::LAYOUT.variant_field(#i, #j));
        }
      });
    }
    let (fields, next) = (
      markers.first().map_or_else(|| quote!(()), |marker| quote!(#marker #ty_generics)),
      shadows.get(i + 1).map_or_else(|| quote!(()), |shadow| quote!(#shadow #ty_generics)),
    );
    items.push(quote! {
      // As visible as the enum, so that it can be named by its implementation of `Pun`.
      #[allow(dead_code, non_camel_case_types)]
      #vis struct #shadow #impl_generics(::core::marker::PhantomData<#this>) #where_clause;

      // SAFETY: `TAG` is the tag of the variant `#i` of the layout of the enum, whose fields are
      // the list from its first marker. `Next` is the variant after it, or `()` after the last.
      // So the list is the variants of the layout, in order, as the `# Safety` of `Variants`
      // requires.
      unsafe impl #impl_generics #root::Variants for #shadow #ty_generics #where_clause {
        type Fields = #fields;
        type Next = #next;
        const TAG: ::core::option::Option<::core::primitive::u128> =
          ::core::option::Option::Some(<#this as #root::Pun>::LAYOUT.variant_tag(#i));
      }
    });
    variants.push(quote!(#root::layout::Variant::new(
      <Self as __PunShapes>::TAGS[#i],
      const {
        &#root::layout::Layout::variant::<Self>(
          const { &#root::layout::Layout::sort_fields([#(#layouts),*]) },
        )
      },
    )));
  }
  let (first, outside, pin) = (&shadows[0], repr.c, pin(root));
  let offset_names = (0..count).map(|i| format_ident!("OFFSETS{}", i));
  let offset_lens = data.variants.iter().map(|variant| variant.fields.len());
  let variant_indices = 0..count;

  Ok(quote! {
    const _: () = {
      #pin

      #homes

      #[repr(#tag_repr)]
      #[derive(Clone, Copy)]
      #[allow(dead_code, non_camel_case_types, clippy::enum_variant_names)]
      enum #tag { #(#tags),* }

      // SAFETY: `#tag` is an enum without fields, of the variants of the enum and their
      // discriminants. As for any enum without fields, `SPEC` accepts exactly the bytes of its
      // variants, which are its valid values, and it holds no `UnsafeCell`.
      unsafe impl #root::Scalar for #tag {
        const SPEC: #root::layout::Spec = {
          let merged: &'static ([(u128, u128); #count], usize) = const {
            // SAFETY: As for `TAGS` below, every byte of a value of `#tag` is initialized, which is
            // what `bits_of` requires.
            &#root::layout::Spec::sorted_ranges([
              #(unsafe { #root::layout::Spec::bits_of(#tag::#names) }),*
            ])
          };
          #root::layout::Spec::native_ranges::<Self>(merged.0.split_at(merged.1).0)
        };
      }

      #(#items)*

      trait __PunShapes: ::core::marker::Sized {
        #(#shape_decls)*
        const SAMPLES: #root::layout::Samples<Self, #count>;
        #(const #offset_names: [#usize; #offset_lens];)*
        const TAGS: [::core::primitive::u128; #count];
      }

      impl #impl_generics __PunShapes for #this #where_clause {
        #(#shape_defs)*
        // A variant with a field without values has no sample, but it is written.
        #[allow(unreachable_code)]
        const SAMPLES: #root::layout::Samples<Self, #count> = [#(#samples),*];
        #(#offsets)*
        const TAGS: [::core::primitive::u128; #count] = {
          let tags =
          // SAFETY (of the generated code): `#tag` is an enum without fields, `repr(C)` or of a
          // primitive representation, without `align`. Such an enum is an integer that holds its
          // discriminant (the crate documentation, "What Rust documents"), so every byte of it is
          // initialized. Each sample is a value of a variant of the enum, which starts with its
          // tag, the enum with all fields removed [1] [2]. With an integer, the tag is that
          // integer, as `#tag` is. With `repr(C)` alone, the discriminants are implicit [3] [4], so
          // the tag and `#tag` are the same enum without fields. Either way, the first
          // `size_of::<#tag>()` bytes of a sample are its tag, an integer, so initialized. These
          // are the requirements of `declared_tags`.
          //
          // [1] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.primitive.adt:
          //
          //     The first field of each struct in the union is the primitive representation
          //     version of the enum with all fields removed ("the tag")
          //
          // [2] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.c.adt.tag:
          //
          //     a `repr(C)` version of the enum with all fields removed ("the tag")
          //
          // [3] Per https://doc.rust-lang.org/1.99.0/reference/items/enumerations.html#r-items.enum.discriminant.explicit.intro:
          //
          //     In two circumstances, the discriminant of a variant may be explicitly set by
          //     following the variant name with `=` and a constant expression:
          //     - if the enumeration is "unit-only".
          //     - if a primitive representation is used.
          //
          // [4] Per https://doc.rust-lang.org/1.99.0/reference/items/enumerations.html#r-items.enum.discriminant.implicit:
          //
          //     If a discriminant for a variant is not specified, then it is set to one higher
          //     than the discriminant of the previous variant in the declaration. If the
          //     discriminant of the first variant in the declaration is unspecified, then it is
          //     set to zero.
          unsafe {
            #root::layout::Variant::declared_tags::<Self, #tag, #count>(
              &[#(#tag::#names),*],
              &<Self as __PunShapes>::SAMPLES,
              [#(#declared),*],
              [#(#exists),*],
            )
          };
          // The byte after the tag that the derive read, set to 0 and to 255 in a copy of each
          // sample, which changes the tag of the enum as compiled if it is wider, and otherwise
          // a field or padding of the variant, which leaves its variant as it is.
          let after = <#tag as #root::Pun>::LAYOUT.size();
          let samples = &<Self as __PunShapes>::SAMPLES;
          let mut i = 0;
          while i < #count {
            let sample = #layout::sample_of(samples, i);
            if !sample.is_null() {
              let (values, mut byte) = ([0, 255], 0);
              while byte < 2 {
                // SAFETY (of the generated code): `sample` is not null, and points to a
                // `MaybeUninit<Self>` in `SAMPLES`, so it is aligned and valid for reads of the
                // bytes of a `Self`, as `with_byte` requires.
                let changed = unsafe {
                  #layout::with_byte(sample, after, values[byte])
                };
                // SAFETY (of the generated code): The reference made from `as_ptr` of `changed`, a
                // local `MaybeUninit<Self>`, is not null, aligned, and not dangling. It points to a
                // sample whose byte `after` may have been changed, so it may be no `Self`. This
                // constant is evaluated at compile time, where the `match`, which reads the
                // variant, relies on the behaviors of `rustc` that `discover` relies on, and that
                // the crate documentation discloses (section Soundness): `rustc` reports an error
                // if the tag is not the tag of a variant, and otherwise reads the variant from the
                // tag alone, without reading the fields.
                let read = match unsafe { &*changed.as_ptr() } {
                  #(Self::#names { .. } => #variant_indices,)*
                };
                #layout::same_variant(read, i);
                byte += 1;
              }
            }
            i += 1;
          }
          tags
        };
      }

      // SAFETY: For every enum that `derive_tagged` accepts, the layout meets the `# Safety` of
      // `Pun`:
      // - The enum is `repr(C)` or of a primitive representation, whose layout Rust specifies
      //   [1] [2]. Its tag is `#tag`, an enum without fields of the same representation and
      //   discriminants, whose values `TAGS` compares with those of the enum, and fails the build
      //   if they differ, and whose width `TAGS` checks on copies of the samples whose byte after
      //   the tag is changed. Each variant is a `repr(C)` struct, after the tag or in a union next
      //   to it. The offsets of its fields are measured on its sample, or, for a variant without
      //   one, computed by `Layout::repr_c` as `repr(C)` specifies, from the representation that
      //   the derive read, which it trusts to be that of the enum as compiled there ("What the
      //   derive trusts"). `Layout::enumeration` asserts the result against the size and the
      //   alignment of the enum, and asserts that the tag and the fields are within it and do not
      //   overlap.
      // - The type of each field is its home in `__PunTypes`, which `item` requires to be the type
      //   of that field of that variant in the enum as compiled, and the enum to have no other
      //   variant and no other field. Each field has the layout of its type, or a restriction of
      //   it. An enum is valid if its tag is that of a variant and the fields of that variant are
      //   valid [3], which is what the layout accepts.
      // - `Variants` lists the variants in order, each with its fields (see the markers above).
      //   The other lists are `()`.
      // - Each field is `Pun`, and the tag is an integer, so no byte is in an `UnsafeCell`.
      //
      // [1] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.primitive.adt:
      //
      //     The representation of a primitive representation enum is a `repr(C)` union of
      //     `repr(C)` structs for each variant with a field. The first field of each struct in
      //     the union is the primitive representation version of the enum with all fields
      //     removed ("the tag") and the remaining fields are the fields of that variant.
      //
      // [2] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.c.adt.intro:
      //
      //     The representation of a `repr(C)` enum with fields is a `repr(C)` struct with two
      //     fields, also called a "tagged union" in C
      //
      // [3] Per https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.enum:
      //
      //     An enum must have a valid discriminant, and all fields of the variant indicated by
      //     that discriminant must be valid at their respective type.
      unsafe impl #impl_generics #root::Pun for #this #where_clause {
        type Elem = ();
        type Fields = ();
        type Variants = #first #ty_generics;
        type Rules = ();
        type Metadata = ();
        const PROBE: *const Self = ::core::ptr::null();
        const LAYOUT: #root::layout::Layout = #root::layout::Layout::enumeration::<Self>(
          const {
            &#root::layout::Enum::new(
              &#tag_layout,
              #outside,
              &#sizes,
              #fields,
              const { &[#(#variants),*] },
            )
          },
        )
        .identified(Self::ID);
        const ID: #root::layout::Id = #id;
      }
    };
  })
}

/// Derives `Pun` for an enum with fields, of the `Rust` representation, whose layout Rust leaves to
/// the compiler.
///
/// The layout is found by experiments in const evaluation on a sample of each variant
/// (`Layout::discover`). The layouts of the fields make the samples (`Layout::sample`). The
/// offsets of the fields are found by a `match` on the sample. Every function that the generated
/// code calls is an associated function of a type that `Layout::PIN` fixes, or a method of a
/// value that one of them makes, as in the rest of the derive. The description that the
/// experiments find has the type that `Layout::discover` returns, whatever the path names.
/// `bounds` are those of `#[pun(bound(..))]`, if any.
fn derive_discovered(
  input: &DeriveInput,
  data: &DataEnum,
  root: &TokenStream,
  bounds: Option<&Bounds>,
) -> Result<TokenStream> {
  let (ident, vis) = (&input.ident, &input.vis);
  let (_, ty_generics, _) = input.generics.split_for_impl();
  let this = quote!(#ident #ty_generics);
  let types = data.variants.iter().flat_map(|variant| &variant.fields).map(|field| &field.ty);
  let generics = bounded(input, (types, core::iter::empty()), bounds, root);
  let id = identity(input, &generics, root);
  // The bounds of the implementations, which `bounded` puts on the parameters where it can.
  let (impl_generics, _, where_clause) = generics.split_for_impl();
  let count = data.variants.len();
  let usize = quote!(::core::primitive::usize);
  let layout = quote!(#root::layout::Layout);
  let discover = quote!(<#this as __PunDiscover>);
  let (homes, home_types) = variant_homes(input, data);

  let (mut items, mut samples, mut offsets, mut observed, mut variants) =
    (vec![], vec![], vec![], vec![], vec![]);
  let shadows = (0..count).map(|i| format_ident!("__PunVariant{}", i)).collect::<Vec<_>>();
  for (i, variant) in data.variants.iter().enumerate() {
    let name = &variant.ident;
    let n = variant.fields.len();
    let members = variant
      .fields
      .iter()
      .enumerate()
      .map(|(j, field)| {
        field
          .ident
          .as_ref()
          .map_or_else(|| Member::Unnamed(Index::from(j)), |ident| Member::Named(ident.clone()))
      })
      .collect::<Vec<_>>();
    let types = &home_types[i];
    let bindings = (0..n).map(|j| format_ident!("__pun_field{}", j)).collect::<Vec<_>>();
    let offsets_name = format_ident!("OFFSETS{}", i);

    // A sample of the variant, if it has values.
    samples.push(quote! {
      if #layout::inhabited_all(&[#(&<#types as #root::Pun>::LAYOUT),*]) {
        #layout::wrap(Self::#name {
          // SAFETY (of the generated code): Each field type has values, which is checked above,
          // so `Layout::sample` makes a valid value of it from its layout, as `assume_init`
          // requires (see the samples of `Option<T>` in `std_enums.rs`).
          #(#members: unsafe { #layout::sample::<#types>().assume_init() }),*
        })
      } else {
        #layout::uninit::<Self>()
      }
    });
    // The offsets of its fields, in its sample.
    offsets.push(quote! {
      const #offsets_name: [#usize; #n] = {
        let sample = &<Self as __PunDiscover>::SAMPLES[#i];
        let base = sample.as_ptr();
        if #layout::inhabited_all(&[#(&<#types as #root::Pun>::LAYOUT),*]) {
          // SAFETY (of the generated code): The reference made from `base` meets the
          // requirements of a reference:
          // - It is not null, aligned, and not dangling, because `base` is `as_ptr` of `sample`,
          //   a reference to a `MaybeUninit<Self>` in the constant `SAMPLES`.
          // - It points to a valid `Self`, because the variant has values, which is checked
          //   above, so its sample is the variant made of valid fields.
          // `SAMPLES` is a constant, which nothing writes while the reference lives.
          match unsafe { &*base } {
            Self::#name { #(#members: #bindings),* } => [#(
              // SAFETY (of the generated code): Each binding is a reference to a field of the
              // value at `base`, made from `base`. So both pointers are derived from the same
              // allocation, and the bytes between them are in it, as `byte_offset_from`
              // requires. The distance is the offset of the field, not negative.
              unsafe { (#bindings as *const #types).byte_offset_from(base) } as #usize
            ),*],
            // Not the variant: `Layout::discover` finds that the sample is not.
            #[allow(unreachable_patterns)]
            _ => [0; #n],
          }
        } else {
          [0; #n]
        }
      };
    });
    let indices = (0..n).map(Index::from);
    observed.push(quote! {
      &[#(#root::layout::Field::new(
        <Self as __PunDiscover>::#offsets_name[#indices],
        &<#types as #root::Pun>::LAYOUT,
      )),*]
    });
    // The layout of the variant: its fields where the sample has them (none if it has no value).
    let mut fields = vec![];
    for (j, field) in variant.fields.iter().enumerate() {
      let options = parse_options(&field.attrs, "a field of an enum", &["valid"])?;
      let ty = &types[j];
      let offset = quote!(<Self as __PunDiscover>::#offsets_name[#j]);
      fields.push(match &options.valid {
        None => quote!(#root::layout::Field::new(#offset, &<#ty as #root::Pun>::LAYOUT)),
        Some(ranges) => {
          let ranges = valid_ranges(ranges, ty, root)?;
          quote!(#root::layout::Field::restricted(
            #offset,
            const { &<#ty as #root::Pun>::LAYOUT.restricted(#ranges) },
          ))
        }
      });
    }
    variants.push(quote!(#root::layout::Variant::discovered(
      #i,
      const { &<Self as __PunDiscover>::DESC }.variant_ranges(#i),
      <Self as __PunDiscover>::DESC.holds_tag(#i),
      const {
        &#layout::rust_variant::<Self>({
          let fields: &'static [#root::layout::Field] =
            if <Self as __PunDiscover>::DESC.inhabited(#i) {
              const { &#layout::sort_fields([#(#fields),*]) }
            } else {
              &[]
            };
          fields
        })
      },
    )));

    // The lists of the fields and of the variants, as for an enum of a specified layout.
    let markers = (0..n).map(|j| format_ident!("__PunVariant{}Field{}", i, j)).collect::<Vec<_>>();
    for (j, ty) in types.iter().enumerate() {
      let (marker, next) = (
        &markers[j],
        markers.get(j + 1).map_or_else(|| quote!(()), |marker| quote!(#marker #ty_generics)),
      );
      items.push(quote! {
        #[allow(non_camel_case_types)]
        #vis struct #marker #impl_generics(::core::marker::PhantomData<#this>) #where_clause;

        // SAFETY: If the variant `#i` has values, `FIELD` is its field `#j` in the layout of the
        // enum, of type `#ty`, at the offset observed on its sample, and `Next` is the field
        // after it. Otherwise the layout of the variant has no field, and `FIELD` is `None`,
        // which ends the list. Either way the list is the fields of the variant in the layout,
        // in order, as the `# Safety` of `Fields` requires.
        unsafe impl #impl_generics #root::Fields for #marker #ty_generics #where_clause {
          type Type = #ty;
          type Next = #next;
          type Last = ();
          type Path<__P: #root::Path> = #root::At<__P, Self>;
          const FIELD: ::core::option::Option<#root::layout::Field> =
            if #discover::DESC.inhabited(#i) {
              ::core::option::Option::Some(*<#this as #root::Pun>::LAYOUT.variant_field(#i, #j))
            } else {
              ::core::option::Option::None
            };
        }
      });
    }
    let (first_field, next) = (
      markers.first().map_or_else(|| quote!(()), |marker| quote!(#marker #ty_generics)),
      shadows.get(i + 1).map_or_else(|| quote!(()), |shadow| quote!(#shadow #ty_generics)),
    );
    let shadow = &shadows[i];
    items.push(quote! {
      #[allow(dead_code, non_camel_case_types)]
      #vis struct #shadow #impl_generics(::core::marker::PhantomData<#this>) #where_clause;

      // SAFETY: `TAG` is the index of the variant `#i`, and `RANGES` its tags, in the layout of
      // the enum. `Next` is the variant after it, or `()` after the last. So the list is the
      // variants of the layout, in order, as the `# Safety` of `Variants` requires.
      unsafe impl #impl_generics #root::Variants for #shadow #ty_generics #where_clause {
        type Fields = #first_field;
        type Next = #next;
        const TAG: ::core::option::Option<::core::primitive::u128> =
          ::core::option::Option::Some(<#this as #root::Pun>::LAYOUT.variant_tag(#i));
        const RANGES: ::core::option::Option<&'static [(::core::primitive::u128, ::core::primitive::u128)]> =
          <#this as #root::Pun>::LAYOUT.variant_ranges(#i);
      }
    });
  }
  let (first, pin) = (&shadows[0], pin(root));
  let offset_decls = data.variants.iter().enumerate().map(|(i, variant)| {
    let (name, n) = (format_ident!("OFFSETS{}", i), variant.fields.len());
    quote!(const #name: [#usize; #n];)
  });

  Ok(quote! {
    const _: () = {
      #pin

      #homes

      #(#items)*

      trait __PunDiscover: ::core::marker::Sized {
        const SAMPLES: [::core::mem::MaybeUninit<Self>; #count];
        #(#offset_decls)*
        const FIELDS: [&'static [#root::layout::Field]; #count];
        const DESC: #root::layout::Desc<#count>;
        const TAG: #root::layout::Layout;
        const VARIANTS: [#root::layout::Variant; #count];
        const ENUM: #root::layout::Enum;
      }

      impl #impl_generics __PunDiscover for #this #where_clause {
        // A variant with a field without values is not made, but it is written.
        #[allow(unreachable_code)]
        const SAMPLES: [::core::mem::MaybeUninit<Self>; #count] = [#(#samples),*];
        #(#offsets)*
        const FIELDS: [&'static [#root::layout::Field]; #count] = [#(#observed),*];
        // SAFETY: `DESC` is an associated constant, so the call is evaluated at compile time,
        // which is what `discover` requires.
        const DESC: #root::layout::Desc<#count> = unsafe {
          #layout::discover(
            &<Self as __PunDiscover>::SAMPLES,
            &<Self as __PunDiscover>::FIELDS,
          )
        };
        const TAG: #root::layout::Layout = const { &<Self as __PunDiscover>::DESC }.tag();
        const VARIANTS: [#root::layout::Variant; #count] = [#(#variants),*];
        const ENUM: #root::layout::Enum = #root::layout::Enum::discovered(
          const { &<Self as __PunDiscover>::TAG },
          <Self as __PunDiscover>::DESC.tag_offset(),
          const { &<Self as __PunDiscover>::VARIANTS },
          #layout::size_align::<Self>(),
        );
      }

      // SAFETY: The layout is the one that `rustc` gave the enum, found by the experiments of
      // `Layout::discover`. These fail to compile rather than find a wrong layout, under the
      // facts of the compiler that the module documentation of `discover` lists. Each variant
      // holds its fields at the offsets observed on its sample, each with the layout of its type,
      // its home in `__PunTypes`, which `item` requires to be the type of that field in the enum
      // as compiled, and the enum to have no other variant and no other field.
      // `Variants` lists the variants in order, each with its fields. The other lists are `()`.
      // Each field is `Pun`, so no byte is in an `UnsafeCell`.
      unsafe impl #impl_generics #root::Pun for #this #where_clause {
        type Elem = ();
        type Fields = ();
        type Variants = #first #ty_generics;
        type Rules = ();
        type Metadata = ();
        const PROBE: *const Self = ::core::ptr::null();
        // Rust specifies nothing of it: the layout that `puns` assumes where it applies, checked
        // against what the experiments found, and otherwise opaque without the `unstable` feature.
        const LAYOUT: #root::layout::Layout =
          #layout::enumeration::<Self>(const { &<Self as __PunDiscover>::ENUM })
            .assumed::<Self, #count>(
              &<Self as __PunDiscover>::DESC,
              &<Self as __PunDiscover>::FIELDS,
            )
            .identified(Self::ID);
        const ID: #root::layout::Id = #id;
      }
    };
  })
}

/// Derives `Pun` for a `repr(C)` union, packed or not.
///
/// Each field is a variant of one field, at offset 0. A value is valid if it is a valid value of
/// one of them (`Layout::union`). `packed(N)` lowers the alignment of the union, which may lower
/// its size, and moves no field. `Layout::union` takes the size and the alignment from the
/// compiler.
///
/// # Errors
///
/// Returns an error if the union is not `repr(C)`, or if an option does not parse or does not
/// belong there.
fn derive_union(input: &DeriveInput, data: &syn::DataUnion) -> Result<TokenStream> {
  let repr = parse_repr(&input.attrs)?;
  if !repr.c {
    return Err(Error::new(
      input.ident.span(),
      "`Pun` is derived for unions of `repr(C)`, whose fields are all at offset 0",
    ));
  }
  let options = parse_options(&input.attrs, "a union", &["crate", "bound"])?;
  let root = options.root.clone().unwrap_or_else(|| quote!(::puns));
  let (ident, vis) = (&input.ident, &input.vis);
  let (_, ty_generics, _) = input.generics.split_for_impl();
  let this = quote!(#ident #ty_generics);
  let types = data.fields.named.iter().map(|field| &field.ty);
  let generics = bounded(input, (types, core::iter::empty()), options.bounds.as_ref(), &root);
  let id = identity(input, &generics, &root);
  // The bounds of the implementations, which `bounded` puts on the parameters where it can.
  let (impl_generics, _, where_clause) = generics.split_for_impl();
  let count = data.fields.named.len();
  let shadows = (0..count).map(|i| format_ident!("__PunVariant{}", i)).collect::<Vec<_>>();
  let (mut items, mut variants, mut named, mut same) = (vec![], vec![], vec![], vec![]);
  for (i, field) in data.fields.named.iter().enumerate() {
    let name = format_ident!("F{}", i);
    let ty = home(&this, &name, &field.ty);
    let member = &field.ident;
    same.push(same_type(&quote!((*p).#member), &ty, &field.ty));
    named.push((name, &field.ty));
    let options = parse_options(&field.attrs, "a field of a union", &["valid"])?;
    // Measured on the union as compiled, as for a struct of the `Rust` representation: 0 if the
    // union is `repr(C)`.
    let offset = quote!({
      let value = #root::layout::Layout::uninit::<Self>();
      let start: *const Self = value.as_ptr();
      // SAFETY: `start` points to `value`, an uninitialized `Self`, which is `Sized` as every
      // union is, so `(*start).#member` is a place in its allocation. `&raw const` makes a pointer
      // to it without reading it, which needs neither initialized bytes nor alignment. That
      // pointer and `start` are derived from the same allocation, and the bytes between them are
      // in it, as `byte_offset_from` requires [1]. The distance is the offset of the field, which
      // is not negative, so `as usize` keeps it.
      //
      // [1] Per https://doc.rust-lang.org/1.99.0/core/primitive.pointer.html#method.offset_from:
      //
      //     `self` and `origin` must either
      //
      //     - point to the same address, or
      //     - both be derived from a pointer to the same allocation, and the memory range
      //       between the two pointers must be in bounds of that object.
      unsafe { (&raw const (*start).#member).byte_offset_from(start) as ::core::primitive::usize }
    });
    let layout = match &options.valid {
      None => quote!(#root::layout::Field::new(#offset, &<#ty as #root::Pun>::LAYOUT)),
      Some(ranges) => {
        let ranges = valid_ranges(ranges, &ty, &root)?;
        quote!(#root::layout::Field::restricted(
          #offset,
          const { &<#ty as #root::Pun>::LAYOUT.restricted(#ranges) },
        ))
      }
    };
    variants.push(quote!(#root::layout::Variant::new(
      #i as ::core::primitive::u128,
      const {
        &#root::layout::Layout::variant::<Self>(
          const { &#root::layout::Layout::sort_fields([#layout]) },
        )
      },
    )));
    let marker = format_ident!("__PunVariant{}Field0", i);
    let next = shadows.get(i + 1).map_or_else(|| quote!(()), |shadow| quote!(#shadow #ty_generics));
    let shadow = &shadows[i];
    items.push(quote! {
      #[allow(non_camel_case_types)]
      #vis struct #marker #impl_generics(::core::marker::PhantomData<#this>) #where_clause;

      // SAFETY: `FIELD` is the one field of the variant `#i` of the layout of the union, the
      // field `#i` of the union, of type `#ty`, at offset 0. `Next` is `()`. So the list is the
      // fields of the variant, as the `# Safety` of `Fields` requires.
      unsafe impl #impl_generics #root::Fields for #marker #ty_generics #where_clause {
        type Type = #ty;
        type Next = ();
        type Last = ();
        type Path<__P: #root::Path> = #root::At<__P, Self>;
        const FIELD: ::core::option::Option<#root::layout::Field> =
          ::core::option::Option::Some(*<#this as #root::Pun>::LAYOUT.variant_field(#i, 0));
      }

      #[allow(dead_code, non_camel_case_types)]
      #vis struct #shadow #impl_generics(::core::marker::PhantomData<#this>) #where_clause;

      // SAFETY: `TAG` is `#i`, the index of the variant of the union layout that is its field `#i`.
      // `Next` is the variant after it, or `()` after the last. So the list is the variants of the
      // layout, in order, as the `# Safety` of `Variants` requires.
      unsafe impl #impl_generics #root::Variants for #shadow #ty_generics #where_clause {
        type Fields = #marker #ty_generics;
        type Next = #next;
        const TAG: ::core::option::Option<::core::primitive::u128> =
          ::core::option::Option::Some(<#this as #root::Pun>::LAYOUT.variant_tag(#i));
      }
    });
  }
  let first = shadows.first().map_or_else(|| quote!(()), |shadow| quote!(#shadow #ty_generics));
  // A field added after the derive read the union would be a value that the layout does not
  // accept, which is sound as a target, and as a source, where it guarantees nothing.
  let homes = homes(input, &named, &quote!(#(#same)*));
  let pin = pin(&root);
  Ok(quote! {
    const _: () = {
      #pin

      #homes

      #(#items)*

      // SAFETY: The union is `repr(C)`, so each field is at offset 0 [1], which is where the layout
      // puts it, with the layout of its type or a restriction of it. `packed(N)` changes no offset:
      // it lowers the alignment of each field for the purpose of placing it [3], and a field at 0
      // is aligned for any alignment. `Layout::union` asserts each offset, measured on the union
      // as compiled. The derive trusts that the `repr` that it read is that of the union as
      // compiled ("What the derive trusts"). The type of each field is its home in `__PunTypes`,
      // which `item` requires to be the type of the field of that name in the union as compiled.
      // A field that the derive did not read only adds values, which the layout does not accept as
      // a target, and guarantees none.
      // `Layout::union` takes the size and the alignment from the compiler. As a target, the
      // layout accepts bytes that are a valid value of one field. Such bytes are a value that safe code makes by writing that field,
      // which is valid [2]. As a source, the layout guarantees nothing, not even that a byte is
      // initialized, which every value holds. `Variants` lists the fields in order. The other lists
      // are `()`. Each field is `Pun`, so no byte is in an `UnsafeCell`.
      //
      // [1] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.c.union.size-align:
      //
      //     Each field lives at byte offset 0 from the beginning of the union.
      //
      // [2] Per https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.union:
      //
      //     Obviously, all values that can be created entirely in safe code are valid.
      //
      // [3] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.alignment.packed-fields:
      //
      //     The alignments of each field, for the purpose of positioning fields, is the smaller of
      //     the specified alignment and the alignment of the field's type.
      unsafe impl #impl_generics #root::Pun for #this #where_clause {
        type Elem = ();
        type Fields = ();
        type Variants = #first;
        type Rules = ();
        type Metadata = ();
        const PROBE: *const Self = ::core::ptr::null();
        const LAYOUT: #root::layout::Layout = #root::layout::Layout::union::<Self>(
          const { &#root::layout::Enum::union(const { &[#(#variants),*] }) },
        )
        .identified(Self::ID);
        const ID: #root::layout::Id = #id;
      }
    };
  })
}

#[cfg(test)]
mod tests {
  use std::ffi::{OsStr, OsString};

  use super::{crate_of, this_derive};

  /// The crate that `rustc` compiles with `args`, without `RUSTC_FORCE_RUSTC_VERSION`.
  fn compiled(args: &[&str]) -> Option<String> {
    crate_of(args.iter().map(OsString::from), None)
  }

  #[test]
  fn the_library_and_the_executables_of_a_package_are_other_crates() {
    let lib = compiled(&["--crate-name", "pkg", "--crate-type", "lib", "-C", "metadata=a"]);
    let bin = compiled(&["--crate-name", "pkg", "--crate-type", "bin", "-C", "metadata=a"]);
    let test = compiled(&["--crate-name", "pkg", "--test", "-C", "metadata=a"]);
    let lib_test = compiled(&["--crate-name=pkg", "--crate-type=lib", "--test", "-Cmetadata=a"]);
    assert_eq!(lib.as_deref(), Some("3:pkg/lib/1:a"));
    assert_eq!(bin.as_deref(), Some("3:pkg/bin/1:a"));
    assert_eq!(test, bin);
    assert_eq!(lib_test, bin);
  }

  #[test]
  fn crates_of_one_name_and_other_metadata_are_other_crates() {
    let one = compiled(&["--crate-name", "pkg", "--crate-type", "lib", "-C", "metadata=a"]);
    let other = compiled(&["--crate-name", "pkg", "--crate-type", "lib", "-C", "metadata=b"]);
    let none = compiled(&["--crate-name", "pkg", "--crate-type", "lib"]);
    assert_ne!(one, other);
    assert_ne!(one, none);
    // Each value is behind its length, so two values are not one that holds the separator.
    let two = compiled(&["--crate-name=x", "--crate-type=lib", "-Cmetadata=a", "-Cmetadata=b"]);
    let joined = compiled(&["--crate-name=x", "--crate-type=lib", "-Cmetadata=a/1:b"]);
    assert_eq!(two.as_deref(), Some("1:x/lib/1:a/1:b"));
    assert_eq!(joined.as_deref(), Some("1:x/lib/5:a/1:b"));
  }

  #[test]
  fn metadata_is_a_set_as_rustc_reads_it() {
    let one = compiled(&["--crate-name=x", "--crate-type=lib", "-Cmetadata=b", "-Cmetadata=a"]);
    let spaced = compiled(&["--crate-name=x", "--crate-type=lib", "-Cmetadata=a b a"]);
    assert_eq!(one.as_deref(), Some("1:x/lib/1:a/1:b"));
    assert_eq!(spaced, one);
  }

  #[test]
  fn every_spelling_of_the_options_is_read() {
    let spelled = [
      &["--crate-name", "x", "--crate-type", "rlib,bin", "--codegen", "metadata=m"][..],
      &["--crate-name=x", "--crate-type=rlib,bin", "--codegen=metadata=m"],
      &["--crate-name", "x", "--crate-type", "bin", "-Cmetadata=m", "-C", "opt-level=3"],
      &["-gOCmetadata=m", "--crate-type", "bin", "--crate-name", "x"],
    ];
    for args in spelled {
      assert_eq!(compiled(args).as_deref(), Some("1:x/bin/1:m"), "{args:?}");
    }
  }

  #[test]
  fn the_values_of_other_options_are_not_read_as_options() {
    let args = [
      "--crate-name",
      "x",
      "--crate-type",
      "lib",
      "-o",
      "--test",
      "--cfg",
      "--crate-name",
      "--extern",
      "--crate-type=bin",
      "-L",
      "-Cmetadata=m",
      "--",
      "--test",
    ];
    assert_eq!(compiled(&args).as_deref(), Some("1:x/lib"));
  }

  /// Writes `source` to a file of its own, and returns what `rustc` compiles from it with `args`
  /// before its path.
  fn compiled_root(source: &str, args: &[&str]) -> Option<String> {
    static FILES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = FILES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("puns-derive-root-{}-{n}.rs", std::process::id()));
    std::fs::write(&path, source).unwrap();
    let path = path.to_str().unwrap().to_owned();
    let krate = compiled(&[args, &[path.as_str()]].concat());
    std::fs::remove_file(&path).unwrap();
    krate
  }

  #[test]
  fn a_crate_without_a_type_has_the_types_of_its_attributes() {
    // As Cargo compiles a test or a bench without a harness.
    let args = ["--crate-name", "x", "-C", "metadata=a", "--cfg", "test"];
    assert_eq!(compiled_root("fn main() {}", &args).as_deref(), Some("1:x/bin/1:a"));
    let lib = compiled_root("//! A library.\n#![crate_type = \"lib\"]\npub fn f() {}", &args);
    assert_eq!(lib.as_deref(), Some("1:x/lib/1:a"));
    let both = "#![crate_type = \"rlib\"]\n#![crate_type = \"bin\"]\nfn main() {}";
    assert_eq!(compiled_root(both, &args).as_deref(), Some("1:x/bin/1:a"));
    // A byte order mark, and a first line `#!` that is not an attribute, come before attributes.
    let shebang = "\u{feff}#!/usr/bin/env run\n#![crate_type = \"cdylib\"]\n";
    assert_eq!(compiled_root(shebang, &args).as_deref(), Some("1:x/lib/1:a"));
    let spaced = "#! \n [crate_type = \"staticlib\"]\n";
    assert_eq!(compiled_root(spaced, &args).as_deref(), Some("1:x/lib/1:a"));
    // A type of `--crate-type`, or `--test`, comes before the attributes.
    let typed = compiled_root("#![crate_type = \"lib\"]", &["--crate-name=x", "--crate-type=bin"]);
    assert_eq!(typed.as_deref(), Some("1:x/bin"));
  }

  #[test]
  fn a_crate_whose_attributes_the_derive_cannot_read_is_none() {
    let args = ["--crate-name", "x"];
    assert_eq!(compiled_root("#![crate_type = \"other\"]", &args), None);
    assert_eq!(compiled_root("#![crate_type = concat!(\"li\", \"b\")]", &args), None);
    assert_eq!(compiled_root("#![crate_type]", &args), None);
    assert_eq!(compiled_root("#! /* a comment */ [crate_type = \"lib\"]", &args), None);
    assert_eq!(compiled_root("#![crate_type = \"lib\"", &args), None);
    let added =
      compiled_root("fn main() {}", &["--crate-name", "x", "-Zcrate-attr=crate_type=\"lib\""]);
    assert_eq!(added, None);
    // Two files, such as the path of `rustc` that a driver is given, then the root file.
    assert_eq!(compiled_root("fn main() {}", &["rustc", "--crate-name", "x"]), None);
    assert_eq!(compiled(&["--crate-name", "x", "-"]), None);
    assert_eq!(compiled(&["--crate-name", "x", "/nonexistent/main.rs"]), None);
  }

  #[test]
  fn an_option_that_rustc_does_not_have_names_no_crate() {
    let typed = ["--crate-name", "x", "--crate-type", "lib"];
    for other in [&["--env-set", "A=B"][..], &["--unknown"], &["--test=1"], &["-Q"]] {
      assert_eq!(compiled(&[&typed[..], other].concat()), None, "{other:?}");
    }
    for known in [&["-j", "4"][..], &["-j4"], &["--jobs=2"], &["--jobs-frontend", "2"], &["-v"]] {
      assert_eq!(compiled(&[&typed[..], known].concat()).as_deref(), Some("1:x/lib"), "{known:?}");
    }
  }

  /// Checks that the options of the derive are those of the `rustc` of the build, as it lists them
  /// with `-Z unstable-options --help -v`: each line names an option, short, long or both, then the
  /// form of its value, if it takes one.
  #[test]
  fn the_options_are_those_of_rustc() {
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let output = std::process::Command::new(rustc)
      .args(["-Z", "unstable-options", "--help", "-v"])
      .env("RUSTC_BOOTSTRAP", "1")
      .output()
      .unwrap();
    assert!(output.status.success(), "{output:?}");
    let help = String::from_utf8(output.stdout).unwrap();
    let (mut short_value, mut short_flags, mut long_value, mut long_flags) =
      (String::new(), String::new(), vec![], vec![]);
    for line in help.lines().filter(|line| line.trim_start().starts_with('-')) {
      let mut words = line.split_whitespace().peekable();
      let mut names = vec![];
      while let Some(name) = words.next_if(|word| word.starts_with('-')) {
        names.push(name.trim_end_matches(','));
      }
      let next = words.next().unwrap_or("");
      // `-C help`, `-W help` and `-Z help` print the options of each.
      if next == "help" {
        continue;
      }
      let value = next.starts_with('<') || next.starts_with('[');
      for name in names {
        match (name.strip_prefix("--"), value) {
          (Some(long), true) => long_value.push(long.to_owned()),
          (Some(long), false) => long_flags.push(long.to_owned()),
          (None, true) => short_value.push_str(&name[1..]),
          (None, false) => short_flags.push_str(&name[1..]),
        }
      }
    }
    let sorted = |mut list: Vec<String>| {
      list.sort();
      list
    };
    let chars = |list: &str| sorted(list.chars().map(String::from).collect());
    let listed = |list: &[&str]| sorted(list.iter().map(|&name| name.to_owned()).collect());
    assert_eq!(chars(&short_value), chars(super::SHORT_WITH_VALUE));
    assert_eq!(chars(&short_flags), chars(super::SHORT_FLAGS));
    assert_eq!(sorted(long_value), listed(super::LONG_WITH_VALUE));
    assert_eq!(sorted(long_flags), listed(super::LONG_FLAGS));
  }

  #[test]
  fn the_forced_version_of_rustc_is_part_of_the_crate() {
    let args = ["--crate-name", "x", "--crate-type", "lib"].map(OsString::from);
    let forced = crate_of(args.iter().cloned(), Some(OsStr::new("1.0")));
    assert_eq!(forced.as_deref(), Some("1:x/lib/version 3:1.0"));
  }

  #[test]
  fn a_crate_that_the_arguments_do_not_name_is_none() {
    assert_eq!(compiled(&["--crate-type", "lib", "-C", "metadata=a"]), None);
    assert_eq!(compiled(&["--crate-name", "x", "--crate-name", "y", "--crate-type", "lib"]), None);
    assert_eq!(compiled(&["--crate-name", "x", "--crate-type", "lib", "@/nonexistent/args"]), None);
    // An `@shell:` file after `-Z shell-argfiles`, which the derive does not read; before it, the
    // file is named `shell:args`.
    assert_eq!(
      compiled(&["--crate-name=x", "--crate-type=lib", "-Z", "shell-argfiles", "@shell:a"]),
      None
    );
    assert_eq!(compiled(&["--crate-name", "x", "--crate-type", "lib", "@shell:args"]), None);
    assert_eq!(compiled(&["--crate-name", "x", "--crate-type", "lib", "-Q"]), None);
    assert_eq!(compiled(&["--crate-type", "lib", "--crate-name"]), None);
  }

  #[cfg(unix)]
  #[test]
  fn an_argument_that_is_not_utf8_names_no_crate() {
    use std::os::unix::ffi::OsStringExt;
    let args =
      [OsString::from("--crate-name"), OsString::from("x"), OsString::from_vec(vec![0xff])];
    assert_eq!(crate_of(args.into_iter(), None), None);
  }

  #[test]
  fn the_arguments_of_a_file_are_its_lines() {
    let path = std::env::temp_dir().join(format!("puns-derive-args-{}", std::process::id()));
    std::fs::write(&path, "--crate-name\nx\n--crate-type\nlib\n-C\nmetadata=m\n").unwrap();
    let krate = compiled(&[&format!("@{}", path.display())]);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(krate.as_deref(), Some("1:x/lib/1:m"));
  }

  #[test]
  fn a_copy_of_the_derive_has_one_name() {
    assert_eq!(this_derive(), this_derive());
  }
}
