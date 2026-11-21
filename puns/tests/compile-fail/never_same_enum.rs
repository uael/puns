//! An enum that holds a scalar that a pun never makes is not proven by an enum of the same layout,
//! not even by itself: as a struct, it relates field by field, and nothing proves the scalar. An
//! `Option` of the scalar is one of the same layout, so `None` would be read as a zero `Off`.
#[path = "../support/never.rs"]
#[allow(dead_code)]
mod never;

#[derive(puns::Pun, Clone, Copy)]
#[repr(u32)]
enum Nullable {
  Next(Option<never::Off>) = 1,
}

#[derive(puns::Pun, Clone, Copy)]
#[repr(u32)]
enum Linked {
  Next(never::Off) = 1,
}

fn main() {
  let _: Linked = puns::pun(Nullable::Next(None));
  let _: &Nullable = puns::pun_ref(&Nullable::Next(None));
}
