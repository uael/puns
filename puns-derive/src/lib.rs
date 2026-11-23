//! The derive macro `#[derive(Pun)]` of the `puns` crate, which documents it.

use std::sync::atomic::{AtomicUsize, Ordering};

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
  "Implements `Pun` for a struct, an enum or a union, or the reading functions of a view, as ",
  "[`puns::Pun`](https://docs.rs/puns/",
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

/// The documentation of `puns` on docs.rs, at the version of this crate, which is that of `puns`,
/// for the links of the documentation that the derive generates.
const DOCS: &str = concat!("https://docs.rs/puns/", env!("CARGO_PKG_VERSION"), "/puns/");

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
/// A use of the type then gives no error that it does not implement `Pun`, so the error of the
/// derive comes first. An evaluation of its layout fails with "`#[derive(Pun)]` failed", next to
/// it. The fallback is never used, since the crate does not compile.
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

/// Returns the identity of the type of `input` (`puns::layout::Id`): where it is declared, and
/// its name.
///
/// A generic type has none (`Id::NONE`), since two of its instances are other types, which `rustc`
/// may lay out differently, and what makes one is not all `Pun` (`PhantomData<T>`). Lifetime
/// parameters do not count.
fn identity(input: &DeriveInput, root: &TokenStream) -> TokenStream {
  // One crate is expanded by one process, in which each call takes the next number, so no two
  // types of a crate have the same one. A derive of an enum with fields calls this twice.
  static DERIVES: AtomicUsize = AtomicUsize::new(0);
  let generic =
    input.generics.params.iter().any(|param| !matches!(param, GenericParam::Lifetime(_)));
  if generic {
    return quote!(#root::layout::Id::NONE);
  }
  let ident = &input.ident;
  let n = proc_macro2::Literal::usize_unsuffixed(DERIVES.fetch_add(1, Ordering::Relaxed));
  // Of the type that `Layout::PIN` fixes, and that `Layout::identified` and `Scalar::ID` take,
  // whatever type the path names.
  quote!({
    let id: #root::layout::Id = #root::layout::Id::new(#root::__site!(#ident, #n), &[]);
    id
  })
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

/// A region of `#[pun(regions(..))]`: `name`, `name: Type`, `name = EXPR` or
/// `name: Type = EXPR`.
struct Region {
  name: Ident,
  ty: Option<Type>,
  size: Option<Expr>,
}

impl Parse for Region {
  fn parse(input: ParseStream) -> Result<Self> {
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
    Ok(Self { name, ty, size })
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
      } else {
        return Err(meta.error("unknown `pun` option"));
      }
      Ok(())
    })?;
  }
  Ok(options)
}

/// Every option of `#[pun(..)]`.
const ALL_OPTIONS: [&str; 9] =
  ["crate", "bound", "size", "len", "assert", "tag", "get", "valid", "regions"];

// -------------------------------------------------------------------------------------------------
// Expressions

/// A field of a struct, as the derive compiles it.
struct FieldInfo<'a> {
  member: Member,
  /// Its type, as written: what the bounds name, and what the derive reads of it.
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
/// Why the size of `le` or `be` is refused, as `layout::check_ops` says it of a constant.
const READ_SIZE: &str = "`le` and `be` read 1 to 16 bytes, an unsigned integer of at most 128 bits";

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
    Some("Lit" | "load" | "Size" | "Get") => (0, 1),
    Some("At" | "Varint" | "Not" | "Walk" | "Shortcut" | "Then") => (1, 1),
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
    return Err(Error::new(block.span(), "a block in a rule must end with its value"));
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

/// Returns the number of operations in a branch of an expression that may not be computed, as an
/// operation holds it (`layout::Op::Shortcut`).
///
/// A number that does not fit a `u16` is `u16::MAX`: the expression then has more than
/// [`MAX_OPS`] operations, which [`limits`] reports with the numbers before any operation is used.
fn part(len: usize) -> u16 {
  u16::try_from(len).unwrap_or(u16::MAX)
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
      let ty = compile(&cast.expr, fields, root, ops, scope)?;
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
        return Err(Error::new(branch.span(), "an `if` in a rule needs an `else`"));
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
      let (first, second) = (part(middle - start + 1), part(ops.len() - middle));
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
    Expr::Field(field) if matches!(&*field.base, Expr::Path(path) if path.path.is_ident("self")) => {
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
        let (either, len) = (matches!(binary.op, BinOp::Or(_)), part(ops.len() - start + 1));
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
        && matches!(&call.args[0], Expr::Path(path) if path.path.is_ident("self")) =>
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
    // `le(offset, size)`, `be(offset, size)`: an integer, anywhere in the value.
    Expr::Call(call) if call.args.len() == 2 && (is_call(call, "le") || is_call(call, "be")) => {
      int(&call.args[0], ops, scope)?;
      let (size, big) = (&call.args[1], is_call(call, "be"));
      // A constant is checked with the layout (`layout::check_ops`), a literal here.
      if let Expr::Lit(ExprLit { lit: Lit::Int(int), .. }) = size
        && !matches!(int.base10_parse::<u8>(), Ok(1..=16))
      {
        return Err(Error::new_spanned(size, READ_SIZE));
      }
      ops.push(quote!(#op::At { size: #size, big: #big }));
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
      Expr::Field(access) if matches!(&*access.base, Expr::Path(path) if path.path.is_ident("self")) => {
        self.value(&access.member, access.span())
      }
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
  let pun = |bounds: &Punctuated<syn::TypeParamBound, Token![+]>| {
    bounds.iter().any(|bound| {
      matches!(bound, syn::TypeParamBound::Trait(bound)
        if bound.path.segments.last().is_some_and(|segment| segment.ident == "Pun"))
    })
  };
  let name = ty.to_token_stream().to_string();
  // `<T: Pun>`
  if generics.type_params().any(|param| param.ident == name && pun(&param.bounds)) {
    return;
  }
  // `where T: Pun`
  let clause = generics.make_where_clause();
  let written = clause.predicates.iter().any(|predicate| {
    matches!(predicate, syn::WherePredicate::Type(predicate)
      if predicate.bounded_ty.to_token_stream().to_string() == name && pun(&predicate.bounds))
  });
  if !written {
    clause.predicates.push(parse_quote!(#ty: #root::Pun));
  }
}

/// Returns the generics of `input`, with the bounds of its implementation of `Pun`: `bounds` if
/// `#[pun(bound(..))]` gives them, and otherwise `ty: Pun` for each of the `types` of its fields
/// that needs it ([`needs_bound`]).
fn bounded<'a>(
  input: &DeriveInput,
  types: impl Iterator<Item = &'a Type>,
  bounds: Option<&Bounds>,
  root: &TokenStream,
) -> syn::Generics {
  let mut generics = input.generics.clone();
  match bounds {
    Some(bounds) => generics.make_where_clause().predicates.extend(bounds.iter().cloned()),
    None => types.filter(|ty| needs_bound(ty, input)).for_each(|ty| bound(&mut generics, ty, root)),
  }
  generics
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
/// The derive reads the tokens of the item, which can name other types than the item that `rustc`
/// compiles: the reference of `puns`, chapter What the derive trusts, says how ("The generated code"
/// and "Attribute macros below the derive"). So the type of each field is written once, as the
/// associated type `name` of `__PunTypes`, in an implementation for the type, where `Self` is the
/// type as in its declaration. Every other place names it through [`home`], so they all name one
/// type.
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
  let (impl_generics, ty_generics, _) = input.generics.split_for_impl();
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
  let mut fields = fields
    .into_iter()
    .flatten()
    .enumerate()
    .map(|(i, field)| {
      Ok(FieldInfo {
        member: field
          .ident
          .as_ref()
          .map_or_else(|| Member::Unnamed(Index::from(i)), |ident| Member::Named(ident.clone())),
        ty: &field.ty,
        home: home(&this, &format_ident!("F{}", i), &field.ty),
        vis: &field.vis,
        options: parse_options(&field.attrs, "a field", &["valid", "len", "regions"])?,
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
  let types = fields.iter().map(|field| field.ty);
  let mut generics = bounded(input, types, options.bounds.as_ref(), &root);
  // The types of the typed regions.
  for field in &fields {
    for ty in field.options.regions.iter().flatten().filter_map(|region| region.ty.as_ref()) {
      bound(&mut generics, ty, &root);
    }
  }
  if sized {
    generics.make_where_clause().predicates.push(parse_quote!(#this: ::core::marker::Sized));
  }
  let where_clause = &generics.where_clause;

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
    let mut sized_memory = input.generics.clone();
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
  // The rules that have an expression: their index, and their operations as a type.
  let mut listed = vec![];
  if let Some(size) = &options.size {
    let Expression { ops, steps } = expression(size, Ty::Int, true, &fields, &root)?;
    listed.push((rules.len(), steps));
    rules.push(quote!(#rule::Size(#ops)));
  }
  for assert in &options.asserts {
    let Expression { ops, steps } = expression(assert, Ty::Bool, false, &fields, &root)?;
    listed.push((rules.len(), steps));
    rules.push(quote!(#rule::Assert(#ops)));
  }
  let mut accessors = vec![];
  for (i, field) in fields.iter().enumerate() {
    let last = i + 1 == count;
    if let Some(len) = &field.options.len {
      if !last {
        return Err(Error::new(len.span(), "`len` is an option of the last field"));
      }
      let Expression { ops, steps } = expression(len, Ty::Int, true, &fields, &root)?;
      listed.push((rules.len(), steps));
      rules.push(quote!(#rule::Len(#ops)));
    }
    if let Some(regions) = &field.options.regions {
      if !last {
        return Err(Error::new(field.member.span(), "`regions` is an option of the last field"));
      }
      let vis = field.vis;
      for (index, region) in regions.iter().enumerate() {
        let name = &region.name;
        rules.push(match &region.size {
          Some(size) => {
            let Expression { ops, steps } = expression(size, Ty::Int, false, &fields, &root)?;
            listed.push((rules.len(), steps));
            quote!(#rule::Region(::core::option::Option::Some(#ops)))
          }
          None if index + 1 == regions.len() => quote!(#rule::Region(::core::option::Option::None)),
          None => return Err(Error::new(name.span(), "only the last region can omit its size")),
        });
        accessors.push(region.ty.as_ref().map_or_else(
          || {
            let doc = format!(
              "Returns the bytes of the `{name}` region of the tail, truncated if the fields that \
               size the regions give more bytes than the tail holds. The region takes the rest of \
               the tail if its expression fails.\n\n# Compile-time errors\n\n\
               A call compiles only if every byte of the struct, up to the end of its tail, is \
               initialized and, without the `unstable` feature of `puns`, this does not depend on \
               a layout that Rust leaves to the compiler.\n\n# Examples\n\nSee [Bit fields and \
               regions]({DOCS}reference/formats/index.html#bit-fields-and-regions)."
            );
            quote! {
              #[doc = #doc]
              #[inline]
              #vis const fn #name(&self) -> &[::core::primitive::u8] {
                #root::region(self, #index)
              }
            }
          },
          |ty| {
            let doc = format!(
              "Returns the `{name}` region of the tail as the type it is said to hold, read with \
               `puns::from_bytes`. The region is truncated if the fields that size the regions \
               give more bytes than the tail holds, and takes the rest of the tail if its \
               expression fails.\n\n# Errors\n\nReturns the error of \
               `puns::from_bytes` if the bytes of the region are not a value of that type.\n\n\
               # Compile-time errors\n\nA call compiles only if every byte of the struct, up \
               to the end of its tail, is initialized and, without the `unstable` feature of \
               `puns`, this does not depend on a layout that Rust leaves to the compiler. It also \
               compiles only under the compile-time conditions of `puns::from_bytes` for the type \
               of the region.\n\n# Examples\n\nSee [A region with a \
               type]({DOCS}reference/formats/index.html#a-region-with-a-type)."
            );
            quote! {
              #[doc = #doc]
              #[inline]
              #vis const fn #name(&self) -> ::core::result::Result<&#ty, #root::Error> {
                #root::from_bytes::<#ty>(#root::region(self, #index))
              }
            }
          },
        ));
      }
    }
  }

  // Getters.
  for getter in &options.getters {
    let (name, ty, vis) = (&getter.name, &getter.ty, &input.vis);
    let expr = rust(&getter.expr, &fields, &root, repr.packed.is_some());
    let doc = format!(
      "Returns `{}`, computed from the fields of the value, as a `{}`.\n\n# Panics\n\nPanics \
       where the expression, evaluated as Rust code, panics, such as on a division by zero.\n\n\
       # Compile-time errors\n\nA call compiles only if each field that the expression names is \
       an unsigned integer, or a sized struct whose only field with a size is one and is as large \
       as the struct.\n\n# Examples\n\nSee [Bit fields and \
       regions]({DOCS}reference/formats/index.html#bit-fields-and-regions).",
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
      // is `#ty`. `Next` is the field declared after it, or `()` after the last, and `Last` is the
      // last. So the list from the first marker is the fields of the layout, in the order of
      // their declarations, as the `# Safety` of `Fields` requires.
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

  // The list of the rules that have an expression, at the type level, with their operations.
  let rule_list = listed
    .iter()
    .rev()
    .fold(quote!(()), |next, (index, steps)| quote!(#root::RuleAt<#index, #steps, #next>));

  let accessors = (!accessors.is_empty()).then(|| {
    quote! {
      impl #impl_generics #this #where_clause {
        #(#accessors)*
      }
    }
  });
  // The `Rust` representation: what every order of the fields has (`Layout::rust`).
  let rust = (!(repr.c || repr.transparent)).then(|| {
    let pack = repr.packed.clone().unwrap_or_else(|| quote!(::core::primitive::usize::MAX));
    quote!(.rust(#pack))
  });
  let id = identity(input, &root);
  // A `repr(transparent)` struct around a type that `core` lays out in `Option` without a tag has
  // that guarantee too ("Representation" of `core::option`). Its other fields have size 0 and
  // alignment 1, which no such type has.
  let null_optimized = repr.transparent.then(|| {
    let types = fields.iter().map(|field| &field.home);
    quote!(const NULL_OPTIMIZED: bool = false #(|| <#types as #root::Pun>::NULL_OPTIMIZED)*;)
  });

  // The home of the type of each field, and what ties the fields to the struct.
  let named = fields
    .iter()
    .enumerate()
    .map(|(i, field)| (format_ident!("F{}", i), field.ty))
    .collect::<Vec<_>>();
  let members = fields.iter().map(|field| &field.member).collect::<Vec<_>>();
  let same = fields.iter().map(|field| {
    let member = &field.member;
    same_type(&quote!((*p).#member), &field.home, field.ty)
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
      //   struct is valid if, and only if, each of its fields is valid, and a byte that no field
      //   covers is padding. The Rust Reference states the "only if" [1]. The derive relies on the
      //   "if", which no documentation states (`docs/rust.md`, "A value whose fields are valid").
      //   Rules are requirements of `puns`, never of the language.
      // - `Fields` lists the fields in order (see the markers above). `Rules` lists the rules that
      //   have an expression, which `validate::check_rules` asserts against the layout. The other
      //   lists are `()`. `NULL_OPTIMIZED` changes only what the analysis takes to be the same in
      //   every layout that Rust allows.
      // - Each field is `Pun`, so no byte is inside an `UnsafeCell`.
      // The functions called are those of the types that `Layout::PIN` fixes, which no path can
      // replace. A struct whose private fields keep an invariant must not derive `Pun`
      // (`docs/soundness.md`, "What deriving `Pun` means"): the layout describes only what the
      // language requires of its fields.
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
        const PROBE: *const Self = #probe;
        // SAFETY: `Layout::structure` requires nothing of a `Sized` struct. An unsized struct has
        // an unsized last field, whose `Pun` type is a slice, `str`, or a struct whose last field
        // is one of these. Its probe is the probe of that field, `<[E] as Pun>::PROBE` cast with
        // `as` casts, cast again to `*const Self`. The size of the struct with 8 elements in its
        // tail must fit in an `isize`, which is not established. The derive relies on `rustc` to
        // report an error in const evaluation where it does not fit. No documentation promises this
        // behavior, which the reference discloses (`docs/rust.md`, "Errors of const evaluation").
        const LAYOUT: #root::layout::Layout = unsafe {
          #root::layout::Layout::structure::<Self>(
            Self::PROBE,
            const { &#root::layout::Layout::sort_fields([#(#layouts),*]) },
            const { &[#(#rules),*] },
          )
        }
        #rust
        .identified(#id);
        #null_optimized
      }

      #accessors

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
  let id = identity(input, &root);
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
        const PROBE: *const Self = ::core::ptr::null();
        const LAYOUT: #root::layout::Layout =
          #root::layout::Layout::uninhabited::<Self>().identified(#id);
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
    // The error is at the first reference without the named lifetime of the first one.
    let other = references
      .iter()
      .find(|(_, reference)| lifetime.is_none() || &reference.lifetime != lifetime);
    if let Some((_, reference)) = other {
      return Err(Error::new_spanned(
        reference,
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
    let examples = format!(
      "\n# Examples\n\nSee the view `Message` of [the examples]({DOCS}derive.Pun.html#examples)."
    );
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
        #[doc = #examples]
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
        #[doc = #examples]
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
  let mut variants = vec![];
  for variant in &data.variants {
    let variant = &variant.ident;
    // SAFETY (of the generated code): `bits_of` requires every byte of the value to be
    // initialized, which is not established: `align` may add padding after the discriminant, and
    // with the `Rust` representation, Rust does not say how the discriminant is stored. The call
    // is evaluated in the constant `SPEC`, where `rustc` reports an error instead of reading an
    // uninitialized byte. No documentation promises this behavior, which the reference discloses
    // (`docs/rust.md`, "Errors of const evaluation").
    variants.push(quote!(unsafe { #root::layout::Spec::bits_of(Self::#variant) }));
  }
  let count = variants.len();
  let repr = parse_repr(&input.attrs)?;
  // With the `Rust` representation, Rust does not specify how a variant is stored, nor its size.
  let specified = repr.c || repr.transparent || repr.int.is_some();
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
    // - Each value that `SPEC` accepts is the bytes of a variant, so a valid `Self`: the derive
    //   relies on a valid discriminant being all that such an enum requires, which no documentation
    //   states (`docs/rust.md`, "A value whose fields are valid"). The derive is told of no other
    //   invariant.
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
/// are those of `repr(C)` structs, measured on the sample of the variant, or, for a variant without
/// a sample, computed from the sizes and the alignments of the fields (`Layout::repr_c`). `bounds`
/// are those of `#[pun(bound(..))]`, if any.
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
  let id = identity(input, root);
  let (ident, vis) = (&input.ident, &input.vis);
  let (impl_generics, ty_generics, _) = input.generics.split_for_impl();
  let this = quote!(#ident #ty_generics);
  let types = data.variants.iter().flat_map(|variant| &variant.fields).map(|field| &field.ty);
  let generics = bounded(input, types, bounds, root);
  let where_clause = &generics.where_clause;

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
          if #layout::sampled_all(#layouts) {
            #option::Some(#layout::wrap(Self::#name {
              // SAFETY (of the generated code): Each field type has a sample, which is checked
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
          // discriminant (`docs/rust.md`, "What Rust documents"), so every byte of it is
          // initialized. Each sample is a value of a variant of the enum, which starts with its
          // tag, the enum with all fields removed [1] [2]. With an integer, the tag is that
          // integer, as `#tag` is. With `repr(C)` alone, the discriminants are implicit [3] [4], so
          // the tag and `#tag` are the same enum without fields. Either way, the first
          // `size_of::<#tag>()` bytes of a sample are its tag, an integer, so initialized, if the
          // enum as compiled has the representation that the derive read. This is not
          // established: an attribute macro below the derive can change it (`docs/trust.md`,
          // "Attribute macros below the derive"), and with a narrower tag, a byte of padding of
          // the sample may be among these bytes. These are the requirements of `declared_tags`,
          // except that one. The constant is evaluated at compile time, where `rustc` reports an
          // error instead of reading an uninitialized byte. No documentation promises this
          // behavior, which the reference discloses (`docs/rust.md`, "Errors of const evaluation").
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
                // the reference discloses (`docs/rust.md`, "Errors of const evaluation" and "The
                // variant read from the tag"): `rustc` reports an error if the tag is not the tag
                // of a variant, and otherwise reads the variant from the tag alone, without
                // reading the fields.
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
      //   it. An enum is valid if, and only if, its tag is that of a variant and the fields of
      //   that variant are valid, which is what the layout accepts. The Rust Reference states the
      //   "only if" [3]. The derive relies on the "if", which no documentation states
      //   (`docs/rust.md`, "A value whose fields are valid").
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
        .identified(#id);
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
  let id = identity(input, root);
  let (ident, vis) = (&input.ident, &input.vis);
  let (impl_generics, ty_generics, _) = input.generics.split_for_impl();
  let this = quote!(#ident #ty_generics);
  let types = data.variants.iter().flat_map(|variant| &variant.fields).map(|field| &field.ty);
  let generics = bounded(input, types, bounds, root);
  let where_clause = &generics.where_clause;
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
        &#layout::variant::<Self>({
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
      // as compiled, and the enum to have no other variant and no other field. The layout
      // accepts a value whose tag is that of a variant whose fields are valid. The derive relies
      // on such a value being valid, which no documentation states
      // (`docs/rust.md`, "A value whose fields are valid").
      // `Variants` lists the variants in order, each with its fields. The other lists are `()`.
      // Each field is `Pun`, so no byte is in an `UnsafeCell`.
      unsafe impl #impl_generics #root::Pun for #this #where_clause {
        type Elem = ();
        type Fields = ();
        type Variants = #first #ty_generics;
        type Rules = ();
        const PROBE: *const Self = ::core::ptr::null();
        // Rust specifies nothing of it: opaque without the `unstable` feature.
        const LAYOUT: #root::layout::Layout =
          #layout::enumeration::<Self>(const { &<Self as __PunDiscover>::ENUM })
            .unspecified()
            .identified(#id);
      }
    };
  })
}

/// Derives `Pun` for a `repr(C)` union.
///
/// Each field is a variant of one field, at offset 0. A value is valid if it is a valid value of
/// one of them (`Layout::union`).
///
/// # Errors
///
/// Returns an error if the union is not `repr(C)`, if it is packed, or if an option does not
/// parse or does not belong there.
fn derive_union(input: &DeriveInput, data: &syn::DataUnion) -> Result<TokenStream> {
  let repr = parse_repr(&input.attrs)?;
  if !repr.c || repr.packed.is_some() {
    return Err(Error::new(
      input.ident.span(),
      "`Pun` is derived for unions of `repr(C)`, whose fields are all at offset 0",
    ));
  }
  let options = parse_options(&input.attrs, "a union", &["crate", "bound"])?;
  let root = options.root.clone().unwrap_or_else(|| quote!(::puns));
  let (ident, vis) = (&input.ident, &input.vis);
  let (impl_generics, ty_generics, _) = input.generics.split_for_impl();
  let id = identity(input, &root);
  let this = quote!(#ident #ty_generics);
  let types = data.fields.named.iter().map(|field| &field.ty);
  let generics = bounded(input, types, options.bounds.as_ref(), &root);
  let where_clause = &generics.where_clause;
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
      // puts it, with the layout of its type or a restriction of it. The derive trusts that the
      // `repr` that it read is that of the union as compiled ("What the derive trusts"). The type
      // of each field is its home in `__PunTypes`, which `item` requires to be the type of the
      // field of that name in the union as compiled. A field that the derive did not read only
      // adds values, which the layout does not accept as a target, and guarantees none.
      // `Layout::union` takes the size and the alignment from the compiler. As a target, the
      // layout accepts bytes that are a valid value of one field. Such bytes are a value that safe
      // code makes by writing that field, which is valid [2]. As a source, the layout guarantees
      // nothing, not even that a byte is initialized, which every value holds. `Variants` lists the
      // fields in order. The other lists are `()`. Each field is `Pun`, so no byte is in an
      // `UnsafeCell`.
      //
      // [1] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.c.union.size-align:
      //
      //     Each field lives at byte offset 0 from the beginning of the union.
      //
      // [2] Per https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.union:
      //
      //     Obviously, all values that can be created entirely in safe code are valid.
      unsafe impl #impl_generics #root::Pun for #this #where_clause {
        type Elem = ();
        type Fields = ();
        type Variants = #first;
        type Rules = ();
        const PROBE: *const Self = ::core::ptr::null();
        const LAYOUT: #root::layout::Layout = #root::layout::Layout::union::<Self>(
          const { &#root::layout::Enum::union(const { &[#(#variants),*] }) },
        )
        .identified(#id);
      }
    };
  })
}
