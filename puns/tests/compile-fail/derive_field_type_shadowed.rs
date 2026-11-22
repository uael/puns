//! A macro in the type of a field expands where the derive writes it, in its generated code, where
//! the names `__Pun..` are its own items. Here the field is a `bool` in the item, and `__PunField0`
//! in the generated code is the marker of the field, whose `Out` is `u8`: the layout of a `u8`
//! would have made a `bool` of 2. The type that the derive writes is compared with the field of
//! the item as compiled, and the derive does not compile.
use puns::Pun;

trait Pick {
  type Out;
}
impl<T> Pick for T {
  type Out = u8;
}
impl Pick for str {
  type Out = bool;
}

#[allow(non_camel_case_types)]
type __PunField0 = str;

macro_rules! out {
  () => {
    <__PunField0 as Pick>::Out
  };
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct Flag {
  on: out!(),
}

fn main() {
  let _ = puns::pun::<u8, Flag>(2);
}
