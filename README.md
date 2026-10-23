# r2deob

deobfuscation PoC with [PolkaVM](https://github.com/paritytech/polkavm)

# What

r2deob is a small tool that does some sort of [program synthesis](https://en.wikipedia.org/wiki/Program_synthesis). For a given PolkaVM program blob, you define a function (an exported symbol) as well as the input and output register(s). r2deob then executes that function in [PolkaVM](https://github.com/paritytech/polkavm) a couple of times with different random inputs and fetches the value stored in the output register after each run. Afterwards the generated input/output behaviour is sent to a deobfuscation backend which will try to find an expression that is mathematically true and semantically represents your target function (this process may or may not deobfuscate something but usually does).

Example:
Check out the guest crate in the `guest` directory of this repository. It contains the following simple function:
```rust
#[polkavm_derive::polkavm_export]
extern "C" fn calc(a: i32, b: i32) -> i32 {
	let d = b.wrapping_mul(2);
	a.wrapping_add(d)
}
```

Given the program blob `calc.polkavm` exporting this function, we can define the following deobfuscation target:
```rust
let target = FcnConfig {
	path: "calc.polkavm".to_string(), // Path to the program blob
	export: "calc".to_string(), // Exported function to call
	input_regs: vec!["a0".to_string(), "a1".to_string()], // Input registers
	output_reg: "a0".to_string(), // Output register
};
```

r2deob will then find out that the target is semantically identical to the expression "a1 + (a0 + a1)", because this expression matches the observed input/output behaviour.
```
$ cargo run --release
Winner! (a1 + (a0 + a1))
```

## Rebuilding the example blob

The guest needs a nightly toolchain with `rust-src` and a `polkatool` matching the `polkavm` version:
```
$ cd guest
$ cargo +nightly build -Z build-std=core -Z json-target-spec --release --target $(polkatool get-target-json-path)
$ polkatool link --strip --output ../calc.polkavm target/riscv64emac-unknown-none-polkavm/release/calc
```

The project is based on [this paper](https://www.usenix.org/system/files/conference/usenixsecurity17/sec17-blazytko.pdf). Check out their [awesome talk](https://media.ccc.de/v/34c3-8789-lets_break_modern_binary_code_obfuscation) and [syntia](https://github.com/RUB-SysSec/syntia) to get an idea on how the Tree deobfuscation backend works.

# Why

Personal fun and learning experience.

# Status and Limitations

Disclaimer: This is just a PoC, YMMV. I didn't test it much yet, the code probably needs cleanup at some places and there are still many TODOs. If you are looking for something that is based on serious research, tested and working I recommend to use [syntia](https://github.com/RUB-SysSec/syntia).

Target functions must not call host functions (`ecalli`); any execution that does not finish normally aborts tracing.

TODOs
- Solving expression with constant
- Improving scoring
- Add user interface and provide some documentation
- Experimental: Trying automated function deobfuscation based on calling conventions
