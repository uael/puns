//! Every ordered pair of the types of 1 and of 2 bytes of the model as a view enum (`Two`), against
//! trying each variant in turn, for every pattern of bytes up to one more than their size (see
//! `puns_model::views`).

use core::num::NonZero;

use puns::num::*;
use puns_model::types::*;

/// Calls `view_pairs!` with the types that Rust lays out the same way in every layout that it
/// allows, and, with the `unstable` feature, with the others too: without it, a view enum of one
/// of them does not compile.
macro_rules! views {
  ($lengths:expr; [$($kept:ty),+ $(,)?]; [$($other:ty),+ $(,)?]) => {
    #[cfg(not(feature = "unstable"))]
    puns_model::view_pairs!($lengths, false; $($kept),+);
    #[cfg(feature = "unstable")]
    puns_model::view_pairs!($lengths, false; $($kept),+, $($other),+);
  };
}

#[test]
fn views() {
  views!(&[0, 1, 2];
    [u8, i8, bool, NonZero<u8>, Option<NonZero<u8>>, u8c<0>, u8c<7>, u8c<255>, u8in<0, 0x7f>,
     u8in<1, 1>, u8in<0x80, 0xff>, u8in<2, 5>, E01, E37, Eff, Eneg, [u8; 1], [bool; 1], (bool,),
     Flag, Byte, Ascii, SoftBit, SoftTrue, Third, core::mem::ManuallyDrop<bool>, Zero,
     core::mem::MaybeUninit<u8>, ChoiceU, ThirdSoft, Fifth, LastNotFf1, ThirdU];
    [Erust, MaybeFlag, NzOpt, Option<bool>, Option<Option<bool>>]
  );
  puns_model::finish("views of 1 byte");
  // In the same test: the report is one for the process, which tests share.
  views!(&[1, 2, 3];
    [u16, i16, NonZero<u16>, Option<NonZero<u16>>, u16le, u16be, u16lec<0x1234>, u16bec<0x1234>,
     u16lec<1>, u16bec<0x100>, u16bec<0x3412>, [u8; 2], [bool; 2], [u8in<0, 0x7f>; 2], E16,
     ByteNz, NzByte, Aligned, AlignedFlag, PackedU16, NotOneOne, Opt, OptFlag, Either, EitherC,
     Skewed, OptSoftBit, [E01; 2], ByteZero, Never, FlagsU, core::mem::MaybeUninit<u16>,
     LastNotFf2, NotOneOneNz, ThirdIn, ThirdSoftIn, ThirdUninit, ThirdSoftUninit, [Third; 2],
     [ThirdSoft; 2]];
    [(u8, bool), (bool, u8), (u8c<1>, u8), RustPair, Pair, Mixed, Single, Flags2, NestPair,
     RustThird, RustThirdSoft]
  );
  puns_model::finish("views of 2 bytes");
}
