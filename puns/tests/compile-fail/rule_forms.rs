//! What a rule does not compute is refused where it is written, with what it computes instead.

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(if a == 1 { true }))]
struct IfWithoutElse {
  a: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(self.b == 1))]
struct NoSuchField {
  a: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(a += 1))]
struct CompoundAssignment {
  a: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(walk(0, |at, step| at < 2, |at| at + 1) == 2))]
struct WalkOfTwoParameters {
  a: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(crc(0, 1) == 0))]
struct CrcWithoutParameters {
  a: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(crc::<8, 7, 0, false, 0>(0) == 0))]
struct CrcOfOneArgument {
  a: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(a == "1"))]
struct StringLiteral {
  a: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(self == 1))]
struct SelfAlone {
  a: u8,
}

#[derive(puns::Pun)]
#[repr(C)]
#[pun(assert(match a { 1 => true, _ => false }))]
struct Match {
  a: u8,
}

fn main() {}
