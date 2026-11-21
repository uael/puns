//! Rules proven by the same rules, in another struct: a packet of a protocol reinterpreted as a
//! packet of any protocol (as `Frame<Ipv4<Udp>> -> Frame<Ipv4>` of `examples/ethernet.rs`), whole,
//! through a `&mut`, from its end, and in an array. Next to it in `compile-fail/`:
//! `same_rule_other_extent`.
use puns::{
  Pun, Tag,
  num::{u8c, u16be},
};

#[derive(Pun, Clone, Copy)]
#[repr(transparent)]
struct Protocol(u8);

#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(tag(Protocol = u8c<17>))]
struct Udp {
  ports: [u8; 4],
}

/// A packet: its length is the one of its header and payload, and its checksum is right. Its
/// payload is a typed region of its bytes, as in `examples/ethernet.rs`: a `Packet<Udp>` has the
/// extent of the `Packet` it is (a payload as a last field would make another tail).
#[derive(Pun)]
#[repr(C)]
#[pun(size = 4 + length)]
#[pun(assert(sum16(0, size_of_val(self)) == 0xffff))]
struct Packet<P: ?Sized + Tag<Protocol> = [u8]> {
  protocol: P::Tag,
  length: u8,
  checksum: u16be,
  #[pun(regions(payload: P))]
  data: [u8],
}

/// Sized, with the same rule: one of UDP, and one of any protocol.
#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(sum16(0, size_of_val(self)) == 0xffff))]
struct FixedUdp {
  protocol: u8c<17>,
  length: u8,
  checksum: u16be,
  payload: Udp,
}

#[derive(Pun, Clone, Copy)]
#[repr(C)]
#[pun(assert(sum16(0, size_of_val(self)) == 0xffff))]
struct Fixed {
  protocol: Protocol,
  length: u8,
  checksum: u16be,
  payload: [u8; 4],
}

fn main() {
  let bytes = [17u8, 4, 0xee, 0xfb, 0, 0, 0, 0];
  let udp: &Packet<Udp> = puns::try_pun_ref(&bytes[..]).unwrap();
  let any: &Packet = puns::pun_ref(udp);
  assert!(any.payload().is_ok());
  let fixed: FixedUdp = puns::try_pun(bytes).unwrap();
  let _: Fixed = puns::pun(fixed);
  let mut both = [fixed, fixed];
  let _: &[Fixed; 2] = puns::pun_ref(&both);
  let _: &Fixed = puns::tail_ref(&both);
  // What is written through it meets the rule of the source if it meets its own, the same.
  let _: &mut FixedUdp = puns::head_mut(&mut both);
}
