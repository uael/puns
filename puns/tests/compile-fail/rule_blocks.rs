//! A block of a rule is `let name = EXPR;` statements, then its value, as in Rust: what is not is
//! refused where it is.

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert({}))]
struct Empty {
  a: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert({ a; a == 1 }))]
struct Statement {
  a: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert({ let b; a == 1 }))]
struct Uninitialized {
  a: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert({ let (b, c) = (a, a); b == c }))]
struct Pattern {
  a: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert({ let b = a == 1; }))]
struct NoValue {
  a: u8,
}

fn main() {}
