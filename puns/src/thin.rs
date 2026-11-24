//! What a thin pointer to a `T` finds at its target: [`Thin<T, M>`], the length of `T` if it has
//! one, then the `T`.
//!
//! The target of a thin [`Aim`](crate::layout::Aim) holds its own metadata, as the target of a
//! thin pointer of Rust does not: a pointer to a slice or a `str` holds a length. A `Thin<T, M>`
//! stores that length at the target as an `M`, before the data, as `nearest` stores the length of
//! a list or a string as its offset type, and its `len` rule reads it ([`Thin::get`]). A sized `T`
//! has no metadata, and `Thin<T, M>` is a `T`. A wide aim holds the metadata of its target itself,
//! and its target is not a `Thin`.
//!
//! `Thin` is `repr(C)`, so where the elements are more aligned than `M`, padding separates the
//! length from the data, as in `Thin<[u32], i16>`, whose data is at offset 4.

use core::marker::PhantomData;

use crate::{
  Fields, Path, Pun, RuleAt, Rules, Steps,
  layout::{self, Field, Id, Layout, Offsets, Op, Rule},
  steps::One,
  validate,
};

/// The seal of [`Store`].
mod seal {
  /// Implemented for `()` and `usize` only.
  pub trait Store {}

  impl Store for () {}
  impl Store for usize {}
}

/// The metadata of a pointer to a [`Pun`] type (`Pun::Metadata`), and how a [`Thin`] stores it as
/// an `M` before the value it is the metadata of.
///
/// The metadata of a sized type, `()`, stores nothing. The length of the tail of an unsized type, a
/// `usize`, is stored as an `M`, an integer type. The trait is sealed, so these are the only two.
///
/// What is stored is a generic associated type, `Stored<M>`, rather than the item of a trait
/// `Store<M>`, so that naming `Thin<T, M>` requires nothing of `T::Metadata`. A bound
/// `T::Metadata: Store<M>` on a type that points at itself, as a pair points at the next pair
/// through `Thin<Pair, M>`, is a requirement that the compiler cannot finish evaluating (E0275,
/// "overflow evaluating the requirement").
pub trait Store: Copy + seal::Store {
  /// What is stored before the value, which `puns` checks as any field.
  type Stored<M: Copy + Pun>: Copy + Pun;
  /// The rules of a `Thin` that stores this metadata as an `M`.
  #[doc(hidden)]
  type Len<M: Copy + Pun>: Length;
}

impl Store for () {
  type Stored<M: Copy + Pun> = ();
  type Len<M: Copy + Pun> = NoLength;
}

impl Store for usize {
  type Stored<M: Copy + Pun> = M;
  type Len<M: Copy + Pun> = LenOps<M>;
}

/// The rules of a [`Thin`], for the metadata that it stores ([`Store`]).
#[doc(hidden)]
pub trait Length {
  /// The rules, as a list at the type level.
  type Rules: Rules;
  /// The rules: none, or a `len` rule that reads the stored length.
  const RULES: &'static [Rule];
  /// The identity of what is stored, which is a part of the identity of the `Thin`.
  const META: Id;
}

/// The rules of a [`Thin`] of a sized type: none.
#[doc(hidden)]
pub struct NoLength;

impl Length for NoLength {
  type Rules = ();
  const RULES: &'static [Rule] = &[];
  const META: Id = Id::new("()", &[]);
}

/// The steps of the `len` rule of a `Thin` of an unsized type (`LenOps`): one operation.
type LenSteps = One<0>;

/// The `len` rule of a `Thin` whose length is stored as an `M` at its start.
#[doc(hidden)]
pub struct LenOps<M>(PhantomData<M>);

impl<M: Pun> LenOps<M> {
  /// The length, the integer of the size, the byte order and the sign of `M`. A negative one of a
  /// signed `M` is sign-extended to more than any length ([`Op::Load`]), which no value has, so the
  /// rule fails where its highest bit is set.
  const OPS: [Op; 1] = {
    let Some(spec) = M::LAYOUT.as_bits() else {
      panic!("the length of a `Thin` is an integer of 1 to 8 bytes, any of whose values is valid")
    };
    let big = matches!(spec.endian, layout::Endian::Big);
    let signed = matches!(spec.number, layout::Number::Signed);
    [Op::Load { offset: 0, size: spec.size as u8, big, signed }]
  };
}

impl<M: Pun> Length for LenOps<M> {
  type Rules = RuleAt<0, LenSteps, ()>;
  const RULES: &'static [Rule] = &[Rule::Len(&Self::OPS)];
  const META: Id = M::ID;
}

/// What a thin pointer to a `T` finds at its target: the metadata of `T`, stored as an `M`
/// ([`Store`]), then the `T`.
///
/// For a sized `T`, it is a `T`. For an unsized `T`, a slice, a `str` or a struct that ends with
/// one, it is the length of the tail of `T` as an `M`, then the `T`, after the padding that its
/// alignment needs.
///
/// # Examples
///
/// ```
/// use puns::Thin;
///
/// // A length of 2, as an `i16` of the target, then the bytes.
/// let mut bytes = [0u8; 4];
/// bytes[..2].copy_from_slice(&2i16.to_ne_bytes());
/// bytes[2..].copy_from_slice(b"hi");
/// let thin = puns::from_bytes::<Thin<str, i16>>(&bytes).unwrap();
/// assert_eq!(thin.value(), "hi");
/// // A negative length is not one.
/// bytes[..2].copy_from_slice(&(-2i16).to_ne_bytes());
/// assert!(puns::from_bytes::<Thin<str, i16>>(&bytes).is_err());
/// ```
#[repr(C)]
pub struct Thin<T: ?Sized + Pun, M: Copy + Pun> {
  meta: Stored<T, M>,
  data: T,
}

impl<T: ?Sized + Pun, M: Copy + Pun> Thin<T, M> {
  /// Returns the value.
  #[inline(always)]
  pub const fn value(&self) -> &T {
    &self.data
  }
}

impl<T: ?Sized + Pun, M: Copy + Pun> Thin<T, M> {
  /// Returns the value of the `Thin<T, M>` at `at`: `at` itself for a sized `T`, and for a slice
  /// or a `str`, the elements or the bytes after the stored length, as many as it says.
  ///
  /// # Safety
  ///
  /// The caller must ensure that `at` is the address of a valid `Thin<T, M>`, aligned for it, whose
  /// bytes are not written for `'a`.
  #[inline(always)]
  #[must_use]
  pub const unsafe fn get<'a>(at: *const u8) -> &'a T {
    // SAFETY: `at` is the address of a valid `Thin<T, M>`, aligned, by contract on the caller, as
    // `thin` requires. The pointer that it returns is `at`, with the length that the `Thin` stores
    // if `T` is unsized, so it points to that `Thin`, which is not written for `'a`, by contract on
    // the caller, and neither is its field `data`.
    unsafe { &(*validate::thin::<Self>(at)).data }
  }
}

/// The field `meta` of a [`Thin`], then `ThinData`.
#[doc(hidden)]
pub struct ThinMeta<T: ?Sized, M>(PhantomData<M>, PhantomData<T>);

/// The field `data` of a [`Thin`], the last one.
#[doc(hidden)]
pub struct ThinData<T: ?Sized, M>(PhantomData<M>, PhantomData<T>);

// SAFETY: `FIELD` is the field of the layout of `Thin<T, M>` declared first, `meta`, of type
// `Stored<T, M>`, and `Next` is the one declared after it (`Layout::field`). So the list is the
// fields of the layout in the order of their declarations, as the `# Safety` of `Fields` requires.
unsafe impl<T: ?Sized + Pun, M: Copy + Pun> Fields for ThinMeta<T, M> {
  type Type = Stored<T, M>;
  type Next = ThinData<T, M>;
  type Last = ThinData<T, M>;
  type Path<P: Path> = crate::At<P, Self>;
  const FIELD: Option<Field> = Some(*<Thin<T, M> as Pun>::LAYOUT.field(0));
}

// SAFETY: `FIELD` is the field `data` of the layout of `Thin<T, M>`, of type `T`, declared last,
// and `Next` is the end of the list.
unsafe impl<T: ?Sized + Pun, M: Copy + Pun> Fields for ThinData<T, M> {
  type Type = T;
  type Next = ();
  type Last = Self;
  type Path<P: Path> = crate::At<P, Self>;
  const FIELD: Option<Field> = Some(*<Thin<T, M> as Pun>::LAYOUT.field(1));
}

// SAFETY: `OFFSETS` projects a pointer to a `Thin<T, M>` onto `meta` and `data`, the fields that
// `ThinMeta` and `ThinData` list, in that order, with `&raw const` and `byte_offset_from` on
// `measure_base`, as the code of `#[derive(Pun)]` does. So it holds their offsets in the type as
// compiled.
unsafe impl<T: ?Sized + Pun, M: Copy + Pun, const N: usize> Offsets<N, 2> for Thin<T, M> {
  const OFFSETS: [usize; 2] = {
    let memory = Layout::uninit::<[u8; N]>();
    // SAFETY: `Thin<T, M>` is `Sized`, or its last field is `T`, a slice, `str` or a struct whose
    // last field is one of these (`Pun`, section Safety), and its probe is that of `T` cast with an
    // `as` cast, as `measure_base` requires.
    let base = unsafe { Layout::measure_base::<Self, N>(&memory, <Self as Pun>::PROBE) };
    // SAFETY: `base` points to a `Thin<T, M>` in `memory`, all of whose places are in that
    // allocation (`measure_base`). `&raw const` makes a pointer to a field without reading it,
    // which needs neither initialized bytes nor alignment, and that pointer and `base` are in the
    // same allocation, as `byte_offset_from` requires. A field is not before the struct.
    unsafe {
      [
        (&raw const (*base).meta).byte_offset_from(base).cast_unsigned(),
        (&raw const (*base).data).byte_offset_from(base).cast_unsigned(),
      ]
    }
  };
}

// SAFETY: The layout is that of a `repr(C)` struct of the fields `meta`, of type `Stored<T, M>`,
// and `data`, of type `T`, at the offsets measured on the struct as compiled (`Offsets`), each with
// the layout of its type. `Layout::structure` takes the size and the alignment from the compiler,
// and checks that the fields do not overlap and that an unsized struct has the size that the
// layout computes. A struct is valid if its fields are, and no byte that no field covers is read.
// - For a sized `T`, `T::Metadata` is `()` (`Pun`, section Safety, asserted below), which stores
//   `()`, and there is no rule, so the valid values are those of `T`.
// - For an unsized `T`, `T::Metadata` is `usize`, which stores an `M`, and the `len` rule says that
//   the length of the tail is the value of `meta`, whose highest bit is clear. The tail of `T` is
//   the tail of the `Thin`. A reference to a `Thin` made by `puns` is checked or proven to have it,
//   and `get` reads it. The rule is a requirement, as every rule is: safe code cannot make or write
//   a `Thin` with another length, since its fields are private and it has no constructor.
// `Fields` is the list `ThinMeta`, `ThinData`. `Rules` is the list of the rules of `Length`, the
// `len` rule or nothing, and `Metadata` that of `T`, since the tail of `T` is that of the `Thin`.
// The other lists are `()`. No byte is in an `UnsafeCell`, since `T` and `Stored<T, M>` are `Pun`.
unsafe impl<T: ?Sized + Pun, M: Copy + Pun> Pun for Thin<T, M> {
  type Elem = ();
  type Fields = ThinMeta<T, M>;
  type Variants = ();
  type Rules = <Len<T, M> as Length>::Rules;
  type Metadata = T::Metadata;
  const PROBE: *const Self = T::PROBE as *const Self;
  const ID: Id = Id::new("Thin", &[T::ID, <Len<T, M> as Length>::META]);
  // SAFETY: `Thin<T, M>` is `Sized`, or its last field is `T`, which is unsized, a slice, `str` or
  // a struct whose last field is one of these, and `Self::PROBE` is the probe of `T` cast with an
  // `as` cast. A value whose tail has 8 elements fits in an `isize`, since one of `T` does. These
  // are the requirements of `offsets` and of `Layout::structure`.
  const LAYOUT: Layout = unsafe {
    assert!(
      T::LAYOUT.is_unsized() == (size_of::<T::Metadata>() != 0),
      "the metadata of a type is `()` exactly when the type is sized"
    );
    Layout::structure::<Self>(
      Self::PROBE,
      const {
        &Layout::sort_fields([
          Field::new(Layout::offsets::<Self, 2>(Self::PROBE)[0], &<Stored<T, M> as Pun>::LAYOUT),
          Field::new(Layout::offsets::<Self, 2>(Self::PROBE)[1], &T::LAYOUT),
        ])
      },
      <Len<T, M> as Length>::RULES,
    )
    .identified(Self::ID)
  };
}

/// What a [`Thin<T, M>`] stores before its `T`.
type Stored<T, M> = <<T as Pun>::Metadata as Store>::Stored<M>;

/// The rules of a [`Thin<T, M>`].
type Len<T, M> = <<T as Pun>::Metadata as Store>::Len<M>;

/// The steps of the `len` rule, checked against the operations where the rule is applied.
const _: () = {
  assert!(<LenSteps as Steps>::FIRST == 0 && <LenSteps as Steps>::END == 1);
};
