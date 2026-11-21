use core::num::NonZero;

use crate::{
  Plan, Pun, Scalar,
  analysis::{Exact, analyze, analyze_tail, mutable},
  layout::{Endian, Spec},
};

/// What the analysis says of a pun from `S` to `D`.
fn verdict<S: ?Sized + Pun, D: ?Sized + Pun>() -> String {
  let a = const { analyze(&S::LAYOUT, &D::LAYOUT) };
  let fit = match a.fit.exact {
    Exact::Always => String::new(),
    Exact::Sometimes => " (some lengths)".into(),
    Exact::Never(why) if a.fit.shrink => format!(" (truncated: {why})"),
    Exact::Never(why) => return format!("never: {why}"),
  };
  let relation = match () {
    () if a.relation.unspecified.is_some() => {
      return format!("unspecified at {}", a.relation.unspecified.unwrap());
    }
    () if a.relation.uninit.is_some() => {
      return format!("uninit at {}", a.relation.uninit.unwrap());
    }
    () if a.relation.disjoint.is_some() => {
      return format!("unreachable at {}", a.relation.disjoint.unwrap());
    }
    () if a.relation.infallible() => "infallible".to_string(),
    () => format!("fallible at {}", a.relation.partial.unwrap()),
  };
  let mutable = if const { mutable(&S::LAYOUT, &D::LAYOUT, &analyze(&S::LAYOUT, &D::LAYOUT).fit) } {
    ", mutable"
  } else {
    ""
  };
  format!("{relation}{mutable}{fit}")
}

macro_rules! scalar {
  ($name:ident($repr:ty) = $endian:ident $ranges:expr) => {
    #[derive(Copy, Clone)]
    #[repr(transparent)]
    struct $name($repr);
    // SAFETY: these types are only given to the analysis, never constructed: there is no value
    // of them that `SPEC` could misdescribe.
    unsafe impl Scalar for $name {
      const SPEC: Spec = Spec::ranges::<$repr>(Endian::$endian, &$ranges);
    }
  };
}

scalar!(Ascii(u8) = Little[(0, 0x7f)]);
scalar!(NzLe(u16) = Little[(1, 0xffff)]);
scalar!(NzBe(u16) = Big[(1, 0xffff)]);
scalar!(Le1234(u16) = Little[(0x1234, 0x1234)]);
scalar!(Be3412(u16) = Big[(0x3412, 0x3412)]);
scalar!(Be1234(u16) = Big[(0x1234, 0x1234)]);
scalar!(GeLe(u32) = Little[(0x100, 0xffff_ffff)]);
scalar!(GeBe(u32) = Big[(0x100, 0xffff_ffff)]);
scalar!(Enum3(u32) = Little [(0x0101_0101, 0x0101_0101), (0x0202_0202, 0x0202_0202), (0x0303_0303, 0x0303_0303)]);

#[derive(Pun)]
#[repr(C)]
#[pun(crate)]
struct Pair<A, B: ?Sized>(A, B);

#[derive(Pun)]
#[repr(C, packed)]
#[pun(crate)]
struct Packed<A, B>(A, B);

#[test]
fn scalars() {
  assert_eq!(verdict::<u8, u8>(), "infallible, mutable");
  assert_eq!(verdict::<bool, u8>(), "infallible");
  assert_eq!(verdict::<u8, bool>(), "fallible at 0, mutable");
  assert_eq!(verdict::<u32, char>(), "fallible at 0, mutable");
  assert_eq!(verdict::<char, u32>(), "infallible");
  assert_eq!(verdict::<bool, NonZero<u8>>(), "fallible at 0");
  assert_eq!(verdict::<Ascii, bool>(), "fallible at 0, mutable");
  assert_eq!(verdict::<u16, u8>(), "infallible (truncated: the source is larger than the target)");
  assert_eq!(verdict::<u8, u16>(), "never: the source is smaller than the target");
  assert_eq!(verdict::<Enum3, NonZero<u32>>(), "infallible");
  assert_eq!(verdict::<NonZero<u32>, Enum3>(), "fallible at 0, mutable");
}

/// A struct with a rule, `a < b`.
#[derive(Pun)]
#[repr(C)]
#[pun(crate, assert(a < b))]
struct Ruled {
  a: u8,
  b: u8,
}

/// The rule of a field holds of the bytes where the field is: in a tuple, which Rust may order
/// otherwise, it is not of the same bytes in every layout.
#[test]
fn a_rule_of_a_field_is_where_the_field_is() {
  assert!(!<(Ruled, [u8; 2])>::LAYOUT.is_specified());
  assert!(<(Ruled, Ruled)>::LAYOUT.is_specified());
  if !crate::layout::UNSTABLE {
    assert_eq!(verdict::<[u8; 4], (Ruled, [u8; 2])>(), "unspecified at 0");
  }
}

#[test]
fn padding() {
  // `Pair<u8, u16>` has a padding byte at 1.
  assert_eq!(verdict::<Pair<u8, u16>, [u8; 4]>(), "uninit at 1");
  assert_eq!(verdict::<[u8; 4], Pair<u8, u16>>(), "infallible");
  assert_eq!(verdict::<Pair<u8, u16>, Pair<bool, u16>>(), "fallible at 0, mutable");
  assert_eq!(verdict::<Pair<bool, u16>, Pair<u8, u16>>(), "infallible");
  assert_eq!(verdict::<Pair<u8, u16>, Pair<u8, i16>>(), "infallible, mutable");
  // A target without padding cannot read the padding of the source.
  assert_eq!(verdict::<Pair<u8, u16>, Pair<u8, [u8; 2]>>(), "uninit at 1");
  assert_eq!(verdict::<Pair<u8, u16>, Packed<u8, Packed<u8, u16>>>(), "uninit at 1");
  // ... and a `&mut` to a target with padding could de-initialize bytes of the source.
  assert_eq!(verdict::<[u8; 4], Pair<u8, u16>>(), "infallible");
}

#[test]
fn misaligned_and_mixed_endian() {
  // Whatever the byte order, a non-zero `u16` is a non-zero `u16`.
  assert_eq!(verdict::<NzLe, NzBe>(), "infallible, mutable");
  assert_eq!(verdict::<Le1234, Be3412>(), "infallible, mutable");
  assert_eq!(verdict::<Le1234, NzBe>(), "infallible");
  // ... but being at least 0x100 depends on it.
  assert_eq!(verdict::<GeLe, GeBe>(), "fallible at 0");
  assert_eq!(verdict::<GeLe, GeLe>(), "infallible, mutable");
  // The first byte of a little-endian `u32 >= 0x100` can be anything, the others cannot be all 0.
  assert_eq!(verdict::<GeLe, Pair<u8, [u8; 3]>>(), "infallible");
  assert_eq!(verdict::<GeLe, [bool; 4]>(), "fallible at 0");
  assert_eq!(verdict::<[Le1234; 2], [u8; 4]>(), "infallible");
  // In memory: `34 12 34 12`.
  assert_eq!(verdict::<[Le1234; 2], Packed<u8, Packed<Be1234, u8>>>(), "infallible");
  assert_eq!(verdict::<[Le1234; 2], Packed<u8, Packed<Le1234, u8>>>(), "unreachable at 1");
  assert_eq!(verdict::<[u8; 2], NzLe>(), "fallible at 0, mutable");
  assert_eq!(verdict::<Pair<u8, NonZero<u8>>, NzLe>(), "infallible");
  assert_eq!(verdict::<Pair<NonZero<u8>, u8>, NzBe>(), "infallible");
}

#[test]
fn huge_arrays_cost_nothing() {
  // As large as the target allows (the largest array is `15 * N` bytes).
  const N: usize = if usize::BITS >= 64 { 1 << 40 } else { 1 << 24 };
  assert_eq!(verdict::<[u8; N], [u8; N]>(), "infallible, mutable");
  assert_eq!(verdict::<[bool; N], [u8; N]>(), "infallible");
  assert_eq!(verdict::<[u8; N], [bool; N]>(), "fallible at 0, mutable");
  assert_eq!(verdict::<[[bool; 4]; N], [u32; N]>(), "infallible");
  assert_eq!(verdict::<[Pair<u8, u16>; N], [u32; N]>(), "uninit at 1");
  assert_eq!(verdict::<[Pair<bool, [u8; 3]>; N], [Pair<u8, [bool; 3]>; N]>(), "fallible at 1");
  assert_eq!(
    verdict::<[[u8; 3]; N], [[bool; 5]; N]>(),
    "never: the source is smaller than the target"
  );
  assert_eq!(verdict::<[[Ascii; 5]; 3 * N], [[bool; 3]; 5 * N]>(), "fallible at 0, mutable");
  // Out of phase: 2 bytes elements against 2 bytes elements, one byte later.
  assert_eq!(verdict::<[NzLe; N], Packed<u8, Packed<[NzLe; N - 1], u8>>>(), "fallible at 1");
  assert_eq!(verdict::<[Le1234; N], Packed<u8, Packed<[Be1234; N - 1], u8>>>(), "infallible");
}

#[test]
fn unsized_types() {
  assert_eq!(verdict::<[u8], [u8]>(), "infallible, mutable");
  assert_eq!(verdict::<[bool], [u8]>(), "infallible");
  assert_eq!(verdict::<[u8], [bool]>(), "fallible at 0, mutable");
  assert_eq!(verdict::<[u32], [u8]>(), "infallible, mutable");
  assert_eq!(verdict::<[u8], [u32]>(), "infallible, mutable (some lengths)");
  assert_eq!(verdict::<[[u8; 3]], [[u8; 2]]>(), "infallible, mutable (some lengths)");
  assert_eq!(verdict::<[u8; 8], [u16]>(), "infallible, mutable");
  assert_eq!(
    verdict::<[u8; 7], [u16]>(),
    "infallible (truncated: the size of the source cannot be reached with elements of the tail slice of the target)"
  );
  assert_eq!(verdict::<[u8], u32>(), "infallible, mutable (some lengths)");
  assert_eq!(verdict::<Pair<u16, [u16]>, [u8]>(), "infallible, mutable");
  assert_eq!(verdict::<Pair<u16, [Pair<u8, bool>]>, Pair<u8, [bool]>>(), "fallible at 1, mutable");
  // A source without any element is too small.
  assert_eq!(
    verdict::<Pair<bool, [Pair<bool, bool>]>, Pair<[bool; 3], [u8]>>(),
    "infallible (some lengths)"
  );
  // `Pair<u32, [u8]>` has trailing padding, but none in what a `[u8]` can see of it.
  assert_eq!(verdict::<Pair<u32, [u8]>, [u8]>(), "infallible, mutable");
  assert_eq!(<Pair<u32, [u8]> as Pun>::LAYOUT.size_of_val(1), Some(8));
  assert_eq!(<Pair<u32, [u8]> as Pun>::LAYOUT.size_of_val(5), Some(12));
}

/// What the analysis says of the last bytes of `S` as a `D`.
fn tail<S: ?Sized + Pun, D: ?Sized + Pun>() -> String {
  let a = const { analyze_tail(&S::LAYOUT, &D::LAYOUT) };
  let shift = a.shift;
  if let (Exact::Never(why), false) = (a.fit.exact, a.fit.shrink) {
    return format!("never: {why}");
  }
  let relation = match () {
    () if a.relation.unspecified.is_some() => {
      format!("unspecified at {}", a.relation.unspecified.unwrap() - shift)
    }
    () if a.relation.uninit.is_some() => {
      format!("uninit at {}", a.relation.uninit.unwrap() - shift)
    }
    () if a.relation.disjoint.is_some() => {
      format!("unreachable at {}", a.relation.disjoint.unwrap() - shift)
    }
    () if a.relation.infallible() => "infallible".to_string(),
    () => format!("fallible at {}", a.relation.partial.unwrap() - shift),
  };
  let aligned = if a.aligned { "" } else { ", misaligned" };
  format!("{relation} from {shift}{aligned}")
}

#[test]
fn tails() {
  assert_eq!(tail::<[u8; 4], [u8; 2]>(), "infallible from 2");
  assert_eq!(tail::<[u8; 4], [u8; 4]>(), "infallible from 0");
  assert_eq!(tail::<[bool; 3], u16>(), "infallible from 1, misaligned");
  assert_eq!(tail::<[u8; 3], [bool; 2]>(), "fallible at 0 from 1");
  assert_eq!(tail::<Pair<bool, u8>, bool>(), "fallible at 0 from 1");
  assert_eq!(tail::<Pair<u8, bool>, bool>(), "infallible from 1");
  // `Pair<u8, u16>` has a padding byte at 1, `Pair<u16, u8>` at 3.
  assert_eq!(tail::<Pair<u8, u16>, u16>(), "infallible from 2");
  assert_eq!(tail::<Pair<u8, u16>, [u8; 3]>(), "uninit at 0 from 1");
  assert_eq!(tail::<Pair<u16, u8>, u8>(), "uninit at 0 from 3");
  assert_eq!(tail::<Pair<u16, u8>, Pair<u16, u8>>(), "infallible from 0");
  // Scalars cut by the shift: the first byte of a little-endian `u32 >= 0x100` can be anything,
  // the others cannot be all 0, but the last two can.
  assert_eq!(tail::<GeLe, [u8; 3]>(), "infallible from 1");
  assert_eq!(tail::<GeLe, Packed<u8, NzLe>>(), "fallible at 1 from 1");
  assert_eq!(tail::<GeLe, Packed<u8, [u8; 2]>>(), "infallible from 1");
  assert_eq!(tail::<Pair<u8, GeBe>, GeBe>(), "infallible from 4");
  assert_eq!(tail::<[Le1234; 2], Packed<Be3412, u8>>(), "unreachable at 0 from 1");
  assert_eq!(tail::<[Le1234; 2], Packed<Be1234, u8>>(), "infallible from 1");
  // Sizes.
  assert_eq!(tail::<u8, u16>(), "never: the source is smaller than the target");
  assert_eq!(
    tail::<[u8], u8>(),
    "never: the source is unsized: where its last bytes start depends on its length, which is \
     only known at run time.\nUse `from_tail` to take a value from the end of bytes"
  );
  assert_eq!(
    tail::<[u8; 4], [u8]>(),
    "never: the target is unsized: its extent cannot be found from the end of the source"
  );
}

#[test]
fn huge_tails_cost_nothing() {
  const N: usize = if usize::BITS >= 64 { 1 << 40 } else { 1 << 24 };
  assert_eq!(tail::<[u8; N], [u8; N]>(), "infallible from 0");
  assert_eq!(tail::<[u8; N], [bool; 7]>(), format!("fallible at 0 from {}", N - 7));
  assert_eq!(tail::<[bool; N], [u16; 3]>(), format!("infallible from {}, misaligned", N - 6));
  // In memory, `34 12 34 12 ..`; from 1, `12 34 12 34 .. 12`: out of phase, every period.
  assert_eq!(tail::<[Le1234; N], Packed<[Be1234; N - 1], u8>>(), "infallible from 1");
  assert_eq!(tail::<[Le1234; N], Packed<[Le1234; N - 1], u8>>(), "unreachable at 0 from 1");
  assert_eq!(tail::<[NzLe; N], [NzLe; N - 1]>(), "infallible from 2");
  assert_eq!(tail::<[NzLe; N], Packed<u8, [NzLe; N - 1]>>(), "infallible from 1");
  assert_eq!(tail::<[NzLe; N], Packed<[NzLe; N - 1], u8>>(), "fallible at 0 from 1");
  // A padding byte every 4, from 1.
  assert_eq!(tail::<[Pair<u8, u16>; N], [u16; 2 * N - 1]>(), "uninit at 3 from 2");
  assert_eq!(tail::<[Pair<u8, u16>; N], Pair<u16, [Pair<u8, u16>; N - 1]>>(), "infallible from 2");
}

#[derive(Pun)]
#[repr(u8)]
#[pun(crate)]
#[expect(dead_code, reason = "only given to the analysis")]
enum Shape {
  Circle(u16),
  Rect(u8, bool),
}

/// An enum without padding.
#[derive(Pun)]
#[repr(u8)]
#[pun(crate)]
#[expect(dead_code, reason = "only given to the analysis")]
enum Code {
  Short(u8, u16),
  Long([u8; 3]),
}

#[derive(Pun)]
#[repr(C)]
#[pun(crate, assert(a != b))]
struct Distinct {
  a: u8,
  b: u8,
}

/// Whatever a `&mut` to the first (or with `tail`, the last) bytes of `S` as a `D` compiles: `ok`,
/// or why not, in a word.
fn part_mut<S: ?Sized + Pun, D: ?Sized + Pun>(tail: bool) -> String {
  let message = if tail { Plan::<S, D>::check_tail_mut() } else { Plan::<S, D>::check_head_mut() };
  let Some(message) = message else { return "ok".into() };
  let text = core::str::from_utf8(&message.bytes[..message.len]).unwrap();
  let word = match () {
    () if text.contains("a rule of the source") => "rule",
    () if text.contains("an enum of the source") => "enum",
    () if text.contains("a string of the source") => "string",
    () if text.contains("could be written through") => "written",
    () if text.contains("is padding") => "padding",
    () => text,
  };
  word.into()
}

/// The rule of `Distinct` over other fields: a field that is restricted, and one that is a `bool`.
#[derive(Pun)]
#[repr(C)]
#[pun(crate, assert(a != b))]
struct DistinctAscii {
  a: crate::num::u8in<0, 0x7f>,
  b: u8,
}

/// The same rule, over a larger struct: `size_of_val(self)` is not the same.
#[derive(Pun)]
#[repr(C)]
#[pun(crate, assert(a != b))]
struct DistinctWide {
  a: u8,
  b: u8,
  c: u8,
}

/// Two rules, and the same two in the other order.
#[derive(Pun)]
#[repr(C)]
#[pun(crate, assert(a != b), assert(a != 0))]
struct TwoRules {
  a: u8,
  b: u8,
}

#[derive(Pun)]
#[repr(C)]
#[pun(crate, assert(a != 0), assert(a != b))]
struct TwoRulesSwapped {
  a: u8,
  b: u8,
}

/// A rule that reads `size_of_val(self)`, over a slice: the same rule, the same extent.
#[derive(Pun)]
#[repr(C)]
#[pun(crate, assert(size_of_val(self) > n))]
struct Longer {
  n: u8,
  t: [u8],
}

#[derive(Pun)]
#[repr(C)]
#[pun(crate, assert(size_of_val(self) > n))]
struct LongerAscii {
  n: u8,
  t: [crate::num::u8in<0, 0x7f>],
}

/// The same rule over a slice of other elements: the extent is not the same.
#[derive(Pun)]
#[repr(C)]
#[pun(crate, assert(size_of_val(self) > n))]
struct LongerWide {
  n: u8,
  t: [[u8; 2]],
}

/// A rule that reads padding, as bytes.
#[derive(Pun)]
#[repr(C)]
#[pun(crate, assert(le(1, 1) == 0))]
struct ReadsPad {
  a: u8,
  b: u16,
}

/// The same rule, where the padding is a field.
#[derive(Pun)]
#[repr(C)]
#[pun(crate, assert(le(1, 1) == 0))]
struct ReadsField {
  a: u8,
  b: u8,
  c: u16,
}

#[derive(Pun)]
#[repr(u8)]
#[pun(crate)]
#[expect(dead_code, reason = "a layout for the analysis, never made")]
enum WithDistinct {
  A(DistinctAscii) = 1,
  B(u8) = 2,
}

#[derive(Pun)]
#[repr(u8)]
#[pun(crate)]
#[expect(dead_code, reason = "a layout for the analysis, never made")]
enum WithDistinctAny {
  A(Distinct) = 1,
  B(u8) = 2,
}

#[test]
fn rules_proven_by_the_same_rules() {
  // The same rules over the same bytes: proven, whatever the fields, which are related as any.
  assert_eq!(verdict::<DistinctAscii, Distinct>(), "infallible");
  assert_eq!(verdict::<Distinct, DistinctAscii>(), "fallible at 0, mutable");
  assert_eq!(verdict::<Pair<u8, DistinctAscii>, Pair<u8, Distinct>>(), "infallible");
  // Not the same extent, not the same order, no such struct.
  assert_eq!(verdict::<DistinctWide, Pair<Distinct, u8>>(), "fallible at 0, mutable");
  assert_eq!(verdict::<TwoRules, TwoRulesSwapped>(), "fallible at 0, mutable");
  assert_eq!(verdict::<[u8; 2], Distinct>(), "fallible at 0, mutable");
  // Parts: a struct of the same extent at the same place.
  assert_eq!(tail::<Pair<u16, DistinctAscii>, Distinct>(), "infallible from 2");
  assert_eq!(tail::<DistinctWide, Distinct>(), "fallible at 0 from 1");
  // Arrays: elements of the same size at the same place, proven all at once.
  assert_eq!(verdict::<[DistinctAscii; 3], [Distinct; 3]>(), "infallible");
  assert_eq!(tail::<[DistinctAscii; 3], [Distinct; 2]>(), "infallible from 2");
  assert_eq!(verdict::<[DistinctAscii], [Distinct]>(), "infallible");
  assert_eq!(verdict::<[DistinctWide; 2], [Pair<Distinct, u8>; 2]>(), "fallible at 0, mutable");
  // A slice tail: the same extent if the elements are of the same size.
  assert_eq!(verdict::<LongerAscii, Longer>(), "infallible");
  assert_eq!(verdict::<Longer, LongerAscii>(), "fallible at 1, mutable");
  assert_eq!(verdict::<LongerWide, Longer>(), "fallible at 0, mutable");
  // Enums: a variant whose struct has the same rules proves the variant. (The reverse
  // inclusions, `mutable`, are of the fields: rules are not invariants, safe code writes fields.)
  assert_eq!(verdict::<WithDistinct, WithDistinctAny>(), "infallible");
  assert_eq!(verdict::<WithDistinctAny, WithDistinct>(), "fallible at 0, mutable");
  // A rule that reads padding: not checked where the source proves it.
  assert_eq!(verdict::<ReadsField, ReadsPad>(), "infallible");
  assert_eq!(verdict::<ReadsPad, ReadsField>(), "uninit at 1");
}

#[test]
fn mutable_parts() {
  // The reverse inclusion, over the bytes that are covered.
  assert_eq!(part_mut::<[u16; 2], u16>(false), "ok");
  assert_eq!(part_mut::<[bool; 2], u8>(false), "written");
  assert_eq!(part_mut::<[bool; 2], bool>(false), "ok");
  assert_eq!(part_mut::<[bool; 2], bool>(true), "ok");
  assert_eq!(part_mut::<Packed<u8, NzLe>, u8>(false), "ok");
  assert_eq!(part_mut::<Packed<u8, NzLe>, [u8; 2]>(true), "written");
  // A scalar that is only partly covered: what is written may not be valid with the rest.
  assert_eq!(part_mut::<Packed<u8, NzLe>, [u8; 2]>(false), "written");
  assert_eq!(part_mut::<Pair<u8, NzLe>, [u8; 2]>(false), "padding");
  assert_eq!(part_mut::<GeLe, [u8; 3]>(true), "written");
  assert_eq!(part_mut::<Pair<u16, u8>, [u8; 3]>(false), "ok");
  assert_eq!(part_mut::<Pair<u8, u16>, [u8; 3]>(false), "padding");
  assert_eq!(part_mut::<Pair<u16, u8>, u8>(true), "padding");
  assert_eq!(part_mut::<[u16; 3], [u16; 2]>(true), "ok");
  // Enums: whole, or not at all.
  assert_eq!(part_mut::<Shape, u8>(false), "enum");
  assert_eq!(part_mut::<Code, [u8; 2]>(true), "enum");
  assert_eq!(part_mut::<Shape, [u8; 2]>(true), "padding");
  assert_eq!(part_mut::<[Shape; 2], Shape>(false), "ok");
  assert_eq!(part_mut::<[Shape; 2], Shape>(true), "ok");
  assert_eq!(part_mut::<[Shape; 2], [Shape; 2]>(true), "ok");
  assert_eq!(part_mut::<[Code; 2], [u8; 2]>(true), "enum");
  assert_eq!(part_mut::<[Code; 2], [u8; 6]>(false), "enum");
  assert_eq!(part_mut::<[Code; 2], Code>(true), "ok");
  assert_eq!(part_mut::<Pair<Shape, u16>, u16>(true), "ok");
  // Rules: nothing that the `&mut` covers.
  assert_eq!(part_mut::<Pair<u8, Distinct>, u8>(false), "ok");
  assert_eq!(part_mut::<Pair<Distinct, u8>, u8>(false), "rule");
  assert_eq!(part_mut::<Pair<Distinct, u8>, u8>(true), "ok");
  assert_eq!(part_mut::<Pair<u8, Distinct>, u8>(true), "rule");
  // But a struct within the part whose rules the target has over the same bytes: what is written
  // then meets them if it meets those of the target.
  assert_eq!(part_mut::<Distinct, Distinct>(false), "ok");
  assert_eq!(part_mut::<[Distinct; 3], [Distinct; 2]>(false), "ok");
  assert_eq!(part_mut::<[Distinct; 3], [Distinct; 2]>(true), "ok");
  assert_eq!(part_mut::<Pair<Distinct, u8>, Distinct>(false), "ok");
  assert_eq!(part_mut::<Pair<Distinct, u8>, [u8; 2]>(false), "rule");
  assert_eq!(part_mut::<[Distinct; 3], [u8; 1]>(true), "rule");
  // Strings: whole, or not at all.
  assert_eq!(part_mut::<str, str>(false), "ok");
  assert_eq!(part_mut::<str, [u8]>(false), "written");
  assert_eq!(part_mut::<Pair<u8, str>, u8>(false), "ok");
  assert_eq!(part_mut::<Pair<u8, str>, Pair<u8, [u8]>>(false), "written");
  // A target that takes as many elements as fit may end within an enum.
  assert_eq!(part_mut::<[Code], [[u8; 3]]>(false), "enum");
  assert_eq!(part_mut::<[Code], [[u8; 2]]>(false), "written");
  // The source is unsized.
  assert_eq!(part_mut::<[u8], u8>(true), TAIL_UNSIZED);
}

const TAIL_UNSIZED: &str = "\ncannot pun: the source is unsized: where its last bytes start \
                            depends on its length, which is only known at run time.\nUse \
                            `from_tail` to take a value from the end of bytes";

/// Word by word, folded at each step: what `sum16` computes faster.
fn sum16_reference(bytes: &[u8]) -> u16 {
  let (mut sum, mut i) = (0u64, 0);
  while i < bytes.len() {
    let low = if i + 1 < bytes.len() { bytes[i + 1] } else { 0 };
    sum += (u64::from(bytes[i]) << 8) | u64::from(low);
    sum = (sum & 0xffff) + (sum >> 16);
    i += 2;
  }
  sum as u16
}

/// Pseudo-random bytes.
fn noise(len: usize, seed: &mut u64) -> Vec<u8> {
  (0..len)
    .map(|_| {
      *seed ^= *seed << 13;
      *seed ^= *seed >> 7;
      *seed ^= *seed << 17;
      (*seed >> 24) as u8
    })
    .collect()
}

#[test]
fn sum16_is_the_sum_word_by_word() {
  use crate::layout::sum16;
  let mut seed = 0x9e37_79b9_7f4a_7c15;
  // Under Miri, the lengths around the blocks of eight bytes; natively, every one up to 300.
  let lengths: Vec<usize> =
    if cfg!(miri) { vec![0, 1, 7, 8, 9, 17, 33] } else { (0..300).collect() };
  for len in lengths {
    for bytes in [noise(len, &mut seed), vec![0; len], vec![0xff; len]] {
      // From several offsets, so that blocks are read at any alignment.
      for start in 0..bytes.len().min(9) {
        assert_eq!(sum16(&bytes[start..]), sum16_reference(&bytes[start..]), "{bytes:?}");
      }
    }
  }
  // Past the number of blocks after which the sum is folded.
  if !cfg!(miri) {
    let bytes = noise(9 << 20, &mut seed);
    assert_eq!(sum16(&bytes), sum16_reference(&bytes));
    assert_eq!(sum16(&vec![0xff; 9 << 20]), 0xffff);
  }
  // At compile time too: a header of IPv4 that holds its checksum.
  let sum =
    const { sum16(&[0x45, 0, 0, 24, 0, 1, 0, 0, 64, 253, 0x65, 0xe6, 10, 0, 0, 1, 10, 0, 0, 2]) };
  assert_eq!(sum, 0xffff);
}

/// Bit by bit, as the catalogue of CRCs defines one by its parameters: what `crc32` and `Crc`
/// compute faster.
fn crc_reference(
  width: u32,
  poly: u64,
  init: u64,
  reflect: bool,
  xorout: u64,
  bytes: &[u8],
) -> u64 {
  let mask = u64::MAX >> (64 - width);
  let mut register = init;
  for &byte in bytes {
    let byte = if reflect { byte.reverse_bits() } else { byte };
    for bit in (0..8).rev() {
      let feedback = ((register >> (width - 1)) ^ u64::from(byte >> bit)) & 1;
      register = (register << 1) & mask;
      if feedback == 1 {
        register ^= poly;
      }
    }
  }
  if reflect {
    register = register.reverse_bits() >> (64 - width);
  }
  register ^ xorout
}

#[test]
fn crcs_are_the_crcs_bit_by_bit() {
  use crate::layout::{Crc, crc32};
  // CRC-32, CRC-32C, CRC-16/MODBUS, CRC-16/XMODEM, CRC-15/CAN, CRC-5/USB, CRC-8/SMBUS,
  // CRC-64/XZ, CRC-64/ECMA-182: both orientations, widths below and above a byte.
  // Their tables are made at compile time: under Miri, making them would be most of the test.
  macro_rules! crcs {
    ($(($width:expr, $poly:expr, $init:expr, $reflect:expr, $xorout:expr)),* $(,)?) => {
      [$((
        const { &Crc::new($width, $poly, $init, $reflect, $xorout) },
        ($width, $poly, $init, $reflect, $xorout),
      )),*]
    };
  }
  /// The width, the polynomial, the initial value, whether it is reflected, the final xor.
  type Params = (u32, u64, u64, bool, u64);
  let crcs: [(&Crc, Params); 9] = crcs![
    (32, 0x04c1_1db7, 0xffff_ffff, true, 0xffff_ffff),
    (32, 0x1edc_6f41, 0xffff_ffff, true, 0xffff_ffff),
    (16, 0x8005, 0xffff, true, 0),
    (16, 0x1021, 0, false, 0),
    (15, 0x4599, 0, false, 0),
    (5, 0x05, 0x1f, true, 0x1f),
    (8, 0x07, 0, false, 0),
    (64, 0x42f0_e1eb_a9ea_3693, u64::MAX, true, u64::MAX),
    (64, 0x42f0_e1eb_a9ea_3693, 0, false, 0),
  ];
  let mut seed = 0x2545_f491_4f6c_dd1d;
  // Under Miri, the lengths around the blocks of sixteen bytes; natively, every one up to 300.
  let lengths: Vec<usize> =
    if cfg!(miri) { vec![0, 1, 15, 16, 17, 33] } else { (0..300).collect() };
  for len in lengths {
    let bytes = noise(len, &mut seed);
    // From several offsets, so that blocks are read at any alignment.
    for start in 0..bytes.len().min(3) {
      let bytes = &bytes[start..];
      let reference = crc_reference(32, 0x04c1_1db7, 0xffff_ffff, true, 0xffff_ffff, bytes);
      assert_eq!(u64::from(crc32(bytes)), reference, "{bytes:?}");
      // Under Miri, three of them: both orientations, the widths of 32, 15 and 64 bits.
      for (crc, (width, poly, init, reflect, xorout)) in
        crcs.iter().step_by(if cfg!(miri) { 4 } else { 1 })
      {
        let reference = crc_reference(*width, *poly, *init, *reflect, *xorout, bytes);
        assert_eq!(crc.check(bytes), reference, "{width} {poly:x}: {bytes:?}");
      }
    }
    // With bytes taken as zeros, anywhere, and past the end.
    let crc = crcs[1].0;
    for (from, to) in [(0, len), (len / 3, len / 2), (len / 2, len + 7), (len, len)] {
      let mut zeroed = bytes.clone();
      zeroed[from.min(len)..to.min(len)].fill(0);
      let reference = crc_reference(32, 0x1edc_6f41, 0xffff_ffff, true, 0xffff_ffff, &zeroed);
      assert_eq!(crc.compute(&bytes, (from, to)), reference, "{from} {to}: {bytes:?}");
    }
  }
  // More zeros than a block of them.
  let bytes = noise(300, &mut seed);
  let mut zeroed = bytes.clone();
  zeroed[10..290].fill(0);
  assert_eq!(crcs[3].0.compute(&bytes, (10, 290)), crc_reference(16, 0x1021, 0, false, 0, &zeroed));
  // At compile time too.
  assert_eq!(const { crc32(b"123456789") }, 0xcbf4_3926);
  assert_eq!(const { Crc::new(16, 0x8005, 0xffff, true, 0).check(b"123456789") }, 0x4b37);
}

/// A CRC with one table, a byte at a time, as on a 16-bit target (`CRC_TABLES`): the same as with
/// sixteen, for every length around the blocks of sixteen bytes.
#[test]
fn a_crc_a_byte_at_a_time_is_the_same() {
  use crate::layout::Crc;
  static CRCS: [Crc; 3] =
    [Crc::CRC32, Crc::new(16, 0x8005, 0xffff, true, 0), Crc::new(15, 0x4599, 0, false, 0)];
  // Under Miri, the lengths around the blocks of sixteen bytes; natively, every one up to 70.
  let lengths: Vec<usize> = if cfg!(miri) { vec![0, 1, 15, 16, 17, 33] } else { (0..70).collect() };
  for crc in &CRCS {
    let bytes: Vec<u8> = (0..70u32).map(|i| (i.wrapping_mul(97) ^ 0x5a) as u8).collect();
    for len in lengths.iter().copied() {
      assert_eq!(crc.check(&bytes[..len]), crc.bytewise(&bytes[..len]), "{len} bytes");
    }
  }
}

/// Two patterns are the same by their states and edges: an `id` is a field that safe code writes,
/// which only tells two apart. Nor is an automaton checked as UTF-8 for having its `id`.
#[test]
fn patterns_are_the_same_by_their_states() {
  use crate::layout::{Dfa, DfaState, Layout};
  /// Any bytes, with the `id` of UTF-8.
  const ANY: &Dfa =
    &Dfa { id: Dfa::UTF8.id, states: &[DfaState { accept: true, edges: &[(0, 0xff, 0)] }] };
  /// The states of UTF-8, with another `id`.
  const AGAIN: &Dfa = &Dfa { id: 7, states: Dfa::UTF8.states };
  // SAFETY: the probe of a slice of bytes, a type of bytes.
  let pattern = |dfa: &'static Dfa| unsafe { Layout::pattern::<[u8]>(<[u8] as Pun>::PROBE, dfa) };
  assert!(pattern(Dfa::UTF8).equals(&pattern(Dfa::UTF8)));
  assert!(!pattern(ANY).equals(&pattern(Dfa::UTF8)));
  // Two `id`s tell two apart: less is proven, never more.
  assert!(!pattern(AGAIN).equals(&pattern(Dfa::UTF8)));
  assert!(Dfa::UTF8.is_utf8());
  assert!(!ANY.is_utf8() && !AGAIN.is_utf8());
}

/// A check reads the bytes of the source that every layout has there: not those of a struct
/// whose fields Rust may put in another order, though every order makes the same bytes.
#[test]
fn a_check_reads_what_every_layout_has_there() {
  // Proven in every layout, whichever field is where.
  assert_eq!(verdict::<(u8, u8), [u8; 2]>(), "infallible, mutable");
  assert_eq!(verdict::<(bool, bool), [bool; 2]>(), "infallible, mutable");
  // What is checked is of a struct of the `C` representation, or of the target only.
  assert_eq!(verdict::<Pair<u8, (u8, u8)>, Pair<bool, [u8; 2]>>(), "fallible at 0, mutable");
  assert_eq!(verdict::<[u8; 2], (bool, bool)>(), "fallible at 0, mutable");
  // The check of a `bool`, of a `NonZero` or of the rule would read one field or the other.
  let moved = [
    verdict::<(u8, u8), [bool; 2]>(),
    verdict::<(u8, u8), (bool, bool)>(),
    verdict::<(u16, u16), [NonZero<u16>; 2]>(),
    verdict::<(u8, u8), Pair<bool, u8>>(),
    verdict::<(bool, u8), Pair<bool, u8>>(),
    verdict::<(u8, u8), Ruled>(),
    verdict::<Pair<u8, (u8, u8)>, Pair<u8, Pair<bool, u8>>>(),
    // An element of two bytes of fields of one: `[b, x, y, z]` or `[x, y, z, b]`.
    verdict::<(bool, [u8; 3]), [NonZero<u16>; 2]>(),
  ];
  if crate::layout::UNSTABLE {
    assert_eq!(
      moved,
      [
        "fallible at 0, mutable",
        "fallible at 0, mutable",
        "fallible at 0, mutable",
        "fallible at 0, mutable",
        "infallible, mutable",
        "fallible at 0, mutable",
        "fallible at 1, mutable",
        "fallible at 0",
      ]
    );
  } else {
    assert_eq!(
      moved,
      [
        "unspecified at 0",
        "unspecified at 0",
        "unspecified at 0",
        "unspecified at 0",
        "unspecified at 0",
        "unspecified at 0",
        "unspecified at 1",
        "unspecified at 0"
      ]
    );
  }
}

/// Declares a struct in each of two functions, at one place: the place of the outermost macro
/// call, which `line!()` and `column!()` give of both.
macro_rules! one_in_each {
  () => {
    fn first() -> &'static crate::layout::Layout {
      #[derive(Pun)]
      #[pun(crate)]
      struct Pair(u8, u16);
      &Pair::LAYOUT
    }
    fn second() -> &'static crate::layout::Layout {
      #[derive(Pun)]
      #[pun(crate)]
      struct Pair(u8, u16);
      &Pair::LAYOUT
    }
  };
}

one_in_each!();

/// Two types declared at one place are two types: what `rustc` makes of one is not what it makes
/// of the other, and they prove nothing of each other.
#[test]
fn two_types_at_one_place_are_two() {
  assert!(!first().id.equals(&second().id));
  if !crate::layout::UNSTABLE {
    assert!(!analyze(first(), second()).relation.infallible());
  }
}
