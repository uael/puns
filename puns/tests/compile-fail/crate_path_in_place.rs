//! `#[pun(crate = ..)]` cannot make the field of an eager aim a type that safe code makes: the path
//! names an `InPlace<T>` that is the one of `puns` for `()` only, and a trait `InPlaceOf` of its
//! own, but the field is tied to the `InPlace<T>` of `puns` for its own `T`, through `Layout`.
mod fake {
  pub use puns::*;

  /// `InPlace<()>` is the `InPlace<()>` of `puns`; any other `InPlace<T>` is `Mine<T>`.
  pub type InPlace<T> = <T as Pick>::Out;

  pub trait Pick {
    type Out: ?Sized;
  }

  impl Pick for () {
    type Out = puns::InPlace<()>;
  }

  impl Pick for i16 {
    type Out = Mine<i16>;
  }

  /// Made by any code.
  #[repr(transparent)]
  pub struct Mine<T>(pub T);

  /// A trait of the name of the one of `puns`, implemented for `Mine`.
  pub trait InPlaceOf {
    type Inner: ?Sized;
  }

  impl<T> InPlaceOf for Mine<T> {
    type Inner = T;
  }
}

#[derive(puns::Pun)]
#[pun(crate = fake)]
#[repr(transparent)]
struct Evil {
  #[pun(pointer(pointee: u64 = off as u128))]
  off: fake::InPlace<i16>,
}

fn main() {
  // Safe code would make a pointer 4096 bytes away, which nothing checked, and read it.
  let evil = Evil { off: fake::Mine(4096) };
  let _ = evil.pointee();
}
