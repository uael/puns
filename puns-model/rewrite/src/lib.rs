//! Attributes that rewrite the item they are on, for the tests of `puns` about the item that
//! `rustc` compiles: the compile-fail case `derive_item_rewritten`, and the offsets measured in
//! `tests/soundness.rs`.
//!
//! Written below `#[derive(Pun)]`, such an attribute is expanded after the derive has read the
//! item, so the item that `rustc` compiles is not the one that the derive read. The derive must
//! describe the type as compiled, or fail to compile.

use proc_macro::{Delimiter, Group, Ident, Punct, Spacing, TokenStream, TokenTree};

/// Swaps the types `bool` and `u8` wherever the item names them.
#[proc_macro_attribute]
pub fn swap(_attr: TokenStream, item: TokenStream) -> TokenStream {
  swapped(item)
}

/// Returns `tokens`, with the identifiers `bool` and `u8` swapped.
fn swapped(tokens: TokenStream) -> TokenStream {
  tokens
    .into_iter()
    .map(|token| match token {
      TokenTree::Ident(ident) if ident.to_string() == "bool" => {
        Ident::new("u8", ident.span()).into()
      }
      TokenTree::Ident(ident) if ident.to_string() == "u8" => {
        Ident::new("bool", ident.span()).into()
      }
      TokenTree::Group(group) => {
        let mut swapped = Group::new(group.delimiter(), swapped(group.stream()));
        swapped.set_span(group.span());
        swapped.into()
      }
      token => token,
    })
    .collect()
}

/// Reverses the order of the named fields of a struct, or of each variant of an enum.
#[proc_macro_attribute]
pub fn reverse(_attr: TokenStream, item: TokenStream) -> TokenStream {
  let mut is_enum = false;
  item
    .into_iter()
    .map(|token| match token {
      TokenTree::Ident(ident) => {
        is_enum |= ident.to_string() == "enum";
        ident.into()
      }
      TokenTree::Group(group) if group.delimiter() == Delimiter::Brace && is_enum => {
        // The variants: each one with named fields has them reversed.
        let variants = group
          .stream()
          .into_iter()
          .map(|token| match token {
            TokenTree::Group(fields) if fields.delimiter() == Delimiter::Brace => {
              reversed(&fields).into()
            }
            token => token,
          })
          .collect();
        let mut variants = Group::new(Delimiter::Brace, variants);
        variants.set_span(group.span());
        variants.into()
      }
      TokenTree::Group(group) if group.delimiter() == Delimiter::Brace => reversed(&group).into(),
      token => token,
    })
    .collect()
}

/// Returns `group`, the fields of a struct or of a variant, with the fields in reverse order: each
/// is the tokens up to a comma outside of a group.
fn reversed(group: &Group) -> Group {
  let mut fields: Vec<Vec<TokenTree>> = vec![vec![]];
  for token in group.stream() {
    match &token {
      TokenTree::Punct(punct) if punct.as_char() == ',' => fields.push(vec![]),
      _ => fields.last_mut().expect("one field at least").push(token),
    }
  }
  fields.retain(|field| !field.is_empty());
  let mut tokens = Vec::new();
  for field in fields.into_iter().rev() {
    tokens.extend(field);
    tokens.push(Punct::new(',', Spacing::Alone).into());
  }
  let mut reversed = Group::new(group.delimiter(), tokens.into_iter().collect());
  reversed.set_span(group.span());
  reversed
}

/// Replaces the `repr` attributes of the item with `repr(..)` of its arguments, or removes them if
/// it has none: `#[rewrite::repr(C, packed)]`, `#[rewrite::repr()]`.
#[proc_macro_attribute]
pub fn repr(attr: TokenStream, item: TokenStream) -> TokenStream {
  let tokens = item.into_iter().collect::<Vec<_>>();
  let mut kept = Vec::new();
  let mut i = 0;
  while i < tokens.len() {
    let is_repr = matches!(
      (&tokens[i], tokens.get(i + 1)),
      (TokenTree::Punct(punct), Some(TokenTree::Group(group)))
        if punct.as_char() == '#'
          && group.delimiter() == Delimiter::Bracket
          && matches!(group.stream().into_iter().next(),
            Some(TokenTree::Ident(ident)) if ident.to_string() == "repr")
    );
    if is_repr {
      i += 2;
    } else {
      kept.push(tokens[i].clone());
      i += 1;
    }
  }
  let mut out = TokenStream::new();
  if !attr.is_empty() {
    let repr = [
      TokenTree::from(Ident::new("repr", proc_macro::Span::call_site())),
      Group::new(Delimiter::Parenthesis, attr).into(),
    ];
    out.extend([
      TokenTree::from(Punct::new('#', Spacing::Alone)),
      Group::new(Delimiter::Bracket, repr.into_iter().collect()).into(),
    ]);
  }
  out.extend(kept);
  out
}
