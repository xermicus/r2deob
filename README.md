# r2deob

deobfuscation PoC with [PolkaVM](https://github.com/paritytech/polkavm)

# What

r2deob is a small tool that does some sort of [program synthesis](https://en.wikipedia.org/wiki/Program_synthesis). For a given PolkaVM program blob, you name an exported function together with its input and output register(s). r2deob calls that function in [PolkaVM](https://github.com/paritytech/polkavm) on random inputs, records the output register, and then searches for a small expression with exactly the same input/output behaviour (this process may or may not deobfuscate something but usually does).

The guest crate in `guest/` builds `calc.polkavm` with a few example targets:

| export | inputs | what it computes |
|--------|--------|------------------|
| `calc` | `a0, a1` (32 bit) | `a + 2 * b`, the original example |
| `mba`  | `a0, a1, a2` | `(a + b) * c - (a ^ b)`, hidden behind mixed boolean-arithmetic identities |
| `mba2` | `a0..a3` | `(a + b) * (c ^ d) - (a & d)`, same idea, one size larger |
| `rsa`  | `a0, a1` | textbook RSA `m^65537 mod n`; not expressible by this search, used as a grinding benchmark |

```
$ cargo run --release -- calc --width 32
825.289µs          400 playouts    0.5 M/s  Winner! ((a1 + a0) + a1)
$ cargo run --release -- mba --inputs a0,a1,a2
  2.760ms       360232 playouts  130.5 M/s  Winner! ((a2 * (a0 + a1)) - (a2 ^ ((a0 ^ a2) ^ a1)))
```

The available operations are chosen per run with `--ops` (default `add,sub,mul,xor`; all of `add sub mul mulhu divu remu and or xor shl shrl shra neg not` with PolkaVM/RISC-V semantics), constant leaves with `--consts`. See `--help` for the search parameters.

## How the search works

The project is based on [this paper](https://www.usenix.org/system/files/conference/usenixsecurity17/sec17-blazytko.pdf). Check out their [awesome talk](https://media.ccc.de/v/34c3-8789-lets_break_modern_binary_code_obfuscation) and [syntia](https://github.com/RUB-SysSec/syntia).

- Monte Carlo tree search over leftmost derivations of the expression grammar. A derivation is a prefix-notation token string, so a tree node needs no expression of its own: its token and child count follow from the path.
- Lock-free trees: a node is 24 bytes of atomics. Children are published with a single compare-and-swap; statistics use plain relaxed loads and stores (lost updates are tolerated when threads share a tree). By default every thread grows its own tree.
- Fully evaluated subtrees are marked exhausted and skipped, so a bounded search space is eventually enumerated and reported as such.
- Each descent runs several random playouts, biased towards small expressions.
- Playouts are scored on 16 random samples at once with AVX2 arithmetic on fixed-size lane arrays. A candidate matching all of them is confirmed on a second batch that includes edge cases (0, 1, `MAX`, the sign bit, ...). Division by an input or constant uses precomputed per-lane reciprocals instead of hardware division.

Measured on a Ryzen 9 3900X (12 cores, 24 threads): about 116 M playouts/s on `mba2` with eight operations (6.8 M/s on one thread), 72 M/s grinding `rsa` with `add,sub,mul,remu`; `mba` with eight operations is solved in a median of 77 ms.

## Rebuilding the example blob

The guest needs a nightly toolchain with `rust-src` and a `polkatool` matching the `polkavm` version:
```
$ cd guest
$ cargo +nightly build -Z build-std=core -Z json-target-spec --release --target $(polkatool get-target-json-path)
$ polkatool link --strip --output ../calc.polkavm target/riscv64emac-unknown-none-polkavm/release/calc
```

The host is built for `x86-64-v3` (AVX2, BMI2, LZCNT; any Zen or Haswell or newer), see `.cargo/config.toml`.

# Why

Personal fun and learning experience.

# Status and Limitations

Disclaimer: This is just a PoC, YMMV. If you are looking for something that is based on serious research, tested and working I recommend to use [syntia](https://github.com/RUB-SysSec/syntia).

Target functions must not call host functions (`ecalli`); any execution that does not finish normally aborts tracing.

The search only finds shallow, tree-shaped expressions whose outputs are close to the target in bit, arithmetic or byte distance. That fits obfuscation (MBA rewrites are shallow deformations of a simple expression) but not modular arithmetic like `rsa`: a tree cannot share subexpressions, 64-bit `mul` wraps where the target reduces a 128-bit product, and a distance score gives no partial credit once every bit is scrambled.

TODOs
- Expression DAGs (shared subexpressions) instead of trees
- Hypothesis-testing scores, e.g. `n | f(x) - y`, and solving for constants
- Lifting per basic block instead of synthesizing whole functions
