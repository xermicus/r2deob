# r2deob

deobfuscation PoC with [PolkaVM](https://github.com/paritytech/polkavm)

# What

r2deob is a small tool that does some sort of [program synthesis](https://en.wikipedia.org/wiki/Program_synthesis). It calls an exported function of a PolkaVM program blob on random inputs, records the output register, and searches for an expression with the same input/output behaviour (this process may or may not deobfuscate something but usually does).

The project is based on [this paper](https://www.usenix.org/system/files/conference/usenixsecurity17/sec17-blazytko.pdf). Check out their [awesome talk](https://media.ccc.de/v/34c3-8789-lets_break_modern_binary_code_obfuscation) and [syntia](https://github.com/RUB-SysSec/syntia).

# Usage

```
$ cargo run --release -- calc --width 32
825.289µs          400 playouts    0.5 M/s  Winner! ((a1 + a0) + a1)
```

See `--help` for the options. The example targets are in `guest/`. Rebuilding `calc.polkavm`:
```
$ cd guest
$ cargo +nightly build -Z build-std=core -Z json-target-spec --release --target $(polkatool get-target-json-path)
$ polkatool link --strip --output ../calc.polkavm target/riscv64emac-unknown-none-polkavm/release/calc
```

# Why

Personal fun and learning experience.

# Status and Limitations

Disclaimer: This is just a PoC, YMMV. If you are looking for something that is based on serious research, tested and working I recommend to use [syntia](https://github.com/RUB-SysSec/syntia).
