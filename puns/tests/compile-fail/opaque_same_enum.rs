//! An enum that holds an opaque scalar is not proven by an enum of the same layout,
//! not even by itself: as a struct, it relates field by field, and nothing proves the scalar. An
//! `Option` of the scalar is one of the same layout, so `None` would be read as a zero `Off`.
#[path = "../support/opaque.rs"]
#[allow(dead_code)]
mod opaque;

#[derive(puns::Pun, Clone, Copy)]
#[repr(u32)]
enum Nullable {
  Next(Option<opaque::Off>) = 1,
}

#[derive(puns::Pun, Clone, Copy)]
#[repr(u32)]
enum Linked {
  Next(opaque::Off) = 1,
}

fn main() {
  let _: Linked = puns::pun(Nullable::Next(None));
  let _: &Nullable = puns::pun_ref(&Nullable::Next(None));
}
