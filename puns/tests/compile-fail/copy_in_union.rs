//! `copy_in` of a union: which of its fields it is, and so which of its bytes are initialized,
//! cannot be told (a `u8` field leaves three bytes of a `u32` field uninitialized).
#[derive(puns::Pun, Clone, Copy)]
#[repr(C)]
union Either {
  small: u8,
  wide: u32,
}

fn main() {
  let value = Either { small: 1 };
  let mut dst = [0u8; 4];
  // SAFETY: `value` and `dst` meet the requirements of `copy_in`. The call is refused anyway.
  unsafe { puns::copy_in::<Either>(core::ptr::from_ref(&value).cast(), dst.as_mut_ptr()) };
}
