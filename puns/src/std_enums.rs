//! The implementations of `Pun` for `Option<T>` and `Result<T, E>`, for any `T` and `E`.
//!
//! Both are enums of the `Rust` representation. Their layouts are found in the same way as the
//! layout of an enum of that representation that derives `Pun` ([`discover`](crate::discover)), for
//! any `T` and `E` whose layout is known. Where `core` documents the layout, as for an `Option` of
//! a `NonZero` (`Pun::NULL_OPTIMIZED`), it is a layout that Rust specifies. Otherwise it is a
//! layout that Rust leaves to the compiler.
//!
//! `Option<NonZero<u32>>` is a niche at 0, so every `u32` is one, proven both ways.
//! `Option<bool>` is the niche 2. `Option<Option<bool>>` has two niches, and `rustc` puts `None`
//! at 255 today, below the valid values rather than after them. This is found, not assumed.
//! `Option<u16>` has a tag of two bytes. `Result<u32, Infallible>` is a struct of its `u32`.
//!
//! An `Option` of an opaque scalar ([`Spec::opaque`](crate::layout::Spec::opaque)) is not found so.
//! Its layout is an opaque scalar of the same size ([`opaque_option`]), whose values are those of
//! the scalar and the bytes of `None`. Found as an enum, it would have the layout that Rust leaves
//! to the compiler, unless `core` documents it, which it does only for a `NonZero` and a
//! `repr(transparent)` struct around one.

use core::{marker::PhantomData, mem::MaybeUninit};

use crate::{
  At, Fields, Path, Pun, Variants,
  layout::{Desc, Enum, Field, Id, Kind, Layout, Spec, UNSTABLE, Valid, Variant},
};

/// Returns the offset of `field` in `whole`.
///
/// # Safety
///
/// The caller must ensure that:
///
/// - `field` is a field of the value that `whole` holds.
const unsafe fn offset<F, E>(field: &F, whole: &MaybeUninit<E>) -> usize {
  // SAFETY: This satisfies the safety contract of `byte_offset_from` [1]. `field` is a field of
  // the value in `whole`, by contract on the caller. So both pointers are derived from the
  // reference `whole`, and the bytes between them are in the allocation of `whole`. The distance
  // is a number of bytes, a multiple of the size of `u8`, and is not negative, so
  // `cast_unsigned` keeps it.
  //
  // [1] Per https://doc.rust-lang.org/1.99.0/core/primitive.pointer.html#method.offset_from:
  //
  //     `self` and `origin` must either point to the same address, or both be derived from a
  //     pointer to the same allocation, and the memory range between the two pointers must be
  //     in bounds of that object.
  unsafe { core::ptr::from_ref(field).byte_offset_from(whole.as_ptr()) }.cast_unsigned()
}

/// What the layout of `Option` and `Result` is found from, and what is found.
///
/// The constants are those that the derive writes for an enum of the `Rust` representation.
trait Found: Sized {
  /// A sample of each of the two variants. It is an uninitialized value for a variant without
  /// values.
  const SAMPLES: [MaybeUninit<Self>; 2];
  /// The offset of the field of each variant, or 0 if it has none.
  const OFFSETS: [usize; 2];
  /// What the experiments on the samples find.
  const DESC: Desc<2>;
  /// The layout of the tag.
  const TAG: Layout;
  /// The layouts of the two variants.
  const VARIANTS: [Variant; 2];
  /// The layout of the enum.
  const ENUM: Enum;
}

/// Makes the layout of the `variant`th variant of `E`.
///
/// With a `field` type, the variant has one field of that type, at the offset that `E::OFFSETS`
/// gives, unless the variant has no value. Without one, the variant has no field.
macro_rules! variant {
  ($e:ty, $variant:literal) => {
    Variant::discovered(
      $variant,
      const { &<$e as Found>::DESC }.variant_ranges($variant),
      <$e as Found>::DESC.holds_tag($variant),
      const { &Layout::variant::<$e>(&[]) },
    )
  };
  ($e:ty, $variant:literal, $field:ty) => {
    Variant::discovered(
      $variant,
      const { &<$e as Found>::DESC }.variant_ranges($variant),
      <$e as Found>::DESC.holds_tag($variant),
      const {
        &Layout::variant::<$e>({
          let fields: &'static [Field] = if <$e as Found>::DESC.inhabited($variant) {
            const {
              &Layout::sort_fields([Field::new(
                <$e as Found>::OFFSETS[$variant],
                &<$field as Pun>::LAYOUT,
              )])
            }
          } else {
            &[]
          };
          fields
        })
      },
    )
  };
}

/// The most ranges of values of an opaque scalar of which an `Option` is laid out
/// ([`opaque_option`]), one more for `None`.
const OPTION_RANGES: usize = 32;

/// Returns the layout of `Option<T>`, where `T` is an opaque scalar: an opaque scalar of the size
/// of `T`, whose values are the values of `T` and the bytes of `None`.
///
/// If `Option<T>` has the size of `T` and every byte of `None` is initialized, `Some(x)` holds `x`
/// in every byte, since there is no other byte for a tag. So a value of `Option<T>` is `None`, or
/// one of the values of `T`. Both conditions are checked here: the size by an assertion, and the
/// bytes of `None` by reading each of them in const evaluation ([`option_values`]), which reports an
/// error on an uninitialized byte. That error is a behavior of `rustc` that no documentation
/// states, and the crate documentation discloses it (section Soundness). `None` has no field, so
/// its bytes are what `rustc` writes for it, at compile time as at run time. `core` promises the
/// size where `T` is a `NonZero` integer, or a `repr(transparent)` struct around one
/// ("Representation" in `core::option`). For another `T`, the size depends on the niche that
/// `rustc` gives it.
///
/// Which bytes `None` has is what `rustc` chose, unless `core` documents it (`Pun::NULL_OPTIMIZED`:
/// zero), or `T` leaves out one value of its size: `None` is then that value, since it is no value
/// of `T` and there is no other. Without the `unstable` feature, another `T` is refused, so that
/// what a check accepts does not depend on that choice.
///
/// # Panics
///
/// Panics if `T` is not a scalar, if `Option<T>` is larger than `T`, if a byte of `None` is not
/// initialized, if `T` has more than `OPTION_RANGES - 1` ranges of values, or, without the
/// `unstable` feature, if `T` leaves out more than one value of its size and `core` does not
/// document its `Option`.
const fn opaque_option<T: Pun>() -> Layout {
  let Kind::Scalar(spec) = T::LAYOUT.kind else { panic!("not a scalar") };
  assert!(
    size_of::<Option<T>>() == size_of::<T>(),
    "an `Option` of an opaque scalar must have the size of the scalar: give the scalar a niche, \
     as a `NonZero` has"
  );
  assert!(
    UNSTABLE || T::NULL_OPTIMIZED || one_left_out(spec),
    "an `Option` of an opaque scalar that leaves out more than one value has the `None` that `rustc` \
     chooses: without the `unstable` feature, give the scalar every value but one"
  );
  // SAFETY: The call is in an inline `const` block, so it is evaluated at compile time, as
  // `option_values` requires.
  let values = const { &unsafe { option_values::<T>() } };
  let option = Spec::any::<Option<T>>().with_endian(spec.endian);
  let option =
    if values.1 == 0 { option } else { option.with_ranges(values.0.split_at(values.1).0) };
  let option = if spec.number { option } else { option.not_a_number() };
  Layout::scalar::<Option<T>>(option.opaque())
}

/// Returns the ranges of values of `Option<T>`, where `T` is an opaque scalar of the same size, and
/// their number, or 0 if every value of its size is one: the values of `T`, and the bytes of
/// `None` read in the byte order of `T`.
///
/// # Panics
///
/// Panics if `T` is not a scalar of at most 16 bytes, if a byte of `None` is not initialized, or if
/// `T` has more than `OPTION_RANGES - 1` ranges of values.
///
/// # Safety
///
/// The call must be evaluated at compile time, in a `const` item or block, as `discover::read`
/// requires.
const unsafe fn option_values<T: Pun>() -> ([(u128, u128); OPTION_RANGES], usize) {
  let Kind::Scalar(spec) = T::LAYOUT.kind else { panic!("not a scalar") };
  let size = size_of::<T>();
  assert!(size <= 16, "a scalar is at most 16 bytes wide");
  let none = MaybeUninit::new(None::<T>);
  let (mut value, mut at) = (0u128, 0);
  while at < size {
    // SAFETY: `at` is within an `Option<T>`, and the call is evaluated at compile time, by contract
    // on the caller, as `read` requires. A byte of `None` that is not initialized fails the build
    // there.
    let byte = unsafe { crate::discover::read(&none, at, 1) };
    // The value of a scalar is its bytes read in its byte order: the byte at `at` is the `at`th
    // least significant in little endian, and the `at`th most significant in big endian.
    let shift = if spec.endian.is_little() { at } else { size - 1 - at };
    value |= byte << (8 * shift);
    at += 1;
  }
  let mut out = ([(0u128, 0u128); OPTION_RANGES], 0);
  let ranges: &[(u128, u128)] = match spec.valid {
    Valid::In(ranges) => ranges,
    // Every value of the size of `T` is one, `None` included.
    _ => return out,
  };
  assert!(
    ranges.len() < OPTION_RANGES,
    "an opaque scalar of which an `Option` is laid out has \
    fewer ranges of values"
  );
  // The ranges of `T`, with `value` merged in their order.
  let (mut i, mut placed) = (0, false);
  while i <= ranges.len() {
    let next = if i < ranges.len() { Some(ranges[i]) } else { None };
    let (lo, hi) = match next {
      Some((lo, hi)) if placed || value > hi.saturating_add(1) => (lo, hi),
      Some((lo, hi)) if value.saturating_add(1) >= lo => {
        placed = true;
        (if value < lo { value } else { lo }, if value > hi { value } else { hi })
      }
      Some(range) => {
        placed = true;
        push(&mut out, (value, value));
        range
      }
      None if placed => break,
      None => {
        placed = true;
        (value, value)
      }
    };
    push(&mut out, (lo, hi));
    i += 1;
  }
  // A full range is every value.
  if out.1 == 1 && out.0[0].0 == 0 && out.0[0].1 == crate::layout::max_value(size) {
    out.1 = 0;
  }
  out
}

/// Returns whether `spec` accepts every value of its size but one.
const fn one_left_out(spec: Spec) -> bool {
  let Valid::In(ranges) = spec.valid else { return false };
  // The values left out: before the first range, between two ranges, and after the last.
  let mut out = ranges[0].0;
  let mut i = 1;
  while i < ranges.len() {
    out = out.saturating_add(ranges[i].0 - ranges[i - 1].1 - 1);
    i += 1;
  }
  out = out.saturating_add(crate::layout::max_value(spec.size) - ranges[ranges.len() - 1].1);
  out == 1
}

/// Appends `range` to the ranges of `out`, merged with the last one if they touch.
const fn push(out: &mut ([(u128, u128); OPTION_RANGES], usize), range: (u128, u128)) {
  if out.1 > 0 && out.0[out.1 - 1].1.saturating_add(1) >= range.0 {
    let last = &mut out.0[out.1 - 1];
    if range.1 > last.1 {
      last.1 = range.1;
    }
  } else {
    out.0[out.1] = range;
    out.1 += 1;
  }
}

/// Returns the layout of `Option<T>` that the experiments on its samples find.
const fn discovered<T: Pun>() -> Layout {
  let layout = Layout::enumeration::<Option<T>>(const { &<Option<T> as Found>::ENUM })
    .identified(Id::new("Option", &[T::LAYOUT.id]));
  // Where `core` documents it (a `NonZero`, a `repr(transparent)` struct around one), what is
  // found is what it documents: a niche at zero, which is `None`. Else Rust specifies nothing of
  // it (see "The `unstable` feature" in the crate documentation).
  if T::NULL_OPTIMIZED { layout } else { layout.unspecified() }
}

impl<T: Pun> Found for Option<T> {
  const SAMPLES: [MaybeUninit<Self>; 2] = if T::LAYOUT.is_inhabited() {
    // SAFETY: `Layout::sample::<T>()` is a valid `T`, as `assume_init` requires. It writes the
    // least valid value of each scalar in the layout of `T`, the tag and the fields of its first
    // variant that has values, and leaves padding as it is. Every value that the layout accepts is
    // a valid `T` (`Pun`, section Safety). `T` has values: the condition checks it.
    [Layout::wrap(None), Layout::wrap(Some(unsafe { Layout::sample::<T>().assume_init() }))]
  } else {
    [Layout::wrap(None), Layout::uninit()]
  };
  const OFFSETS: [usize; 2] = if T::LAYOUT.is_inhabited() {
    let sample = &<Self as Found>::SAMPLES[1];
    // SAFETY: The reference made from `sample.as_ptr()` meets the requirements of a reference [1]:
    // - It is not null, aligned, and not dangling, because `sample` is a reference to a
    //   `MaybeUninit<Option<T>>`, which has the layout of an `Option<T>`.
    // - It points to a valid `Option<T>`, because `sample` is `SAMPLES[1]`, `Some` of a valid `T`.
    // `SAMPLES` is a constant, which nothing writes while the reference lives.
    //
    // [1] Per https://doc.rust-lang.org/1.99.0/reference/behavior-considered-undefined.html#r-undefined.validity.reference-box:
    //
    //     A reference or `Box<T>` must be aligned and non-null, it cannot be dangling, and it must
    //     point to a valid value
    match unsafe { &*sample.as_ptr() } {
      // SAFETY: `value` is the field of `Some` in the value that `sample` holds, as `offset`
      // requires.
      Some(value) => [0, unsafe { offset(value, sample) }],
      // Not the variant: `discover` finds that the sample is not.
      None => [0, 0],
    }
  } else {
    [0, 0]
  };
  // SAFETY: `DESC` is an associated constant, so the call is evaluated at compile time, which is
  // what `discover` requires.
  const DESC: Desc<2> = unsafe {
    crate::discover::discover::<Self, 2>(
      &<Self as Found>::SAMPLES,
      &[&[], &[Field::new(<Self as Found>::OFFSETS[1], &T::LAYOUT)]],
    )
  };
  const TAG: Layout = const { &<Self as Found>::DESC }.tag();
  const VARIANTS: [Variant; 2] = [variant!(Self, 0), variant!(Self, 1, T)];
  const ENUM: Enum = Enum::discovered(
    const { &<Self as Found>::TAG },
    <Self as Found>::DESC.tag_offset(),
    const { &<Self as Found>::VARIANTS },
    Layout::size_align::<Self>(),
  );
}

impl<T: Pun, E: Pun> Found for Result<T, E> {
  const SAMPLES: [MaybeUninit<Self>; 2] = [
    if T::LAYOUT.is_inhabited() {
      // SAFETY: `Layout::sample::<T>()` is a valid `T`, as `assume_init` requires (see the
      // samples of `Option<T>`). `T` has values: it is checked above.
      Layout::wrap(Ok(unsafe { Layout::sample::<T>().assume_init() }))
    } else {
      Layout::uninit()
    },
    if E::LAYOUT.is_inhabited() {
      // SAFETY: `Layout::sample::<E>()` is a valid `E`, as `assume_init` requires (see the
      // samples of `Option<T>`). `E` has values: it is checked above.
      Layout::wrap(Err(unsafe { Layout::sample::<E>().assume_init() }))
    } else {
      Layout::uninit()
    },
  ];
  const OFFSETS: [usize; 2] = {
    let (ok, err) = (&<Self as Found>::SAMPLES[0], &<Self as Found>::SAMPLES[1]);
    let at = if T::LAYOUT.is_inhabited() {
      // SAFETY: `ok` is `SAMPLES[0]`, `Ok` of a valid `T`, since `T` has values. So it is a
      // valid `Result<T, E>`, and a reference to it is valid, as for the samples of `Option<T>`.
      match unsafe { &*ok.as_ptr() } {
        // SAFETY: `value` is the field of `Ok` in the value that `ok` holds, as `offset`
        // requires.
        Ok(value) => unsafe { offset(value, ok) },
        // Not the variant: `discover` finds that the sample is not.
        Err(_) => 0,
      }
    } else {
      0
    };
    let other = if E::LAYOUT.is_inhabited() {
      // SAFETY: `err` is `SAMPLES[1]`, `Err` of a valid `E`, since `E` has values. So it is a
      // valid `Result<T, E>`, and a reference to it is valid, as for the samples of `Option<T>`.
      match unsafe { &*err.as_ptr() } {
        // SAFETY: `value` is the field of `Err` in the value that `err` holds, as `offset`
        // requires.
        Err(value) => unsafe { offset(value, err) },
        Ok(_) => 0,
      }
    } else {
      0
    };
    [at, other]
  };
  // SAFETY: `DESC` is an associated constant, so the call is evaluated at compile time, which is
  // what `discover` requires.
  const DESC: Desc<2> = unsafe {
    crate::discover::discover::<Self, 2>(
      &<Self as Found>::SAMPLES,
      &[
        &[Field::new(<Self as Found>::OFFSETS[0], &T::LAYOUT)],
        &[Field::new(<Self as Found>::OFFSETS[1], &E::LAYOUT)],
      ],
    )
  };
  const TAG: Layout = const { &<Self as Found>::DESC }.tag();
  const VARIANTS: [Variant; 2] = [variant!(Self, 0, T), variant!(Self, 1, E)];
  const ENUM: Enum = Enum::discovered(
    const { &<Self as Found>::TAG },
    <Self as Found>::DESC.tag_offset(),
    const { &<Self as Found>::VARIANTS },
    Layout::size_align::<Self>(),
  );
}

// SAFETY: The layout is the one that `rustc` gave `Option<T>`, found by the experiments of
// `discover`. These fail to compile rather than find a wrong layout, under the facts of the
// compiler that its module documentation lists. The variant `Some` holds the layout of `T` at the
// offset observed on a sample, and the lists of variants and fields are those of this layout (see
// `OptionNone`, `OptionSome` and `OptionSomeField`). Where `T` is an opaque scalar, the layout is
// that of `opaque_option`, whose shape is checked there: a scalar, whose lists `puns` does not
// read, so they may be anything (`Pun`, section Safety). `Option<T>` holds a `T` or
// nothing, and a tag, so no `UnsafeCell`, since `T: Pun`.
unsafe impl<T: Pun> Pun for Option<T> {
  type Elem = ();
  type Fields = ();
  type Variants = OptionNone<T>;
  type Rules = ();
  const PROBE: *const Self = core::ptr::null();
  const LAYOUT: Layout =
    if T::LAYOUT.is_opaque() { opaque_option::<T>() } else { discovered::<T>() };
}

// SAFETY: The layout is the one that `rustc` gave `Result<T, E>`, found by the experiments of
// `discover`, as for `Option<T>`. The variants `Ok` and `Err` hold the layouts of `T` and `E` at
// the offsets observed on their samples, and the lists of variants and fields are those of this
// layout (see `ResultOk`, `ResultErr`, `ResultOkField` and `ResultErrField`). `Result<T, E>` holds
// a `T` or an `E`, and a tag, so no `UnsafeCell`.
unsafe impl<T: Pun, E: Pun> Pun for Result<T, E> {
  type Elem = ();
  type Fields = ();
  type Variants = ResultOk<T, E>;
  type Rules = ();
  const PROBE: *const Self = core::ptr::null();
  const LAYOUT: Layout = {
    let layout = Layout::enumeration::<Self>(const { &<Self as Found>::ENUM })
      .identified(Id::new("Result", &[T::LAYOUT.id, E::LAYOUT.id]));
    // `core` documents the layout where one of `T` and `E` is a type for which it documents
    // `Option`, and the other has size 0 and alignment 1: `Result<T, E>` has the size of that type,
    // and its variant has the bytes of its value. The other variant, if it has a value, is then the
    // one initialized value that is not a value of that type, all zeros.
    if T::NULL_OPTIMIZED && is_1_zst::<E>() || E::NULL_OPTIMIZED && is_1_zst::<T>() {
      layout
    } else {
      layout.unspecified()
    }
  };
}

/// Returns whether `T` has size 0 and alignment 1 (a "1-ZST").
const fn is_1_zst<T>() -> bool {
  size_of::<T>() == 0 && align_of::<T>() == 1
}

/// The variant `None` of `Option<T>` in the list of its variants (`Pun::Variants`).
#[doc(hidden)]
pub struct OptionNone<T>(PhantomData<T>);
/// The variant `Some` of `Option<T>` in the list of its variants.
#[doc(hidden)]
pub struct OptionSome<T>(PhantomData<T>);
/// The field of `Some` in the list of its fields (`Variants::Fields`).
#[doc(hidden)]
pub struct OptionSomeField<T>(PhantomData<T>);
/// The variant `Ok` of `Result<T, E>` in the list of its variants (`Pun::Variants`).
#[doc(hidden)]
pub struct ResultOk<T, E>(PhantomData<(T, E)>);
/// The variant `Err` of `Result<T, E>` in the list of its variants.
#[doc(hidden)]
pub struct ResultErr<T, E>(PhantomData<(T, E)>);
/// The field of `Ok` in the list of its fields (`Variants::Fields`).
#[doc(hidden)]
pub struct ResultOkField<T, E>(PhantomData<(T, E)>);
/// The field of `Err` in the list of its fields.
#[doc(hidden)]
pub struct ResultErrField<T, E>(PhantomData<(T, E)>);

// The lists of variants and fields of an `Option` of an opaque scalar, whose layout is no enum
// (`opaque_option`), are those of a variant without a tag or a field. Nothing reads them, but a
// check of `Option<T>` names them, so their constants are evaluated, and must not run the
// experiments, whose layout would not be the one of `opaque_option`.

/// Returns whether the layout of `E` is an enum.
const fn is_enum<E: Pun>() -> bool {
  matches!(E::LAYOUT.kind, Kind::Enum(_))
}

/// Returns the tag of the `index`th variant of `E`, or 0 if `E` is no enum.
const fn tag<E: Pun + Found>(index: usize) -> u128 {
  if is_enum::<E>() { E::LAYOUT.variant_tag(index) } else { 0 }
}

/// Returns the values of the tag of the `index`th variant of `E`, or `None` if they are its tag
/// alone or `E` is no enum.
const fn ranges<E: Pun + Found>(index: usize) -> Option<&'static [(u128, u128)]> {
  if is_enum::<E>() { E::LAYOUT.variant_ranges(index) } else { None }
}

/// Returns the field of the `index`th variant of `E`, or `None` if the variant has no value or
/// `E` is no enum.
const fn field<E: Pun + Found>(index: usize) -> Option<Field> {
  if is_enum::<E>() { found_field::<E>(index) } else { None }
}

/// Returns the field of the `index`th variant of the enum `E`, or `None` if the variant has no
/// value. It reads what the experiments find, so it is called only where they run.
const fn found_field<E: Pun + Found>(index: usize) -> Option<Field> {
  if E::DESC.inhabited(index) { Some(*E::LAYOUT.variant_field(index, 0)) } else { None }
}

// SAFETY: `TAG` and `RANGES` are those of the variant 0 of the layout of `Option<T>`, `None`,
// which has no field, and `Next` is the variant 1. So the list is the variants of the layout, in
// order, as the `# Safety` of `Variants` requires. Where `Option<T>` is no enum (`opaque_option`),
// `tag` and `ranges` give 0 and `None`, and `puns` reads no list of its variants, which may then be
// anything (`Variants`, section Safety).
unsafe impl<T: Pun> Variants for OptionNone<T> {
  type Fields = ();
  type Next = OptionSome<T>;
  const TAG: Option<u128> = Some(tag::<Option<T>>(0));
  const RANGES: Option<&'static [(u128, u128)]> = ranges::<Option<T>>(0);
}

// SAFETY: As for `OptionNone`: the variant 1, `Some`, whose field is `OptionSomeField<T>`, and
// the last. Where `Option<T>` is no enum, nothing reads the list, as for `OptionNone`.
unsafe impl<T: Pun> Variants for OptionSome<T> {
  type Fields = OptionSomeField<T>;
  type Next = ();
  const TAG: Option<u128> = Some(tag::<Option<T>>(1));
  const RANGES: Option<&'static [(u128, u128)]> = ranges::<Option<T>>(1);
}

// SAFETY: `FIELD` is the field of the variant `Some` in the layout of `Option<T>`, a `T` at the
// offset observed on its sample, and the only one. It is `None`, an empty list, if the variant
// has no value, as the layout has no field there, or if `Option<T>` is no enum (`opaque_option`),
// whose lists no function reads.
unsafe impl<T: Pun> Fields for OptionSomeField<T> {
  type Type = T;
  type Next = ();
  type Last = ();
  type Path<P: Path> = At<P, Self>;
  const FIELD: Option<Field> = field::<Option<T>>(1);
}

// SAFETY: `TAG` and `RANGES` are those of the variant 0, `Ok`, of the layout of `Result<T, E>`,
// whose field is `ResultOkField`, and `Next` is the variant 1. The list is the variants of the
// layout, in order, as the `# Safety` of `Variants` requires.
unsafe impl<T: Pun, E: Pun> Variants for ResultOk<T, E> {
  type Fields = ResultOkField<T, E>;
  type Next = ResultErr<T, E>;
  const TAG: Option<u128> = Some(tag::<Result<T, E>>(0));
  const RANGES: Option<&'static [(u128, u128)]> = ranges::<Result<T, E>>(0);
}

// SAFETY: As for `ResultOk`: the variant 1, `Err`, whose field is `ResultErrField`, and the last.
unsafe impl<T: Pun, E: Pun> Variants for ResultErr<T, E> {
  type Fields = ResultErrField<T, E>;
  type Next = ();
  const TAG: Option<u128> = Some(tag::<Result<T, E>>(1));
  const RANGES: Option<&'static [(u128, u128)]> = ranges::<Result<T, E>>(1);
}

// SAFETY: `FIELD` is the field of the variant `Ok` in the layout of `Result<T, E>`, a `T` at the
// offset observed on its sample, and the only one. It is `None`, an empty list, if the variant
// has no value, as the layout has no field there.
unsafe impl<T: Pun, E: Pun> Fields for ResultOkField<T, E> {
  type Type = T;
  type Next = ();
  type Last = ();
  type Path<P: Path> = At<P, Self>;
  const FIELD: Option<Field> = field::<Result<T, E>>(0);
}

// SAFETY: As for `ResultOkField`: the field of the variant `Err`, an `E`.
unsafe impl<T: Pun, E: Pun> Fields for ResultErrField<T, E> {
  type Type = E;
  type Next = ();
  type Last = ();
  type Path<P: Path> = At<P, Self>;
  const FIELD: Option<Field> = field::<Result<T, E>>(1);
}
