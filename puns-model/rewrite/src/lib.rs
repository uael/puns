//! An attribute that rewrites the item it is on, for the compile-fail case
//! `puns/tests/compile-fail/derive_item_rewritten.rs`.
//!
//! Written below `#[derive(Pun)]`, it is expanded after the derive has read the item, so the item
//! that `rustc` compiles is not the one that the derive read. The derive must not describe a type
//! that it did not read: the generated code must fail to compile.

use proc_macro::{Group, Ident, TokenStream, TokenTree};

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
