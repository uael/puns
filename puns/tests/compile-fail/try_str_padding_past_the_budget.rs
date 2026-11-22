//! A `str` from 8,192 `bool`s, which are not an array, then a union that may hold no value. Every
//! `bool` is UTF-8, but the analysis stops past a budget of runs, so the string is checked at run
//! time, and the check reads the byte of the union.
use puns::Pun;

#[derive(Pun, Clone, Copy)]
#[repr(C)]
union Byte {
  any: u8,
  none: (),
}

#[derive(Pun, Clone, Copy, Default)]
#[repr(C)]
struct Two<T> {
  a: T,
  b: T,
}

type B2 = Two<bool>;
type B4 = Two<B2>;
type B8 = Two<B4>;
type B16 = Two<B8>;
type B32 = Two<B16>;
type B64 = Two<B32>;
type B128 = Two<B64>;
type B256 = Two<B128>;
type B512 = Two<B256>;
type B1024 = Two<B512>;
type B2048 = Two<B1024>;
type B4096 = Two<B2048>;
type B8192 = Two<B4096>;

#[derive(Pun, Clone, Copy)]
#[repr(C)]
struct BoolsThenHole {
  bools: B8192,
  hole: Byte,
}

fn main() {
  let source = BoolsThenHole { bools: B8192::default(), hole: Byte { none: () } };
  let _ = puns::try_pun_ref::<BoolsThenHole, str>(&source);
}
