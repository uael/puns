//! Values that hold self-relative pointers, read as other types: the next version of a schema,
//! through recursive types, sequences, text and enums (`view_ref`); what only some values are,
//! checked at run time (`try_view_ref`); pointers read as bytes.
//!
//! The pointers are the ones of `support/relative.rs`, declared through the public API only, as
//! a crate of its own would. They are followed through their buffer, so these tests are for
//! Miri with stacked borrows as well as tree borrows.

#[path = "support/relative.rs"]
#[expect(dead_code, reason = "what the model check uses, and these tests do not")]
mod relative;

use puns::{
  Pun,
  num::{i16be, i16le, i32be, i32le, i64be, i64le, u16le, u32le},
};
use relative::{Builder, Fix, Offset, One, Opt, Region, Seq, Text, addr, field};

// -------------------------------------------------------------------------------------------------
// A schema, generic over the offset

/// An enum without fields: a scalar.
#[derive(Pun, Debug, PartialEq, Eq, Clone, Copy)]
#[repr(u8)]
pub enum Kind {
  File,
  Dir,
  Link,
}

#[derive(Pun)]
#[repr(C)]
pub struct Item<O: Offset> {
  pub id: u32,
  pub hidden: bool,
  pub kind: Kind,
  pub name: Text<O>,
  pub tags: Seq<u16, O>,
  pub next: Opt<Item<O>, O>,
}

#[derive(Pun)]
#[repr(C)]
pub struct Dir<O: Offset> {
  pub size: u64,
  pub flags: [bool; 3],
  pub entries: Seq<Item<O>, O>,
  pub parent: Opt<Dir<O>, O>,
}

#[derive(Pun)]
#[repr(u8)]
pub enum Expr<O: Offset> {
  Lit(i64),
  Neg(One<Expr<O>, O>),
  Add(One<Expr<O>, O>, One<Expr<O>, O>),
  Var(Text<O>),
  Call(Text<O>, Seq<Expr<O>, O>),
  Cond(Opt<Expr<O>, O>, char),
}

// SAFETY: no pointer.
unsafe impl Fix for Kind {
  fn fix(&mut self, _: usize) {}
}

// SAFETY: every pointer, at its position.
unsafe impl<O: Offset> Fix for Item<O> {
  fn fix(&mut self, at: usize) {
    let whole = addr(self);
    field(whole, at, &mut self.name);
    field(whole, at, &mut self.tags);
    field(whole, at, &mut self.next);
  }
}

// SAFETY: every pointer, at its position.
unsafe impl<O: Offset> Fix for Dir<O> {
  fn fix(&mut self, at: usize) {
    let whole = addr(self);
    field(whole, at, &mut self.entries);
    field(whole, at, &mut self.parent);
  }
}

// SAFETY: every pointer of the variant, at its position.
unsafe impl<O: Offset> Fix for Expr<O> {
  fn fix(&mut self, at: usize) {
    let whole = addr(self);
    match self {
      Self::Lit(_) => {}
      Self::Neg(a) => field(whole, at, a),
      Self::Add(a, b) => {
        field(whole, at, a);
        field(whole, at, b);
      }
      Self::Var(name) => field(whole, at, name),
      Self::Call(name, args) => {
        field(whole, at, name);
        field(whole, at, args);
      }
      Self::Cond(a, _) => field(whole, at, a),
    }
  }
}

/// `f(2 + -5, x, 40 if .. else 'z', 'A')`.
fn expression<O: Offset>() -> Region<Expr<O>> {
  let mut b = Builder::new();
  let (two, five) = (b.one(Expr::Lit(2)), b.one(Expr::Lit(5)));
  let add = Expr::Add(two, b.one(Expr::Neg(five)));
  let var = Expr::Var(b.text("x"));
  let some = Expr::Cond(b.opt(Some(Expr::Lit(40))), 'z');
  let none = Expr::Cond(b.opt(None), 'A');
  let args = b.seq([add, var, some, none]);
  let name = b.text("f");
  // SAFETY: every pointer made by `b`, with what it aims at, in one value.
  unsafe { Region::new(b, Expr::Call(name, args)) }
}

// -------------------------------------------------------------------------------------------------
// A value read as another type

/// The next version of the schema, which reads every value of the first as it is: a variant
/// added at the end of each enum, a `bool` read as a `u8`, a `char` as a `u32`, `[u16]` as
/// `[[u8; 2]]`.
mod v2 {
  use puns::Pun;

  use super::{Offset, One, Opt, Region, Seq, Text};

  #[derive(Pun, Debug, PartialEq, Eq, Clone, Copy)]
  #[repr(u8)]
  pub enum Kind {
    File,
    Dir,
    Link,
    /// New.
    Socket,
  }

  #[derive(Pun)]
  #[repr(C)]
  pub struct Item<O: Offset> {
    pub id: u32,
    /// Was a `bool`: there are more flags now.
    pub hidden: u8,
    pub kind: Kind,
    pub name: Text<O>,
    /// Was `[u16]`: read as bytes.
    pub tags: Seq<[u8; 2], O>,
    pub next: Opt<Item<O>, O>,
  }

  #[derive(Pun)]
  #[repr(C)]
  pub struct Dir<O: Offset> {
    pub size: u64,
    pub flags: [u8; 3],
    pub entries: Seq<Item<O>, O>,
    pub parent: Opt<Dir<O>, O>,
  }

  #[derive(Pun)]
  #[repr(u8)]
  #[expect(dead_code, reason = "a type that values are read as, never made")]
  pub enum Expr<O: Offset> {
    Lit(i64),
    Neg(One<Expr<O>, O>),
    Add(One<Expr<O>, O>, One<Expr<O>, O>),
    Var(Text<O>),
    Call(Text<O>, Seq<Expr<O>, O>),
    /// Was a `char`.
    Cond(Opt<Expr<O>, O>, u32),
    /// New.
    Mul(One<Expr<O>, O>, One<Expr<O>, O>),
  }

  /// Evaluates an expression of `r`; variables are 0, calls sum their arguments.
  pub fn eval<R, O: Offset>(r: &Region<R>, e: &Expr<O>) -> i64 {
    match e {
      Expr::Lit(n) => *n,
      Expr::Neg(a) => eval(r, r.get(a)).wrapping_neg(),
      Expr::Add(a, b) => eval(r, r.get(a)).wrapping_add(eval(r, r.get(b))),
      Expr::Mul(a, b) => eval(r, r.get(a)).wrapping_mul(eval(r, r.get(b))),
      Expr::Var(_) => 0,
      Expr::Call(_, args) => r.seq(args).iter().map(|a| eval(r, a)).fold(0, i64::wrapping_add),
      Expr::Cond(c, k) => r.opt(c).map_or_else(|| i64::from(*k), |c| eval(r, c)),
    }
  }
}

/// An expression read as its next version, with offsets of the type `O`.
fn an_expression_as_its_next_version<O: Offset>() {
  let r = expression::<O>();
  let new: &v2::Expr<O> = puns::view_ref(&*r);
  // Nothing is copied: the same bytes.
  assert!(core::ptr::addr_eq(new, &raw const *r));
  assert_eq!(v2::eval(&r, new), 2 - 5 + 40 + 65);
  let v2::Expr::Call(name, args) = new else { panic!("a call") };
  assert_eq!(r.text(name), "f");
  let args = r.seq(args);
  assert!(matches!(&args[1], v2::Expr::Var(x) if r.text(x) == "x"));
  assert!(matches!(&args[3], v2::Expr::Cond(none, 65) if r.opt(none).is_none()));
  // The elements of a sequence, as a slice of the new ones.
  let Expr::Call(_, old) = &*r else { panic!("a call") };
  let slice: &[v2::Expr<O>] = puns::view_ref(r.seq(old));
  assert!(core::ptr::addr_eq(slice, args));
}

/// Each offset: both byte orders, each width (an `i64le` holds more than an `isize` on a 32-bit
/// target, and is converted with a check).
#[test]
fn expressions_as_their_next_version() {
  an_expression_as_its_next_version::<i32le>();
  an_expression_as_its_next_version::<i16be>();
  an_expression_as_its_next_version::<i64le>();
  // Under Miri, one of each width and byte order is enough: the others run the same code.
  if !cfg!(miri) {
    an_expression_as_its_next_version::<i16le>();
    an_expression_as_its_next_version::<i32be>();
    an_expression_as_its_next_version::<i64be>();
  }
}

#[test]
fn a_directory_as_its_next_version() {
  let mut b = Builder::new();
  let item = |b: &mut Builder, id, next| {
    let (name, tags) = (b.text(if id == 7 { "a" } else { "b" }), b.seq([0x0102u16, 0x0304]));
    Item::<i32le> { id, hidden: id == 7, kind: Kind::Link, name, tags, next: b.opt(next) }
  };
  let last = item(&mut b, 8, None);
  let first = item(&mut b, 7, Some(last));
  let other = item(&mut b, 9, None);
  let entries = b.seq([first, other]);
  let parent = Dir { size: 3, flags: [false; 3], entries: b.seq([]), parent: b.opt(None) };
  let root =
    Dir { size: 1 << 40, flags: [true, false, true], entries, parent: b.opt(Some(parent)) };
  // SAFETY: every pointer made by `b`, with what it aims at, in one value.
  let r = unsafe { Region::new(b, root) };

  let new: &v2::Dir<i32le> = puns::view_ref(&*r);
  let entries = r.seq(&new.entries);
  assert_eq!((new.size, new.flags, entries.len()), (1 << 40, [1, 0, 1], 2));
  let e = &entries[0];
  assert_eq!((e.id, e.hidden, e.kind, r.text(&e.name)), (7, 1, v2::Kind::Link, "a"));
  assert_eq!(r.seq(&e.tags), &[0x0102u16.to_ne_bytes(), 0x0304u16.to_ne_bytes()][..]);
  let next = r.opt(&e.next).expect("a next item");
  assert_eq!((next.id, next.hidden, r.text(&next.name)), (8, 0, "b"));
  assert!(r.opt(&next.next).is_none());
  let parent = r.opt(&new.parent).expect("a parent");
  assert_eq!((parent.size, r.seq(&parent.entries).len()), (3, 0));
}

/// What only some values are: checked, for the value itself.
#[test]
fn checked_at_run_time() {
  #[derive(Pun)]
  #[repr(C)]
  struct Maybe {
    level: u8,
    owner: Opt<u32>,
    more: Seq<Maybe>,
  }

  #[derive(Pun)]
  #[repr(C)]
  struct Named {
    #[pun(valid(..=3))]
    level: u8,
    owner: One<u32>,
    more: Seq<Maybe>,
  }

  // SAFETY: every pointer, at its position.
  unsafe impl Fix for Maybe {
    fn fix(&mut self, at: usize) {
      let whole = addr(self);
      field(whole, at, &mut self.owner);
      field(whole, at, &mut self.more);
    }
  }

  let maybe = |level, owner: Option<u32>, more: Vec<Maybe>| {
    let mut b = Builder::new();
    let (owner, more) = (b.opt(owner), b.seq(more));
    // SAFETY: every pointer made by `b`, with what it aims at, in one value.
    unsafe { Region::new(b, Maybe { level, owner, more }) }
  };

  let mut b = Builder::new();
  let inner = Maybe { level: 9, owner: b.opt(None), more: b.seq([]) };
  let (seven, more) = (b.opt(Some(7)), b.seq([inner]));
  // SAFETY: every pointer made by `b`, with what it aims at, in one value.
  let owned = unsafe { Region::new(b, Maybe { level: 1, owner: seven, more }) };
  let view = puns::try_view_ref::<Maybe, Named>(&owned).expect("an owner");
  assert_eq!((view.level, *owned.get(&view.owner), owned.seq(&view.more)[0].level), (1, 7, 9));
  let anonymous = maybe(2, None, vec![]);
  assert_eq!(puns::try_view_ref::<Maybe, Named>(&anonymous).err(), Some(puns::Error::Validity));
  let high = maybe(4, Some(7), vec![]);
  assert_eq!(puns::try_view_ref::<Maybe, Named>(&high).err(), Some(puns::Error::Validity));

  // A variant that the target does not have.
  let mut b = Builder::new();
  // SAFETY: no pointer.
  let lit = unsafe { Region::new(Builder::new(), Expr::<i32le>::Lit(3)) };
  assert!(puns::try_view_ref::<Expr<i32le>, Small>(&lit).is_ok_and(|s| matches!(s, Small::Lit(3))));
  let name = b.text("x");
  // SAFETY: every pointer made by `b`, with what it aims at, in one value.
  let var = unsafe { Region::new(b, Expr::<i32le>::Var(name)) };
  assert!(puns::try_view_ref::<Expr<i32le>, Small>(&var).is_err());
}

/// The first variants of `Expr`.
#[derive(Pun)]
#[repr(u8)]
#[expect(dead_code, reason = "a type that values are read as, never made")]
enum Small {
  Lit(i64),
  Neg(One<Expr<i32le>>),
}

/// Two generic types that hold each other: the bounds that the derive would find go round in a
/// circle (`Ping<O>` is `Pun` if the pointer to `Pong<O>` is, which is if `Pong<O>` is...), so
/// they are told: none but the ones that the types declare.
#[derive(Pun)]
#[repr(C)]
#[pun(bound())]
struct Ping<O: Offset> {
  id: u8,
  pong: Opt<Pong<O>, O>,
}

#[derive(Pun)]
#[repr(u8)]
#[pun(bound())]
enum Pong<O: Offset> {
  End(#[expect(dead_code, reason = "a value that is only read as itself")] bool),
  Ping(One<Ping<O>, O>),
}

// SAFETY: every pointer, at its position.
unsafe impl<O: Offset> Fix for Ping<O> {
  fn fix(&mut self, at: usize) {
    let whole = addr(self);
    field(whole, at, &mut self.pong);
  }
}

// SAFETY: every pointer of the variant, at its position.
unsafe impl<O: Offset> Fix for Pong<O> {
  fn fix(&mut self, at: usize) {
    let whole = addr(self);
    if let Self::Ping(ping) = self {
      field(whole, at, ping);
    }
  }
}

/// A generic type whose parameter is what it holds: the derive finds its bounds.
#[derive(Pun)]
#[repr(C)]
struct Boxed<T: Pun> {
  id: u8,
  value: One<T>,
  more: Seq<T>,
}

// SAFETY: every pointer, at its position.
unsafe impl<T: Pun> Fix for Boxed<T> {
  fn fix(&mut self, at: usize) {
    let whole = addr(self);
    field(whole, at, &mut self.value);
    field(whole, at, &mut self.more);
  }
}

#[test]
fn generic_types() {
  let mut b = Builder::new();
  let end = b.opt(Some(Pong::End(true)));
  let inner = Pong::Ping(b.one(Ping::<i16be> { id: 2, pong: end }));
  let first = b.opt(Some(inner));
  // SAFETY: every pointer made by `b`, with what it aims at, in one value.
  let r = unsafe { Region::new(b, Ping { id: 1, pong: first }) };
  let view: &Ping<i16be> = puns::view_ref(&*r);
  let Some(Pong::Ping(next)) = r.opt(&view.pong) else { panic!("a ping") };
  assert_eq!((view.id, r.get(next).id), (1, 2));

  let mut b = Builder::new();
  let (value, more) = (b.one(true), b.seq([false, true]));
  // SAFETY: every pointer made by `b`, with what it aims at, in one value.
  let flags = unsafe { Region::new(b, Boxed { id: 1, value, more }) };
  let bytes: &Boxed<u8> = puns::view_ref(&*flags);
  assert_eq!((bytes.id, *flags.get(&bytes.value), flags.seq(&bytes.more)), (1, 1, &[0, 1][..]));
}

// -------------------------------------------------------------------------------------------------
// What the pointers themselves allow

/// Over-aligned: what a buffer holds at its own alignment.
#[derive(Pun, Debug, PartialEq, Eq, Clone, Copy)]
#[repr(C, align(16))]
struct Wide {
  value: u64,
  more: u64,
}

// SAFETY: no pointer.
unsafe impl Fix for Wide {
  fn fix(&mut self, _: usize) {}
}

/// A value is read as a prefix of it that is not more aligned; a sequence as a sequence of
/// elements of the same size, aligned as its declarer says: here the larger of the alignment of
/// the elements and the width of the offset, so that `[u8; 4]`s behind an `i32le` are `u32`s
/// (but one `[u8; 4]` is not one `u32`).
// Tuples of the `Rust` representation, read as other tuples: the `unstable` feature.
#[cfg(feature = "unstable")]
#[test]
fn prefixes_and_sequences() {
  let wide = |value| Wide { value, more: !value };
  let mut b = Builder::new();
  let one = b.one::<_, i32le>(wide(1));
  let many = b.seq::<_, i32le>([wide(2), wide(3)]);
  let words = b.seq::<[u8; 4], i32le>([[1, 2, 3, 4], [5, 6, 7, 8]]);
  // SAFETY: every pointer made by `b`, with what it aims at, in one value.
  let r = unsafe { Region::new(b, (one, many, words)) };
  // `One<Wide>` is 16-aligned: its first 8 bytes are a `u64`.
  let (prefix, many, numbers): &(One<u64>, Seq<Wide>, Seq<u32>) = puns::view_ref(&*r);
  assert_eq!(*r.get(prefix), 1);
  assert_eq!(r.seq(many), [wide(2), wide(3)]);
  let expected = [u32::from_ne_bytes([1, 2, 3, 4]), u32::from_ne_bytes([5, 6, 7, 8])];
  assert_eq!(r.seq(numbers), expected);
}

// SAFETY: every pointer, at its position.
unsafe impl<A: Fix, B: Fix, C: Fix> Fix for (A, B, C) {
  fn fix(&mut self, at: usize) {
    let whole = addr(self);
    field(whole, at, &mut self.0);
    field(whole, at, &mut self.1);
    field(whole, at, &mut self.2);
  }
}

/// A pointer that may be null is read as one that may be null, and as one that may not when it is
/// not, checked at run time; text as text.
// Tuples of the `Rust` representation, read as other tuples: the `unstable` feature.
#[cfg(feature = "unstable")]
#[test]
fn null_and_text() {
  let mut b = Builder::new();
  let some = b.opt::<u8, i32le>(Some(5));
  let none = b.opt::<u8, i32le>(None);
  let text = b.text::<i32le>("\u{e9}t\u{e9}");
  // SAFETY: every pointer made by `b`, with what it aims at, in one value.
  let r = unsafe { Region::new(b, (some, none, text)) };
  let view: &(Opt<u8>, Opt<u8>, Text) = puns::view_ref(&*r);
  assert_eq!((r.opt(&view.0), r.opt(&view.1), r.text(&view.2)), (Some(&5), None, "\u{e9}t\u{e9}"));
  let (some, _, _) = &*r;
  let one: &One<u8> = puns::try_view_ref(some).expect("not null");
  assert_eq!(*r.get(one), 5);
  assert_eq!(puns::try_view_ref::<_, One<u8>>(&r.1).err(), Some(puns::Error::Validity));
}

/// A value that holds pointers is read as plain data: an offset is initialized bytes.
#[test]
fn pointers_are_read_as_bytes() {
  #[derive(Pun)]
  #[repr(C)]
  struct Table {
    version: u8,
    count: u32le,
    numbers: Seq<u16le>,
  }

  // SAFETY: every pointer, at its position.
  unsafe impl Fix for Table {
    fn fix(&mut self, at: usize) {
      let whole = addr(self);
      field(whole, at, &mut self.numbers);
    }
  }

  let mut b = Builder::new();
  let numbers = b.seq([u16le::new(3)]);
  // SAFETY: every pointer made by `b`, with what it aims at, in one value.
  let r = unsafe { Region::new(b, Table { version: 1, count: u32le::new(2), numbers }) };
  // The numbers are at 8 (their length at 4), the table at 11 (after a byte that nothing
  // writes): its pointer, at 16, is -8.
  assert_eq!(r.root(), 11);
  assert_eq!(puns::as_bytes(&*r), [1, 2, 0, 0, 0, 0xf8, 0xff, 0xff, 0xff]);
  assert_eq!(puns::head_ref::<Table, u8>(&r), &1);
  assert_eq!(puns::tail_ref::<Table, i32le>(&r).get(), -8);
  // Where the `bool` of the tuple is: the `unstable` feature.
  #[cfg(feature = "unstable")]
  assert_eq!(puns::try_pun_ref::<Table, (bool, [u8; 8])>(&r).map(|pair| pair.0), Ok(true));
  assert_eq!(r.seq(&r.numbers)[0].get(), 3);
}

/// `Option` of a pointer whose offset has a niche at zero is the same pointer, that may be null
/// (what `self_relative!` gives it): it agrees with a pointer declared nullable (`Opt`), both
/// ways, and is followed as one.
#[test]
fn option_of_a_pointer_is_a_nullable_pointer() {
  use relative::Nz32le;

  #[derive(Pun)]
  #[repr(C)]
  struct Declared {
    id: u8,
    first: Opt<u32>,
    second: Opt<u32>,
  }

  #[derive(Pun)]
  #[repr(C)]
  struct WithOption {
    id: u8,
    first: Option<One<u32, Nz32le>>,
    second: Option<One<u32, Nz32le>>,
  }

  // SAFETY: every pointer, at its position.
  unsafe impl Fix for Declared {
    fn fix(&mut self, at: usize) {
      let whole = addr(self);
      field(whole, at, &mut self.first);
      field(whole, at, &mut self.second);
    }
  }

  // SAFETY: every pointer, at its position.
  unsafe impl Fix for WithOption {
    fn fix(&mut self, at: usize) {
      let whole = addr(self);
      field(whole, at, &mut self.first);
      field(whole, at, &mut self.second);
    }
  }

  // The pointer of zeros is `None`, any other a pointer of four bytes.
  assert_eq!(size_of::<Option<One<u32, Nz32le>>>(), 4);
  let mut b = Builder::new();
  let first = b.opt(Some(7u32));
  // SAFETY: every pointer made by `b`, with what it aims at, in one value.
  let r = unsafe { Region::new(b, Declared { id: 1, first, second: Opt::none() }) };
  let view: &WithOption = puns::view_ref(&*r);
  let (first, second) = (view.first.as_ref(), view.second.as_ref());
  assert_eq!((view.id, r.option(first), r.option(second)), (1, Some(&7), None));

  let mut b = Builder::new();
  let first = Some(b.one(9u32));
  // SAFETY: every pointer made by `b`, with what it aims at, in one value.
  let r = unsafe { Region::new(b, WithOption { id: 2, first, second: None }) };
  let view: &Declared = puns::view_ref(&*r);
  assert_eq!((view.id, r.opt(&view.first), r.opt(&view.second)), (2, Some(&9), None));
  // As a pointer that is not null, checked.
  let some: &One<u32, Nz32le> = puns::try_view_ref(&r.first).expect("not null");
  assert_eq!(*r.get(some), 9);
  assert_eq!(puns::try_view_ref::<_, One<u32>>(&r.second).err(), Some(puns::Error::Validity));
}

/// A pointer whose offset can be zero (it then aims at its own address), read as one of the same
/// scheme whose offset cannot (a `NonZero`): only when it is not zero, checked at run time.
#[test]
fn a_pointer_that_may_be_zero_read_as_one_that_cannot() {
  use relative::Nz32le;

  #[derive(Pun)]
  #[repr(C)]
  struct Plain {
    pointer: One<u8>,
  }

  #[derive(Pun)]
  #[repr(C)]
  struct NeverZero {
    pointer: One<u8, Nz32le>,
  }

  // SAFETY: the pointer, at its position.
  unsafe impl Fix for Plain {
    fn fix(&mut self, at: usize) {
      let whole = addr(self);
      field(whole, at, &mut self.pointer);
    }
  }

  let mut b = Builder::new();
  let pointer = b.one(5u8);
  // SAFETY: every pointer made by `b`, with what it aims at, in one value.
  let r = unsafe { Region::new(b, Plain { pointer }) };
  let view: &NeverZero = puns::try_view_ref(&*r).expect("not zero");
  assert_eq!(*r.get(&view.pointer), 5);
  // The other way is proven: a pointer that is not zero is a pointer.
  let back: &Plain = puns::view_ref(view);
  assert_eq!(*r.get(&back.pointer), 5);

  // An offset of zero: the pointer aims at itself, at a `u8` of zero; no `NonZero` is that.
  // SAFETY: `One::here`, of a target that zeros are.
  let r = unsafe { Region::new(Builder::new(), Plain { pointer: One::here() }) };
  assert_eq!(*r.get(&r.pointer), 0);
  assert_eq!(puns::try_view_ref::<Plain, NeverZero>(&r).err(), Some(puns::Error::Validity));
}
