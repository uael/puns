//! Self-relative pointers for the tests, and a builder of the values that hold them.
//!
//! The pointers are declared through the public API of `puns` only ([`puns::SelfRelative`] and
//! [`puns::self_relative!`]), as another crate would declare them.
//!
//! In the scheme [`SCHEME`], a pointer is an offset of type `O` from the address of the pointer.
//! `O` is a signed integer of a given byte order ([`Offset`]). It is one of the numbers of
//! `puns::num`, which are byte arrays without alignment. The offset leads to the following:
//!
//! - For [`One<T, O>`], one `T`, at an address aligned for it. [`Opt<T, O>`] is the same, or null
//!   if the offset is zero.
//! - For [`Seq<T, O>`], the first element of a sequence of `T`, at an address that is a multiple
//!   of the larger of the alignment of `T` and the width of `O`. The length, an `O`, is right
//!   before that element.
//! - For [`Text<O>`], the first byte of UTF-8 text. The length, an `O`, is right before that byte.
//!
//! A pointer is followed through the [`Region`] that holds it ([`Region::get`], [`Region::opt`],
//! [`Region::seq`], [`Region::text`]). The read uses the provenance of the whole buffer, never the
//! address of the pointer. What is read through a view is therefore sound under stacked borrows as
//! well as under tree borrows. Following a pointer that is not in the buffer of the region panics.
//!
//! What the unsafe code here assumes of the target is asserted where it is used. What a buffer
//! holds is aligned to at most [`ALIGN`]. An offset is converted to and from `isize` by checked
//! conversions, as for an `i64le` on a 32-bit target, and it has its own byte order.

use core::{fmt, marker::PhantomData, mem::MaybeUninit, ptr};

use puns::{
  Pun, SelfRelative,
  layout::{Endian, Pointer},
  num::{i16be, i16le, i32be, i32le, i64be, i64le},
};

/// The name of the scheme of these pointers.
pub const SCHEME: &str = "puns tests: an offset to the target, a length before a sequence";

/// The alignment of a buffer, which is the largest alignment of what is put in one.
pub const ALIGN: usize = 16;

/// The offset of a pointer, a signed integer of a given byte order.
pub trait Offset: Pun + Copy + 'static {
  /// The byte order of the offset.
  const ENDIAN: Endian;

  /// Returns the offset whose value is `n`.
  ///
  /// # Panics
  ///
  /// Panics if the offset cannot hold `n`.
  fn new(n: isize) -> Self;

  /// Returns the value of the offset.
  ///
  /// # Panics
  ///
  /// Panics if an `isize` cannot hold the value.
  fn get(self) -> isize;
}

macro_rules! offsets {
  ($($name:ident: $raw:ident $endian:ident;)+) => {$(
    impl Offset for $name {
      const ENDIAN: Endian = Endian::$endian;

      fn new(n: isize) -> Self {
        Self::new($raw::try_from(n).expect("an offset that holds the distance"))
      }

      fn get(self) -> isize {
        isize::try_from(self.get()).expect("an offset that an `isize` holds")
      }
    }
  )+};
}

offsets! {
  i16le: i16 Little;
  i16be: i16 Big;
  i32le: i32 Little;
  i32be: i32 Big;
  i64le: i64 Little;
  i64be: i64 Big;
}

/// An offset of four bytes in little-endian byte order that is never zero.
///
/// Its zero is a niche, as the zero of a `NonZero` is. An `Option` of a pointer with this offset
/// is therefore the same pointer, and `None` is the pointer of zero bytes
/// (`puns::layout::Layout::nullable`, which `self_relative!` gives `Option`). The offset is packed,
/// so it has no alignment, as the other offsets, and packing keeps the niche. It is for [`One`]
/// only, because a length can be zero.
#[derive(Clone, Copy, Pun)]
#[repr(C, packed)]
pub struct Nz32le(core::num::NonZero<u32>);

impl Offset for Nz32le {
  const ENDIAN: Endian = Endian::Little;

  fn new(n: isize) -> Self {
    let n = i32::try_from(n).expect("an offset that holds the distance");
    // The bytes of `n` in little-endian order, as the bytes of the `u32`.
    Self(core::num::NonZero::new(u32::from_ne_bytes(n.to_le_bytes())).expect("never zero"))
  }

  fn get(self) -> isize {
    let Self(value) = self;
    i32::from_le_bytes(value.get().to_ne_bytes()) as isize
  }
}

macro_rules! pointer {
  ($(#[$doc:meta])* $name:ident [$($param:ident)?] $target:ty, $pointer:expr) => {
    $(#[$doc])*
    ///
    /// While a value is built, its offset holds the position of its target in the buffer.
    /// [`Fix::fix`] replaces that position with the distance.
    #[repr(transparent)]
    pub struct $name<$($param,)? O = i32le> {
      offset: O,
      target: PhantomData<($($param,)?)>,
    }

    // SAFETY: Each item of the `# Safety` of `SelfRelative` holds:
    // - The type is its offset: it is `repr(transparent)` over an `O`, a byte array or a packed
    //   `NonZero<u32>`, every byte of which is initialized, and none in an `UnsafeCell`.
    // - The scheme is the way to the target: where it aims, and how many elements or bytes, is
    //   the scheme `SCHEME` of this file, from its address, its offset, its kind of aim and the
    //   length before a sequence or a text, for every type of the scheme. Only this file
    //   declares types of `SCHEME`.
    // - What it aims at is there, valid, and its own: a value of this type in a buffer is made by a
    //   `Builder`, which `Region::new` turns into a region whose pointers aim at valid targets,
    //   aligned as `POINTER` says (the `# Safety` of `Region::new`, and `Builder::reserve`). The
    //   target of `One::here` is the zero bytes of its offset, which are a valid target by that `#
    //   Safety`, and which hold it at any address, since `here` asserts that the target is no
    //   larger than the offset and no more aligned than the pointer. A region never writes to its
    //   buffer, nor gives a `&mut` to any of it.
    // - Null: `Opt` is made with `or_null`, and `Region::opt` follows no pointer of offset zero.
    // - The target is the type: `POINTER` is made for `Target` by `Pointer::one`,
    //   `Pointer::sequence` or `Pointer::text`.
    // - The code of this file follows a pointer only through `Region::get`, `Region::opt`,
    //   `Region::seq` and `Region::text`, which rely on nothing more.
    unsafe impl<$($param: Pun,)? O: Offset> SelfRelative for $name<$($param,)? O> {
      type Target = $target;
      const POINTER: Pointer = $pointer;
    }

    puns::self_relative!(impl[$($param: Pun,)? O: Offset] $name<$($param,)? O>);

    impl<$($param,)? O: Offset> $name<$($param,)? O> {
      /// Returns a pointer that holds the position of its target, not yet the distance to it.
      ///
      /// # Panics
      ///
      /// Panics if an `isize` or an `O` cannot hold `position`.
      fn to(position: usize) -> Self {
        let position = isize::try_from(position).expect("a position that an `isize` holds");
        Self { offset: O::new(position), target: PhantomData }
      }
    }

    impl<$($param,)? O: Offset> fmt::Debug for $name<$($param,)? O> {
      fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({:+})", stringify!($name), self.offset.get())
      }
    }

    // SAFETY: `fix` calls nothing, and changes only the offset of the pointer, whose position is
    // `at`: from the position of its target to the distance to it. A null `Opt`, of offset zero,
    // is left as it is. This is what the `# Safety` of `Fix` requires of a pointer.
    unsafe impl<$($param,)? O: Offset> Fix for $name<$($param,)? O> {
      fn fix(&mut self, at: usize) {
        let target = self.offset.get();
        if target != 0 {
          self.offset = O::new(target - isize::try_from(at).expect("a position"));
        }
      }
    }
  };
}

pointer! {
  /// A pointer to one `T`.
  One[T] T, Pointer::one::<T>(SCHEME, size_of::<O>(), O::ENDIAN)
}

pointer! {
  /// A pointer to one `T` that may be null. Its offset is zero when it is null.
  Opt[T] T, Pointer::one::<T>(SCHEME, size_of::<O>(), O::ENDIAN).or_null()
}

pointer! {
  /// A pointer to a sequence of `T`.
  Seq[T] T, Pointer::sequence::<T>(SCHEME, size_of::<O>(), O::ENDIAN, first::<T, O>())
}

pointer! {
  /// A pointer to UTF-8 text.
  Text[] (), Pointer::text(SCHEME, size_of::<O>(), O::ENDIAN)
}

/// Returns the alignment of the first element of a sequence of `T` whose length is an `O`.
const fn first<T, O>() -> usize {
  if align_of::<T>() > size_of::<O>() { align_of::<T>() } else { size_of::<O>() }
}

impl<T, O: Offset> One<T, O> {
  /// Returns a pointer of offset zero, which aims at its own address, at the zero bytes of its
  /// offset.
  ///
  /// It is for a target of which zero bytes are a valid value, such as `u8`. That target is then
  /// there, as the `# Safety` section of `SelfRelative` requires. The pointer has an offset of
  /// zero but is not null. A pointer whose offset cannot be zero cannot be this pointer.
  ///
  /// # Compile-time errors
  ///
  /// A call compiles only if `T` is at most as large as the offset, and at most as aligned as the
  /// pointer, so that the bytes of the offset hold a `T` at any address of the pointer.
  #[must_use]
  pub fn here() -> Self {
    const {
      assert!(
        size_of::<T>() <= size_of::<O>() && align_of::<T>() <= align_of::<Self>(),
        "the offset of a pointer cannot hold its target"
      );
    }
    Self { offset: O::new(0), target: PhantomData }
  }
}

impl<T, O: Offset> Opt<T, O> {
  /// Returns a null pointer.
  #[must_use]
  pub fn none() -> Self {
    Self { offset: O::new(0), target: PhantomData }
  }
}

impl<T> Opt<T> {
  /// A null pointer, for a constant.
  pub const NONE: Self = Self { offset: i32le::new(0), target: PhantomData };
}

/// A value that can be put in a buffer.
///
/// Once the position of the value is known, the pointers that it holds are made to aim at the
/// positions that they hold.
///
/// # Safety
///
/// An implementation must guarantee that `fix` calls `Fix::fix` on every pointer of the value,
/// with the position of the pointer, and changes nothing else. Each type that holds pointers
/// implements this trait by hand.
pub unsafe trait Fix: Sized {
  /// Makes the pointers of the value aim at their targets, the value being at the position `at`.
  fn fix(&mut self, at: usize);
}

/// Returns the address of `value` as an integer. The difference of two addresses gives the offset
/// of a field in its value.
#[must_use]
pub fn addr<T: ?Sized>(value: &T) -> usize {
  ptr::from_ref(value).addr()
}

/// Fixes `field`, a field of the value at the position `at` whose address is `whole`.
pub fn field<F: Fix>(whole: usize, at: usize, field: &mut F) {
  let offset = addr(field) - whole;
  field.fix(at + offset);
}

macro_rules! plain {
  ($($ty:ty)+) => {$(
    // SAFETY: The type holds no pointer, and `fix` changes nothing, as the `# Safety` of `Fix`
    // requires.
    unsafe impl Fix for $ty {
      fn fix(&mut self, _: usize) {}
    }
  )+};
}

plain! {
  () bool char u8 u16 u32 u64 i8 i16 i32 i64 Nz32le
  puns::num::u16le puns::num::u32le puns::num::u64le puns::num::u16be
  i16le i32le i64le i16be i32be i64be
}

// SAFETY: `fix` calls `Fix::fix` on the value of `Some`, at its position: `at` plus its offset in
// the `Option`, which is not 0 if the `Option` has a tag before it. It changes nothing else, as
// the `# Safety` of `Fix` requires.
unsafe impl<T: Fix> Fix for Option<T> {
  fn fix(&mut self, at: usize) {
    let whole = addr(self);
    if let Some(value) = self {
      field(whole, at, value);
    }
  }
}

// SAFETY: `fix` calls `Fix::fix` on each element, at its position, `at + i * size_of::<T>()`, and
// changes nothing else. Each element fixes its own pointers, as the `# Safety` of `Fix` requires
// of `T`.
unsafe impl<T: Fix, const N: usize> Fix for [T; N] {
  fn fix(&mut self, at: usize) {
    for (i, element) in self.iter_mut().enumerate() {
      element.fix(at + i * size_of::<T>());
    }
  }
}

/// A block of a buffer, with the alignment of every buffer.
#[derive(Clone, Copy)]
#[repr(C, align(16))]
struct Block([MaybeUninit<u8>; ALIGN]);

const _: () = assert!(align_of::<Block>() == ALIGN && size_of::<Block>() == ALIGN);

/// A buffer that values are put in, each at a position aligned for it. What a value points to is
/// put before the value.
pub struct Builder {
  blocks: Vec<Block>,
  len: usize,
}

impl Default for Builder {
  fn default() -> Self {
    Self::new()
  }
}

impl Builder {
  /// Returns an empty buffer. Nothing is put at position 0, which is the position that a null
  /// `Opt` holds while a value is built.
  #[must_use]
  pub const fn new() -> Self {
    Self { blocks: Vec::new(), len: 1 }
  }

  /// Reserves `size` bytes at a position that is a multiple of `align`, with `before` more bytes
  /// before them, and returns that position.
  ///
  /// A byte that nothing writes follows the reserved bytes. A read past a value reads that byte,
  /// which Miri and the oracle of the model check report. A pointer is put after what it aims at,
  /// so it never aims at itself, even at an empty sequence. Its offset is therefore zero only if
  /// it is null.
  ///
  /// # Panics
  ///
  /// Panics if `align` is not a power of two or is larger than [`ALIGN`].
  fn reserve(&mut self, before: usize, size: usize, align: usize) -> usize {
    // What the buffer holds is at most as aligned as the buffer.
    assert!(align.is_power_of_two() && align <= ALIGN, "a buffer is aligned to {ALIGN}");
    let at = (self.len + before).next_multiple_of(align);
    self.len = at + size + 1;
    self.blocks.resize(self.len.div_ceil(ALIGN), Block([MaybeUninit::uninit(); ALIGN]));
    at
  }

  /// Returns the start of the buffer, for writes.
  const fn base(&mut self) -> *mut u8 {
    self.blocks.as_mut_ptr().cast()
  }

  /// Puts `value` in the buffer, fixes its pointers, and returns its position.
  fn put<T: Fix>(&mut self, mut value: T) -> usize {
    let at = self.reserve(0, size_of::<T>(), align_of::<T>());
    value.fix(at);
    // SAFETY: `reserve` resized the blocks to hold `at + size_of::<T>() + 1` bytes, so `add(at)`
    // stays in their allocation, and the `size_of::<T>()` bytes from there are valid for writes.
    // `base` comes from `as_mut_ptr` of the blocks, with the provenance of all of them. The
    // blocks are aligned to `ALIGN`, at least the alignment of `T`, which `reserve` asserts, and
    // `at` is a multiple of the alignment of `T`. These are the requirements of `add` and of
    // `write`.
    unsafe { self.base().add(at).cast::<T>().write(value) };
    at
  }

  /// Writes the length `len` right before the position `at`.
  ///
  /// # Panics
  ///
  /// Panics if an `O` cannot hold `len`.
  fn length<O: Offset>(&mut self, at: usize, len: usize) {
    let len = O::new(isize::try_from(len).expect("a length that an `isize` holds"));
    // SAFETY: `reserve` left `size_of::<O>()` bytes before `at`, so `at - size_of::<O>()..at` is
    // in the blocks, valid for writes. An `O` is a byte array or a packed struct, of alignment 1.
    // These are the requirements of `add` and of `write`.
    unsafe { self.base().add(at - size_of::<O>()).cast::<O>().write(len) };
  }

  /// Puts `value`, and returns a pointer to it.
  pub fn one<T: Fix, O: Offset>(&mut self, value: T) -> One<T, O> {
    One::to(self.put(value))
  }

  /// Puts `value`, if there is one, and returns a pointer to it, or a null pointer.
  pub fn opt<T: Fix, O: Offset>(&mut self, value: Option<T>) -> Opt<T, O> {
    value.map_or_else(Opt::none, |value| Opt::to(self.put(value)))
  }

  /// Puts the `values` one after the other, with their number before them, and returns a pointer
  /// to the first.
  pub fn seq<T: Fix, O: Offset>(&mut self, values: impl IntoIterator<Item = T>) -> Seq<T, O> {
    let values: Vec<T> = values.into_iter().collect();
    let size = values.len() * size_of::<T>();
    let at = self.reserve(size_of::<O>(), size, first::<T, O>());
    self.length::<O>(at, values.len());
    for (i, mut value) in values.into_iter().enumerate() {
      let place = at + i * size_of::<T>();
      value.fix(place);
      // SAFETY: `reserve` made room for `values.len()` elements from `at`, so `place` and the
      // `size_of::<T>()` bytes from it are in the blocks, valid for writes. `at` is a multiple of
      // `first::<T, O>()`, the alignment of `T` or the size of `O`, whichever is greater. Both are
      // powers of two (an offset is 2, 4 or 8 bytes), so `first::<T, O>()` is a multiple of the
      // alignment of `T`, and so is `at`, as `size_of::<T>()` is. So `place` is aligned for `T`.
      // These are the requirements of `add` and of `write`.
      unsafe { self.base().add(place).cast::<T>().write(value) };
    }
    Seq::to(at)
  }

  /// Puts the bytes of `text`, with their number before them, and returns a pointer to the first.
  pub fn text<O: Offset>(&mut self, text: &str) -> Text<O> {
    let at = self.reserve(size_of::<O>(), text.len(), 1);
    self.length::<O>(at, text.len());
    // SAFETY: This satisfies the safety contract of `copy_nonoverlapping`. `text` is valid for
    // reads of its `len()` bytes. `reserve` made room for them from `at`, valid for writes. A `u8`
    // is aligned at any address. `self` is borrowed exclusively, so `text` is not in its blocks.
    unsafe { ptr::copy_nonoverlapping(text.as_ptr(), self.base().add(at), text.len()) };
    Text::to(at)
  }
}

/// A buffer made by a [`Builder`], whose root is a `T`.
pub struct Region<T> {
  blocks: Vec<Block>,
  len: usize,
  root: usize,
  value: PhantomData<T>,
}

impl<T: Fix> Region<T> {
  /// Puts `root` in the buffer of `builder`, and returns the region of the value.
  ///
  /// # Safety
  ///
  /// The caller must ensure that:
  ///
  /// - Every pointer of `root`, and of what was put in `builder`, was made by `builder` (`one`,
  ///   `opt`, `seq` or `text`) with what it aims at, or is `One::here` of a target of which zero
  ///   bytes are a valid value. Such a target fits in the offset of the pointer, at an address
  ///   aligned for it, which `One::here` asserts.
  /// - Every such pointer is in one value only.
  /// - Every value that holds pointers implements [`Fix`] as its `# Safety` section requires.
  pub unsafe fn new(mut builder: Builder, root: T) -> Self {
    let root = builder.put(root);
    Self { blocks: builder.blocks, len: builder.len, root, value: PhantomData }
  }
}

impl<T> Region<T> {
  /// Returns the start of the buffer, with the provenance of all of it.
  const fn base(&self) -> *const u8 {
    self.blocks.as_ptr().cast()
  }

  /// Returns the bytes of the buffer, some of which are uninitialized (padding, gaps).
  #[must_use]
  pub const fn bytes(&self) -> &[MaybeUninit<u8>] {
    // SAFETY: This satisfies the safety contract of `slice::from_raw_parts`. A region is made by
    // `Region::new`, after `Builder::put`, so its blocks hold at least `len` bytes, valid for
    // reads, in one allocation. A `MaybeUninit<u8>` is valid whatever the byte, and aligned at
    // any address. A region never writes to its blocks, and the slice borrows `self`.
    unsafe { core::slice::from_raw_parts(self.base().cast(), self.len) }
  }

  /// Returns the position of the root.
  #[must_use]
  pub const fn root(&self) -> usize {
    self.root
  }

  /// Returns the position of `pointer` in the buffer, and the value of `offset`.
  ///
  /// # Panics
  ///
  /// Panics if `pointer` is not in the buffer, or if an `isize` cannot hold `offset`.
  fn place<P, O: Offset>(&self, pointer: &P, offset: O) -> (usize, isize) {
    let here = addr(pointer).wrapping_sub(self.base().addr());
    assert!(here + size_of::<P>() <= self.len, "a pointer of another buffer");
    (here, offset.get())
  }

  /// Returns the position of a target of `size` bytes at `offset` from the position `here`.
  ///
  /// # Panics
  ///
  /// Panics if the target is not in the buffer.
  fn aim(&self, here: usize, offset: isize, size: usize) -> usize {
    let to = here.checked_add_signed(offset).filter(|to| to + size <= self.len);
    to.expect("a pointer that aims in its buffer")
  }

  /// Returns the length that the `O` right before the position `to` holds.
  ///
  /// # Panics
  ///
  /// Panics if that `O` starts before the buffer, or if its value is not a length.
  fn length<O: Offset>(&self, to: usize) -> usize {
    let at = to.checked_sub(size_of::<O>()).expect("a length in the buffer");
    // SAFETY: `to` is in the buffer, which `aim` asserted, and `at` is before it, so `add(at)`
    // stays in the blocks. A pointer of a sequence or of a text aims after a length, an `O`,
    // which the builder wrote there (`Builder::length`), by the `# Safety` of `Region::new`. So
    // the bytes are an initialized `O`, of alignment 1, valid for reads.
    let len = unsafe { *self.base().add(at).cast::<O>() };
    usize::try_from(len.get()).expect("a length")
  }

  /// Returns what `pointer` aims at.
  ///
  /// # Panics
  ///
  /// Panics if the pointer is not in this region, or if what it aims at is not in the buffer.
  pub fn get<'a, U, O: Offset>(&'a self, pointer: &'a One<U, O>) -> &'a U {
    let (here, offset) = self.place(pointer, pointer.offset);
    let to = self.aim(here, offset, size_of::<U>());
    // SAFETY: `place` asserted that `pointer` is in the buffer of this region, and `aim` that its
    // target, `size_of::<U>()` bytes from `to`, is too. So `add(to)` stays in the blocks. The
    // pointer was made by the builder with its target, or is `One::here` of a target that its
    // offset holds at any address, by the `# Safety` of `Region::new`, or was reinterpreted as a
    // `One<U, O>` by `puns::view_ref`, which proves that what it aims at is a valid `U`. So the
    // bytes there are a valid `U`, aligned for it, by the requirement of `SelfRelative` that what
    // it aims at is there and valid (`SelfRelative`, section Safety). The region never writes to
    // its blocks, and the reference borrows `self`.
    unsafe { &*self.base().add(to).cast::<U>() }
  }

  /// Returns what `pointer`, an `Option` of a pointer, aims at, if it is `Some`.
  ///
  /// # Panics
  ///
  /// Panics if the pointer is not in this region, or if what it aims at is not in the buffer.
  #[must_use]
  pub fn option<'a, U, O: Offset>(&'a self, pointer: Option<&'a One<U, O>>) -> Option<&'a U> {
    pointer.map(|pointer| self.get(pointer))
  }

  /// Returns what `pointer` aims at, or `None` if it is null.
  ///
  /// # Panics
  ///
  /// Panics if the pointer is not in this region, or if what it aims at is not in the buffer.
  pub fn opt<'a, U, O: Offset>(&'a self, pointer: &'a Opt<U, O>) -> Option<&'a U> {
    let (here, offset) = self.place(pointer, pointer.offset);
    if offset == 0 {
      return None;
    }
    let to = self.aim(here, offset, size_of::<U>());
    // SAFETY: As in `get`: the offset is not zero, so the pointer is not null, and aims at a
    // valid `U` in the buffer.
    Some(unsafe { &*self.base().add(to).cast::<U>() })
  }

  /// Returns the sequence that `pointer` aims at.
  ///
  /// # Panics
  ///
  /// Panics if the pointer is not in this region, or if what it aims at is not in the buffer.
  pub fn seq<'a, U, O: Offset>(&'a self, pointer: &'a Seq<U, O>) -> &'a [U] {
    let (here, offset) = self.place(pointer, pointer.offset);
    let to = self.aim(here, offset, 0);
    let len = self.length::<O>(to);
    self.aim(to, 0, len * size_of::<U>());
    // SAFETY: As in `get`, the pointer aims at `len` valid `U`s from `to`, aligned for them, which
    // `aim` asserted are in the buffer. `len` is the length that the builder wrote, the number of
    // the values it put, so `len * size_of::<U>()` does not overflow. These are the requirements
    // of `slice::from_raw_parts`.
    unsafe { core::slice::from_raw_parts(self.base().add(to).cast::<U>(), len) }
  }

  /// Returns the text that `pointer` aims at.
  ///
  /// # Panics
  ///
  /// Panics if the pointer is not in this region, or if what it aims at is not in the buffer.
  pub fn text<'a, O: Offset>(&'a self, pointer: &'a Text<O>) -> &'a str {
    let (here, offset) = self.place(pointer, pointer.offset);
    let to = self.aim(here, offset, 0);
    let len = self.length::<O>(to);
    self.aim(to, 0, len);
    // SAFETY: As in `get`, the pointer aims at `len` bytes from `to`, which `aim` asserted are in
    // the buffer, as `slice::from_raw_parts` requires. The builder copied them from a `str`
    // (`Builder::text`), so they are UTF-8, as `from_utf8_unchecked` requires.
    unsafe { core::str::from_utf8_unchecked(core::slice::from_raw_parts(self.base().add(to), len)) }
  }
}

impl<T> core::ops::Deref for Region<T> {
  type Target = T;

  fn deref(&self) -> &T {
    // SAFETY: `Region::new` put the root with `Builder::put`, which wrote a `T` at `root`, in the
    // blocks and aligned for it. Its fix changed only offsets, which any bytes are, so it is a
    // valid `T`. The region never writes to its blocks, and the reference borrows `self`.
    unsafe { &*self.base().add(self.root).cast::<T>() }
  }
}
