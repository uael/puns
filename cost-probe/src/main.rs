//! Temporary: checks a tree of alignment 1, `fan` children a node, `levels` deep, `runs` times, in
//! `runs` (for `valgrind --tool=callgrind --toggle-collect='*runs*'`).

use std::hint::black_box;

use puns::{InPlace, Pun, num::u32le};

#[derive(Pun)]
#[repr(C)]
struct Node {
  #[pun(pointer([Node] = off as u128, len = n as u128))]
  off: InPlace<u32le>,
  n: u8,
}

fn build(bytes: &mut Vec<u8>, at: usize, levels: usize, fan: usize) {
  let list = bytes.len();
  let n = if levels == 0 { 0 } else { fan };
  bytes.resize(list + 5 * n, 0);
  bytes[at..at + 4].copy_from_slice(&u32::try_from(list - at).unwrap().to_le_bytes());
  bytes[at + 4] = n as u8;
  for i in 0..n {
    build(bytes, list + 5 * i, levels - 1, fan);
  }
}

#[inline(never)]
fn runs(bytes: &[u8], n: usize) -> usize {
  let mut ok = 0;
  for _ in 0..n {
    ok += usize::from(puns::from_head::<Node>(black_box(bytes)).is_ok());
  }
  ok
}

fn main() {
  let args = std::env::args().skip(1).map(|a| a.parse().unwrap()).collect::<Vec<usize>>();
  let (fan, levels, n) = (args[0], args[1], args[2]);
  let mut bytes = vec![0; 5];
  build(&mut bytes, 0, levels, fan);
  assert_eq!(runs(&bytes, n), n);
  println!("bytes {}", bytes.len());
}
