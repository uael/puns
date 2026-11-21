//! A model check of self-relative pointers, against an oracle that does not use the analysis.
//!
//! The model checks what `puns::view_ref` and `puns::try_view_ref` accept, with the pointers of the
//! tests of `puns` ([`relative`](crate::relative)), declared through the public API only. A pointer
//! is an offset in a given byte order, which aims at one value, at a sequence whose length is right
//! before it, or at text whose length is right before it, and some pointers may be null.
//!
//! Each type of the model is described by hand ([`Desc`]). A description gives the plain bytes of
//! the type and what they may be, its fields and variants at their offsets, and its pointers and
//! what they aim at. The oracle is a reference validator ([`valid`]) that walks concrete bytes
//! with such a description, as a debugger would. Every byte that it reads must be in bounds and
//! initialized, and every value must be [valid]. Every pointer must lead to a place that is
//! aligned for its target and holds a valid target, and so on behind it. The oracle knows nothing
//! of layouts, of pairs of types or of instances of functions.
//!
//! [`check_pair`] reinterprets every value of a corpus of `S` as a `D` with the oracle. The corpus
//! ([`Model::corpus`]) holds every shape up to a small depth, with every value of a small
//! alphabet. The bytes that the type writes are marked as written ([`mark`]). If
//! `view_ref::<S, D>` compiles, every value must be a valid `D`, or the verdict is unsound.
//! `hook::views` tells whether it compiles, by asking the code of the library that decides it. The
//! view is then made and read through ([`Model::show`]), for Miri. `try_view_ref` must accept the
//! values that are a `D`, and only them. A pair that is refused while every value of the corpus is
//! a `D` is imprecise, and is counted.
//!
//! [`sound`] returns what the oracle alone says of a pair. The tests check it on pairs whose answer
//! is known, because an oracle that accepted everything would check nothing.
//!
//! The same oracle checks `puns::validate_in` ([`check_validate`]), `puns::pointers_in`
//! ([`check_pointers_in`]) and `puns::copy_in` ([`check_copy_in`]).
//!
//! [valid]: https://doc.rust-lang.org/reference/behavior-considered-undefined.html#r-undefined.validity

use std::{collections::BTreeMap, mem::MaybeUninit, sync::Mutex};

use puns::{Pun, hook};

use crate::relative::{Fix, Region};

/// The number of pointers that `hook::views` follows.
///
/// It is more than the number of pairs of types that a pair of the model can meet.
pub const FUEL: usize = 6;

/// What a pointer aims at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aim {
  /// One value.
  One,
  /// Values one after the other. The first is at a multiple of the larger of their alignment
  /// and 4, and their number is in the 4 bytes right before it.
  Seq,
  /// Bytes of UTF-8, with their number in the 4 bytes right before the first.
  Text,
}

/// A description of a type, written by hand.
#[derive(Clone)]
pub enum Desc {
  /// Bytes that are a value if `valid` accepts them.
  Plain { size: usize, align: usize, valid: fn(&[u8]) -> bool },
  /// Fields at their offsets. The bytes that no field covers are padding.
  Struct { size: usize, align: usize, fields: Vec<(usize, Desc)> },
  /// A tag of one byte at offset 0, and for each tag the fields of its variant at their offsets.
  Enum { size: usize, align: usize, variants: Vec<(u8, Vec<(usize, Desc)>)> },
  /// A signed offset of 4 bytes, aligned to 1, big-endian if `big`.
  ///
  /// The offset is the distance from the pointer to what `target` describes, which is the first
  /// element of a sequence or the first byte of text. An offset of zero is a null pointer, which
  /// is valid only if `nullable`.
  Pointer { big: bool, nullable: bool, aim: Aim, target: fn() -> Desc },
}

impl Desc {
  /// Returns the size of the type.
  #[must_use]
  pub const fn size(&self) -> usize {
    match self {
      Self::Plain { size, .. } | Self::Struct { size, .. } | Self::Enum { size, .. } => *size,
      Self::Pointer { .. } => 4,
    }
  }

  /// Returns the alignment of the type.
  #[must_use]
  pub const fn align(&self) -> usize {
    match self {
      Self::Plain { align, .. } | Self::Struct { align, .. } | Self::Enum { align, .. } => *align,
      Self::Pointer { .. } => 1,
    }
  }
}

/// The bytes of a buffer, and which of them a value was written to.
///
/// The other bytes are padding or gaps between values, and are never read.
///
/// # Invariant
///
/// A byte that is marked as written is initialized.
pub struct Image {
  /// The bytes of the buffer.
  bytes: Vec<MaybeUninit<u8>>,
  /// Whether a value was written to each byte.
  init: Vec<bool>,
}

impl Image {
  /// Returns the `size` bytes at `at`.
  ///
  /// # Errors
  ///
  /// Returns an error if a byte is out of bounds or was not written to.
  fn read(&self, at: usize, size: usize) -> Result<Vec<u8>, String> {
    let end = at.checked_add(size).filter(|end| *end <= self.bytes.len());
    let end = end.ok_or_else(|| format!("{size} bytes at {at} are out of bounds"))?;
    if !self.init[at..end].iter().all(|init| *init) {
      return Err(format!("a byte at {at}..{end} is not initialized"));
    }
    // SAFETY: The bytes from `at` to `end` are marked as written, which is checked above, so they
    // are initialized (`Image`, section Invariant), as `assume_init` requires.
    Ok(self.bytes[at..end].iter().map(|byte| unsafe { byte.assume_init() }).collect())
  }

  /// Marks the `size` bytes at `at` as written to, and returns them.
  ///
  /// # Panics
  ///
  /// Panics if a byte is out of bounds.
  ///
  /// # Safety
  ///
  /// The caller must ensure that:
  ///
  /// - The `size` bytes at `at` are initialized, so that the `Image` keeps its invariant.
  unsafe fn write(&mut self, at: usize, size: usize) -> Vec<u8> {
    self.init[at..at + size].fill(true);
    self.read(at, size).expect("written bytes")
  }

  /// Returns the signed integer of 4 bytes at `at`, big-endian if `big`.
  ///
  /// # Errors
  ///
  /// Returns an error if a byte is out of bounds or was not written to.
  fn int(&self, at: usize, big: bool) -> Result<i64, String> {
    let bytes: [u8; 4] = self.read(at, 4)?.try_into().map_err(|_| "4 bytes".to_owned())?;
    Ok(i64::from(if big { i32::from_be_bytes(bytes) } else { i32::from_le_bytes(bytes) }))
  }
}

/// Returns the position that the pointer at `at` aims at, or `None` if its offset is zero.
///
/// # Errors
///
/// Returns an error if the pointer cannot be read, or if it aims out of the buffer.
fn aim(image: &Image, at: usize, big: bool) -> Result<Option<usize>, String> {
  let offset = image.int(at, big)?;
  if offset == 0 {
    return Ok(None);
  }
  let to = i64::try_from(at).ok().and_then(|at| at.checked_add(offset));
  let to = to.and_then(|to| usize::try_from(to).ok()).filter(|to| *to <= image.bytes.len());
  to.map(Some).ok_or_else(|| format!("the pointer at {at} aims out of the buffer"))
}

/// Returns the length in the 4 bytes right before `to`.
///
/// # Errors
///
/// Returns an error if there are no 4 bytes before `to`, if they cannot be read, or if the length
/// is negative.
fn length(image: &Image, to: usize, big: bool) -> Result<usize, String> {
  let at = to.checked_sub(4).ok_or_else(|| format!("no length before {to}"))?;
  usize::try_from(image.int(at, big)?).map_err(|_| format!("a negative length at {at}"))
}

/// Checks that a value that `desc` describes can be read at `at` of `image`.
///
/// This is the reference validator. It follows any number of pointers.
///
/// # Errors
///
/// Returns a description of what is wrong, if something is.
pub fn valid(desc: &Desc, image: &Image, at: usize) -> Result<(), String> {
  let mut fuel = usize::MAX;
  valid_with(desc, image, at, &mut fuel)
}

/// The error of [`valid_with`] when it has no fuel left to follow a pointer.
///
/// Pointers that go round in a circle exhaust the fuel. A corrupted image can hold such pointers,
/// and the oracle then decides nothing of it.
pub const NO_FUEL: &str = "no fuel";

/// Checks what [`valid`] checks, following at most `fuel` pointers.
///
/// Each pointer followed takes one unit of `fuel`.
///
/// # Errors
///
/// Returns a description of what is wrong, if something is. Returns [`NO_FUEL`] if the fuel runs
/// out before an error is found.
pub fn valid_with(desc: &Desc, image: &Image, at: usize, fuel: &mut usize) -> Result<(), String> {
  if !at.is_multiple_of(desc.align()) {
    return Err(format!("{at} is not aligned to {}", desc.align()));
  }
  match desc {
    Desc::Plain { size, valid, .. } => {
      if valid(&image.read(at, *size)?) {
        Ok(())
      } else {
        Err(format!("an invalid value at {at}"))
      }
    }
    Desc::Struct { fields, .. } => {
      fields.iter().try_for_each(|(offset, field)| valid_with(field, image, at + offset, fuel))
    }
    Desc::Enum { variants, .. } => {
      let tag = image.read(at, 1)?[0];
      let (_, fields) = (variants.iter().find(|(of, _)| *of == tag))
        .ok_or_else(|| format!("{tag} at {at} is not a tag"))?;
      fields.iter().try_for_each(|(offset, field)| valid_with(field, image, at + offset, fuel))
    }
    Desc::Pointer { big, nullable, aim: what, target } => {
      let Some(to) = aim(image, at, *big)? else {
        return if *nullable { Ok(()) } else { Err(format!("a null pointer at {at}")) };
      };
      *fuel = fuel.checked_sub(1).ok_or(NO_FUEL)?;
      let target = target();
      match what {
        Aim::One => valid_with(&target, image, to, fuel),
        Aim::Seq => {
          let first = target.align().max(4);
          if !to.is_multiple_of(first) {
            return Err(format!("a sequence at {to} that is not aligned to {first}"));
          }
          let len = length(image, to, *big)?;
          (0..len).try_for_each(|i| valid_with(&target, image, to + i * target.size(), fuel))
        }
        Aim::Text => {
          let len = length(image, to, *big)?;
          match std::str::from_utf8(&image.read(to, len)?) {
            Ok(_) => Ok(()),
            Err(_) => Err(format!("text at {to} that is not UTF-8")),
          }
        }
      }
    }
  }
}

/// Marks as written the bytes of the value at `at` that `desc` describes, and those of everything
/// it points to.
///
/// The lengths before sequences and text count as written. The bytes left unmarked are padding or
/// gaps.
///
/// # Panics
///
/// Panics if the bytes are not a value of `desc`, which means that the description or the corpus
/// is wrong.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - The bytes of `image` hold a value at `at`, and `desc` describes its type: each byte that
///   `desc` says the value holds is initialized, and so is each byte that it says what a pointer
///   aims at holds. The `# Safety` of [`Model`] requires this of `Model::desc`.
pub unsafe fn mark(desc: &Desc, image: &mut Image, at: usize) {
  match desc {
    // SAFETY: In each arm, `desc` describes the value at `at`, by contract on the caller. So the
    // bytes that it says a scalar, a tag, a pointer or a length holds are initialized, as `write`
    // requires. A field is a value of the type that its description describes, at its offset, and
    // what a pointer aims at is a value of its target, as `mark` requires.
    Desc::Plain { size, .. } => unsafe {
      image.write(at, *size);
    },
    Desc::Struct { fields, .. } => {
      for (offset, field) in fields {
        // SAFETY: As above, for a field.
        unsafe { mark(field, image, at + offset) };
      }
    }
    Desc::Enum { variants, .. } => {
      // SAFETY: As above, for the tag.
      let tag = unsafe { image.write(at, 1) }[0];
      let (_, fields) = variants.iter().find(|(of, _)| *of == tag).expect("a tag");
      for (offset, field) in fields {
        // SAFETY: As above, for a field of the variant of the tag.
        unsafe { mark(field, image, at + offset) };
      }
    }
    Desc::Pointer { big, aim: what, target, .. } => {
      // SAFETY: As above, for the offset of a pointer.
      unsafe { image.write(at, 4) };
      let Some(to) = aim(image, at, *big).expect("a pointer") else { return };
      let target = target();
      match what {
        // SAFETY: As above, for what a pointer aims at.
        Aim::One => unsafe { mark(&target, image, to) },
        Aim::Seq => {
          // SAFETY: As above, for the length before a sequence.
          unsafe { image.write(to - 4, 4) };
          let len = length(image, to, *big).expect("a length");
          for i in 0..len {
            // SAFETY: As above, for each value of the sequence.
            unsafe { mark(&target, image, to + i * target.size()) };
          }
        }
        Aim::Text => {
          // SAFETY: As above, for the length before text and for its bytes.
          unsafe {
            image.write(to - 4, 4);
            let len = length(image, to, *big).expect("a length");
            image.write(to, len);
          }
        }
      }
    }
  }
}

/// A type of the model, with its description, a corpus of its values, and a way to read all of a
/// value.
///
/// # Safety
///
/// An implementation must guarantee that `desc` describes `Self`: each byte that it says a value
/// of `Self` holds is initialized in every value, and so is each byte that it says what a pointer
/// aims at holds. A wrong description makes [`image`] read an uninitialized byte, which Miri
/// reports.
pub unsafe trait Model: Pun + Fix + 'static {
  /// Returns the description of the type.
  fn desc() -> Desc;

  /// Returns values of the type, each in a region of its own.
  ///
  /// The corpus holds every shape up to a small depth, with every value of a small alphabet.
  ///
  /// # Panics
  ///
  /// Panics for a type that is only a target, such as `bool`.
  fn corpus() -> Vec<Region<Self>>;

  /// Formats all of the value, following its pointers through `region`, which holds it.
  ///
  /// This reads every byte of the value and of what it points to, so that Miri checks each read.
  fn show<R>(&self, region: &Region<R>) -> String;
}

/// Returns the name of `T` without its path.
fn name<T>() -> &'static str {
  core::any::type_name::<T>().rsplit("::").next().unwrap_or_default()
}

/// Returns the image of `region`, with the bytes that its `T` was written to marked as written.
///
/// # Panics
///
/// Panics if the oracle does not accept the value, which means that the description or the corpus
/// is wrong.
#[must_use]
pub fn image<T: Model>(region: &Region<T>) -> Image {
  let bytes = region.bytes().to_vec();
  let mut image = Image { init: vec![false; bytes.len()], bytes };
  // SAFETY: The bytes of `region` hold a `T` at `region.root()`, and `T::desc()` describes `T`, by
  // the `# Safety` of `Model`. These are the requirements of `mark`.
  unsafe { mark(&T::desc(), &mut image, region.root()) };
  if let Err(why) = valid(&T::desc(), &image, region.root()) {
    panic!("the oracle refuses a {} of the corpus: {why}", name::<T>());
  }
  image
}

/// Returns whether the oracle accepts every value of the corpus of `S` as a `D`.
///
/// The library takes no part in the answer.
///
/// # Panics
///
/// Panics if the oracle does not accept a value of the corpus of `S` as an `S`.
#[must_use]
pub fn sound<S: Model, D: Model>() -> bool {
  let target = D::desc();
  S::corpus().iter().all(|region| valid(&target, &image(region), region.root()).is_ok())
}

/// The imprecise verdicts that [`check_pair`] found, by cause.
pub static IMPRECISE: Mutex<BTreeMap<&'static str, Vec<String>>> = Mutex::new(BTreeMap::new());

/// Records `pair` in [`IMPRECISE`], under `cause`.
///
/// # Panics
///
/// Panics if [`IMPRECISE`] is poisoned.
fn imprecise(cause: &'static str, pair: &str) {
  IMPRECISE.lock().unwrap().entry(cause).or_default().push(pair.to_owned());
}

/// Checks the verdicts of the library on reinterpreting an `S` as a `D` against the oracle, on
/// every value of the corpus of `S`.
///
/// A view that `view_ref` makes, or that `try_view_ref` accepts, is read through. The pair is
/// recorded in [`IMPRECISE`] if `view_ref` is refused while every value of the corpus is a `D`. It
/// is also recorded if `try_view_ref` is refused while some value of the corpus is a `D`.
///
/// # Panics
///
/// Panics if a verdict is unsound, because a value that is not a `D` is reinterpreted as one.
/// Panics if a verdict is incorrect, because `try_view_ref` rejects a value that is a `D`. Panics
/// if `view_ref` compiles for the pair while `try_view_ref` does not. Panics if [`IMPRECISE`] is
/// poisoned.
pub fn check_pair<S: Model, D: Model>() {
  let pair = format!("{} -> {}", name::<S>(), name::<D>());
  let (views, tries) = const { (hook::views::<S, D>(FUEL), hook::tries_view::<S, D>(FUEL)) };
  let target = D::desc();
  let (mut all, mut some) = (true, false);
  for region in S::corpus() {
    let verdict = valid(&target, &image(&region), region.root());
    (all, some) = (all && verdict.is_ok(), some || verdict.is_ok());
    let value = || region.show(&region);
    // SAFETY: `FUEL` is more than the number of pairs of types that a pair of the model can meet,
    // as `hook::view_ref` requires.
    if let Some(view) = unsafe { hook::view_ref::<S, D, FUEL>(&*region) } {
      if let Err(why) = &verdict {
        panic!("UNSOUND: {pair}: `view_ref` reads {} as a target, but {why}", value());
      }
      // All of it, pointers followed: what Miri is there to see.
      let _ = view.show(&region);
    }
    // SAFETY: As above, for `hook::try_view_ref`.
    match (unsafe { hook::try_view_ref::<S, D, FUEL>(&*region) }, &verdict) {
      (Some(Ok(_)), Err(why)) => {
        panic!("UNSOUND: {pair}: `try_view_ref` accepts {}, but {why}", value())
      }
      (Some(Err(error)), Ok(())) => {
        panic!("INCORRECT: {pair}: `try_view_ref` refuses {}, a valid target: {error}", value())
      }
      (Some(Ok(view)), Ok(())) => {
        let _ = view.show(&region);
      }
      (Some(Err(_)), Err(_)) | (None, _) => {}
    }
  }
  assert!(tries || !views, "{pair}: what is proven can be tried");
  if all && !views {
    imprecise("`view_ref` refused, while every value of the corpus is a target", &pair);
  }
  if some && !tries {
    imprecise("`try_view_ref` refused, while some values of the corpus are targets", &pair);
  }
}

/// Prints the imprecise verdicts recorded in [`IMPRECISE`].
///
/// # Panics
///
/// Panics if [`IMPRECISE`] is poisoned, because a test panicked while it held it.
pub fn report() {
  for (cause, pairs) in IMPRECISE.lock().unwrap().iter() {
    println!("   imprecise, {cause}: {}", pairs.len());
    for pair in pairs {
      println!("      {pair}");
    }
  }
}

/// Checks every ordered pair of the types with `pointers::$check`, a type with itself included.
#[macro_export]
macro_rules! pointer_pairs {
  ($check:ident; $($ty:ty),+) => {
    $crate::pointer_pairs!(@ $check; ($($ty),+); $($ty),+);
  };
  (@ $check:ident; $targets:tt; $($source:ty),+) => {
    $($crate::pointer_pairs!(@@ $check; $source; $targets);)+
  };
  (@@ $check:ident; $source:ty; ($($target:ty),+)) => {
    $($crate::pointers::$check::<$source, $target>();)+
  };
}

// -------------------------------------------------------------------------------------------------
// `puns::validate_in`, against the oracle

/// The error of [`Follow`], which tells why it stops.
#[derive(Debug, PartialEq, Eq)]
pub enum Stop {
  /// Bytes that are not a valid value, at this position.
  Invalid(usize),
  /// A pointer that does not lead to a value, at this position.
  Pointer(usize),
  /// More pointers to follow than the fuel allows.
  NoFuel,
}

impl From<puns::Invalid> for Stop {
  fn from(invalid: puns::Invalid) -> Self {
    Self::Invalid(invalid.at)
  }
}

/// A visitor that follows the pointers of the tests (`relative`) through `bytes`.
///
/// It validates what each pointer aims at with `puns`, in turn, as a crate that has pointers does
/// with `validate_in`, and is written from the layouts of the pointers only (`as_pointer`,
/// `Pointee`).
pub struct Follow<'b> {
  /// The buffer that holds the values and what their pointers aim at.
  pub bytes: &'b [u8],
  /// The number of pointers that may still be followed.
  pub fuel: usize,
}

impl Follow<'_> {
  /// Returns the signed integer of `width` bytes at `at`, in the byte order `endian`.
  ///
  /// # Errors
  ///
  /// Returns [`Stop::Pointer`] at `at` if the bytes are not all in `bytes`.
  fn int(&self, at: usize, width: usize, endian: puns::layout::Endian) -> Result<i64, Stop> {
    let bytes = self.bytes.get(at..).and_then(|rest| rest.get(..width)).ok_or(Stop::Pointer(at))?;
    let mut wide = [0u8; 8];
    for (i, byte) in bytes.iter().enumerate() {
      wide[if matches!(endian, puns::layout::Endian::Little) { i } else { width - 1 - i }] = *byte;
    }
    let shift = 64 - 8 * u32::try_from(width).unwrap_or(8);
    Ok(i64::from_le_bytes(wide) << shift >> shift)
  }

  /// Returns the length in the `width` bytes right before `to`.
  ///
  /// # Errors
  ///
  /// Returns [`Stop::Pointer`] if those bytes are not all in `bytes`, or if the length is
  /// negative.
  fn length(&self, to: usize, width: usize, endian: puns::layout::Endian) -> Result<usize, Stop> {
    let at = to.checked_sub(width).ok_or(Stop::Pointer(to))?;
    usize::try_from(self.int(at, width, endian)?).map_err(|_| Stop::Pointer(at))
  }
}

/// What a pointer of the tests aims at, found by [`Follow::target`].
enum Target {
  /// One value, at this position.
  One(usize),
  /// A sequence, by the position of its first element and its length.
  Sequence(usize, usize),
  /// Text, which is valid UTF-8.
  Text,
}

impl Follow<'_> {
  /// Returns what the pointer `P` at `at` aims at, from the layout of `P` only (`as_pointer`).
  ///
  /// It checks what the scheme of the pointer requires: the target must be in `bytes` and aligned,
  /// a length must not be negative, and text must be valid UTF-8. Following the pointer takes one
  /// unit of fuel.
  ///
  /// # Errors
  ///
  /// Returns [`Stop::Pointer`] if the offset is zero or if a check fails. Returns
  /// [`Stop::NoFuel`] if there is no fuel left.
  ///
  /// # Panics
  ///
  /// Panics if `P` is not a pointer, or not a pointer of the tests.
  fn target<P: ?Sized + Pun>(&mut self, at: usize) -> Result<Target, Stop> {
    let pointer = const { P::LAYOUT.as_pointer() }.expect("a pointer is handed over");
    assert_eq!(pointer.scheme(), crate::relative::SCHEME, "a pointer of the tests");
    let (width, endian) = (pointer.width(), pointer.endian());
    let offset = self.int(at, width, endian)?;
    // An offset of zero, which the type of the pointer allows (it is no `NonZero`): the oracle
    // reads it as null, so does this (the tests make one only for a target that zeros are).
    if offset == 0 {
      return Err(Stop::Pointer(at));
    }
    self.fuel = self.fuel.checked_sub(1).ok_or(Stop::NoFuel)?;
    let to = i64::try_from(at).ok().and_then(|at| at.checked_add(offset));
    let to = to.and_then(|to| usize::try_from(to).ok()).filter(|to| *to <= self.bytes.len());
    let to = to.ok_or(Stop::Pointer(at))?;
    match pointer.aim() {
      puns::layout::Aim::One => {
        if !to.is_multiple_of(pointer.align()) {
          return Err(Stop::Pointer(at));
        }
        Ok(Target::One(to))
      }
      puns::layout::Aim::Sequence { align } => {
        if !to.is_multiple_of(align) {
          return Err(Stop::Pointer(at));
        }
        Ok(Target::Sequence(to, self.length(to, width, endian)?))
      }
      puns::layout::Aim::Text => {
        let len = self.length(to, width, endian)?;
        let text =
          self.bytes.get(to..).and_then(|rest| rest.get(..len)).ok_or(Stop::Pointer(at))?;
        std::str::from_utf8(text).map(|_| Target::Text).map_err(|_| Stop::Pointer(at))
      }
    }
  }
}

impl puns::Visit for Follow<'_> {
  type Error = Stop;

  fn pointer<P: ?Sized + Pun>(&mut self, at: usize) -> Result<(), Stop> {
    match self.target::<P>(at)? {
      Target::One(to) => puns::validate_in::<puns::Pointee<P>, Self>(self.bytes, to, self),
      Target::Sequence(to, len) => {
        puns::validate_elems_in::<puns::Pointee<P>, Self>(self.bytes, to, len, self)
      }
      Target::Text => Ok(()),
    }
  }
}

/// A visitor that follows the pointers of the tests as [`Follow`] does, and records each pointer
/// handed to it.
///
/// It follows the pointers with `validate_in` if `trusted` is false. It follows them with
/// `pointers_in` if `trusted` is true, which is only for bytes that `validate_in` accepted, every
/// pointer followed.
struct Trace<'b> {
  /// What follows the pointers.
  follow: Follow<'b>,
  /// Whether the bytes were validated, every pointer followed.
  trusted: bool,
  /// The position and the type of each pointer handed over, in order.
  seen: Vec<(usize, &'static str)>,
}

impl puns::Visit for Trace<'_> {
  type Error = Stop;

  fn pointer<P: ?Sized + Pun>(&mut self, at: usize) -> Result<(), Stop> {
    self.seen.push((at, core::any::type_name::<P>()));
    let target = self.follow.target::<P>(at)?;
    let (bytes, trusted) = (self.follow.bytes, self.trusted);
    match (target, trusted) {
      (Target::One(to), false) => puns::validate_in::<puns::Pointee<P>, Self>(bytes, to, self),
      (Target::Sequence(to, len), false) => {
        puns::validate_elems_in::<puns::Pointee<P>, Self>(bytes, to, len, self)
      }
      // SAFETY: `trusted` is true only in the `Trace` that `trace` makes after `validate_in`
      // accepted the same bytes with a `Trace` that followed every pointer. That walk accepted
      // what this pointer aims at, `to` in `bytes`, as a `Pointee<P>`. So those bytes are valid
      // for reads, and meet the requirements of `pointers_in`.
      (Target::One(to), true) => unsafe {
        puns::pointers_in::<puns::Pointee<P>, Self>(bytes.as_ptr(), to, self)
      },
      // SAFETY: As for one target, the walk of `validate_in` accepted the `len` values from `to`
      // in `bytes`. So each meets the requirements of `pointers_in`, as `pointers_elems_in`
      // requires.
      (Target::Sequence(to, len), true) => unsafe {
        puns::pointers_elems_in::<puns::Pointee<P>, Self>(bytes.as_ptr(), to, len, self)
      },
      (Target::Text, _) => Ok(()),
    }
  }
}

/// The number of pointers that the oracle and [`Follow`] follow in a corrupted image.
///
/// It is more than any value of a corpus holds. An image that needs more has pointers that go
/// round in a circle.
const VALIDATE_FUEL: usize = 64;

/// Returns the bytes of `image`, with `fill` for each byte that no value was written to, then 16
/// more bytes of `fill`.
///
/// The bytes that no value was written to are padding and gaps. The 16 bytes at the end are there
/// because `validate_in` requires every byte of a value to be in bounds, as a reference to the
/// value covers them. The oracle reads only the bytes of the variant that the value is. A value of
/// a corrupted image may start near the end of the buffer.
fn initialized(image: &Image, fill: u8) -> Vec<u8> {
  (0..image.bytes.len() + 16)
    .map(|at| {
      if image.init.get(at) == Some(&true) { image.read(at, 1).expect("written")[0] } else { fill }
    })
    .collect()
}

/// Returns an image of `bytes`, every byte of it written.
fn whole(bytes: &[u8]) -> Image {
  Image {
    bytes: bytes.iter().map(|byte| MaybeUninit::new(*byte)).collect(),
    init: vec![true; bytes.len()],
  }
}

/// Returns whether `puns::validate_in` with [`Follow`] and the oracle both answer on a `T` at `at`.
///
/// `bytes` and `image` hold the same bytes. A function that runs out of fuel gives no answer.
/// `what` names the image in the message of a panic.
///
/// # Panics
///
/// Panics if both answer and the answers differ.
fn agree<T: Model>(bytes: &[u8], image: &Image, at: usize, what: &str) -> bool {
  let oracle = valid_with(&T::desc(), image, at, &mut { VALIDATE_FUEL });
  let mut follow = Follow { bytes, fuel: VALIDATE_FUEL };
  let ours = puns::validate_in::<T, Follow<'_>>(bytes, at, &mut follow);
  match (&oracle, &ours) {
    (Err(why), _) if why == NO_FUEL => false,
    (_, Err(Stop::NoFuel)) => false,
    (Ok(()), Ok(())) | (Err(_), Err(_)) => true,
    _ => panic!("{}: {what}: the oracle says {oracle:?}, `validate_in` {ours:?}", name::<T>()),
  }
}

/// Checks that `puns::validate_in`, with a visitor that follows the pointers, accepts what the
/// oracle accepts, and returns how many images were compared.
///
/// The images are the values of the corpus of `T`, with their padding and gaps filled with `0x00`,
/// then with `0xa5`. Each copy of one with one byte changed by one of `masks` (`xor`) is an image
/// too, padding and gaps included. So are the bytes cut right after the value. When the bytes are
/// cut before the end of the value, `validate_in` must reject them.
///
/// # Panics
///
/// Panics if `validate_in` and the oracle disagree on an image, if a value of the corpus needs more
/// fuel than `VALIDATE_FUEL`, or if `validate_in` accepts bytes cut before the end of the value.
#[must_use]
pub fn check_validate<T: Model>(masks: &[u8]) -> usize {
  let mut compared = 0;
  for region in T::corpus() {
    let image = image(&region);
    for fill in [0, 0xa5] {
      let bytes = initialized(&image, fill);
      assert!(
        agree::<T>(&bytes, &whole(&bytes), region.root(), "the corpus"),
        "fuel for the corpus"
      );
      compared += 1;
      for at in 0..bytes.len() {
        for &mask in masks {
          let mut bad = bytes.clone();
          bad[at] ^= mask;
          let what = format!("{mask:#x} at {at}");
          compared += usize::from(agree::<T>(&bad, &whole(&bad), region.root(), &what));
        }
      }
      // Cut right after the root, which is all there: the same. Cut before its end: `validate_in`
      // wants every byte of a value (a reference to it covers them), where the oracle reads only
      // those of the variant that it is; and so does it of what pointers aim at, before the root.
      let end = region.root() + size_of::<T>();
      compared +=
        usize::from(agree::<T>(&bytes[..end], &whole(&bytes[..end]), region.root(), "cut"));
      for len in [region.root(), end - 1] {
        let mut follow = Follow { bytes: &bytes[..len], fuel: VALIDATE_FUEL };
        let result = puns::validate_in::<T, Follow<'_>>(&bytes[..len], region.root(), &mut follow);
        assert_eq!(result, Err(Stop::Invalid(region.root())), "{}: cut at {len}", name::<T>());
      }
    }
  }
  compared
}

/// Compares the pointers that `validate_in` and `pointers_in` hand over for the `T` at `at` of
/// `bytes`, and for everything they aim at.
///
/// Returns `None` if `validate_in` rejects the bytes or runs out of fuel. Otherwise `pointers_in`
/// runs on the same bytes, and the function returns `Some(())`.
///
/// # Panics
///
/// Panics if `pointers_in` fails, or if it does not hand over the same pointers in the same order.
fn trace<T: Model>(bytes: &[u8], at: usize, what: &str) -> Option<()> {
  let follow = |bytes| Follow { bytes, fuel: VALIDATE_FUEL };
  let mut checked = Trace { follow: follow(bytes), trusted: false, seen: vec![] };
  puns::validate_in::<T, Trace<'_>>(bytes, at, &mut checked).ok()?;
  let mut trusted = Trace { follow: follow(bytes), trusted: true, seen: vec![] };
  // SAFETY: `validate_in` accepted the bytes as a `T` at `at`, so they are in `bytes`, valid for
  // reads, and meet the requirements of `pointers_in`. `trusted` follows what each pointer aims
  // at with `pointers_in` too, which the same walk accepted (see `Trace::pointer`).
  let walked = unsafe { puns::pointers_in::<T, Trace<'_>>(bytes.as_ptr(), at, &mut trusted) };
  assert_eq!(walked, Ok(()), "{}: {what}: `pointers_in` fails", name::<T>());
  assert_eq!(trusted.seen, checked.seen, "{}: {what}: other pointers", name::<T>());
  Some(())
}

/// Checks that `puns::pointers_in` hands over the pointers that `validate_in` hands over, in the
/// same order, and returns how many images were compared.
///
/// The images are those that `validate_in` accepts, following the pointers through the buffer.
/// They are the values of the corpus of `T`, and each copy of one with one byte changed by one of
/// `masks` that is still valid, padding and gaps included.
///
/// # Panics
///
/// Panics if the two functions disagree on an image, or if `validate_in` rejects a value of the
/// corpus.
#[must_use]
pub fn check_pointers_in<T: Model>(masks: &[u8]) -> usize {
  let mut compared = 0;
  for region in T::corpus() {
    let image = image(&region);
    for fill in [0, 0xa5] {
      let bytes = initialized(&image, fill);
      assert!(trace::<T>(&bytes, region.root(), "the corpus").is_some(), "the corpus is valid");
      compared += 1;
      for at in 0..bytes.len() {
        for &mask in masks {
          let mut changed = bytes.clone();
          changed[at] ^= mask;
          let what = format!("{mask:#x} at {at}");
          compared += usize::from(trace::<T>(&changed, region.root(), &what).is_some());
        }
      }
    }
  }
  compared
}

/// Marks in `out` the bytes of the value at `at` of `bytes` that `copy_in` copies, as `desc`
/// describes them.
///
/// These are the bytes that the value holds initialized: its scalars, the tag of an enum and the
/// fields of its variant. The bytes of a pointer are not marked unless the pointer is null, and
/// what the pointers aim at is not marked. This is the oracle of [`check_copy_in`].
///
/// # Panics
///
/// Panics if the bytes are not a value of `desc`. The caller validates them first.
fn data(desc: &Desc, bytes: &[u8], at: usize, out: &mut [bool]) {
  let fields = match desc {
    Desc::Plain { size, .. } => {
      out[at..at + size].fill(true);
      return;
    }
    // The bytes of a pointer are left to the caller, but those of a null one (zeros).
    Desc::Pointer { nullable, .. } => {
      if *nullable && bytes[at..at + 4] == [0; 4] {
        out[at..at + 4].fill(true);
      }
      return;
    }
    Desc::Struct { fields, .. } => fields,
    Desc::Enum { variants, .. } => {
      out[at] = true;
      &variants.iter().find(|(tag, _)| *tag == bytes[at]).expect("a valid tag").1
    }
  };
  for (offset, field) in fields {
    data(field, bytes, at + offset, out);
  }
}

/// Checks that `puns::copy_in` copies the bytes that the oracle says a value holds initialized,
/// except those of its pointers that are not null, and no other byte. Returns how many images were
/// compared.
///
/// The images are those that `validate_in` accepts, following the pointers through the buffer.
/// They are the values of the corpus of `T`, and each copy of one with one byte changed by one of
/// `masks` that is still valid. The root is copied into bytes that are all `0x00`, then all
/// `0xee`. Each byte that the oracle marks must then be the byte of the source. Every other byte
/// must keep its value.
///
/// # Panics
///
/// Panics if a byte differs from what the oracle says, or if `validate_in` rejects a value of the
/// corpus.
#[must_use]
pub fn check_copy_in<T: Model>(masks: &[u8]) -> usize {
  let one = |bytes: &[u8], at: usize, what: &str| -> bool {
    let mut follow = Follow { bytes, fuel: VALIDATE_FUEL };
    if puns::validate_in::<T, Follow<'_>>(bytes, at, &mut follow).is_err() {
      return false;
    }
    let mut held = vec![false; size_of::<T>()];
    data(&T::desc(), &bytes[at..], 0, &mut held);
    for fill in [0x00, 0xee] {
      let mut dst = vec![fill; size_of::<T>()];
      // SAFETY: `validate_in` accepted a `T` at `at`, so the bytes from `at` are valid for reads
      // and meet the requirements of `copy_in`. `dst` is `size_of::<T>()` bytes, valid for
      // writes, in another allocation.
      unsafe { puns::copy_in::<T>(bytes[at..].as_ptr(), dst.as_mut_ptr()) };
      for (i, &byte) in dst.iter().enumerate() {
        let expected = if held[i] { bytes[at + i] } else { fill };
        assert_eq!(byte, expected, "{}: {what}: byte {i}", name::<T>());
      }
    }
    true
  };
  let mut compared = 0;
  for region in T::corpus() {
    let image = image(&region);
    for fill in [0, 0xa5] {
      let bytes = initialized(&image, fill);
      assert!(one(&bytes, region.root(), "the corpus"), "the corpus is valid");
      compared += 1;
      for at in 0..bytes.len() {
        for &mask in masks {
          let mut changed = bytes.clone();
          changed[at] ^= mask;
          compared += usize::from(one(&changed, region.root(), &format!("{mask:#x} at {at}")));
        }
      }
    }
  }
  compared
}

/// The types of the model of self-relative pointers, with their descriptions and corpora.
pub mod types {
  use core::mem::offset_of;

  use puns::{
    Pun,
    num::{i32be, u16le},
  };

  use super::{Aim, Desc, Model};
  use crate::relative::{Builder, Fix, One, Opt, Region, Seq, Text, addr, field};

  /// Accepts any bytes.
  const fn any(_: &[u8]) -> bool {
    true
  }

  /// Accepts bytes whose first byte is 0 or 1.
  ///
  /// # Panics
  ///
  /// Panics if `bytes` is empty.
  const fn zero_or_one(bytes: &[u8]) -> bool {
    bytes[0] <= 1
  }

  /// Returns the description of `size` bytes that are any value.
  const fn bytes(size: usize) -> Desc {
    Desc::Plain { size, align: 1, valid: any }
  }

  /// Returns the description of a `bool`.
  const fn flag() -> Desc {
    Desc::Plain { size: 1, align: 1, valid: zero_or_one }
  }

  /// Returns the description of a little-endian pointer.
  const fn pointer(nullable: bool, aim: Aim, target: fn() -> Desc) -> Desc {
    Desc::Pointer { big: false, nullable, aim, target }
  }

  /// Returns the description of a struct of the size and the alignment of `T`.
  const fn structure<T>(fields: Vec<(usize, Desc)>) -> Desc {
    Desc::Struct { size: size_of::<T>(), align: align_of::<T>(), fields }
  }

  /// Returns the region of the root that `build` returns, with what `build` put in the builder.
  ///
  /// # Safety
  ///
  /// The caller must ensure that:
  ///
  /// - `build` makes every pointer of the root, and of what it aims at, with the builder that it
  ///   receives, with what the pointer aims at, and puts each pointer in one value only.
  unsafe fn region<T: Fix>(build: impl FnOnce(&mut Builder) -> T) -> Region<T> {
    let mut b = Builder::new();
    let root = build(&mut b);
    // SAFETY: Every pointer of `root`, and of what `b` holds, was made by `b` with what it aims
    // at, and is in one value only, by contract on the caller. `T` and the types that `b` holds
    // implement `Fix` as its `# Safety` requires. These are the requirements of `Region::new`.
    unsafe { Region::new(b, root) }
  }

  /// Formats what a pointer to one value aims at.
  fn one<R, T: Model>(r: &Region<R>, pointer: &One<T>) -> String {
    r.get(pointer).show(r)
  }

  /// Formats what a pointer that may be null aims at, or `-` if it is null.
  fn opt<R, T: Model>(r: &Region<R>, pointer: &Opt<T>) -> String {
    r.opt(pointer).map_or_else(|| "-".to_owned(), |value| value.show(r))
  }

  /// Formats the sequence that a pointer aims at.
  fn seq<R, T: Model>(r: &Region<R>, pointer: &Seq<T>) -> String {
    let values: Vec<String> = r.seq(pointer).iter().map(|value| value.show(r)).collect();
    format!("[{}]", values.join(", "))
  }

  /// Formats what a big-endian pointer to a `u8` aims at.
  fn big_endian<R>(r: &Region<R>, pointer: &One<u8, i32be>) -> String {
    r.get(pointer).to_string()
  }

  /// Returns the text that a pointer aims at.
  fn text<R>(r: &Region<R>, pointer: &Text) -> String {
    r.text(pointer).to_owned()
  }

  /// Implements [`Model`] for types that are only targets, each with its description.
  macro_rules! plain {
    ($($ty:ty: $desc:expr;)+) => {$(
      // SAFETY: `$desc` describes the bytes of `$ty`, a scalar or an array of scalars whose bytes
      // are all initialized, as the `# Safety` of `Model` requires.
      unsafe impl Model for $ty {
        fn desc() -> Desc {
          $desc
        }

        fn corpus() -> Vec<Region<Self>> {
          unreachable!("a target only")
        }

        fn show<R>(&self, _: &Region<R>) -> String {
          format!("{self:?}")
        }
      }
    )+};
  }

  plain! {
    bool: flag();
    u8: bytes(1);
    u16le: bytes(2);
    [u8; 2]: bytes(2);
  }

  /// Returns the region of a chain of `values`, each holding a pointer to the next, with the
  /// first at the root.
  ///
  /// # Panics
  ///
  /// Panics if `values` is empty.
  ///
  /// # Safety
  ///
  /// The caller must ensure that:
  ///
  /// - `make` makes a value whose only pointer is the one that it receives.
  unsafe fn chain<T: Fix, V: Copy>(values: &[V], make: impl Fn(V, Opt<T>) -> T) -> Region<T> {
    // SAFETY: The first pointer is null, and each other one is made by `b.opt` with the value
    // that it aims at. Each is put in the one value that `make` makes of it, which holds no other
    // pointer, by contract on the caller. These are the requirements of `region`.
    unsafe {
      region(|b| {
        let mut next = Opt::none();
        for value in values[1..].iter().rev() {
          next = b.opt(Some(make(*value, next)));
        }
        make(values[0], next)
      })
    }
  }

  /// Returns every chain of one and of two values of `alphabet`, and one chain of three values.
  ///
  /// # Panics
  ///
  /// Panics if `alphabet` is empty.
  ///
  /// # Safety
  ///
  /// The caller must ensure the requirement of [`chain`].
  unsafe fn chains<T: Fix, V: Copy>(
    alphabet: &[V],
    make: impl Fn(V, Opt<T>) -> T,
  ) -> Vec<Region<T>> {
    let mut corpus = vec![];
    // SAFETY: `make` is the one of the caller, which meets the requirement of `chain`.
    unsafe {
      for first in alphabet {
        corpus.push(chain(&[*first], &make));
        for second in alphabet {
          corpus.push(chain(&[*first, *second], &make));
        }
      }
      let last = alphabet[alphabet.len() - 1];
      corpus.push(chain(&[alphabet[0], last, last], &make));
    }
    corpus
  }

  /// A recursive struct of a `bool` and of a pointer to another `Flags`, which may be null.
  #[derive(Pun)]
  #[repr(C)]
  pub struct Flags {
    pub flag: bool,
    pub next: Opt<Flags>,
  }

  /// A recursive struct like `Flags`, with a `u8` in place of the `bool`.
  #[derive(Pun)]
  #[repr(C)]
  pub struct Bytes {
    pub flag: u8,
    pub next: Opt<Bytes>,
  }

  /// Implements `Fix` and [`Model`] for structs of a value and of a pointer to the next value,
  /// which may be null.
  macro_rules! chained {
    ($($ty:ident: $flag:expr, $alphabet:expr;)+) => {$(
      // SAFETY: `fix` calls `field` on the one field that holds a pointer, which fixes it at its
      // position, and changes nothing else, as the `# Safety` of `Fix` requires.
      unsafe impl Fix for $ty {
        fn fix(&mut self, at: usize) {
          let whole = addr(self);
          field(whole, at, &mut self.next);
        }
      }

      // SAFETY: `desc` gives each field of the struct at its offset, with the description of its
      // type, and the pointer aims at another `$ty`, as the `# Safety` of `Model` requires.
      unsafe impl Model for $ty {
        fn desc() -> Desc {
          structure::<Self>(vec![
            (offset_of!(Self, flag), $flag),
            (offset_of!(Self, next), pointer(true, Aim::One, Self::desc)),
          ])
        }

        fn corpus() -> Vec<Region<Self>> {
          // SAFETY: The closure makes a value whose only pointer is `next`, as `chains` requires.
          unsafe { chains(&$alphabet, |flag, next| $ty { flag, next }) }
        }

        fn show<R>(&self, r: &Region<R>) -> String {
          format!("{}({}, {})", stringify!($ty), self.flag, opt(r, &self.next))
        }
      }
    )+};
  }

  chained! {
    Flags: flag(), [false, true];
    Bytes: bytes(1), [0, 1, 2, 255];
  }

  /// A tree of values, as a value and its children.
  struct Node<V>(V, Vec<Node<V>>);

  /// Puts the subtrees of `node` in `b`, then returns the value that `make` makes of `node` and
  /// the sequence of its children.
  fn tree<T: Fix, V: Copy>(b: &mut Builder, node: &Node<V>, make: &impl Fn(V, Seq<T>) -> T) -> T {
    let children: Vec<T> = node.1.iter().map(|child| tree(b, child, make)).collect();
    let rest = b.seq(children);
    make(node.0, rest)
  }

  /// Returns regions of trees over `alphabet`, of one to four levels.
  ///
  /// # Safety
  ///
  /// The caller must ensure that:
  ///
  /// - `make` makes a value whose only pointer is the one that it receives.
  unsafe fn lists<T: Fix, V: Copy>(
    alphabet: &[V],
    make: impl Fn(V, Seq<T>) -> T,
  ) -> Vec<Region<T>> {
    let leaf = |value| Node(value, vec![]);
    let mut corpus = vec![];
    for &first in alphabet {
      let mut shapes = vec![leaf(first)];
      for &second in alphabet {
        shapes.push(Node(first, vec![leaf(second)]));
        shapes.push(Node(first, vec![leaf(second), leaf(first)]));
        let deep = Node(second, vec![leaf(first), Node(first, vec![leaf(second)])]);
        shapes.push(Node(first, vec![leaf(second), deep]));
      }
      // SAFETY: `tree` makes each sequence with `b.seq`, of the values that it holds, and puts it
      // in the one value that `make` makes of it, which holds no other pointer, by contract on the
      // caller. These are the requirements of `region`.
      corpus.extend(shapes.iter().map(|shape| unsafe { region(|b| tree(b, shape, &make)) }));
    }
    corpus
  }

  /// A recursive struct of a `bool` and of a sequence of other `FlagList`s.
  #[derive(Pun)]
  #[repr(C)]
  pub struct FlagList {
    pub flag: bool,
    pub rest: Seq<FlagList>,
  }

  /// A recursive struct like `FlagList`, with a `u8` in place of the `bool`.
  #[derive(Pun)]
  #[repr(C)]
  pub struct ByteList {
    pub flag: u8,
    pub rest: Seq<ByteList>,
  }

  /// Implements `Fix` and [`Model`] for structs of a value and of a sequence of values of the same
  /// type.
  macro_rules! listed {
    ($($ty:ident: $flag:expr, $alphabet:expr;)+) => {$(
      // SAFETY: `fix` calls `field` on the one field that holds a pointer, which fixes it at its
      // position, and changes nothing else, as the `# Safety` of `Fix` requires.
      unsafe impl Fix for $ty {
        fn fix(&mut self, at: usize) {
          let whole = addr(self);
          field(whole, at, &mut self.rest);
        }
      }

      // SAFETY: `desc` gives each field of the struct at its offset, with the description of its
      // type, and the pointer aims at a sequence of `$ty`s, as the `# Safety` of `Model` requires.
      unsafe impl Model for $ty {
        fn desc() -> Desc {
          structure::<Self>(vec![
            (offset_of!(Self, flag), $flag),
            (offset_of!(Self, rest), pointer(false, Aim::Seq, Self::desc)),
          ])
        }

        fn corpus() -> Vec<Region<Self>> {
          // SAFETY: The closure makes a value whose only pointer is `rest`, as `lists` requires.
          unsafe { lists(&$alphabet, |flag, rest| $ty { flag, rest }) }
        }

        fn show<R>(&self, r: &Region<R>) -> String {
          format!("{}({}, {})", stringify!($ty), self.flag, seq(r, &self.rest))
        }
      }
    )+};
  }

  listed! {
    FlagList: flag(), [false, true];
    ByteList: bytes(1), [0, 1, 2, 255];
  }

  /// Declares structs that are one pointer, and implements `Fix` and [`Model`] for them.
  ///
  /// For each struct, the macro takes the type of the pointer, its description, how a value is
  /// made from what it aims at, how it is shown, and the values that it aims at.
  macro_rules! holders {
    ($($(#[$doc:meta])* $ty:ident($pointer:ty): $desc:expr; $build:expr; $show:expr; $values:expr;)+) => {$(
      $(#[$doc])*
      #[derive(Pun)]
      #[repr(C)]
      pub struct $ty {
        pub to: $pointer,
      }

      // SAFETY: `fix` calls `field` on the one field that holds a pointer, which fixes it at its
      // position, and changes nothing else, as the `# Safety` of `Fix` requires.
      unsafe impl Fix for $ty {
        fn fix(&mut self, at: usize) {
          let whole = addr(self);
          field(whole, at, &mut self.to);
        }
      }

      // SAFETY: `desc` gives the one field of the struct at its offset, with `$desc`, the
      // description of its pointer and of what it aims at, as the `# Safety` of `Model` requires.
      unsafe impl Model for $ty {
        fn desc() -> Desc {
          structure::<Self>(vec![(offset_of!(Self, to), $desc)])
        }

        fn corpus() -> Vec<Region<Self>> {
          // SAFETY: `$build` makes the one pointer of the value with `b`, with what it aims at,
          // as `region` requires.
          $values.into_iter().map(|value| unsafe { region(|b| $ty { to: $build(b, value) }) }).collect()
        }

        fn show<R>(&self, r: &Region<R>) -> String {
          format!("{}({})", stringify!($ty), $show(r, &self.to))
        }
      }
    )+};
  }

  holders! {
    /// A pointer to a `bool`.
    ToFlag(One<bool>): pointer(false, Aim::One, flag);
      |b: &mut Builder, value| b.one(value); one; [false, true];
    /// A pointer to a `u8`.
    ToByte(One<u8>): pointer(false, Aim::One, || bytes(1));
      |b: &mut Builder, value| b.one(value); one; [0u8, 1, 2, 255];
    /// A pointer to a `u8`, or none.
    MaybeByte(Opt<u8>): pointer(true, Aim::One, || bytes(1));
      |b: &mut Builder, value| b.opt(value); opt; [None, Some(0u8), Some(1), Some(2), Some(255)];
    /// A pointer to a `u8`, whose offset is big-endian.
    ToByteBe(One<u8, i32be>): Desc::Pointer { big: true, nullable: false, aim: Aim::One, target: || bytes(1) };
      |b: &mut Builder, value| b.one(value); big_endian; [0u8, 1, 2, 255];
    /// A pointer to a little-endian `u16`.
    ToWord(One<u16le>): pointer(false, Aim::One, || bytes(2));
      |b: &mut Builder, value| b.one(u16le::new(value)); one;
      [0u16, 1, 0x0100, 0x0201, 0xffff];
    /// A pointer to two bytes.
    ToPair(One<[u8; 2]>): pointer(false, Aim::One, || bytes(2));
      |b: &mut Builder, value| b.one(value); one; [[0u8, 0], [1, 0], [0, 1], [2, 1], [255, 255]];
    /// A pointer to a sequence of bytes.
    ToBytes(Seq<u8>): pointer(false, Aim::Seq, || bytes(1));
      |b: &mut Builder, value: &[u8]| b.seq(value.iter().copied()); seq;
      [&[][..], &[0][..], &[1, 2][..], &[255, 0, 1][..], &[97][..]];
    /// A pointer to a sequence of `bool`s.
    ToFlags(Seq<bool>): pointer(false, Aim::Seq, flag);
      |b: &mut Builder, value: &[bool]| b.seq(value.iter().copied()); seq;
      [&[][..], &[true][..], &[false, true][..]];
    /// A pointer to text.
    ToText(Text): pointer(false, Aim::Text, || bytes(1));
      |b: &mut Builder, value| b.text(value); text; ["", "a", "\u{e9}", "ab"];
  }

  /// Four bytes, as many as a pointer, that are not a pointer.
  #[derive(Pun)]
  #[repr(C)]
  pub struct Raw {
    pub raw: [u8; 4],
  }

  // SAFETY: The type holds no pointer, and `fix` changes nothing, as the `# Safety` of `Fix`
  // requires.
  unsafe impl Fix for Raw {
    fn fix(&mut self, _: usize) {}
  }

  // SAFETY: `desc` gives the one field of the struct at its offset, 4 initialized bytes, as the
  // `# Safety` of `Model` requires.
  unsafe impl Model for Raw {
    fn desc() -> Desc {
      structure::<Self>(vec![(offset_of!(Self, raw), bytes(4))])
    }

    fn corpus() -> Vec<Region<Self>> {
      [[0; 4], [1, 0, 0, 0], [4, 0, 0, 0], [0, 0, 0, 4], [0xff; 4], [8, 0, 0, 0]]
        .into_iter()
        // SAFETY: A `Raw` holds no pointer, as `region` requires.
        .map(|raw| unsafe { region(|_| Raw { raw }) })
        .collect()
    }

    fn show<R>(&self, _: &Region<R>) -> String {
      format!("Raw({:?})", self.raw)
    }
  }

  /// An enum whose variants hold a `bool`, or a pointer to another `Tree`.
  #[derive(Pun)]
  #[repr(u8)]
  pub enum Tree {
    Leaf(bool),
    Node(One<Tree>),
  }

  // SAFETY: `fix` calls `field` on the pointer of the variant that holds one, which fixes it
  // at its position, and changes nothing else, as the `# Safety` of `Fix` requires.
  unsafe impl Fix for Tree {
    fn fix(&mut self, at: usize) {
      let whole = addr(self);
      if let Self::Node(node) = self {
        field(whole, at, node);
      }
    }
  }

  // SAFETY: `desc` gives the tag of each variant, a `u8` at offset 0 since the enum is `repr(u8)`,
  // and its field at offset 1, after the tag: a `bool`, or a pointer to another `Tree`, as the
  // `# Safety` of `Model` requires.
  unsafe impl Model for Tree {
    fn desc() -> Desc {
      Desc::Enum {
        size: size_of::<Self>(),
        align: align_of::<Self>(),
        variants: vec![
          (0, vec![(1, flag())]),
          (1, vec![(1, pointer(false, Aim::One, Self::desc))]),
        ],
      }
    }

    fn corpus() -> Vec<Region<Self>> {
      let mut corpus = vec![];
      for value in [false, true] {
        // SAFETY: Each closure makes every pointer with `b.one`, of the value that it aims at, and
        // puts it in one value, as `region` requires.
        unsafe {
          corpus.push(region(|_| Tree::Leaf(value)));
          corpus.push(region(|b| Tree::Node(b.one(Tree::Leaf(value)))));
          corpus.push(region(|b| {
            let leaf = b.one(Tree::Leaf(value));
            Tree::Node(b.one(Tree::Node(leaf)))
          }));
        }
      }
      corpus
    }

    fn show<R>(&self, r: &Region<R>) -> String {
      match self {
        Self::Leaf(value) => format!("Leaf({value})"),
        Self::Node(node) => format!("Node({})", one(r, node)),
      }
    }
  }

  /// An enum like `Tree`, with a `u8` in place of the `bool`, and a third variant that holds a
  /// sequence of `Wood`s.
  #[derive(Pun)]
  #[repr(u8)]
  pub enum Wood {
    Leaf(u8),
    Node(One<Wood>),
    Pair(Seq<Wood>),
  }

  // SAFETY: `fix` calls `field` on the pointer of the variant that holds one, which fixes it
  // at its position, and changes nothing else, as the `# Safety` of `Fix` requires.
  unsafe impl Fix for Wood {
    fn fix(&mut self, at: usize) {
      let whole = addr(self);
      match self {
        Self::Leaf(_) => {}
        Self::Node(node) => field(whole, at, node),
        Self::Pair(pair) => field(whole, at, pair),
      }
    }
  }

  // SAFETY: `desc` gives the tag of each variant, a `u8` at offset 0 since the enum is `repr(u8)`,
  // and its field at offset 1, after the tag: a `u8`, a pointer to another `Wood`, or a pointer
  // to a sequence of them, as the `# Safety` of `Model` requires.
  unsafe impl Model for Wood {
    fn desc() -> Desc {
      Desc::Enum {
        size: size_of::<Self>(),
        align: align_of::<Self>(),
        variants: vec![
          (0, vec![(1, bytes(1))]),
          (1, vec![(1, pointer(false, Aim::One, Self::desc))]),
          (2, vec![(1, pointer(false, Aim::Seq, Self::desc))]),
        ],
      }
    }

    fn corpus() -> Vec<Region<Self>> {
      let mut corpus = vec![];
      for value in [0, 1, 2, 255] {
        // SAFETY: Each closure makes every pointer with `b.one` or `b.seq`, of the values that it
        // aims at, and puts it in one value, as `region` requires.
        unsafe {
          corpus.push(region(|_| Wood::Leaf(value)));
          corpus.push(region(|b| Wood::Node(b.one(Wood::Leaf(value)))));
          corpus.push(region(|b| {
            let leaf = b.one(Wood::Leaf(value));
            Wood::Node(b.one(Wood::Node(leaf)))
          }));
          corpus.push(region(|b| Wood::Pair(b.seq([Wood::Leaf(value), Wood::Leaf(1)]))));
          corpus.push(region(|b| {
            let (leaf, empty) = (b.one(Wood::Leaf(value)), b.seq([]));
            Wood::Pair(b.seq([Wood::Node(leaf), Wood::Pair(empty)]))
          }));
        }
      }
      corpus
    }

    fn show<R>(&self, r: &Region<R>) -> String {
      match self {
        Self::Leaf(value) => format!("Leaf({value})"),
        Self::Node(node) => format!("Node({})", one(r, node)),
        Self::Pair(pair) => format!("Pair({})", seq(r, pair)),
      }
    }
  }
}
