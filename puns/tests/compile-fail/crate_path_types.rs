//! `#[pun(crate = ..)]` cannot lead to other values than those of the crate for what a layout is
//! made of: the values that `valid(..)` restricts a field to (`Spec`), the identity of a generic
//! type (`Id::new`, and `Id::NONE` where a parameter is not bounded by `Pun`), the form of a
//! variable-length integer (`Varint`), a checksum (`Checksum`) and a CRC (`Crc`) that a rule
//! computes.

mod restricted {
  pub use puns::*;
  pub mod layout {
    pub use puns::layout::*;

    #[expect(non_snake_case, reason = "a module that stands for the type `Spec`")]
    pub mod Spec {
      pub const fn range<T>(_: Option<u128>, _: Option<u128>, _: bool) -> (u128, u128) {
        (0, 0)
      }
      /// Every value of a byte, whatever the field is restricted to.
      pub const fn merge_ranges<const N: usize>(
        _: [(u128, u128); N],
      ) -> ([(u128, u128); N], usize) {
        ([(0, 255); N], 1)
      }
    }
  }
}

mod identified {
  pub use puns::*;
  pub mod layout {
    pub use puns::layout::*;

    #[expect(non_snake_case, reason = "a module that stands for the type `Id`")]
    pub mod Id {
      /// An identity, for a type that has none.
      pub const NONE: puns::layout::Id = puns::layout::Id::new("chosen", &[]);
      /// One identity, whatever the parameters are.
      pub const fn new(_: &'static str, _: &'static [puns::layout::Id]) -> puns::layout::Id {
        NONE
      }
    }
  }
}

mod varint {
  pub use puns::*;
  pub mod layout {
    pub use puns::layout::*;

    #[expect(non_snake_case, reason = "a module that stands for the type `Varint`")]
    pub mod Varint {
      /// `leb128(..)`, read as QUIC writes it.
      #[expect(non_upper_case_globals, reason = "a constant that stands for a variant")]
      pub const Leb128: puns::layout::Varint = puns::layout::Varint::Quic;
    }
  }
}

mod checksum {
  pub use puns::*;
  pub mod layout {
    pub use puns::layout::*;

    #[expect(non_snake_case, reason = "a module that stands for the type `Checksum`")]
    pub mod Checksum {
      /// `sum(..)`, computed as Adler-32.
      #[expect(non_upper_case_globals, reason = "a constant that stands for a variant")]
      pub const Sum: puns::layout::Checksum = puns::layout::Checksum::Adler32;
    }
  }
}

mod crc {
  pub use puns::*;
  pub mod layout {
    pub use puns::layout::*;

    pub struct Crc;

    impl Crc {
      /// Any CRC, computed as CRC-32C.
      pub const fn new(_: u32, _: u64, _: u64, _: bool, _: u64) -> puns::layout::Crc {
        puns::layout::Crc::new(32, 0x1edc_6f41, 0xffff_ffff, true, 0xffff_ffff)
      }
    }
  }
}

#[derive(puns::Pun, Clone, Copy)]
#[pun(crate = restricted)]
#[repr(C)]
struct Restricted {
  #[pun(valid(0..=1))]
  a: u8,
}

#[derive(puns::Pun, Clone, Copy)]
#[pun(crate = identified)]
#[repr(C)]
struct Identified<T> {
  a: T,
}

#[derive(puns::Pun, Clone, Copy)]
#[pun(crate = identified)]
#[repr(C)]
struct Unidentified<T> {
  a: u8,
  b: core::marker::PhantomData<T>,
}

#[derive(puns::Pun, Clone, Copy)]
#[pun(crate = varint)]
#[pun(assert(leb128(0) == 0x40))]
#[repr(C)]
struct Leb {
  a: [u8; 2],
}

#[derive(puns::Pun, Clone, Copy)]
#[pun(crate = checksum)]
#[pun(assert(sum(0, 2) == 3))]
#[repr(C)]
struct Summed {
  a: [u8; 2],
}

#[derive(puns::Pun, Clone, Copy)]
#[pun(crate = crc)]
#[pun(assert(crc::<32, 0x04c1_1db7, 0xffff_ffff, true, 0xffff_ffff>(0, 4) == c))]
#[repr(C)]
struct Checked {
  a: [u8; 4],
  c: u32,
}

fn main() {
  let _ = puns::try_pun::<u8, Restricted>(7);
  let _ = puns::try_pun::<u8, Identified<u8>>(7);
  let _ = puns::try_pun::<u8, Unidentified<u8>>(7);
  let _ = puns::try_pun::<[u8; 2], Leb>([0x40, 0]);
  let _ = puns::try_pun::<[u8; 2], Summed>([1, 2]);
  let _ = puns::try_pun::<[u8; 8], Checked>([0; 8]);
}
