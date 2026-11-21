# Design

## Goal

Type punning proven at compile time, on stable Rust: a safe alternative to `transmute`, and to
hand-written parsing of bytes. A pun reads a value of one "plain data" type, the *source*, as a
value of another, the *target*: structs of any `repr`, enums of any `repr` (without one, the
layout that `rustc` chose is found), `repr(C)` unions, tuples, arrays, slices and slice-tailed
structs, `str`, `MaybeUninit`, types without values. A pun is

- **refused** (a compile error) when it can never be valid,
- **proven** (no code at all) when it is always valid,
- **checked** (only what could not be proven, by a `try_` or a `from_*` function) when it is
  valid for some values,

with sizes, lengths and assertions that are in the data itself, told by `#[derive(Pun)]`.

`tests/` and `examples/` (Ethernet, then services over IP, transports, routing, links, images,
media, archives, executables, storage and encodings) are the reference for what works; each
example says what it does not cover, and why.

## API

The conversions are a grid: what is read (a typed value, or bytes) by how much of it (the whole,
its front, its back).

|               | Whole                                      | Front                          | Back                           |
|---------------|--------------------------------------------|--------------------------------|--------------------------------|
| a typed value | `pun`, `pun_ref`, `pun_mut` (+ `try_pun*`) | `head`, `head_ref`, `head_mut` | `tail`, `tail_ref`, `tail_mut` |
| bytes         | `from_bytes`, `from_bytes_mut`, `as_bytes` | `from_head`, `from_head_mut`   | `from_tail`, `from_tail_mut`   |

- The first row is proven (nothing at run time) but for `try_pun*`, which is checked. A part is
  the first or the last `size_of::<D>()` bytes (or, for `head*`, as many elements of a slice as
  fit); `tail*` needs a sized source.
- The second row is checked: the address, the size, then what bytes do not prove of the target
  (`from_tail*` takes the size first, since where the last bytes start depends on it, and
  `from_head*` that there are enough bytes for the sized part, then the address).
  `from_bytes*` and `as_bytes` are sugar for `try_pun_ref::<[u8], D>`, `try_pun_mut::<[u8], D>`
  and `pun_ref::<S, [u8]>`, with identical behaviour: they pin one side to bytes (an unsuffixed
  integer literal array would otherwise be an `[i32; N]`) and take one type parameter.
  `from_head*` takes as much as the `size` or `len` rule of the target says; `from_tail*` needs
  a sized target, whose extent cannot be found from the end otherwise.
- A view enum (variants that hold references, see `#[derive(Pun)]`) gets `from_bytes` and
  `from_head`: the first variant that the bytes are.
- Every function is a `const fn`.

A value that holds self-relative pointers (declared by the crate that has them, as `nearest`
does: see "Self-relative pointers") is read by these functions as plain data, and made by none
of them. It has its own: `view_ref` (proven, pointers followed, by reference only) and
`try_view_ref`, which are not `const fn`s.

## What stable Rust gives us

Probed, not remembered. Where a feature is stable, the version is the first stable release that has
it, probed on that release and on the one before (1.76 and 1.78 for `offset_of!`, 1.78 and 1.82 for
inline `const`, 1.82 and 1.83 for `&mut`, 1.98 and 1.99 for `size_of_val_raw`):

| Needed                                       | Stable?                                        |
|----------------------------------------------|------------------------------------------------|
| `TransmuteFrom`, `ptr_metadata`              | no                                             |
| const traits, `generic_const_exprs`          | no                                             |
| reflection, `offset_of!` on enums/slices     | no                                             |
| `offset_of!` on sized fields                 | 1.77, even when the struct is unsized          |
| inline `const { }` in generic code           | 1.79                                           |
| `&mut` in `const fn` (with loops, raw ptrs)  | 1.83                                           |
| `size_of_val_raw`, `align_of_val_raw`, const | 1.99                                           |

The last one sets `rust-version`: the layout of an unsized type (a struct that ends with a slice
or a `str`) is read from a pointer to it with a made-up length (`Pun::PROBE`), in `const`, which
`size_of_val` cannot do (it needs a reference, which needs a valid value). On 1.98, `puns` does
not build: `layout_for_ptr` is unstable, and so is calling it in `const`. CI builds the crate and
its examples on 1.99 (job "Minimum Rust version").

Const evaluation runs about 400 000 loop iterations per second and is stopped by a deny-by-default
lint after 2 000 000 basic blocks. Any analysis whose cost grows with the *size* of a type (one
step per byte) or with its *length* (one step, or one type instantiation, per array element) is
therefore out. This is what both previous attempts ran into.

## Architecture

### 1. The layout of a type is a value, not a type (`layout.rs`)

`Pun::LAYOUT` is a `const` tree built by `const fn`s from what the compiler says of the type
(`size_of`, `offset_of!`, `size_of_val_raw`...):

- **scalar**: 1 to 16 bytes, a byte order, a sorted list of valid ranges. `bool` is `0..=1`,
  `char` is two ranges, `NonZero` is `1..`, an enum without fields is the values of its variants
  (read from the compiler, whatever its `repr`). What a scalar requires (`valid`) and what it is
  known to be (`known`) are two sets, which are the same but for a field restricted by
  `valid(..)` (required, not known: section 5) and a scalar that may be uninitialized
  (`Spec::maybe_uninit`: required to be some values, known to be anything, padding included);
- **struct**: fields at their actual offsets (any `repr`: the offsets come from the compiler);
  bytes that no field covers are padding. A struct may have **rules** (see below);
- **enum** with fields: a tag (a scalar at a fixed place), and for each variant its tags and a
  struct of its fields. The representations that Rust specifies (`repr(C)`, `repr(u8)`..., RFC
  2195) are made of `repr(C)` structs, which the derive declares and asks the compiler for; the
  result is checked against the size and the alignment of the enum. Without `repr`, the layout
  that `rustc` chose is found by experiments in const evaluation (section 10): a direct tag, or
  a *niche* in the fields of one variant, which is then the variant of several tags (the values
  of the scalar that holds the niche) and holds the tag in its fields (`Variant::holds_tag`). A
  variant that holds a type without values has no tag (none of its values exist);
- **union** (`repr(C)`): an enum without tag, whose variants are its fields, each at offset 0,
  of which a value must be one (`Enum::union`, section 5);
- **no value**: `Infallible` and enums without variants are an enum without variants
  (`Layout::uninhabited`), whose layout no value is (`Layout::is_inhabited`), and which proves
  anything as a source (no value of it exists). `MaybeUninit<T>` is padding of the size of `T`;
- **array** and **slice**: an element layout and a count (or none);
- **pattern**: a tail of bytes that a DFA accepts. `str` is the UTF-8 automaton (8 states). An
  automaton is known by its states and edges: its `id`, a field that safe code writes, only tells
  two apart (two patterns are the same layout only if their automata are the same, state by state),
  and a value is checked by `str::from_utf8` only where the automaton is the one of UTF-8, which
  is decided at compile time.

This tree *is* an automaton, in a compact hierarchical form: a position in it is a state.

### 2. The analysis is ordinary `const fn` code over those values (`analysis.rs`)

For a source `S` and a target `D`, `analyze` decides language inclusion, `L(S) ⊆ L(D)`, by walking
both trees in lockstep (a product automaton), with what keeps it cheap:

- **runs**: a subtree without constraint is one step, whatever its size (`[u8; 1 << 40]`);
- **scalars are compared as sets, not byte by byte**: aligned scalars compare their range lists;
  misaligned ones, or ones of different byte orders, go through an exact decomposition of integer
  ranges into byte boxes (`digit_box`, `box_meets`), so `NonZero<u16>` little-endian is known to
  be `NonZero<u16>` big-endian, and `u32 >= 0x100` is known not to be;
- **periods**: arrays and slices are not unrolled. When the target is at the start of an element
  and the source is inside a repetition, the least common multiple of both strides is analysed
  once and the rest is skipped. Two slice tails, which never end, are settled in one period;
- **patterns** are run with sets of states over the runs of the other side, each box of a scalar
  on its own; a repetition is left as soon as one of its elements changes nothing to the set.
  Two patterns are compared by a product of their automata;
- **enums** are related variant by variant: each variant of the source with the variants of the
  target whose tags it has (none: that variant is not valid), as two structs, *knowing the tag*
  (`Known`: the tags of both, which prove a niche of the target that the source variant has as
  padding). A variant of the source is proven if all its tags are of one variant of the target,
  which it proves. From something that is not an enum (or an enum whose tag is elsewhere),
  what the source holds at the tag is found (what a scalar of the same size and byte order is
  known to be, else any value: `tag_values`), and each variant whose tags meet it is related to
  the source over the bytes of the enum, knowing the tag there: the enum is proven if every
  value the source may hold there is the tag of a variant that has values, each of which is
  proven (`relate_enum_from`); else the enum is checked as a whole at run time. A variant that
  the source can never be is not looked at; a byte that one that it may be needs initialized
  must be, and a source that may be padding at the tag is an error. As a source, an enum is
  known by its tag and by the bytes that every variant that has values initializes.
- **unions** are proven by one of their fields that the source proves; else they are checked
  field by field at run time, if no field's check would read a byte that may be padding in the
  source (else refused: `relate_union_from`). As a source, a union is padding. Only the same
  union proves a union from a union (`Layout::equals`).
- **types without values** are no value of any type that has some (`relate_at`: `disjoint`), and
  a variant or a struct that holds one has no value either.

It answers:

- *fit*: how sizes map (always / for some slice lengths / never / truncated), with the trailing
  padding of unsized structs accounted for, and checked against `size_of_val_raw`;
- *uninit*: a target byte that must be initialized would be read from padding (always an error);
- *infallible* / *fallible* / *unreachable*;
- *marks*: the scalars, patterns and rules of the target that the source does not prove: exactly
  what a checked pun must check at run time;
- *mutable*: the reverse inclusion, required to hand out a `&mut`.

**Shifted relations** (`relate_at`, `analyze_tail`). The cursors of both layouts count offsets from
the start of the source, and each starts at its own base: the target of `tail` starts at
`shift = size(S) - size(D)`, the source of the reverse relation of `tail_mut` there as well.
Nothing else changes with the shift, since every step only compares what is at the same offset of
both: a run of the target with what lies under it (scalars cut by the shift are decomposed into
byte boxes as misaligned ones are), an enum only with an enum whose base is that offset, the
rules of a struct only with the same rules of a struct of the same extent at that offset in the
source (`mark_rules` from the shift, section 4). Periods stay correct: a period is the least common multiple of the strides of two
repetitions, and is only taken when the target is at the start of one of its elements; what lies
under a run of the target a period later is the same whatever the phase of the two repetitions,
which a shift only changes. The relation reports offsets of the source; messages subtract the
shift. `tail_ref` needs the alignment of the target not to be greater than the one of the
source: the shift is then a multiple of it, as sizes are multiples of alignments.

**Mutable parts** (`mutable_part`). Through `head_mut` or `tail_mut`, any value of the target can
be written over the bytes `shift..shift + size` of the source, and the source must stay valid:

1. the reverse relation over those bytes must be infallible, as `pun_mut` requires of the whole:
   every value of the target, padding included, is valid for what the source has there. A
   scalar of the source that the part covers only in part is not proven (what the rest holds is
   not known), and neither is an enum whose base is outside the part;
2. nothing of the source outside those bytes may depend on them (`cut`): a struct of the source
   with rules (`size`, `len`, `assert`, regions, walks) that covers any of them is refused, since
   its rules read its fields, and so is an enum (or a string) of the source of which they are
   only a part: its tag on one side, fields on the other. A target that takes as many elements
   as fit may end anywhere in the source, and is refused over a source with enums. A struct with
   rules that lies within the bytes is not refused if the target has the same rules over the
   same extent there (`proves_rules`, the test of section 4 the other way): what is written
   through the target meets the rules of the source if it meets those of the target. The same of
   the elements of an array that lie within the bytes (all at once where the target has elements
   of the same size there, else each, up to 64).

This is conservative: a rule that does not read the bytes of the part, or an enum all of whose
variants would accept what is written, is refused all the same. The model check counts these as
imprecise, never as unsound.

Errors are post-monomorphization (`const { .. }` in the generic function), since there are no
const traits to turn them into trait bounds. `cargo check` does not see them, `cargo build` does.

### 3. Run-time validation is type-directed and guarded by the analysis (`validate.rs`)

`try_*` and `from_*` functions recurse over the *types* (associated types: element, list of
fields) in `const fn`s; each check sits behind `if const { marks say so }`. What the analysis
proved never reaches code generation.

Measured on x86-64, `opt-level = 3` (the functions are in `examples/codegen.rs`):

| Function                                           | Machine code                               |
|----------------------------------------------------|--------------------------------------------|
| `&Position -> &[u8; 2]`, `[bool; 4] -> u32`        | `mov rax, rdi; ret`                        |
| `&Frame<Ipv4<[u8]>> -> &[u8]`                      | `lea rdx, [rsi + 34]` (the length)         |
| `&Packet<Datagram> -> &Packet` (the same rules)    | `mov rdx, rsi; mov rax, rdi; ret`          |
| `&[u8; 2] -> Option<&Position>` (a `u8`, a `bool`) | one `cmp byte ptr [rdi + 1], 2`            |
| `&[u8] -> Option<&Header>`                         | size, 4 bytes compare, `bt`, `cmp`         |
| `from_head::<Frame<Ipv4>>(&[u8])`                  | 29 instructions, no loop, no call          |
| `Ipv4::options()` (a region)                       | 10 instructions, no branch                 |
| `&[u8] -> Option<&[bool]>`                         | 64 bytes at a time, no branch in a block: 46 |
| `&[Shape] -> &[Drawing]` (enums with fields)      | `mov rdx, rsi; mov rax, rdi; ret`          |
| `&Drawing -> Option<&Shape>`                       | the tag, then one `cmp` for a `bool`: 12   |
| `[u8; 4] -> Option<Shape>`                         | 14 instructions, no loop, no call          |
| `Ticket <-> u32` (no `repr`, a niche in `NonZero`) | `mov eax, edi; ret`, both ways             |
| `[u8; 4] -> Option<Small>` (no `repr`, direct tag) | the tag, one `cmp` for the `bool`: 13      |
| `u8 -> Option<Choice>` (a union: `bool`, `u8in`)   | one `cmp cl, 4`: 7                         |
| `&[u8] -> Option<&Ipv6>` (a walk, two checksums)   | 207 instructions, no call                  |
| `head_ref::<Ipv6, [u8; 40]>`                       | `mov rax, rdi; ret`                        |
| `tail_ref::<[u32; 4], u32>`                        | `lea rax, [rdi + 12]; ret`                 |
| `from_head::<Position>(&[u8])`                     | the size, one `cmp` for the `bool`: 7      |
| `from_head::<Udp>(&[u8])` (a size in the value)    | 15 instructions                            |
| `from_tail::<Trailer>(&[u8])` (an enum, a magic)   | the size, `bt`, one `cmp`: 18, no loop     |
| `view_ref::<Old, New>` (recursive, with pointers)  | `mov rax, rdi; ret`                        |
| `view_ref::<[Old], [New]>`                         | `mov rdx, rsi; mov rax, rdi; ret`          |
| `try_view_ref::<Maybe, Owned>` (checks a pointer)  | one `cmp dword ptr [rdi + 4], 0`: 4        |

A count is not a time. The slice of `bool`s was a loop of one `cmp` per byte, 13 instructions,
which stopped at the first invalid one; it is now 46 and takes a fraction of the time on a long
slice (`benches/checks.rs` times the checks next to the same ones written by hand). The IPv6
packet walks with a state of two registers ("Walks", section 4): with the state it had before
(the kind in the low 8 bits), it was 233 instructions when the operations of its walk were applied
as they are now and the others by an unrolled evaluator, and 254 with every operation applied so:
the same arithmetic, which LLVM allocates registers to and selects instructions for a little
otherwise (a step of the walk was two instructions longer). Computed in the width that its values
fit ("Narrowing", section 4), it went from 236 instructions to 207, `from_head::<Frame<Ipv4>>`
from 37 to 29 and `Ipv4::options()` from 16 to 10.

`from_head` turns the value of a `size` or `len` rule into a number of elements in a `usize`,
where the optimizer knows that a field fits one (it was 29 instructions for `Udp`, 43 for
`Frame<Ipv4>`).

### 4. Rules: what is not a property of bytes

A struct can have rules, which `#[derive(Pun)]` compiles from expressions over its integer
fields to a constant postfix program (`layout::Op`):

- `#[pun(size = EXPR)]`: the size in bytes of the value;
- `#[pun(len = EXPR)]`: the number of elements of its slice;
- `#[pun(assert(EXPR))]`: anything else (bit fields, relations between fields, a minimum size);
- `#[pun(regions(a = EXPR, b))]`: consecutive regions of the tail, with an accessor for each.

An expression means what the same Rust expression means where every integer (a field, a
constant, a literal) is a `u128`, or is refused at compile time: never something else (which is
not how it is computed: see "Narrowing" below). The derive
types it as Rust does (an integer or a `bool`: `!` is bitwise on one and logical on the other, an
integer is not a condition), reads literals itself (no float, no negative number, no signed
suffix), converts constants through a trait that only unsigned integer types implement, and makes
`as uN` keep the low bits. Where Rust would panic (an overflow, a division by zero, a shift by
128 or more), the rule fails; what Rust does not compute (the right side of a `&&` that the left
side decides, the branch of an `if` that is not taken) is not computed, and what it computes (a
`let`, used or not) is: the derive puts an `Op::Shortcut` after the left side of a `&&` or a
`||`, and an `Op::Then` and an `Op::Else` before the branches of an `if`, which skip what follows
when it is not computed (`check_ops` checks that what they skip is exactly a right side or a
branch, so that the stack is as deep after it either way). Nothing that is computed can then be
left out of the value of an expression: its first failure is its failure, the evaluator stops
there, and what follows is compiled knowing that nothing failed. The
derive also counts what the evaluator has room for (the depth of its
stack, the variables, the operations of an expression and the walks one in another) and reports
an excess at the expression, with the numbers; `check_ops` checks it again.

An expression can read the value beyond its fields: `size_of_val(self)`, `le(offset, N)` and
`be(offset, N)` (an integer anywhere in the value), `crc32(start, end)` and `sum16(start, end)`
(the frame check sequence of Ethernet, the checksum of IP), and variable-length integers:
`leb128(offset)`, `quic(offset)` and `vlq(offset)`, and with `_len` the number of bytes of each
(`Op::Varint`: an offset is replaced by what is there, as `be` does; a read never goes past the
value). So a trailer is a region, and "the 4 last bytes are the CRC of what is before" is an
assertion; an MQTT packet is `size = 1 + leb128_len(1) + leb128(1)`.

Checksums other than these two are told by their parameters (`Op::Crc`, `Op::CrcZeros`,
`Op::Checksum`): `crc::<WIDTH, POLY, INIT, REFLECT, XOROUT>(start, end)` is any CRC of up to 64
bits, whose table of 256 entries (2 KiB) is made in const evaluation, once for each parameters;
with two more offsets, the bytes between them are taken as zeros, for a CRC over the header that
holds it (GPT). `fletcher16`, `adler32` and `sum` are the other checksums of the formats of the
examples. As every read of the value, they are bounded by it: a range out of it fails. A size or a length reads the value too
(not its size, which it defines): `from_head` evaluates it over the bytes that there are, which
are initialized and in bounds whatever it reads, and the value it takes is then validated as a
whole, the rule included, over its own bytes; a rule that reads beyond what it sizes fails there. A checksum that covers several layers
(the one of UDP and TCP covers the addresses of the IP header) is an assertion of the struct that
holds them all: the IPv4 packet.

An expression can name what it computes and choose (`{ let header = ..; .. }`,
`if .. { .. } else { .. }`; what a `let` binds is computed, as in Rust, whether it is used or not),
and it can **walk**: `walk(START, |at| COND, |at| NEXT)` is what
`START` becomes when it is replaced by `NEXT` for as long as `COND` is true of it. This is how
what lies at an offset that depends on what is before is reached: the checksum of UDP, TCP and
ICMPv6 is checked by the IPv6 packet behind any chain of extension headers, each of which tells
what follows it and how long it is; at the end of a string, in a `size`. A walk can be in any rule,
in another walk, several in an expression. A walk takes at most a step for each bit of the value, plus
128 (the bits of a `u128`), so that it ends: a walk over the bits of a field fits, whatever the
size of the value.

**Walks.** The state of a walk is one `u128`: two numbers that it carries are packed in it. On a
64-bit target a `u128` is two registers, so the place to pack them is bit 64: `(kind << 64) +
start`, read as `at >> 64` and `at & 0xffff_ffff_ffff_ffff`, is each a register, where `kind +
(start << 8)` needs a `shrd` to read `start` and a merge to write it back, on the path from one
step to the next. The IPv6 packet of the examples walked so: behind 12 extension headers it took
1.3 times the instructions of the same loop by hand, but 2.2 times the time (on the code before
these changes as well); packed at bit 64, its loop was the loop by hand (21 instructions against
about 20, fewer in all: 310 against 322), but for the carries of the low half into the high one
that a `u128` has (`adc`), which kept `start + 8 + length * 8` from being one `lea`, and it took
1.37 times the time. The halves are now computed apart where the low one is known not to carry
("Narrowing"): the loop is 16 instructions, the check 210 against 322, and 0.74 times the time of
the loop by hand (the median of 5 runs).

What cannot be computed (an overflow, a division by zero, a shift by 128 or more, a read out of
the value) makes a rule fail where Rust would panic, unless it is the right side of a `&&` or
`||` that the left side decides without, as Rust does not compute it: `protocol != 17 ||
be(..) == 0` holds for a packet that is not UDP, however short, and costs nothing more for it.
`<<` drops the bits that it shifts out, as Rust does.

Two more options are not rules, but save the code that would otherwise be written by hand:

- `#[pun(tag(Key = Type))]` implements `Tag<Key>`: "when carried by something that tells what
  it carries with a `Key`, this type comes with a `Type`". A frame is then
  `struct Frame<P: ?Sized + Tag<EtherType> = [u8]> { .., ether_type: P::Tag, payload: P }`, and
  `Frame<Ipv4>` requires the EtherType of IPv4 while `Frame` takes any (bytes go with any tag);
- `#[pun(get(name: Type = EXPR))]` generates a `const fn` getter, for bit fields. Here `EXPR`
  is plain Rust, in which an integer field, wherever it is named, is its value as a `u128`: any
  `const fn` can be called;
- a region can be given a type, `regions(options = EXPR, payload: P)`: its accessor then returns
  a `&P`, if the bytes of the region are one. This is how a packet whose header has a variable
  size carries a typed payload;
- on an enum whose variants each hold a reference,
  `enum AnyFrame<'a> { Ipv4(&'a Frame<Ipv4>), Arp(&'a Frame<Arp>), Other(&'a Frame) }`, the derive
  generates `from_bytes` and `from_head`, which return the first variant that the bytes are (and
  no `from_tail`: the variants may be unsized): telling one thing from another is a declaration
  too. How each variant is decided once is "Views", below.

Rules are checked when a value comes from something else, tell `from_head` how long a value is, and
are proven only by the same rules, at the same place of the source, in a struct of the same extent
(`Layout::same_rules`, `analysis::proves_rules`). A rule is a function of the bytes of its
struct, of its size (`size_of_val(self)`, a `size` rule), of the number of elements of its tail
(a `len` rule) and of the bytes of that tail (regions): a struct of the source with the same
rules in the same order (`Rule::equals`), the same sized part and the same tail (where it starts,
the size of an element, and for a sized struct its bytes and the elements of the array it ends
with) computes the same thing over the same bytes, whatever its fields, which are related to the
target as any other. So `Frame<Ipv4<Udp>>` proves the rules of `Frame<Ipv4>` (the payload of
`Ipv4` is a typed region of its bytes, not a field of another type), and a part of a dynamic
struct, which has another size, does not prove those of a struct of its own size. The elements
of an array or a slice share their leaves: their rules are proven at once, where the source has
elements of the same size at the same place (what the first one is to the source, each is), else
checked for each. A rule that reads the bytes of its struct reads its padding (section 5): the
same tests decide whether it is checked, and so whether the padding is read
(`Cursor::padding_is_read`).

**No interpreter.** Every operation of every rule is applied by an instance of a function of its
own (`validate::steps`), in which it is a constant: what is compiled is the arithmetic of the
expression, and the loop of each walk, never a `match` on operations at run time. The derive
writes the rules of a struct that have an expression as a list at the type level (`Pun::Rules`,
`RuleAt<RULE, STEPS, NEXT>`), and the operations of each as a balanced tree (`steps::One<I>`,
`steps::Two<A, B>`, and `steps::Walk<I, COND, NEXT>` for a walk, whose two expressions are trees
of their own, at any depth): the recursions down the tree are as deep as its logarithm, where a
recursion over a number that is a type, before, was as deep as the expression (and was only used
for the first walk of an `assert`). Nothing of what a list says is trusted: where it is used, it
is checked against the layout (`validate::listed`, `covers`): every rule with an expression is
listed, once, in order, every operation of each, a walk where there is one and only there (the
functions name every part of the tree, whatever its kind, so where a part is not a walk they name
the expression they are in again, nothing new). `Machine::apply` never sees a walk. The list
being a type, a struct without rules costs nothing more to compile; `from_head` finds a `size`
rule of the last field by the types too (`Fields::Last`), and checks that it is the one of the
layout.

The machine that applies the operations (`layout::Machine`) has a stack of 16 values and 8
variables, each a variable of the machine rather than an element of an array, reached by a
`match` whose cases are all values (the last one is the default case: a `match` with one more
case costs instructions): once inlined, a local variable per stack slot. An expression has at
most 256 operations, and nests at most 4 walks; the derive says where one goes beyond, and by how
much, and `check_ops` checks it again. In a debug build, where nothing is inlined, each operation
is a call.

Before, the first 24 operations of an expression were unrolled and the rest applied in a loop, a
`match` on each at run time; the walks of an expression other than the first of an `assert`, and
their expressions, were applied in that loop too. The IPv4 packet of the examples, whose last
`assert` (the checksums of UDP and TCP over the addresses of the packet) is longer than 24
operations, was then checked 15 times as slowly as by hand; it then ran 1.18 times the
instructions of the same check by hand (`valgrind --tool=callgrind`, UDP in 64 bytes: 256
against 217, where it was 1885), and 1.11 times since "Narrowing" (241).

What is not computed is told by an index (`Machine::skip`): the operations before it are not
applied. A `Shortcut` that decides sets it past the right side, a failure to the end. Where the
operations are constants, so is the index on each path through the code, and the optimizer
makes jumps of it: no test is left where nothing is skipped. This replaced a bit for each value
of the stack that told whether it had failed, which made each operation carry failures along
(a step of the walk of IPv6 took a third more time for it, and the side of a `||` that was not
needed was computed all the same).

**Narrowing.** An expression means what it means on `u128`, but most of its values are known to
be much smaller: a field of two bytes, a sum of two of them, a checksum of 16 bits, an offset in
the value. On a 64-bit target a `u128` is two registers, its addition a carry from one into the
other (`adc`), its product three multiplications, its division a call; in a loop, the optimizer
does not know that the high half of a state is zero. So the range of every value of an expression
is found at compile time (`narrow.rs`), and each operation is computed as narrowly as its range
allows, with the same result and the same failure as on `u128`:

- A value is known as two intervals, one for each half of a `u128` (`narrow::Range`): `(kind <<
  64) + start` is a kind of `0..=255` in the high half and an offset in the low one. A field, a
  read of `n` bytes and a checksum are as wide as they are, a literal is itself, a comparison
  `0..=1`, `size_of_val(self)` at most `isize::MAX` (a QUIC integer 62 bits, the other
  variable-length integers anything). Each operation makes the range of its result from those of
  its operands, and tells whatever it may fail: a sum whose bounds do not overflow cannot, a
  difference whose left side is never less than its right side cannot.
- A read that succeeds (`be(at, 1)`, a checksum over `start..end`) tells that its offsets are in
  the value: a variable that was read at is at most `isize::MAX` after the read, for the rest of
  the expression. Not after a part that may not have been computed (the right side of a `&&`, a
  branch of an `if`), where the variables are what they were before it or after it, and not in
  the `else` branch, which starts from what was before the first one. This is what bounds the
  offset of a walk, whatever the number of its steps: the step of IPv6 reads at `start` before it
  adds to it, so `start + 8 + length * 8` is less than `2^64`, and does not carry.
- The state of a walk is a fixed point: what it starts as, joined with what a step makes of it,
  twice, then each bound that still moves goes as far as it can (at most four more steps). The
  number of steps of a walk (at most `8 * size + 128`) bounds nothing: on 64 bits it is more than
  `2^64` for a value of `isize::MAX` bytes. The walks of a walk of a walk (two deep or more) have
  a state that may be anything, so that the analysis stays a few times the length of the
  expression.
- How each operation is computed is a `narrow::How`, a constant of the instance of the function
  that applies it, as the operation is ("No interpreter"): the width, a register (64 bits, or 32
  where `usize` is narrower) or 128 bits; whatever it is tested (an addition, a product or a shift
  that cannot overflow is not); an addition or a difference of 128 bits whose low halves cannot
  carry or borrow, computed on each half alone; and the width of its result, to which it is
  truncated. The truncation changes nothing, and tells the optimizer what the value is where it
  could not know: the state of a walk, at each step, and the variables read in its expressions.
- The plan of an expression is a constant of its type (`narrow::Planned<X>::PLAN`), and where the
  expressions of a walk start (the variables, the state) one of theirs (`Ops::ENTRY`): made once
  at compile time, for each expression. The tests of `puns` build in about 4% more time.

Not narrower than a register: on 64 bits, everything in 32 bits where it fits was as many
instructions in all, but `40 + payload_length` in 32 bits is not known to be the size of the value
that the `size` rule compared in 64, and the bounds of what it reads were tested again (the UDP
packet behind 3 headers: 217 instructions against 209); a remainder in 32 bits after a sum in 64
was worse than either (243 against 241 for IPv4). The bounds of a read are one comparison of the
offset, with `size - n`, which does not change in a walk, where they were two.

That the narrowed evaluation is the one on `u128` is tested apart from the analysis of puns
(`narrow_tests.rs`): every binary operation, of every pair of ranges at the limits of 32, 64 and
128 bits and of the halves, over values at their bounds and others, as on a target of 64 and of
32 bits, against the operation on `u128` with every test (the result is in the range that the
analysis says, and what it says cannot fail does not); random expressions, with variables, parts,
reads and walks (random ones, an offset that moves by what it reads, two numbers in the halves of
the state), over random and boundary bytes, narrowed and not, which must have the same value or
fail alike; and `tests/rules_are_rust.rs`, which compares rules at the limits of the widths, and
walks, with Rust. An analysis that does not test a carry, that takes no fixed point, that does not
join what follows a part, or that is wrong of the range of a product, a remainder or a shift, was
tried, and is caught. What it cannot catch is a bound of `isize::MAX` taken smaller: no value of
the tests is large enough to tell.

Measured with `benches/checks.rs` (instructions per check, `callgrind`):

| Check                                  | before | now | by hand |
|----------------------------------------|-------:|----:|--------:|
| IPv4, UDP, 64 bytes                    |    256 | 241 |     217 |
| IPv4, protocol 253                     |    127 | 120 |     106 |
| IPv6, TCP behind 3 extension headers   |    103 |  75 |     106 |
| IPv6, TCP behind 12 extension headers  |    310 | 210 |     322 |
| IPv6, UDP behind 3, 64 bytes           |    251 | 209 |     229 |
| IPv6, UDP, 1400 bytes                  |    821 | 803 |     798 |
| a frame of 6 things: UDP over IPv4     |    283 | 267 |     244 |

The cases of the bench that compute no rule are unchanged.

**Views.** `from_bytes` and `from_head` of a view enum answer what trying each variant in turn
would answer: the order of the variants is the order of preference. But each variant is checked
once at most, and only for what is not known of it already (`view.rs`):

- A variant whose first constrained scalar the bytes do not have (`rejects`: an EtherType, a
  version) is passed over on that scalar alone, before anything else of it is computed. These
  comparisons of the same bytes, one for each variant, the optimizer makes one decision of: a VLAN
  frame meets the EtherTypes of IPv4 and of ARP as in a `match`.
- A variant has a *base*: the first later variant whose type takes every value of the type of the
  variant, over the same bytes. The analysis decides it at compile time, as it decides that
  `pun_ref` is proven (`Frame<Ipv4>` takes every `Frame<Ipv4<Udp>>`, `Frame` takes both). The base
  is decided first: bytes that are not one are not the variant either; of bytes that are, the
  variant is checked for what its base does not prove of it, the marks of `try_pun_ref` from the
  base (the protocol of a packet, and not its checksums again).
- What is decided is kept (`view::Known`, a state for each variant): a base that several variants
  share (`Frame<Ipv4>` of UDP, TCP and ICMP), or that is tried in its turn after it was a base, is
  decided once. Each variant is a function of the derive, which decides the base of the variant
  (calls the function of the base, chosen by a `match` on a constant) before the variant; once
  inlined, the states are constants on each path, and no state is kept at run time.
- For `from_head`, a base must also take the bytes that the variant takes (`same_extent`): both
  tell their extent (a size, a `size` or a `len` rule, which a valid value meets), or neither does
  and both take as many elements as fit alike (the same alignment and element, a head of the same
  remainder). The variant then takes the extent of its base, without trailing padding, which each
  adds of its own.

That the answer is the same follows from what the analysis proves and checks. Bytes that are not a
value of the base are not a value of the variant, since every value of the variant is one of the
base. Bytes that are a value of the base meet everything that it requires, and from such a value
`try_pun_ref` checks exactly what the variant requires more: what it leaves unchecked is implied
by the requirements of the source (what the model check checks of every pair). A base is a later
variant: it is decided before its turn only as a step of an earlier variant, and returned only in
its turn. The error is `Error::Validity`, as it was: a view does not tell why no variant fits.

Soundness does not rest on the code that the derive writes: the types of the variants are a tree
at the type level (`view::List`, balanced, so that what proves a type of it is as deep as the
logarithm of the number of variants), of which the layouts and the types are both taken, by a
place in it that is a type (`view::First<view::Here>`), sealed; a state is only written by
`view::decide` for the type of its place; a variant is decided from its base only where the state
of the base says that the bytes are one, of the extent kept with it. The derive chooses the order in which variants are decided:
a variant whose base was not decided first is not found, which is never unsound, and which the
model check would see.

The bases are found by the analysis of pairs of variants, at compile time: for each variant, the
later ones, until one takes it, but those that a scalar at the same place tells apart (another
EtherType, another protocol: `view::apart`, a walk of the layouts), then the analysis of what is
left of the variant from the base found. The base of `from_head` is the one of `from_bytes` if it
takes the same bytes, which it does where both tell their extent. A variant is validated by one
instance of the validation for both functions: what the two bases do not prove (the same marks
where the base is the same), or everything bytes do not prove where it has none. The function of
each variant names the functions of all the variants after it (one is kept, by the `match` on a
constant), so what is compiled grows as the square of the number of variants: the Ethernet example
(19 variants) builds in 5.2 s in debug, where it took 4.4 s, and a view of 131 variants in 7.9 s,
where it took 3.5 s.

Measured with `benches/checks.rs` (instructions per check, `callgrind`), against the same check by
hand:

| `AnyFrame` of the bench (6 variants)   | before | now | now, a call | by hand (a call) |
|----------------------------------------|-------:|----:|------------:|-----------------:|
| UDP over IPv4 (the first variant)      |    287 | 283 |         273 |              244 |
| TCP over IPv4                          |    307 | 302 |         290 |              286 |
| UDP over IPv4, a wrong checksum        |    508 | 266 |         255 |              246 |
| ARP                                    |     49 |  25 |          41 |               46 |
| VLAN                                   |     53 |  25 |          41 |               45 |
| another EtherType (the last variant)   |     48 |  21 |          36 |               40 |

A wrong checksum was computed twice, for `Frame<Ipv4<Udp>>` and again for `Frame<Ipv4>`; it is
now computed once, for `Frame<Ipv4>`, the base of the first three variants, and the UDP variant is
then its protocol alone. The EtherType is now one decision (a `cmp` of its 16 bits for each
value, then the variant), where the size and the EtherType of each variant were tested in turn.
The optimizer now inlines the view where the bench calls it (its checksums stay calls), where it
called it before, as it calls the check by hand: "now, a call" is `from_bytes` behind a function
that is not inlined, as the check by hand is.

### 5. What the type system enforces, and what it does not

Soundness rests on one distinction (`Spec::valid` and `Spec::known`):

- what a type **guarantees**, whatever safe code does: `bool`, enums (with or without fields),
  `NonZero`, `str`, and library types with a private field and a checked constructor
  (`u16bec<V>`, `u8in<MIN, MAX>`);
- what a struct merely **requires** of what it is made from: `#[pun(valid(..))]` on a plain
  field, and rules. Safe code can write any value to a `pub version: u8`.

A requirement is checked (or proven) on the way in, but it never proves what another type
guarantees: `&[Unchecked] -> &str` needs a check even if `Unchecked` is "`valid(..=0x7f)`", while
`&[u8in<0, 0x7f>] -> &str` does not. For the same reason, the accessors of regions cut regions
short rather than trust the fields that size them. Two layouts are only the same
(`Layout::equals`, by which an enum proves an enum without looking into it) if they require the
same *and* guarantee the same: `V(#[pun(valid(0..=1))] u8)` is not `V(bool)`.

**Uninitialized bytes.** A scalar may be required to be some values while nothing is known of it
as a source, not even that it is initialized (`Spec::maybe_uninit`, `known: Uninit`). Wherever a
source is read (`atom_relation`, a run of the target without constraint, a pattern, `segment`),
it is then padding: before, such a scalar was read as initialized. Two of them are never equal
(`Layout::equals`): what a type described so is lies between what it requires and what it is
known to be, and two types can have the same of both while one may be padding where the other
may not. `MaybeUninit<T>` is padding of the size and alignment of `T`: as a target it takes
anything, as a source it proves nothing but what takes anything. So a `&mut` to one is only made
of a source that may hold anything there (the reverse relation of `pun_mut`: what is written
through it may be uninitialized), and a `&mut T` of a `MaybeUninit<T>` never (a byte that `T`
needs would be read from padding). The same holds of a union as a source, below.

**Unions.** The validity of unions is not settled by the language. Some rules being considered
let a union hold any bytes, others require them to be one of its fields; safe code may write one
field over another. So what is taken is what is sound under each: as a target, bytes are a union
if they are one of its fields (required more than some rules do, never less); as a source,
nothing is known of a union, not even that its bytes are initialized. A union is proven by one
field that the source proves, else checked field by field at run time (`validate_union`), each
field read only if the analysis found that no byte it reads may be padding in the source; if one
might be, and no field is proven, the pun is refused, rather than check fewer fields than the
union has (which would be sound, but which the marks of the analysis cannot tell the validation).

**No value.** `Infallible` and an enum without variants are types without values: no value of a
type that has some is one of them (`relate_at` reports it as never valid), and a variant or a
struct that holds one has no value either (`Layout::is_inhabited`). Before, a variant of a
`repr(u8)` enum whose field had no value would have been proven from a variant whose field
had one (no byte of a type of size 0 is ever compared).

A rule that reads the bytes of its value (`le`, `be`, `crc32`, `sum16`, and `walk` through its
expressions) reads its padding as well. Where such a rule may be checked (the source does not
prove it, or it is in the variant of an enum, which is validated as a whole), the padding of its
struct is a byte that the target needs initialized.

What the type system enforces is all that a type is known by, which has two consequences that
are by design:

- `PhantomData<T>` is a scalar of no byte, `Scalar` for any `T`: two types that differ only by
  phantom parameters or by lifetimes have the same layout, and each converts to the other freely,
  by value and by `&mut`. A type whose parameters are a brand (an invariant lifetime that ties a
  value to a scope, a marker that says what a value was checked against) must not derive `Pun`:
  `pun` would turn one brand into another.
- Deriving `Pun` makes every field as accessible as if it were public: a value of the struct can
  be made from anything whose bytes are valid for its fields and its rules (`pun` and
  `try_pun` check what the derive is told, nothing that a constructor would), read as
  anything its bytes are, and written through a `&mut` to another type. A type whose private
  fields keep an invariant that the derive is not told of must not derive `Pun`.

### 6. What `#[derive(Pun)]` trusts

The derive writes `unsafe impl`s in the crate that uses it, where every path can be made to lead
elsewhere by safe code: `#[pun(crate = ..)]` names any module, `#[pun(crate)]` the root of the
crate, and `extern crate other as core;` makes `::core` another crate. So what the layout is made
of does not depend on any path:

- the functions that the generated code calls are associated functions of the types that the
  implemented traits name (`Layout`, `Spec`, `Field`), or of types that `Layout::PIN` fixes with
  `fn(T)` types (`Rule`, `Op`, `Enum`, `Variant`), which no path can stand for;
- the offsets of the fields are not `offset_of!` (a macro, named by a path): with `repr(C)` they
  are what `repr(C)` specifies, from the sizes and alignments of the fields; otherwise the struct
  is `Sized`, and they are found by projecting a pointer to an uninitialized value, which costs
  nothing in const evaluation whatever the size. The variants of enums with fields are laid out
  as RFC 2195 specifies, from the sizes and alignments of their fields. Without `repr`, they are
  found by `Layout::discover` from samples made by `Layout::sample` (associated functions, as
  every other), and from offsets in those samples that the generated code finds by matching on
  them, with `byte_offset_from` (a method of a pointer, of no path): a fake `MaybeUninit` or
  `Desc` named by a path does not have the type that these functions take or give, and does not
  compile;
- `Path` and `Count` are sealed, and the leaf that a list of fields says a field is at is checked
  against the layout;
- names that start with `__Pun` are reserved, and `Self` in the type of a field is replaced by
  the type it means, since the generated code writes these types where both mean something else.

The rest of `::core` that the derive names (`Option`, `PhantomData`, `usize`...) is checked by the
types of the traits it implements. The derive still trusts the compiler for what it parses
(`repr`), and that the types of the fields are the same wherever it writes them (a procedural
macro in the type of a field that expands to a different type each time it is expanded would break
that).

### 7. Pointers to unsized types (`ptr.rs`)

Splitting and building wide pointers in generic `const fn` code is done by reinterpreting
`*const T` as `*const [u8]`. That representation is not documented, so it is **verified at compile
time for each unsized type** against a pointer made with a genuine `as` cast (`Pun::PROBE`,
`ptr::verify`): if it ever changes, the crate stops compiling instead of misbehaving.

The tests, and the Miri selection of the model check (below), pass under Miri with strict
provenance, with stacked and with tree borrows, on x86-64 and on i686 (a 32-bit `usize`, a `u64`
aligned to 4). With `-Zmiri-symbolic-alignment-check`, Miri reports a misaligned reference where
`try_pun_ref` checked the address of bytes that happened to be aligned: the check reads the
address as an integer (`ptr::is_aligned`), which Miri's symbolic alignment cannot follow, and the
stable `align_offset` is not `const`.

### 8. Self-relative pointers (`layout::Pointer`, `relative.rs`, `deep.rs`)

A value may hold a *self-relative pointer*: the distance from its own address to a target in the
same buffer, as the `Near<T, O>` of the crate `nearest` is. The core knows the kind, and nothing
of any crate that has such pointers: a crate declares its pointer types (below), and the
description of a pointer (`layout::Pointer`) holds what the proof needs and nothing else:

- its **scheme**: a name that the declaring crate chooses, which stands for the way a pointer and
  the bytes of its buffer tell where its target is and how long it is (a length next to the
  target, say). `puns` does not know the way, and does not need to: a pointer is only read as a
  pointer of the same scheme, which finds the same target from the same bytes;
- its **offset**: an integer of 1 to 16 bytes (the size of the pointer type) in a byte order;
- what it **aims** at (`Aim`): one value of the target type, at an address aligned for it; a
  **sequence** of values of the target type, `size_of` apart, the first at an address that is a
  multiple of an alignment that the declarer guarantees (a number of the description, at least
  the alignment of the type: `nearest` says the larger of it and the width of the offset); or
  **text**, UTF-8. The size and the alignment of the target type (of an element) are kept;
- whatever it is **nullable** (`Pointer::or_null`): an offset whose bytes are all zero is then no
  pointer. This is what a nullable pointer is described with, and `Option` of a pointer is one
  (section 10).

What a pointer aims at is *not* in the layout: the layout of `struct Node { next: Opt<Node> }`
would have no end, and const evaluation has no identity of a type by which to stop where it has
been (`TypeId` is not compared in `const`, pointers neither, and a static reached through an
associated constant is a cycle). It is a type, `Pun::Pointee` (the element, for a sequence; `()`
for text). The layout keeps the size and the alignment of the target, which the compiler gives
without looking into it.

**How a crate declares a pointer.** With an `unsafe impl SelfRelative for P` (`relative.rs`),
which says what the pointer is (`const POINTER: Pointer`, made by `Pointer::one::<T>`,
`Pointer::sequence::<T>` or `Pointer::text`, and `or_null`) and what it aims at
(`type Target: Pun`), then `self_relative!(impl[..] P<..>)`, which writes the implementation of
`Pun` from it: `Pointee` is the `Target`, the layout is `Layout::pointer::<P, Target>(POINTER)`,
which asserts that `P` has the size of the offset and that `POINTER` was made for `Target`. The
trait carries the promise, in words (its `# Safety`): the macro is safe to use, and writes nothing
that the trait does not say. It is not a blanket implementation of `Pun` for every
`SelfRelative` type, which would overlap the one for every `Scalar` (coherence cannot tell that
no type is both); and the items of `Pun` stay the crate's own, which it may change: the macro is
expanded with the version that defines them. The doc test of `SelfRelative` is such a crate (doc
tests are outside it), and so are the tests (`tests/support/relative.rs`), the model check and
`examples/codegen.rs`.

What the declarer promises (S1 to S7, the contract of `SelfRelative`, restated):

- S1. The type is its offset: an integer of the width and byte order of `POINTER`, and nothing
  else; every byte initialized, none in an `UnsafeCell` (it is read as plain data). A type whose
  offset has no zero (a `NonZero`) is told apart, found by `Layout::pointer` (`Pointer::nonzero`:
  `Option` of it has its size, `None` all zeros); any other restriction of the values of the
  offset is not supported (a niche that is not zero is refused there).
- S2. The scheme is the way to the target: where a pointer aims and how many elements or bytes
  it aims at depend only on the scheme, the offset (width, byte order), the kind of aim, the
  address of the pointer, the bytes of its offset and bytes of its buffer: never on the type
  that holds it, its `Target`, its alignment or whatever it is nullable. The declaring crate
  owns the name of its scheme, and changes the way only with the name.
- S3. What it aims at is there, valid, in the same allocation, as long as a shared reference to
  the pointer lives: a valid `Target` aligned for it; that many valid `Target`s, the first at a
  multiple of the declared alignment; that many bytes of UTF-8.
- S4. What it aims at is its own: nothing writes to it, nor gives a `&mut` to it, while a shared
  reference to the pointer lives.
- S5. Null: with `or_null`, all bytes zero is no pointer, never followed; any other value is a
  pointer as one of the same scheme that is not nullable.
- S6. The target is the type: `Target` is the type aimed at (an element, `()` for text), and
  `POINTER` is made with it (asserted).
- S7. The crate relies on S1 to S6 and on nothing more when it follows a pointer. In return,
  `puns` makes a value that holds a pointer of the type only by `view_ref` and `try_view_ref`,
  at the address of a pointer of the same scheme, offset and kind of aim (nullable only where the
  type is, or checked not null), of which the target is proven to be a valid target of the type;
  it reads the bytes of a pointer and never writes them.

**What a pointer guarantees is not a property of its bytes**: its target is where it says,
valid, and its own. Three rules follow, which every function keeps.

1. **No bytes prove a pointer.** A pointer of a target is proven only by a pointer of the source
   at the same place whose target it can read (`Pointer::reads_as`): the same scheme, the same
   offset (width and byte order: one is read as the other), the same kind of aim, and a target
   that is no larger and needs no more alignment (one value: its size and alignment; a sequence:
   elements of the same size, and a declared alignment of the target's that is not greater than
   the source's; text: nothing more), and not nullable where the target's is not, nor of a type
   that has an offset of zero where the target's has none (a `NonZero`), unless that is checked
   (`try_view_ref`: not zero; `view_ref` refuses it). Anything else is `Relation::pointer`, which no function accepts and no check makes up
   for. An enum that holds pointers is proven by an enum, variant by variant, or not at all.
2. **Only what proves what pointers aim at makes a value that holds one.** `view_ref` and
   `try_view_ref` prove it (below). Every other function refuses a target that holds a pointer
   (`Plan::gate`). A crate that has pointers checks bytes from outside with a walk of its own
   (`nearest` does): `puns` does not load a buffer.
3. **No `&mut` to a pointer, nor next to one.** `mutable` and `mutable_part` refuse a source or a
   target that holds a pointer, wherever it is: through a `&mut`, safe code could write over a
   pointer, swap two, or move one to where it aims at something else.

As a *source*, a pointer is initialized bytes of which nothing more is known: a value that holds
pointers is read as plain data by any function that reads (`as_bytes`, `head_ref`...).

Every public function, and why it is safe with a pointer in its target or in its source:

- `pun`, `head`, `tail`, `try_pun`, `zeroed`: they take and make `Copy` values. The target is
  gated (2), and `zeroed` asserts it. A source that holds a pointer is read as bytes.
- `pun_ref`, `head_ref`, `tail_ref`, `try_pun_ref`: a target that holds a pointer is refused (2).
  Of a source that does, the bytes are read, and nothing is made of them.
- `pun_mut`, `head_mut`, `tail_mut`, `try_pun_mut`: refused with a pointer on either side (3),
  after the target is (2).
- `as_bytes`: `pun_ref::<S, [u8]>`: the bytes of a pointer are read.
- `from_bytes`, `from_head`, `from_tail`: `try_pun_ref` from bytes (`Plan::TRY_REF`, `FROM_TAIL`):
  the target is refused (2).
- `from_bytes_mut`, `from_head_mut`, `from_tail_mut`: the same plans, then `MUTABLE`: refused (2),
  (3).
- `from_bytes` and `from_head` of a view enum (the derive): `view::decide` for the type of each
  variant, which requires of it what `from_bytes` does (`Plan::TRY_REF`): refused (2).
- getters (`#[pun(get(..))]`), `int`: they read an integer field of a `&self`; a pointer is not one
  (`Layout::as_integer`), which fails to compile.
- accessors of regions, `region`: they need `pun_ref::<S, [u8]>`, and return bytes of the tail:
  read.
- typed regions (`regions(name: P)`): `from_bytes::<P>` of the bytes of the region: refused (2).
- rules (`size`, `len`, `assert`, `regions`): an expression reads unsigned integer fields (a
  pointer is not one: an error of the derive's code) or bytes (`le`, `be`, `crc32`...: read).
- `Tag`, `num::*`, `crc32`, `sum16`: no pointer.
- `view_ref`, `try_view_ref`: a pointer of the target is a pointer of the source (1), what it aims
  at is proven (`deep`, below), and the result is a `&`.
- `SelfRelative`, `self_relative!`: a declaration, whose promise is the `unsafe impl`; the macro
  writes `Pun` from it, and asserts what can be (P1).
- `layout::*`: values that describe layouts, for the code that the derive and `self_relative!`
  write. They make no value of a type, and a layout only stands for a type through `Pun`, an
  `unsafe trait`. `Pointer` is public, made by its constructors only (its fields are the crate's).
- `hook::*` (the test-only feature `verdict`): the same plans, answered instead of enforced. Its
  `view_ref` follows pointers a bounded number of times: it is for the model check, whose types
  that bound covers.

**`view_ref`: what pointers aim at, proven by instances of functions (`deep.rs`).** `view_ref::<S,
D>` runs the plan of `pun_ref` with pointers allowed, then proves, for every pointer of `D`, that
the `X` its source pointer aims at can be read as the `Y` it aims at itself: `Y` is the first
bytes of `X`, every `X` is valid for them, and nothing is left to check (nothing can be, behind a
pointer). The pointers of `Y` are then pointers of `X`, and so on. For a recursive type this
never ends as an analysis; as *instances* it does:

- `target::<X, Y>` holds, in a `const` block, the proof of one pair up to the next pointers
  (`behind`: the analysis of `X` as `Y`, and `local`, below);
- `pair::<A, B>` *names* `target::<A::Pointee, B::Pointee>` for the pointers that `A` and `B` hold
  at the same place, through their fields, variants and elements. A named function is an
  instance, which exists once however often it is named: a recursive type closes a cycle, and
  the proof is an induction over the pairs of types that can be met.

`view_ref` calls `proof`, and with it `pair` over everything up to the first pointers: `const`
blocks of code that runs, which the language evaluates. Behind a pointer, `target` is named and
never called (calling it would not end): its `const` block is evaluated because **the compiler
makes every function that code it compiles names**, whether that code runs or not and whatever
the optimizations (it has to build the function pointer; `rustc` collects what a function
mentions before it optimizes, so that errors of const evaluation do not depend on the
optimization level). This is what the compiler does, more than what the Reference promises of a
`const` block that is not executed: the compile-fail test `view_behind_pointers` (a `u8` read as
a `bool` behind pointers of a recursive type) holds the compiler to it in a debug build, and
`model.yml` builds the same program optimized, with LTO, every night: it must be refused there
too (to run when `rust-toolchain` changes; the program declares its own pointer, and needs no
other crate). For the same reason `view_ref` is not a `const fn`: a call in a constant would make
no instance. An interpreter that only evaluates the constants of the functions it runs, as Miri
does, would not refuse such a view: the refusal is one of a build.

The compiler also makes the instances that a branch names when a constant rules the branch out,
so `pair` cannot choose what it names: it pairs by position (fields with fields and variants with
variants, in the order of their declarations; elements with elements), which names more pairs
than matter. Where the source has nothing, its side is `()`, and `target` has nothing to prove
of a type without size (no pointer aims at one: `Pointer::one`, `Pointer::sequence`; the target
type of text is `()`). The pairs that matter are decided exactly by `local`, which is `const`
code: every pointer of the target must be, in the source, a pointer at the same offset that
`pair` pairs it with, or the build fails. So what `local` accepts, `pair` names, by the same walk
of the same lists; and a pair that is named without mattering has one side without pointers.
Those lists (`Fields`, `Variants`, `Elem`, `Pointee`) are types, written next to the layout that
the analysis went by: `local` checks, on its way, that they are what the layouts hold (`listed`:
the same fields in the order of their declarations, the same variants, the same elements, a
target of the size and alignment that the pointer says), so that a pointer is not missed, nor
taken for another, by an implementation of `Pun` whose lists and layout differ (compile-fail
test `lists_not_the_layout`). Hence the limits, which are refusals: a field that holds a pointer
keeps its position among the fields of its struct, a variant that does keeps its position among
the variants of its enum (new ones go last: a variant found by its tag at another position is
refused, compile-fail test `view_variant_moved`), and a struct that only holds a pointer is not
that pointer.

`try_view_ref` checks what is not proven of the value itself (`validate`, with the marks of the
analysis: a variant that the target does not have, a restricted scalar, a pointer that may be
null read as one that is not: its bytes are not all zero), and requires of what it points to what
`view_ref` does.

**Invariants**, each kept where it is said:

- P1. A `Kind::Pointer` layout is made by `Layout::pointer` only, which `self_relative!` calls for
  a `SelfRelative` type (an `unsafe trait`): a type that is the offset and nothing else (its size
  is asserted), aiming at its `Target`, for which the description was made (size and alignment
  asserted), a type with a size (`Pointer::one`, `Pointer::sequence`) or `()` for text.
- P2. `Relation::pointer` is set for every pointer of a target that a pointer of the source does
  not prove (`relate_range`, `relate_enums`, the two places that give up on proving), and every
  plan refuses it, fallible or not (`check`).
- P3. Every plan but `VIEW` and `TRY_VIEW` refuses a target that holds a pointer (`Plan::gate`,
  called by `Plan::check` and `Plan::check_tail`, which every plan goes through).
- P4. `mutable` and `mutable_part` are false with a pointer on either side.
- P5. `view_ref` and `try_view_ref` are the only functions with the plans of P3, and both call
  `deep::proof` for the same pair of types; neither is a `const fn`.
- P6. What `deep::local` accepts, `deep::pair` names: both walk the fields, the variants and
  the elements of the two types by position, and `local` refuses whatever is not at the
  position where `pair` looks for it, and a type whose lists are not what its layout holds
  (`listed`).
- P7. `Pointer::reads_as` relates only pointers of the same scheme, width, byte order and kind of
  aim, with the numbers of their descriptions (sizes, alignments, the declared alignment of a
  sequence), and says when nullability, or an offset of zero read as a `NonZero`, is to be
  checked; `Pointer::same` (by which two layouts
  are equal) compares every field of the description.
- P8. What the declarer promises is S1 to S7, which `puns` relies on and nothing more: S2 for a
  pointer read as another of the same scheme, S3 and S4 for what `deep` proves of the targets,
  S5 for `try_view_ref`'s check of a nullable pointer.

### 9. Who builds on this

[`nearest`](https://docs.rs/nearest) (regions: a value and everything it points to in one
buffer, with `Near<T, O>` pointers) declares its pointers with `SelfRelative`, and depends on
`puns`, not the reverse. The layout and the validity of bytes are the general thing; a region is
one structure made of them, with a byte format of its own, which `puns` does not state again (a
scheme is a name for it). One validator of values, the one of `puns`, serves both: what a field
may be, enums, rules, `try_view_ref`; what is a region's (where its targets are, its blocks, its
walk of an image) is `nearest`'s. The version of `puns` that had a feature `nearest` (its
loaders, the IR example, the corpus tests) is at `e5875f5`.

**The validator of a value in a buffer (`visit.rs`).** `validate_in::<T, V>(bytes, at, visit)`
validates the `T` at the position `at` of `bytes`, everything of it but what its pointers aim
at, and hands each of its pointers to the visitor, `V: Visit`, with its type and its position:
`visit.pointer::<P>(at)`. The visitor is the crate that has the pointers: it reads the
description of `P` (`Layout::as_pointer`, the getters of `Pointer`), finds the target as its
scheme says, places it as its buffer wants, and validates it in turn (`validate_in::<Pointee<P>,
_>`, `validate_elems_in` for a sequence) or queues it. `validate_elems_in` does `count` values
one after the other. Both are safe functions, for any bytes and any position: they read nothing
out of `bytes` (`get`, checked arithmetic, then reads at positions in bounds), do not panic, and
recurse over types only, never through pointers. Their contract, which `unsafe` code of another
crate relies on (in their documentation, word for word): after `Ok(())`,

1. `at + size_of::<T>()` is at most `bytes.len()` (for `validate_elems_in`, `at + count *
   size_of::<T>()`, which does not overflow);
2. those bytes are a valid `T` as its layout says, provided that every pointer handed over is a
   valid pointer of its type (S1 to S7): every scalar is one of its values (`valid(..)` included),
   every enum has the tag (or the niche) of a variant whose fields are valid, every union is one
   of its fields, and every rule of every struct holds, evaluated on the bytes of that struct;
3. every pointer of the value, where it is, was handed over once, and the visitor returned `Ok`;
   bytes of a pointer that are all zero are no pointer if it may be null (not handed over), not a
   value if its type has none of zeros (its offset is a `NonZero`, `Pointer::nonzero`), and a
   pointer of offset zero otherwise (handed over: its scheme says what it aims at).

It says nothing of what the pointers aim at (in the bytes or not, shared or not: the scheme is
the visitor's business), nor of addresses: positions only. A `&T` at `bytes.as_ptr() + at` must
be aligned for `T`, which is the caller's duty (a buffer at an aligned address, `at` a multiple of
the alignment). The bytes are initialized (a `&[u8]`), so `MaybeUninit` and padding are what they
are; a `MaybeUninit` of a pointer is nothing, and nothing is handed over of it. A union that holds
a pointer does not compile there (which field it is cannot be told, so neither can the pointer).
`T` is `Sized`: a value of a size that its bytes tell is not one that a buffer holds by value.
`Invalid { at }` is where the bytes stop being a value (the scalar, the tag, the union, the struct
whose rule fails, the value that is not all there), `#[non_exhaustive]`, so that it may one day
say why.

What holds no pointer is validated by `validate`, the code of `try_pun_ref`, at the same speed,
and its answer is the verdict: only where it fails is it walked again, by the positional walk
(`value`, `fields`, `enumeration`), to tell where, and if the walk finds nothing the error is at
the value itself (less precise, never wrong); likewise for the elements of `validate_elems_in`
(`the_verdict_is_the_one_of_try_pun_ref` holds the two to the same answers). The walk goes by the
lists of types that the derive writes (`Fields`, `Variants`, `Elem`, `Pointee`), which are
checked against the layout for every type it goes through (`listed`: every field by its
declaration index, offset, layout and type; every variant by its tag, its tags of a niche and its
fields; the element; the target of a pointer), as `deep::listed` does for `view_ref`: a list that
left a field out would leave it unchecked (compile-fail `validate_lists_not_the_layout`). A struct that holds pointers is
walked field by field, so that its pointers are handed over in order with the checks of the rest
(a visitor whose queue is full stops the walk where it is).

**The walk of the pointers of a valid value.** `pointers_in::<T, V>(base, at, visit)` (and
`pointers_elems_in` for `count` values) hands the visitor the pointers of a `T` known to be valid,
the same calls that `validate_in` makes on the same bytes, in the same order, and checks nothing:
for a crate that copies a value with what it aims at, or moves it in its buffer and re-aims its
pointers, where validating again would cost a check of every scalar for nothing. It is `unsafe`:
the bytes at `base + at` hold a `T` as its types say (every scalar initialized and valid for its
type, the tag of every enum or its niche initialized and telling a variant, whose bytes are so in
turn; padding, and what the variant does not hold, may be uninitialized, since neither is read),
at any alignment. Its rules need not hold, nor the `valid(..)` of its fields, which `validate_in`
checks: safe code can break them in a value it makes (`Span { lo: 2, hi: 1 }` of
`assert(lo <= hi)`), which `nearest` walks and copies all the same. They cannot matter here: the
walk goes by types, reads the tags of enums (niches included, as `validate_in` does: `variants`
and `follow_variants` choose the variant by the same test) and the offsets of pointers that may be
null, nothing else, and evaluates no rule; a restricted field is a scalar, which holds no pointer.
It goes by the same lists of types (`listed`) and refuses the same unions. A type without pointers
is not walked at all.

**The copy of a valid value.** `copy_in::<T>(src, dst)` copies the bytes that a valid `T` holds
initialized, but those of its pointers: each scalar (one that may be uninitialized,
`MaybeUninit`, is none), the tag of each enum and the fields of the variant that it tells. A
pointer that may be null and whose bytes are all zero is no pointer: its zeros are copied. The
others are the pointers that `pointers_in` hands over, in the same order, and their bytes are left
to the caller, who writes their offsets: re-aimed for where the copy is, or aimed at copies of
their targets (copying them first would write them twice). Bytes of a valid `T` that may be
uninitialized (padding, the bytes of the variants that it is not, fields that may be
uninitialized) are never read from `src` nor written to `dst`, whose bytes there keep what they
held. It is for a crate that writes values into a buffer of initialized bytes (a region that hands
out its bytes as a `&[u8]`), where a typed copy would leave padding uninitialized and a copy of the
bytes would copy the source's, uninitialized on the stack. A part without such bytes and without
pointers (`is_plain`) is copied whole, at once; the rest is walked by the same lists of types as
`validate_in` (`listed`), choosing the variant by the same test, so that it compiles to a few moves
per type: 3 instructions for a struct of a `bool` and a pointer, 8 for an enum of an `i64` or two
pointers, 19 for a struct of a `u32` and an enum of four variants (x86-64), against 29 for the code
that `nearest` generated per type for the last. A union does not compile there (`copy_in_union`):
which of its fields it is, and so which bytes are initialized, cannot be told. Its source is what
`pointers_in` reads, as its types say, whatever its rules say: no rule is read, and a restricted
field is copied as the scalar of its type that it is.

The model check holds it to the oracle of pointers (below): with a visitor that follows the
pointers of the tests through their buffer, `validate_in` accepts exactly what the oracle accepts,
on the corpus and on every copy with one byte changed, padding and gaps included
(`check_validate`; a sample on a push, every type and every mask of one bit with `full`); and on
every image that it accepts, `pointers_in`, following the pointers in turn, hands over the same
pointers (position and type) in the same order (`check_pointers_in`, the same sample and the same
`full`); and `copy_in` writes the bytes that the oracle says the value holds (its scalars, the tag
and the fields of its variant, a null pointer; not the bytes of the other pointers), and only
those, into bytes of two fills (`check_copy_in`, the same sample and the same `full`). It checks
`pointers_in` and `copy_in` on images that `validate_in` accepts only, so on none whose rules do
not hold: that they need not is held by the argument above (no rule is read), and by a test.
`tests/validate_in.rs` gives each kind its answer, the positions of errors, and random bytes,
positions and counts (some that overflow) under Miri on every target of CI; and on what
`validate_in` accepts there, that `pointers_in` and `pointers_elems_in` hand over the same
pointers, at any alignment, with the bytes that they need not read uninitialized; and that
`copy_in` of a value on the stack (padding and the other variant uninitialized) into memory that
is then read whole as a `&[u8]` copies its value and nothing else, and of random accepted bytes a
value that `validate_in` accepts again. And on a value with pointers that breaks its `assert` and
the `valid(..)` of a field, in a buffer, which `validate_in` refuses: that `pointers_in` hands over
its pointers, and `copy_in` copies its scalars and its null pointer, as for a value that keeps them
(`rules_need_not_hold_for_pointers_in_and_copy_in`).

### 10. Enums without `repr` (`discover.rs`)

The layout of an enum with fields and no `repr` is not specified, and stable Rust names neither
where its tag is nor its values (`offset_of!` on variants, `mem::variant_count`,
`discriminant_value` are unstable; nothing tells a niche). But const evaluation runs code on its
values. `#[derive(Pun)]` makes a *sample* of each variant (`Layout::sample`: every scalar of each
field the least of its values, padding uninitialized), finds the offsets of its fields by
matching on it, and `Layout::discover` finds the rest by experiments on copies of the samples,
which read the variant with `mem::discriminant`:

- **No variant has values**: no tag, no value. **One has**, and no other is laid out (`rustc`
  lays out a variant without values only if one of its fields has a size), or its fields start
  at 0 (a niche of its own fields, or none): a struct of its fields. Its gaps are filled with
  zeros, then with ones, and it must stay that variant.
- **A direct tag**: the fields of no variant start at 0. `rustc` puts the tag before them, widened
  to all the bytes before the first field of any variant (`enum { A(u16), B(u32) }` has a tag of
  2 bytes, in which the high byte of each value is 0). Those bytes are *read* in every sample:
  a byte that is not of the tag would be padding, uninitialized, and reading it fails the build
  (R2, R4). So the tag is exact, on either byte order: no claim is made of a byte that may be
  padding, and the tag is an integer of native byte order whatever where its low byte is.
- **A niche**: some field starts at 0. The scalars of the fields of each variant that the type
  restricts (`bool`, `char`, `NonZero`, enums without fields, and tags of enums; not a
  `valid(..)`, which safe code can break), and that the fields of no other variant cover, are
  tried, the largest variant first: the bytes of the scalar are moved from the sample of each
  variant to the sample of a variant that does not hold them, which must become that variant (a
  wrong scalar changes nothing there and is dropped, without failing). Then the tag of each other
  variant is read there (initialized, so the tag: R4), and the variant that holds the niche has
  the values of that scalar, which the tags of the others must not be.

In both cases the gaps of each variant that has one tag, outside the tag, are filled with zeros
then with ones, which must leave it that variant (a byte of a tag outside would have two values
for one variant), and so are all the gaps of the variant that holds a niche (its scalar must tell
it alone: a layout in which another place took part, a two-level encoding say, is refused), and
the tags must differ. The header of `discover.rs` says which fact each experiment holds. What is found is checked against the size and the
alignment of the enum (`Layout::enumeration`). An experiment that goes wrong either refuses the
enum with a message ("no tag found", "unsupported layout"...) or fails the build (an error of const
evaluation): it never finds a wrong layout. Every experiment copies a sample and reads a variant;
there are about four for each variant, and none is for a byte of a field: an enum of 300 variants
takes 3 seconds to build with its tests, an enum of a specified layout 1.5.

What this rests on, of the compiler, and what holds each:

- **R0** the layout of a type is the same in const evaluation as at run time, for a target: const
  evaluation computes it with the same code, for that target. A property of how `rustc` is made.
- **R1** the variant of a value is told by one scalar at a fixed place: a direct tag before the
  fields of every variant, one value per variant, or a niche in a scalar of the fields of one
  variant, which no field of another variant covers, whose values that are valid for that field
  are that variant and whose other values are each one other variant, or no value. This is how
  `rustc` lays out enums (stable since 2017 for one variant with fields, 1.64 for several), not a
  promise of the language: the experiments verify what they rely on of it (the tag decides, and
  nothing outside it), and a layout of another shape is refused.
- **R2** const evaluation fails on reading an uninitialized byte, and on reading the variant of a
  value whose tag is not the one of a variant that has values. Behaviour of the compiler, which
  the Reference does not promise of undefined behaviour in const: the compile-fail tests
  `discover_reads_padding` and `discover_invalid_tag` hold it to it.
- **R3** `mem::Discriminant<E>` of an enum without `repr` holds its discriminant, the index of
  the variant (no explicit discriminant is allowed there), as an `isize`: its size is asserted,
  and every sample is checked to read as its own index.
- **R4** a value made in const evaluation (`E::A(x)`) has its padding uninitialized: making a
  variant writes its fields and its tag, nothing else. So a byte of a fresh sample outside its
  fields that can be read is a byte of the tag. Held by `discover_reads_padding`.

And of the library: the valid values of a type, as `Pun::LAYOUT` gives them, are valid for
`rustc` (what `Pun` promises). So the values that `rustc` gives to the other variants of a niche,
which are invalid for the scalar that holds it, are not values of that scalar in its layout.

Such an enum is then what an enum of a specified layout is: proven variant by variant, checked
by its tag and its variant, with the same precision. A variant whose fields have no value is in
the layout with no tag. The lists of fields and variants are the declared ones: a field of a
variant without values is not in the layout, and its entry in the list says so (`FIELD` is
`None`). No field may hold a self-relative pointer (a sample of one is not made of nothing).

**`Option<T>` and `Result<T, E>`** (`std_enums.rs`) are found the same way, for any `T` and `E`,
when their layout is known: `Option<NonZero<u32>>` is a niche at 0 (every `u32` is one, both ways
proven), `Option<bool>` the niche 2, `Option<Option<bool>>` two niches (`rustc` puts `None` at 255
today, below the valid values rather than after them: found, not assumed), `Option<u16>` a tag
of two bytes, `Result<u32, Infallible>` a struct of its `u32`.

**`Option` of a self-relative pointer** is the same pointer, that may be null. A pointer is not
looked into by the discovery, so `Pun` has a hidden associated constant, `OPTION`, the layout of
`Option<Self>` if `Self` gives it (`None` by default: found). `self_relative!` gives
`Layout::nullable::<Self, Target>(POINTER)`, which is `POINTER.or_null()` after checking, where
`Option` of the pointer is used, that `Option<Self>` has the size of `Self` and that `None` is all
zero bytes (read in const evaluation). Then `rustc` has put `None` in a niche of the bytes of the
pointer (they are all of its offset, initialized: S1), which is zeros, and `Some(p)` is the bytes
of `p` (R1), which are not all zero: S5 holds of `Option<Self>`. An offset that has no niche at
zero (a byte array) makes `Option` of the pointer larger, with a tag of its own: that fails to
build where it is used, with a message (compile-fail test `option_of_pointer_without_niche`). The
generic `impl Pun for Option<T>` takes `T::OPTION` when it is `Some`: nothing is then found (an
empty `Desc`), its lists of variants end at once (a pointer is not looked into by `deep.rs` but
for what it aims at, its `Pointee`, which is `T::Pointee`), and the rules of section 8 apply to
it as to any pointer. A crate that declares a pointer gets this from the macro alone; the tests
show it with an offset whose zero is a niche (`Nz32le`), `Option<One<T, Nz32le>>` read as `Opt<T,
i32le>` and back, through values of a buffer.

### 11. What `usize` is assumed to be

`puns` builds for every width of `usize` that Rust supports, 16 bits included (`msp430-none-elf`,
built by CI with `-Zbuild-std=core`; Miri cannot run a 16-bit target, so what holds there is
asserted). What the code assumes of `usize`, and what holds each:

| Assumption (where) | Held by |
|---|---|
| A value is at most `isize::MAX` bytes, so a size plus a little does not overflow (`walk_limit`) | Rust; `const` assertion next to `walk_limit` |
| An offset, a length or a range read by a rule is compared as a `u128` before it is made a `usize` (`Machine::apply`: `At`, `Varint`, checksums) | the code; on every target the comparison is with the size of the value |
| A position plus a size in the bytes given to `validate_in` and `validate_elems_in` | `checked_mul`, `get`: the arithmetic of the bounds is checked, so is any position in them |
| The tables of a `Crc` fit a value (`Crc::new`, `Crc::update`) | `CRC_TABLES`: 16 tables (32 KiB), 1 (2 KiB) where `usize` is 16 bits; `const` assertion; the CRCs are the same, the tests of `Crc` compare with references |
| `sum16` folds before a carry could leave its `u64` (`sum16_inline`) | `SUM16_FOLD`: 2^20 blocks, or every block where `usize` cannot count that far; `const` assertion |
| The 16 KiB table of `crc32` fits a value | it does on every width (`isize::MAX` is 32 KiB minus 1 on 16 bits) |
| The size given to the evaluator of rules is at most `isize::MAX`, and so are an offset that a rule read at and `size_of_val(self)` (`narrow::SIZE_MAX`, the ranges of "Narrowing") | Rust: it is the size of a value, or the length of a slice of bytes |
| A register is 64 bits where `usize` is, else 32 (`narrow::REGISTER`) | a choice of speed, not of results: every width computes the same, and `narrow_tests.rs` checks both |

### 12. What Rust keeps of a layout, and the `unstable` feature

Rust leaves a part of the layout of some types to the compiler, which may choose otherwise in its
next version, or at random with `-Zrandomize-layout`: the order of the fields of a struct or of a
tuple of the `Rust` representation (and so its size: `(u8, u16, u8)` is 4 or 6 bytes), the place
and the values of the tag of an enum without `repr`, the niches of `Option` but where `core`
documents them. `puns` reads what the compiler chose (section 1), so a pun is sound for the build
that makes it, whatever it chose. What the `unstable` feature is about is whether the pun is the
same with another choice.

**The rule.** Without the `unstable` feature, `-Zrandomize-layout` changes nothing that `puns` does:
what compiles, what is checked and which values a check accepts are the same whatever layout `rustc`
chose where Rust lets it choose. The message of a refusal may name what it chose (`(u8, u16, u8)` as
`[u8; 4]` is refused for its layout, or for its size when it is 6 bytes). With the feature, `puns`
takes the layout that `rustc` chose, as it is, and does everything it can with it (the model check,
section "Model check", runs with it). Within one build, the feature only adds: a pun that compiles
without it compiles with it, and does the same.

It is a property of a pun, not of a type: `(bool, u8)` read as bytes is proven whatever byte its
`bool` is, while bytes read as `(bool, u8)` would check one byte or the other.

| Pun, without `unstable` | | Why |
|---|---|---|
| `[u8; 2]` as `(u8, u8)`, and back | proven | bytes of any value, in either order |
| `[u8; 2]` as `(bool, bool)` | checked | both bytes are a `bool`, in either order |
| `(u8, u8)` as `[bool; 2]` | refused | the check of each element reads a byte that, in another layout, holds the other field |
| `(bool, u8)` as `[u8; 2]`, `as_bytes` | proven | both bytes are initialized, in either order |
| `[u8; 4]` as `(u8, u16)` | proven | it only requires initialized bytes, wherever its padding is |
| a `repr(C)` struct that holds a `(bool, u8)` as itself, or its first bytes as the `(bool, u8)` | proven | the same type at the same place |
| an enum without `repr` as itself | proven | a type has one layout in a program |
| `[bool; 2]` as `(bool, u8)` | proven | the `bool` may be either byte, and both are one |
| `[u8; 2]` as `(bool, u8)` | refused | the byte that must be 0 or 1 is one or the other |
| the first two bytes of `(u8, u16)` | refused | the second is padding, or not |
| bytes as a `struct P { a: u8, b: u8 }` of which `a < b` | refused | which byte is `a` |
| `(u8, u8)` as a `repr(C)` `{ bool, u8 }` | refused | the check reads one field or the other: `(0, 2)` passes in one order only |
| `(u8, u8)` as `NonZero<u16>` | refused | a check of a scalar reads its bytes in an order, which is the order of the fields |
| anything of `(u8, u16, u8)` but itself | refused | 4 or 6 bytes |
| bytes as an enum without `repr`, or as `Option<bool>` | refused | nothing documents where its tag is, nor its values |
| `validate_in::<(bool, u8)>` | refused | which byte it checks |

Which field a byte is, is not part of it: `pun::<[u8; 2], (u8, u8)>([1, 2])` is `(1, 2)` or
`(2, 1)`, a valid value either way, which is what a pun promises (and `transmute` does). What is
the same is what `puns` does with given bytes; which bytes a value of a struct without `repr` has
is the compiler's choice, with or without `puns`. What a check says of a value is what `puns`
does: whether `try_pun::<(u8, u8), F>((0, 2))` is `Ok`, for `F` a `repr(C)` `{ bool, u8 }`, must
not depend on which byte holds the `0`. So a check that reads a byte of a struct whose fields Rust
may put in another order is refused (`analysis::reads_moved`). The rule is general: it does not
look for the cases where every order would give the same answer, such as `(u8, u8)` as
`[bool; 2]`, or `(u8, u8)` as `NonZero<u16>`, which no order makes fail.

**How.** `Pun::LAYOUT` is the layout that `rustc` chose, with or without the feature: everything
that reads values (validation, copies, the generated accessors) reads them where they are. Each
node of it says what Rust keeps of it (`layout::Stable`), computed from the layouts of its parts:

- `fixed`: its offsets and its size are the same in every layout that Rust allows, given the
  sizes of its parts. A `repr(C)` or `repr(transparent)` struct, an enum of `repr(C)` or of an
  integer, a `repr(C)` union, an array: if the sizes of their parts are kept (their offsets are
  computed from them). A struct or a tuple of the `Rust` representation (`Layout::rust`): if every
  order of its fields is the same bytes, which is so if one field has a size (it is at 0); or if
  the fields that have one are all multiples of the alignment of every field (no padding between
  them in any order) and are all bytes of any value with no rule in them (`(u8, [u8; 3])`), or
  all the same layout of a type that is fixed through and through (`(bool, bool)`), and the
  struct has no rule: a rule names fields, so `a < b` holds of other bytes in another order, and
  the rule of a field holds of the bytes where the field is. An enum without `repr`,
  `Option` and `Result` but where `core` documents them: never (`Layout::unspecified`).
- `size`: its size is the same in every layout. Of a struct of the `Rust` representation that is
  not fixed, every order of its fields is tried, each placed at the first offset that its
  alignment allows, as `rustc` places them (`Layout::orders`, up to 6 fields that matter; the
  fields of `(u8, u16)` are 4 bytes in any order, the ones of `(u8, u16, u8)` are not).
- `init`, of what is not fixed: its bytes are initialized in every layout (no padding in any
  order, and the fields are).
- `deep`: nothing in it, at any depth, is not fixed.
- `ordered`: each byte of a value is of the same part in every layout. As `fixed`, but not of a
  struct whose orders all make the same bytes: `(u8, u8)` is fixed (two bytes of any value in
  either order) and not ordered (which byte is the first field is not kept). What a check reads
  must be ordered, but as above.

Without the feature, the analysis (`analysis::STRICT`) does not look into a node that is not
fixed (`Layout::is_opaque`): the cursor makes it one run, `RunKind::Opaque`.

- **As a source**, it is what every layout of it is: initialized bytes if `init`, else bytes that
  may be padding (all of it or none: that the first byte of a `(u8, u16)` is never padding is
  not used).
- **As a target**, it is proven by the same type at the same place (below), or, a struct or a
  tuple whose size is kept, field by field: **a field may be anywhere, so it is to be proven
  everywhere it may be** (`analysis::relate_fields`). A struct is valid if each of its fields
  is, at its place, so the source proves it in every order of its fields if, and only if, it
  proves each field at each offset that the field has in some order: this is exact, and no order
  is tried. The offsets taken are the multiples of the alignment of the field where it fits, or,
  where no order needs padding, the multiples of the greatest common divisor of the sizes (every
  sum of sizes is one); a few are in no order (the second byte of a `(bool, [u8; 3])`), where
  more is then required, never less. Two `bool`s are so a `(bool, u8)`, four initialized bytes a
  `(u8, u16)`, two `NonZero<u16>` a `(NonZero<u16>, u16)`. A struct that has a rule (which names
  fields) or a pointer (which is paired by its place) only proves itself, as does an enum.
- **Nothing in it is checked at run time**: a check would read where the compiler may not have
  put the field. What is not proven does not compile (`Relation::unspecified`), which whatever
  reads a relation reads: what a pointer aims at (`deep::behind`), what a variant of a view takes
  from another (`view::base`), and an enum or a union that would be checked as a whole, fields
  included (the relations of its variants hand it over). So the marks of the analysis, which
  tell validation what to check, are at places that every layout keeps.

Nor is anything found in a source that is not fixed, where `rustc` put it: a rule is proven by
the same rule of the same struct at the same place (section 4), which is looked for through the
fields of the source (`proves_rules`, `covered`, `same_elements`): not through a node that is
not fixed, and two layouts are the same there only if what Rust does not keep of them is of the
same type (`analysis::equal`). A `&mut` to a part covers such a node whole, by the same type, or
not at all (`cut`). A pun of a type whose size is not kept compiles only if both sides are the
same type (`analysis::sizes_kept`), and a function of `Visit` only if nothing in what is not kept
has something to check or a pointer (`Layout::checks_kept`). `zeroed` does not take the zeros of
an enum that is not fixed.

**The same type** is told by an identity (`layout::Id`), and by the layouts being the same: a
type has one layout in a program, but two types, however alike, may each be arranged otherwise
(`Wrapper<u8, u8>` and `Wrapper<u8, i8>`, which `-Zrandomize-layout` does arrange otherwise).
Stable Rust compares no types in `const` (`TypeId` cannot be compared there, `type_name` is not
`const`), so an identity is a name and the identities of what makes the type: the primitive
types, the types of `num`, arrays, slices, tuples, `Option`, `Result`, `MaybeUninit` (and
`ManuallyDrop<T>` is `T`). What derives `Pun` is named by where it is declared (`__site!`: the
module, the file, the line and the column, and its name). A generic type that derives `Pun`, a
`PhantomData`, a `Scalar` implemented by hand and a self-relative pointer have none
(`Id::NONE`), equal to nothing: a type that holds one only proves itself with the feature. An
identity only ever refuses: the layouts are compared as well, so two types that would share a
name are still read by their layouts.

**What this rests on, that Rust does not document**: that `rustc` places the fields of a struct
of the `Rust` representation one after the other, each at the first offset that its alignment
allows, in an order of its choosing, so that the sizes of the orders are the sizes it may give
(`-Zrandomize-layout` draws the order at random, and does not do otherwise). Rust only bounds the
offsets: should a compiler put padding where no alignment needs it, a size that is read would
differ from the one that `Layout::rust` takes as kept, which makes it not kept (the size is
compared with the one of the compiler), and a struct that was fixed would stop being so, which
fails the pun at compile time rather than change what it does. And the alignment of a type, which
is read: Rust gives the one of a primitive per platform, and of a struct the largest of its
fields (`-Zrandomize-layout` keeps it).

**`Option` of a self-relative pointer** is the same pointer, that may be null, with or without the
feature (section 10): Rust documents it of a `repr(transparent)` struct around a `NonZero`, which
is how a pointer is to be declared (`self_relative!` says so; this crate cannot see the `repr`).

**What this rests on**, each documented by Rust (R: the Reference, `doc.rust-lang.org/reference`;
C: `doc.rust-lang.org/core`):

| Guarantee | Where |
|---|---|
| Layout may change with each compilation; only what is listed is guaranteed | R `type-layout.html#r-layout.guarantees` |
| A size is a multiple of the alignment, and the stride of an array | R `type-layout.html#r-layout.properties.size` |
| Sizes of `bool`, integers, floats, `char`; `usize` and `isize` | R `type-layout.html#r-layout.primitive.size`, `#r-layout.primitive.size-align` |
| The alignment of a primitive is the one of the platform (read, not assumed) | R `type-layout.html#r-layout.primitive.platform-specific-alignment` |
| Any initialized bytes are an integer or a float | R `types/numeric.html#r-type.numeric.validity` |
| `bool` is one byte, 0 or 1 | R `types/boolean.html#r-type.bool.layout`, `#r-type.bool.repr` |
| `char` is a `u32` that is a Unicode scalar value | R `types/char.html#r-type.char.value`, `#r-type.char.layout` |
| `[T; N]`: `N` elements one after the other; `[T]` as the array; `str` as `[u8]` | R `type-layout.html#r-layout.array`, `#r-layout.slice`, `#r-layout.str` |
| `()`: size 0, alignment 1. `!` (`Infallible`): the same, and no value | R `type-layout.html#r-layout.tuple.unit`, `types/never.html#r-type.never.layout` |
| A value is valid if its fields, or its elements, are; padding may be uninitialized | R `behavior-considered-undefined.html#r-undefined.validity.struct`, `#r-undefined.validity.undef` |
| An enum has a valid discriminant, and the fields of its variant are valid | R `behavior-considered-undefined.html#r-undefined.validity.enum` |
| `repr(C)` struct: the fields in order, each at the next offset that its alignment allows | R `type-layout.html#r-layout.repr.c.struct` |
| `repr(C)` union: every field at 0 | R `type-layout.html#r-layout.repr.c.union.size-align` |
| The validity of a union is not decided: a target is one of its fields, a source proves nothing | R `behavior-considered-undefined.html#r-undefined.validity.union` |
| Enum without fields, `repr(C)` or of an integer: that integer, holding the discriminant | R `type-layout.html#r-layout.repr.c.enum`, `#r-layout.repr.primitive.enum`, `items/enumerations.html#r-items.enum.discriminant.access-memory` |
| Enum with fields, `repr(C)`, of an integer, or both (RFC 2195) | R `type-layout.html#r-layout.repr.c.adt`, `#r-layout.repr.primitive.adt`, `#r-layout.repr.primitive-c` |
| `repr(transparent)`: the layout of its field | R `type-layout.html#r-layout.repr.transparent.layout-abi` |
| `packed(N)` and `align(N)` | R `type-layout.html#r-layout.repr.alignment` |
| `Rust` representation: each field at a multiple of its alignment; the type at least as aligned as its fields | R `type-layout.html#r-layout.repr.rust.layout` |
| `Rust` representation: the fields of a struct do not overlap, in an order that is not specified | R `type-layout.html#r-layout.repr.rust.struct` |
| `Rust` representation: a struct whose fields have no size has none; an enum without variants, or of one variant whose fields have no size, has none | R `type-layout.html#r-layout.repr.rust.struct-zst`, `#r-layout.repr.rust.enum-empty-zst`, `#r-layout.repr.rust.enum-struct-like-zst` |
| `Rust` representation: nothing else | R `type-layout.html#r-layout.repr.rust.unspecified` |
| A tuple has the `Rust` representation | R `type-layout.html#r-layout.tuple.def` |
| The representation of a type does not change the layout of its fields | R `type-layout.html#r-layout.repr.inter-field` |
| A type has one layout in a program; a similar type may have another | C `mem/macro.offset_of.html#layout-is-subject-to-change`, R `types/struct.html#r-type.struct.layout` |
| The discriminant of an enum without `repr`: neither where nor how it is stored is defined | R `items/enumerations.html#r-items.enum.discriminant.type`, C `mem/fn.discriminant.html` |
| A union without `repr`: its fields need not be at 0 (refused) | R `types/union.html#r-type.union.layout` |
| `NonZero<T>`: `T` but zero | C `num/struct.NonZero.html#layout` |
| `Option<T>` is `T`, and `None` zeros, for a `NonZero` and a `repr(transparent)` struct around one | C `option/index.html#representation` |
| `Result<T, E>` is the same when the other of `T` and `E` has size 0 and alignment 1 | C `result/index.html#representation` |
| `ManuallyDrop<T>`: the layout and the values of `T` | C `mem/struct.ManuallyDrop.html` |
| `MaybeUninit<T>`: the size and the alignment of `T`, any bytes | R `special-types-and-traits.html#r-lang-types.maybe-uninit.layout`, `#r-lang-types.maybe-uninit.validity` |
| `PhantomData<T>`: size 0, alignment 1 | C `marker/struct.PhantomData.html#layout` |

What Rust documents and `puns` does not use yet: `UnsafeCell`, `Cell`, `Wrapping`, `Saturating`
and the atomics have the layout of what they hold (no implementation of `Pun`); an enum of two
variants, one of a type that `Option` is documented for and one without sized field, is
"ABI-compatible" with that type (`primitive.fn.html#abi-compatibility`), which says its size but
not which bytes its other variant is.

Niches are not guaranteed beyond the lines of `Option` and `Result` above: that `Option<bool>` is
one byte, with `None` at 2, is how `rustc` does it today (`MaybeUninit` and `UnsafeCell` say that
such optimizations vary). It needs `unstable`.

Niches that Rust documents only as "ABI-compatible" are not taken either: an enum without `repr`
of a variant that holds a `NonZero` and one without field (the `Ticket` of the README) needs the
feature, though `rustc` lays it out as the `Option` of it.

**How it is checked**: the tests of `puns` run without the feature and with it. Without it (`cargo
test -p puns --features verdict`), `tests/seeds.rs` records the verdict of every function on every
pair of 34 types (the ones of the table above, and next to them what holds them: an enum and a union
of a specified representation, a struct with a rule, arrays, pointers), whole, in part (by `&mut`,
from the end) and behind pointers, and which byte patterns each type accepts from bytes (not the
message of a refusal, which may name what `rustc` chose), and compares the record with
`tests/seeds.txt`; CI runs it without `-Zrandomize-layout` and with it, of several seeds, and the
record must be the same each time. With the feature, the same record differs from one seed to the
next (11 of the lines of a smaller one, between seeds 1 and 3): the check is not empty. It is what
found that a rule, an enum checked as a whole and what a pointer aims at had each been left to the
layout that `rustc` chose: a type of each is in the record. The cases of `tests/compile-fail-kept`
hold each refusal and its message, and the ones of `tests/compile-pass-kept` what compiles
(`tests/compile_fail.rs` takes them without the feature, and `tests/compile-fail-unstable` with it;
the other cases are the same both ways). With the feature (`--features unstable`), the targets that
need it run as well (`required-features` in `Cargo.toml`: enums without `repr`, `validate_in` of
`Option<bool>`, the `codegen` example, the benchmark), and the tests that are about the layout that
`rustc` chose (`#[cfg(feature = "unstable")]`).


## Model check

`puns-model/` compares every verdict of the analysis with oracles that do not use it,
through the test-only `verdict` feature (`puns::hook`: what each function would do, without
failing to compile): about a hundred types of 1 to 8 bytes, every ordered pair of the same size,
every value of the source over an alphabet (all bytes up to 2 bytes); slices and slice-tailed
structs from every pattern of up to 6 bytes. It tells what a type requires (`valid(..)`, rules)
from what it guarantees, and finds unsound verdicts (a value let through that the type system
does not allow, a byte of padding read), incorrect ones (a valid value rejected at run time, a
requirement of the target left unchecked that the source does not imply) and imprecise ones
(counted).

What the requirements of a source imply is found by the oracle, one requirement at a time: the
oracle of each type says what it requires, with where and what each requirement is
(`puns_model::require`), and a requirement of the target is implied if every value of the source
that meets the requirements of the source meets it (`implied`). A requirement may be left
unchecked only if it is implied; nothing of the library's test (the same rules, the same extent)
is in the oracle. Before, an unchecked requirement was not reported for a value that met the
requirements of its source, whatever they were: a rule of the target wrongly taken as proven by
another one could not be seen. The types include the same rules over other fields, another
rule, another extent, in a struct, an enum with and without `repr`, an array, a union and a
struct with a byte that may be uninitialized; the model check reports both a rule proven by
another (`same_rules` that compares nothing) and a `&mut` part over a rule that the target does
not have (`cut` without its test), which were tried.

The types include enums without `repr` (a niche in a `bool`, a `NonZero`, a `char`, a niche shared
by two variants next to a field of a third, a direct tag of one byte and one that `rustc` widened,
one variant, and a variant without values that `rustc` gives a tag), whose oracles are written
from how `rustc` lays enums out, checked against the bytes of values natively and by Miri on the
values the oracle makes, with the offsets of the fields taken from values, never from the
library; unions, whose oracle is "one of its fields"; a variant without values; `MaybeUninit`;
and a scalar that may be uninitialized (`Zero`), against which the reading of such a scalar as
initialized is found unsound.

Parts are checked over the same types, for every pair whose target is not larger than its source
(about four thousand): `head*` and `tail*` against the oracle of the target at offset 0 and at
`size(S) - size(D)`, the bytes and the address that they give at run time, and `head_mut` and
`tail_mut` by writing every value of the target into every value of the source: the result must
be a valid source, and one that still meets its requirements if it did and if what was written
meets those of the target (which is what a rule of the source over the part, or a part of an
enum, would break). A wrong shift, in the analysis or in the pointer, and a `mutable_part`
without its second condition, are caught.

View enums are checked against what they mean (`puns_model::views`): the first variant that the
bytes are, found by trying each variant in turn with `from_bytes` and `from_head` of its type,
which the pairs above check against the oracles. Every ordered pair of the types of 1 and of 2
bytes is a view of two variants (`Two<A, B>`, a generic view enum: `A` is decided from `B` where
`B` takes every `A`), compared on every pattern of bytes up to one more than their size, at an
aligned and a misaligned address: the variant, its address, its size and what remains. A variant
that its type does not take from the bytes is unsound, another variant or none is incorrect. A
sample runs on a push (`tests/views.rs`, with views of three variants, a chain of bases and a base
shared by two, and of unsized types, whose extents `from_head` takes from a rule or as many
elements as fit), and under Miri a view for each unsafe path of `view.rs`; every pair with
`full`. The report counts the variants that have a base, since a view without one checks nothing
new. A variant checked for nothing from its base, a base chosen without the test that it takes
the variant, a base for `from_head` that takes other bytes, extents compared with their trailing
padding, and a variant rebuilt from its state with another length were tried, and are reported.

Every check that CI runs on a push takes at most a minute. So `cargo test` runs a sample of the
model check, and Miri a selection of pairs chosen for the unsafe paths they reach, not for the
verdicts: no undefined behaviour is what Miri is there to see (`puns-model/tests/miri_sample.rs`
lists what each pair is for). The examples other than `ethernet` are not run under Miri either:
they are declarations, and the unsafe code they reach is that of the library, which its own
tests and `ethernet` run there. The
exhaustive run, every pair and every part of every type, is behind the `full` feature, which
`cargo test` does not build: `cargo test --profile model -p puns-model --features full`
(optimized, without LTO: minutes to build, two to run). The `model.yml` workflow runs it every
night and on demand. See
`puns-model/src/lib.rs` for what is checked.

Self-relative pointers have a model of their own (`puns_model::pointers`), on the pointers of
the tests of `puns` (`tests/support/relative.rs`: one value, a sequence, text, and nullable,
declared through the public API only; built in a buffer, and followed through it). Sixteen types
that hold pointers (recursive structs, through a pointer that may be null and through a
sequence; enums; one pointer to a `bool`, a `u8`, two bytes, a sequence, text; another byte
order; bytes) are each described by hand, and the oracle is a reference validator that walks
concrete bytes with such a description: every byte it reads is in bounds and was written to,
every value is valid, every pointer leads to a place aligned for its target, which holds one.
Every value of a corpus of a source is read as each target: what `view_ref` accepts must be
valid (else it is unsound), and is then read through (under Miri too); `try_view_ref` must
accept the values that are valid and only them. What the oracle alone says of pairs whose answer
is known is asserted, so that it is not an oracle that accepts everything; the builder leaves a
byte that nothing writes after every value, so that a target read past its end is seen.
`hook::views` answers with the `const` code that decides the pairing (`deep::local`, pointers
followed a bounded number of times); the functions themselves are called for pairs that it
accepts, and the compile-fail tests are the pairs that they refuse. A sample runs on a push
(`cargo test -p puns-model --test pointers`), every pair of every type with `full`, at night. It
counts as imprecise what is refused by design: a target of another size, a sequence or text read
as something else, a check that would be behind a pointer.

## Not done yet

- **One walk to copy a value and hand its pointers.** A deep copy is `copy_in` then `pointers_in`
  on the same value: the tag of an enum is read and its variant chosen twice, about 2 instructions
  per enum value (`nearest`'s compaction of a tree of 2^12 leaves: 401,330 instructions against
  393,080 for its former generated code; a table of structs is cheaper than before). One walk
  would hand the pointers to the visitor during the copy, but the visitor of a deep copy allocates
  the targets, which can move the buffer under the copy: it would have to keep the pointers aside
  until the walk ends. A prototype that did so on a stack of its own was twice as slow (an
  indirect call and a push and a pop per pointer).

- **Checks behind pointers**: `try_view_ref` checks the value it is given, and asks of what that
  value points to what `view_ref` does. Checking behind pointers is a walk of the buffer with the
  marks of each pair of types, which only the crate that has the pointers can make (it knows the
  way to the targets). `validate_in` is the walk for one type (everything checked); the pair
  version would be a function of its own, `validate_pair_in::<S, D, V>`, which checks what the
  marks of the analysis of `S` as `D` leave, and hands over pairs of pointers through a method
  added to `Visit` with a default body (`pointer_pair::<PS, PD>(at)`, which by default validates
  the target of `PD` alone): adding either breaks nothing of `validate_in`.
- **Precise mutable parts**: `head_mut` and `tail_mut` refuse any source with a rule over the
  part that the target does not have over the same extent, while the operations of a rule tell
  which fields and bytes it reads (a rule over other fields only could be allowed), and any enum
  only partly covered, while an enum all of whose variants accept what is written could be
  allowed. A `pun_mut` of a whole value does not ask this of rules: through `&mut Plain`, a
  struct read from a `&mut S` whose rule `Plain` does not have can be written past that rule, as
  through the fields of `S` itself (a rule is a requirement, see "What the type system enforces").
  The two could be made the same, either way.
- **Ranges from conditions**: a value is narrowed by what it is made of and by the reads that
  succeeded, not by a comparison that held. The condition of a walk (`|i| i < count`) does not
  narrow its step, nor the condition of an `if` its branches; so a counter that a walk increments
  until a field is computed on 128 bits, unless it is read at. The walks of a walk of a walk have
  a state that may be anything. Neither is in the examples, whose walks move offsets.
- **Rules implied by other rules**: a rule is proven by the same rules, not by others that imply
  it (`v % 6 == 0` implies `v % 3 == 0`), nor by the same rule of a struct of another extent that
  does not read the extent (`a != b` in a struct of 2 bytes and in one of 3). The model check
  counts what the source implies and the library checks all the same as nothing: it is not
  wrong.
- **A rule that two variants of a view share, where neither takes the other**: a variant is
  decided from a later one that takes all its values ("Views"), not from one that only shares a
  rule with it. Of two such variants, which the bytes could both be but for the rule (each
  requires a field of its own that the bytes have, or a rule of its own after the shared one),
  bytes that fail the rule compute it for each. Ending the later variants that have a rule that
  failed, over the same extent at the same place (`Layout::same_rules`), would have the validation
  report which rule failed, which every check would carry, where it is now a `bool` that the
  optimizer folds. A view that declares, after them, a variant that takes both (`Frame<Ipv4>` of
  two kinds of IPv4 packet) gets this, and shares what succeeds as well.
- No checked parts of a typed value (`try_head`...): a part is proven or refused; bytes have
  `from_head` and `from_tail`.
- **Precise checks of an enum made from something else**: when only some of its variants are
  proven, or some values of the source are no tag, the whole enum is checked at run time (its
  tag, then the fields of its variant), not only what was not proven.
- **An enum checked into what is not an enum**: `try_pun::<Shape, CirclePad>` is refused for a
  byte of padding of a variant that the check of the tag would exclude. As a source, an enum is
  known by the bytes that every variant initializes, not variant by variant; relating the target
  to each variant of the source whose tag it may accept would make it checked.
- **Checking some fields of a union**: when the check of one field would read a byte that may be
  padding, and no field is proven, the pun is refused; checking the other fields only would be
  sound, but the marks of the analysis cannot tell the validation which fields to skip.
- **The identity of an enum whose bytes may be uninitialized**: a scalar of `Spec::maybe_uninit`
  is equal to none, so a struct or an enum that holds one does not prove itself by
  `Layout::equals` (the library makes none such today: discovered tags are exact).
- Patterns other than `str` as types: the engine takes any DFA of at most 64 states, as a type
  that guarantees it, but such a type cannot come from the derive, whose fields safe code can
  write: a struct requires a pattern of its bytes with a rule, `matches(DFA, start, end)`
  (`Op::Matches`), proven only by the same rule, as any rule. Automata are compared by their
  states and edges, not by their `id`, which a user can choose.
- A byte order that a value tells (`EI_DATA` of ELF, the byte order mark of TIFF): a type has a
  byte order of its own (`u32be`, `i16le`, `f64be`...), so this is a choice between two types,
  which an enum of references makes.
- `default()`: building a value from the defaults of its fields (literals, sizes).
- A rule names the integer fields of its struct, not a field of a field (`header.length`) or an
  element of an array (`magic[0]`), which it reads with `be` or `le` at their offsets. Loading
  them would be sound if the offset were checked against the layout of the field (a scalar
  there, of that size), as it is for the fields of the struct.
- Rules compute on unsigned integers: a signed or floating-point field is refused, and read as
  bits with `le` or `be`. A getter reads fields, not bytes: a value that only the bytes hold
  (a variable-length integer) is decoded by use code.
- A size that is not in the value: "to the end of what holds it" (an MP4 box of size 0).
- Sums of types in sequences, and recursion: `OPTIONAL` and `CHOICE` of ASN.1, the containers of
  CBOR and MessagePack. A chain of types (`Then<A, B>`) is one sequence of types.
- An arbitrary function in a rule (getters take any). The validator is `const fn` code, generic
  over the type it validates, and on stable (1.100) such code cannot call a function that the type
  chooses: a call through a function pointer is refused ("function pointer calls are not allowed
  in constant functions"), a trait method too (E0015, "cannot call non-const associated
  function"), and const traits are unstable (E0658), all probed. A user function could only be
  called if validation stopped being `const`, which every function of the crate is, or behind
  a check that only runs outside of `const`, which stable Rust cannot tell. So what a rule
  computes is built in, and described by data where it varies: walks, variable-length integers,
  CRCs by their parameters, sums. The data is all the analysis sees, and it does not reason about
  it (a rule is proven only by the same rule): a checksum only makes a check stricter.

## Limits

- Pointers are paired by position: reading a value that holds pointers as another type
  (`view_ref`) needs the fields that hold pointers at the same position in both structs, the
  variants that do at the same position in both enums, and does not look through a struct that
  only holds a pointer. See "Self-relative pointers".
- The derive bounds its implementation by "the type of each field that depends on a parameter is
  `Pun`", but for the fields that name the type itself (a recursive type would be `Pun` if it
  is). So the parameter that is the offset of a recursive type is declared with what its
  pointers need (`O: Offset` in the tests, a trait of the crate that declares the pointers); and
  two generic types that hold each other, whose bounds would be found in a circle (`overflow
  evaluating the requirement`), tell theirs: `#[pun(bound())]` on both when the bounds that the
  types declare are enough, as `O: Offset` is.
- The types of the fields of a struct must be at least as visible as the struct: the list of
  fields is an associated type, which the privacy rules of Rust do not allow to name a less
  visible type.
- A struct of more than about a hundred fields may need `#![recursion_limit]` to be raised in
  the crate that validates it.
- A packed struct whose unsized field has trailing padding of its own is rejected.
- An enum with fields of more than about 120 variants needs `#![recursion_limit]` to be raised in
  the crate that validates it (validation recurses over the list of the variants).
- The build of a view enum grows as the square of the number of its variants ("Views"): 131
  variants build in 7.9 s in debug.
- `PhantomData<T>` is `Scalar` for any `T`, and deriving `Pun` exposes private fields: see
  "What the type system enforces" for which types must not derive `Pun`.
- The evaluator of rules has a stack of 16 values and 8 variables, an expression at most 256
  operations and at most 4 walks one in another (the derive says when an expression needs more,
  and how much), and a walk takes at most `8 * size + 128` steps.
- Unions: of `repr(C)` only (the fields of another are not known to be at 0), without a field
  that holds a pointer, and known as nothing as a source (section 5).
- An enum without `repr` holds no self-relative pointer; its direct tag, all the bytes before the
  first field of any variant, is 1, 2, 4, 8 or 16 bytes (else it is refused: `rustc` makes no
  other); a variant without values whose fields `rustc` places before those of the others makes
  the build fail rather than be guessed (its fields cannot be observed, R4).
