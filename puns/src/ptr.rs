//! Raw pointers to sized and to slice-tailed types, split and made in `const fn`.
//!
//! Stable Rust cannot split a `*const T` into an address and a length, or make one from them,
//! when `T` is a generic `?Sized` type. `core::ptr::metadata` is unstable, and `as` casts between
//! wide pointers compile only for concrete types.
//!
//! This module reinterprets a `*const T` as a `*const [u8]`. This assumes that a pointer to a
//! slice-tailed type is represented as a pointer to a slice, whose length is the length of the
//! tail. The compiler does so, but Rust does not guarantee it. So the layout of each unsized type
//! is made with [`verify`], which checks the assumption at compile time against a pointer made
//! with `as` casts (`Pun::PROBE`). If the representation changes, the crate fails to build
//! instead of misbehaving.
//!
//! # Miri
//!
//! CI runs the tests under Miri with `-Zmiri-strict-provenance`, with tree borrows on x86-64, and
//! with stacked borrows on `i686-unknown-linux-gnu` and `s390x-unknown-linux-gnu`. The i686 target
//! has a 32-bit `usize` and a `u64` aligned to 4. The s390x target is big-endian. The selection of
//! the model check for Miri runs on x86-64 only, with tree borrows.
//!
//! `-Zmiri-symbolic-alignment-check` reports a misaligned reference where `try_pun_ref` checked the
//! address of bytes that happened to be aligned. The check reinterprets the address as an integer
//! ([`is_aligned`]), which the symbolic alignment of Miri cannot follow. The stable `align_offset`
//! is not a `const fn`, so it cannot replace that check. CI does not use that flag.

use crate::Pun;

/// The length of the slice tail of `Pun::PROBE`, for an unsized type.
#[doc(hidden)]
pub const PROBE_LEN: usize = 3;

// Reinterprets the bits of an `A` as a `B`. The union is `repr(C)`, so both fields are at offset 0
// [1], and reading `to` reads the first `size_of::<B>()` bytes of `from` as a `B` [2]. Each use
// reads a `B` of the size of `A`. A `B` is a raw pointer or a `usize`, which is valid if its bytes
// are initialized [3].
//
// [1] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.repr.c.union.size-align:
//
//     Each field lives at byte offset 0 from the beginning of the union.
//
// [2] Per https://doc.rust-lang.org/1.99.0/reference/items/unions.html#r-items.union.fields.read:
//
//     Reading a union field reads the bits of the union at the field's type.
//
// [3] Per https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.scalar:
//
//     An integer (`i*`/`u*`), floating point value (`f*`), or raw pointer must be initialized,
//     i.e., must not be obtained from uninitialized memory.
#[repr(C)]
union Cast<A: Copy, B: Copy> {
  from: A,
  to: B,
}

/// Returns the length of the slice tail of `ptr`.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `T` is a slice, `str`, or a struct whose last field is one of these, at any depth.
/// - Either a `*const T` is represented as a `*const [u8]`, whose length is the length of the tail,
///   which holds if [`verify`] accepted a probe of `T`. Or the caller is `verify`, a `*const T` has
///   the size of a `*const [u8]`, and every byte of it is initialized.
#[inline(always)]
pub const unsafe fn raw_len<T: ?Sized>(ptr: *const T) -> usize {
  // SAFETY: The read reinterprets the bytes of `ptr` as a `*const [u8]`, which is valid if its
  // bytes are initialized and of its size (see `Cast`). Both cases of the contract give this.
  // Either a `*const T` is represented as a `*const [u8]`, or the caller is `verify`, a `*const T`
  // has the size of a `*const [u8]`, and every byte of `ptr` is initialized. In the second case,
  // the length read need not be the length of the tail, which `verify` checks.
  unsafe { Cast::<*const T, *const [u8]> { from: ptr }.to }.len()
}

/// Returns a pointer to the address of `ptr`, whose slice tail has `len` elements.
///
/// # Safety
///
/// The caller must ensure the requirements of [`raw_len`].
#[inline(always)]
pub const unsafe fn raw_with_len<T: ?Sized>(ptr: *const T, len: usize) -> *const T {
  let slice = core::ptr::slice_from_raw_parts(ptr.cast::<u8>(), len);
  // SAFETY: `slice_from_raw_parts` made `slice`, so its bytes are initialized. The read
  // reinterprets them as a `*const T`, a raw pointer, which is valid if its bytes are initialized
  // and of its size (see `Cast`). Both cases of the contract give a `*const T` the size of a
  // `*const [u8]`. In the second case, the caller is `verify`, which checks that the result has
  // the address of `ptr` and the tail length `len`.
  unsafe { Cast::<*const [u8], *const T> { from: slice }.to }
}

/// Asserts that pointers to `T` are represented as pointers to slices.
///
/// It compares `probe` with what [`raw_len`] reads of it and with what [`raw_with_len`] makes of
/// it: the size and the alignment of the pointer, the length of the tail, and the address.
///
/// # Panics
///
/// Panics if the representation differs. In const evaluation, the panic is a compile error.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `T` is a slice, `str`, or a struct whose last field is one of these, at any depth.
/// - `probe` is `core::ptr::slice_from_raw_parts(core::ptr::null(), PROBE_LEN)`, cast to
///   `*const T` with `as` casts only.
pub const unsafe fn verify<T: ?Sized>(probe: *const T) {
  const MESSAGE: &str = "unsupported compiler: unexpected wide pointer representation";
  assert!(size_of::<*const T>() == size_of::<*const [u8]>(), "{}", MESSAGE);
  assert!(align_of::<*const T>() == align_of::<*const [u8]>(), "{}", MESSAGE);
  // SAFETY: By contract on the caller, `T` is a slice, `str`, or a struct whose last field is one
  // of these. The call is made by `verify`, as `raw_len` and `raw_with_len` require: the pointers
  // have the same size, which is asserted above, and the assertions below check the rest of the
  // representation. If a byte of `*const T` were not initialized, the read would be undefined
  // behavior. In the evaluation of a `LAYOUT` constant, which calls `verify`, `rustc` reports such
  // a read as an error (`discover`, module documentation). Rust does not promise that a pointer
  // has no uninitialized byte, so a call at run time relies on the representation that `rustc`
  // gives pointers, which has none.
  unsafe {
    assert!(raw_len(probe) == PROBE_LEN, "{}", MESSAGE);
    let other = raw_with_len(probe, PROBE_LEN + 39);
    assert!(raw_len(other) == PROBE_LEN + 39, "{}", MESSAGE);
    // `other` and `probe` must have the same (null) address.
    assert!(other.cast::<u8>().is_null() && probe.cast::<u8>().is_null(), "{}", MESSAGE);
  }
}

/// Returns the length of the slice tail of `ptr`, or 0 if `T` is `Sized`.
#[inline(always)]
pub const fn metadata<T: ?Sized + Pun>(ptr: *const T) -> usize {
  if const { !T::LAYOUT.unsized_ } {
    return 0;
  }
  // SAFETY: `T::LAYOUT.unsized_` is `true`. Only `Layout::slice`, `Layout::pattern` and
  // `Layout::structure` of a struct whose last field is unsized make such a layout, and each
  // calls `verify` on the probe of the type. By the `# Safety` of `Pun`, `T::LAYOUT` is the
  // layout of `T`. `ManuallyDrop<T>` reuses the layout of `T`, is a struct whose only field is a
  // `T`, and its `LAYOUT` calls `verify` on its own probe. So `T` is a slice, `str`, or a struct
  // whose last field is one of these, and `verify` accepted a probe of `T`. These are the
  // requirements of `raw_len`.
  unsafe { raw_len(ptr) }
}

/// Returns a pointer to a `T` at `addr`, whose slice tail has `len` elements.
///
/// `len` is ignored if `T` is `Sized`.
#[inline(always)]
pub const fn from_raw_parts<T: ?Sized + Pun>(addr: *const u8, len: usize) -> *const T {
  if const { T::LAYOUT.unsized_ } {
    let slice = core::ptr::slice_from_raw_parts(addr, len);
    // SAFETY: `T::LAYOUT.unsized_` is `true`, so `verify` accepted a probe of `T`, as in
    // `metadata`: a `*const T` is represented as a `*const [u8]`. So the bytes of `slice`, which
    // are initialized, are a `*const T` of the same size (see `Cast`).
    unsafe { Cast::<*const [u8], *const T> { from: slice }.to }
  } else {
    // SAFETY: `T::LAYOUT.unsized_` is `false`, so `T` is `Sized`, by the `# Safety` of `Pun`. A
    // pointer to a sized type has the size of a `usize` [1], as `addr` does. So the bytes of
    // `addr`, which are initialized, are a `*const T` of the same size (see `Cast`).
    //
    // [1] Per https://doc.rust-lang.org/1.99.0/reference/type-layout.html#r-layout.pointer.thin:
    //
    //     Pointers to sized types have the same size and alignment as `usize`.
    unsafe { Cast::<*const u8, *const T> { from: addr }.to }
  }
}

/// Returns a pointer to the field of type `F` at `offset` in the `T` at `ptr`.
///
/// An unsized field is the last field of `T`, so its tail has the length of the tail of `ptr`.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `ptr` points to the start of a `T`, all of whose bytes are in one allocation. If `T` is
///   unsized, the length of the tail of `ptr` is the length of the tail of that `T`. The bytes
///   need not be initialized or a valid `T`.
/// - `T` has a field of type `F` at `offset`.
#[inline(always)]
pub const unsafe fn field<T: ?Sized + Pun, F: ?Sized + Pun>(
  ptr: *const T,
  offset: usize,
) -> *const F {
  // An unsized field is the last one: it shares the slice length of its parent.
  let len = if const { F::LAYOUT.unsized_ } { metadata(ptr) } else { 0 };
  // SAFETY: This satisfies the safety contract of `add` [1]. By contract on the caller, `ptr`
  // points to the start of a `T` in one allocation, and `T` has a field at `offset`. So `offset`
  // is at most the size of that `T`, and `ptr..ptr + offset` is in bounds of the allocation.
  //
  // [1] Per https://doc.rust-lang.org/1.99.0/core/primitive.pointer.html#method.add:
  //
  //     If the computed offset is non-zero, then `self` must be derived from a pointer to some
  //     allocation, and the entire memory range between `self` and `result` [...] must be in
  //     bounds of that allocation.
  //
  //     Allocations can never be larger than `isize::MAX` bytes [...], so technically the last
  //     condition implies the first two.
  from_raw_parts(unsafe { ptr.cast::<u8>().add(offset) }, len)
}

/// The size of [`BUFFER`]: the largest head of a type whose fields [`in_buffer`] lets the derive
/// measure.
pub const BUFFER_SIZE: usize = 4096;

/// Memory into which `#[derive(Pun)]` projects pointers to a type, to measure the offsets of its
/// fields on the type as compiled (`Layout::measure_base`). Nothing reads it or writes it: a
/// projection with `&raw const` makes a pointer without an access. It is a promoted constant, so
/// it is an allocation of const evaluation only.
const BUFFER: &[core::mem::MaybeUninit<u64>; BUFFER_SIZE / 8] =
  &[core::mem::MaybeUninit::uninit(); BUFFER_SIZE / 8];

/// Returns a pointer to a `T` at the start of [`BUFFER`], whose slice tail, if any, has no element,
/// or a null pointer if such a `T` is larger than [`BUFFER_SIZE`].
///
/// Every place of that `T` is in the allocation of `BUFFER`, so `&raw const (*ptr).field` and
/// `byte_offset_from` on the result are defined, in const evaluation as at run time.
///
/// # Safety
///
/// The caller must ensure that either `T` is `Sized`, or `T` is a struct whose last field is a
/// slice, `str`, or a struct whose last field is one of these, at any depth, and `probe` meets the
/// requirements of [`verify`] for `T`.
pub const unsafe fn in_buffer<T: ?Sized>(probe: *const T) -> *const T {
  let addr = BUFFER.as_ptr().cast::<u8>();
  let thin = size_of::<*const T>() == size_of::<*const u8>();
  if thin {
    // A thin pointer: `T` is `Sized`, by contract on the caller, since a slice-tailed type has
    // wide pointers.
    // SAFETY: `T` is `Sized`, so `size_of_val_raw` is always safe to call [1].
    //
    // [1] Per https://doc.rust-lang.org/1.99.0/core/mem/fn.size_of_val_raw.html:
    //
    //     If `T` is `Sized`, this function is always safe to call.
    let fits = unsafe { core::mem::size_of_val_raw(probe) } <= BUFFER_SIZE;
    let addr = if fits { addr } else { core::ptr::null() };
    // SAFETY: A `*const T` has the size of a `*const u8`, which is checked above, and the bytes
    // of `addr`, a pointer, are initialized, so they are a valid `*const T` (see `Cast`).
    unsafe { Cast::<*const u8, *const T> { from: addr }.to }
  } else {
    // SAFETY: `T` is unsized, so by contract on the caller, it is slice-tailed and `probe` meets
    // the requirements of `verify`.
    unsafe { verify(probe) };
    // SAFETY: `verify` accepted `probe`, so a `*const T` is represented as a `*const [u8]`, as
    // `raw_with_len` requires. A `T` whose tail has no element is not larger than one with
    // `PROBE_LEN` elements, which fits in an `isize` by contract on the caller of `verify`, as
    // `size_of_val_raw` requires [1].
    //
    // [1] Per https://doc.rust-lang.org/1.99.0/core/mem/fn.size_of_val_raw.html:
    //
    //     If the unsized tail of `T` is: a slice `[U]`, `str`, or a trait object
    //     `dyn Trait`, then the size of the entire value (dynamic tail length + statically
    //     sized prefix) must fit in `isize`.
    let fits = unsafe { core::mem::size_of_val_raw(raw_with_len(probe, 0)) } <= BUFFER_SIZE;
    let slice = core::ptr::slice_from_raw_parts(if fits { addr } else { core::ptr::null() }, 0);
    // SAFETY: `verify` accepted `probe`, so a `*const T` is represented as a `*const [u8]` whose
    // length is the length of the tail. So the bytes of `slice`, which are initialized, are a
    // `*const T` at the address of `slice` whose tail has no element (see `Cast`).
    unsafe { Cast::<*const [u8], *const T> { from: slice }.to }
  }
}

/// Returns whether `addr` is a multiple of `align`.
///
/// `align` must be a power of two, or the result is meaningless. The address is reinterpreted as an
/// integer, which const evaluation cannot do for a pointer to memory: such a call evaluated at
/// compile time does not compile. A pointer made from an integer is reinterpreted as that integer.
///
/// The address is read through a union, not with `addr`, `is_aligned_to` or `align_offset`: none
/// of these is a `const fn` on Rust 1.99.
#[inline(always)]
pub const fn is_aligned(addr: *const u8, align: usize) -> bool {
  // SAFETY: A pointer to a sized type has the size of a `usize` (see `from_raw_parts`). So the
  // bytes of `addr`, which are initialized, are a `usize` of the same size (see `Cast`). This
  // reinterprets a pointer as an integer, which `core` says to avoid at run time [1], since its
  // semantics are not settled. The quoted text does not make it undefined behavior there, but no
  // documentation promises that it reads the address either, so `puns` relies on it, as the chapter
  // What `puns` relies on of the guide discloses. The integer is never made into a pointer again.
  // In const evaluation, `core` requires the pointer to have been made from an integer [1], which
  // is not established. `core` also says that using the integer stops the evaluation [1], so such a
  // call evaluated at compile time does not compile.
  //
  // [1] Per https://doc.rust-lang.org/1.99.0/core/mem/fn.transmute.html#transmutation-between-pointers-and-integers:
  //
  //     Transmuting pointers to integers in a const context is undefined behavior, unless the
  //     pointer was originally created from an integer. (That includes [...] semantically-
  //     equivalent conversions such as punning through `repr(C)` union fields.) Any attempt to
  //     use the resulting value for integer operations will abort const-evaluation. (And even
  //     outside `const`, such transmutation is touching on many unspecified aspects of the Rust
  //     memory model and should be avoided. [...])
  let addr = unsafe { Cast::<*const u8, usize> { from: addr }.to };
  addr & (align - 1) == 0
}
