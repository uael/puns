# Performance

A proven pun compiles to no check, though it may still compute an address or a length. A checked pun
compiles to the checks of what the analysis could not prove.

## Machine code

The table below gives the machine code of some functions of
[`puns/examples/codegen.rs`][codegen]. That file defines their types, records the machine code
of more functions, and gives the command that writes the assembly. The code was measured on
`x86_64-unknown-linux-gnu` with `rustc` 1.99.1 and 1.100.0, which give the same code, in release
with LTO and one codegen unit.

| Function                                           | Machine code                                  |
|----------------------------------------------------|-----------------------------------------------|
| `&Position -> &[u8; 2]`                            | `mov rax, rdi; ret`                           |
| `[bool; 4] -> u32`                                 | `mov eax, edi; ret`                           |
| `&Packet<Datagram> -> &Packet` (the same rules)    | `mov rdx, rsi; mov rax, rdi; ret`             |
| `&[Shape] -> &[Drawing]` (enums with fields)       | `mov rdx, rsi; mov rax, rdi; ret`             |
| `&[u8; 2] -> Option<&Position>` (a `u8`, a `bool`) | one `cmp byte ptr [rdi + 1], 2`               |
| `&Drawing -> Option<&Shape>`                       | the tag, then one `cmp` for a `bool`, 12 instructions |
| `from_head::<Frame<Ipv4>>(&[u8])`, no checksum     | 29 instructions, no loop, no call             |
| `&[u8] -> Option<&Ipv6>` (a walk, two checksums)   | 207 instructions, no call                     |
| `&[u8] -> Option<&[bool]>`                         | 64 bytes at a time, no branch in a block      |

### Compared with `zerocopy` and `bytemuck`

`zerocopy` and `bytemuck` decide what a conversion needs from each type on its own. A `bool` does
not accept every byte, so with them, reinterpreting a `&[Switch]` as a `&[bool]` is a checked
conversion, even when every `Switch` is 0 or 1. `puns` proves it from the two types. The functions
below were compiled with `zerocopy` 0.8.63, `bytemuck` 1.25.2 and `rustc` 1.100.0, on
`aarch64-apple-darwin`, in a crate of their own built with `lto = true` and `codegen-units = 1`, by
`cargo rustc --release --lib -- --emit asm`:

```rust,ignore
#[derive(puns::Pun, zerocopy::IntoBytes, zerocopy::Immutable, bytemuck::NoUninit, Clone, Copy)]
#[repr(u8)]
pub enum Switch {
  Off = 0,
  On = 1,
}

#[unsafe(no_mangle)]
pub fn zc_slice(s: &[Switch]) -> Option<&[bool]> {
  use zerocopy::{IntoBytes, TryFromBytes};
  <[bool]>::try_ref_from_bytes(s.as_bytes()).ok()
}

#[unsafe(no_mangle)]
pub fn bm_slice(s: &[Switch]) -> Option<&[bool]> {
  bytemuck::checked::try_cast_slice(s).ok()
}

#[unsafe(no_mangle)]
pub fn puns_slice(s: &[Switch]) -> &[bool] {
  puns::pun_ref(s)
}
```

`zc_slice` compiles to a loop of 9 instructions, and `bm_slice` to one of 10, each comparing every
byte with 2. `puns_slice` compiles to `ret`. For a single `Switch` reinterpreted as a `bool`, by
value, all three compile to `ret`, because the optimizer knows that a `Switch` is 0 or 1 and removes
the check.

## Time

[`puns/benches/checks.rs`][checks] times each check made by `puns` next to the same check written
by hand. The times below are in nanoseconds per check. They are the median of 5 runs of the
benchmark, and each run takes the best of 7 samples of each case. They were measured on
2026-11-21 with:

- the library at commit [`4176671`][commit],
- `rustc` 1.100.0, with the `bench` profile of the workspace (`opt-level = 3`, LTO, one codegen
  unit),
- an Apple M4 Max (14 cores, 36 GiB) on macOS 26.7.2,
- `cargo bench -p puns --features unstable --bench checks`.

The load average of the machine was between 3.5 and 3.9 during the runs. From one run to the
next, the time of `puns` varied by less than 2% in 26 of the 33 cases, and by 22% at most (`ipv6,
tcp, 12 headers`). The time by hand varied by less than 2% in 19 cases, and by 40% at most (`ipv6,
tcp, 0 headers`). The output of each run is in [`docs/checks-2026-11-21.txt`][record].

The chart and the table are written from the record by `cargo xtask checks-chart`.

<!-- checks:start -->
![The time of each check by puns relative to the same check by hand: from 0.038 to 1.52 times,
below 1 for 22 of the 33 cases](charts/checks.svg)

<details>
<summary>The times in nanoseconds per check, the median of 5 runs</summary>

| Case | `puns` | by hand |
|---|---:|---:|
| `ipv4, udp, 64 bytes` | 5.07 | 4.59 |
| `ipv4, udp, 1472 bytes` | 22.1 | 22.4 |
| `ipv4, tcp, 1460 bytes` | 22.2 | 22.0 |
| `ipv4, protocol 253, 1480 bytes` | 2.80 | 2.48 |
| `ipv4, udp, 64 bytes, bad checksum` | 5.07 | 4.59 |
| `ipv6, tcp, 0 headers` | 1.52 | 1.38 |
| `ipv6, tcp, 3 headers` | 2.29 | 3.46 |
| `ipv6, tcp, 12 headers` | 5.87 | 10.7 |
| `ipv6, udp, 3 headers, 64 bytes` | 4.57 | 5.59 |
| `ipv6, udp, 0 headers, 1400 bytes` | 20.4 | 21.1 |
| `udp, from_head` | 0.77 | 0.77 |
| `view enum, udp over ipv4 (first)` | 5.58 | 5.10 |
| `view enum, tcp over ipv4` | 6.97 | 6.25 |
| `view enum, udp, bad checksum` | 5.09 | 5.24 |
| `view enum, arp` | 0.86 | 1.15 |
| `view enum, vlan` | 0.73 | 1.27 |
| `view enum, other ethertype (last)` | 0.76 | 1.02 |
| `[u8] -> [bool], 64 KiB` | 632 | 16,640 |
| `[u8] -> [Kind] (1, 2, 3, 7), 4 KiB` | 97.2 | 1,057 |
| `[u8] -> [Small] (repr(Rust): a tag, a bool), 4 KiB` | 490 | 813 |
| `[u8] -> [MaybeFlag] (repr(Rust): a niche), 64 KiB` | 25,352 | 16,646 |
| `[u32] -> [char], 4 KiB` | 85.4 | 276 |
| `crc32, 64 bytes` | 10.8 | 11.4 |
| `crc32, 64 bytes (by hand: a byte at a time)` | 10.8 | 66.3 |
| `crc-32c (a Crc), 64 bytes` | 12.4 | 11.4 |
| `crc-16/modbus (a Crc), 64 bytes` | 12.4 | 66.0 |
| `crc-16/xmodem (a Crc), 64 bytes` | 12.1 | 74.2 |
| `crc32, 1518 bytes` | 492 | 1,174 |
| `crc32, 1518 bytes (by hand: a byte at a time)` | 490 | 2,813 |
| `crc-32c (a Crc), 1518 bytes` | 399 | 1,187 |
| `crc-16/modbus (a Crc), 1518 bytes` | 389 | 2,813 |
| `crc-16/xmodem (a Crc), 1518 bytes` | 451 | 3,188 |
| `sum16, 1500 bytes` | 22.1 | 21.9 |

</details>
<!-- checks:end -->

Cases are named as in the benchmark, so `cargo bench -p puns --features unstable --bench checks --
'ipv6'` runs the cases whose name contains `ipv6`. The size of an IPv4 case is the size of its UDP
datagram or TCP segment, after the 20 bytes of the IPv4 header. The IPv6 cases of TCP time only the
walk over the extension headers, and the IPv6 case of 1400 bytes computes a checksum that fails.

The code written by hand computes CRC-32 and CRC-32C 16 bytes at a time, unless the name of the
case says a byte at a time, and checks a slice with an iterator, such as `bytes.iter().all(|&b| b
<= 1)`. It computes the two CRC-16s a byte at a time, with a table, while `puns` computes every CRC
16 bytes at a time, so the rows of the CRC-16s compare two algorithms rather than the cost of the
check around them.

`puns` checks a slice of small elements, each a few comparisons, in blocks of up to 64 bytes,
without a branch inside a block. A slice of `MaybeFlag`, an enum of the `Rust` representation whose
tag is a niche in a `bool`, is not checked this way, and is slower than by hand.

In a view enum, when a later variant accepts every value of an earlier one, the earlier variant is
checked only for what the later one does not prove. So for UDP over IPv4 with a wrong checksum, the
checksum is computed once, for the IPv4 variant.

## Build time

The analysis runs in the compiler, once for each pair of types that a function of `puns` is used
with. `cargo xtask build-time` ([`xtask/src/build_time.rs`][build-time]) builds a crate that uses
`puns` in several ways, and prints the best of 3 builds of that crate alone, in seconds. These
times were measured on 2026-11-21, with the library at commit [`4176671`][commit], `rustc`
1.100.0, on the machine above, at a load average between 3.9 and 4.3.

Each struct has five fields, among them an enum and a `bool`. Reading each of 100 structs from
bytes adds 1.7 s to a debug build, and 0.74 s to a release build: about 17 ms and 7 ms for each
pair of types.

| Structs | debug, declared | debug, read | release, declared | release, read |
|---:|---:|---:|---:|---:|
| 100 | 0.42 | 2.13 | 0.43 | 1.17 |
| 200 | 0.75 | 4.19 | 0.77 | 2.24 |

A view enum compares its variants with each other. Where each variant starts with a different
constant, it distinguishes most of them by that constant, and its build grows about in proportion to
its number of variants. Where the values of each variant are also values of the next one, the
analysis compares more pairs, and the build grows as the square of the number of variants, or faster
in a release build.

| Variants | distinct, debug | distinct, release | overlapping, debug | overlapping, release |
|---:|---:|---:|---:|---:|
| 4 | 0.19 | 0.18 | 0.20 | 0.19 |
| 8 | 0.25 | 0.22 | 0.28 | 0.26 |
| 16 | 0.42 | 0.32 | 0.72 | 0.47 |
| 32 | 0.81 | 0.59 | 2.53 | 1.89 |
| 64 | 1.74 | 1.26 | 9.89 | 24.9 |

## Costs

A check never allocates and never panics. Without a checksum or a walk inside a walk, its time grows
with the size of the value, or, for a `size` or `len` rule, with the number of bytes given. A `walk`
takes at most `8 * size + 128` steps, where `size` is that number of bytes, so a checksum or another
`walk` inside a `walk` multiplies the time by the size, at each level. Each checked pun is code
specialized for its two types, and it relies on inlining, which a debug build mostly does not do, so
measure the checks in release.

> [!WARNING]
> Walks nest up to 4 deep, so a checksum inside the deepest one takes time in proportion to the
> fifth power of the size. Consider this before you give such a type untrusted bytes.

[codegen]: ../puns/examples/codegen.rs
[checks]: ../puns/benches/checks.rs
[record]: checks-2026-11-21.txt
[build-time]: ../xtask/src/build_time.rs
[commit]: https://github.com/uael/puns/commit/4176671c690b6a75de775d3d4c6f6c607736ff00
