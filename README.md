# r2deob

Function deobfuscation PoC with r2 + ESIL

# What

r2deob is a small tool that does some sort of [program synthesis](https://en.wikipedia.org/wiki/Program_synthesis). For a given binary, you can define a function (basically just an offset and amount of instructions to be considered after that offset) as well the input and output register(s). Using [ESIL](https://github.com/radare/radare2), r2deob will then emulate that codesection a couple of times with different random inputs each time and fetch the value stored inside the specified output register after each emulation. Afterwards the generated input/output behaviour is sent to a deobfuscation backend which will try to find an expression that is mathematcially true and semantically represents your target function (this process may or may not deobfuscate something but usually does).

Example:
Check out the main.c file in the root directory of this repository. It contains the following simple function:
```c
int calc (int a, int b) {
	int d = b * 2;
	int c = a + d;
	return c;
}
```

Given a binary containing this function at the location "sym.calc" we can define the following deobfuscation target:
```rust
	let target = r2deob::engine::FcnConfig {
		path: "/home/cyrill/r2deob/calc".to_string(), // Path to binary
		loc: "sym.calc".to_string(), // target location, can be a flag or address
		len: "12".to_string(), // #numbers of emulation steps before output register is considered
		input_regs: vec!["esi".to_string(),"edi".to_string()], // Input registers
		output_reg: "rax".to_string() // Outpout register
};
```

r2deob will then find out that the target is semantically identical to the expression "esi + (esi + edi)", because this expression matches the observed input/output behaviour.
```
$ ./target/debug/r2deob
Winner! (esi+(esi+edi))
```

The project is based on [this paper](https://www.usenix.org/system/files/conference/usenixsecurity17/sec17-blazytko.pdf). Check out their [awesome talk](https://media.ccc.de/v/34c3-8789-lets_break_modern_binary_code_obfuscation) and [syntia](https://github.com/RUB-SysSec/syntia) to get an idea on how the Tree deobfuscation backend works.

# Why

Personal fun and learning experience.

Using ESIL for the umulation part also means that you can deobfuscate binaries compiled for any target architecture that is also supported by ESIL (a lot).

# Status and Limitations

Disclaimer: This is just a PoC, YMMV. I didn't test it much yet, the code probably needs cleanup at some places and there are still many TODOs. If you are looking for something that is based on serious research, tested and working I recommend to use [syntia](https://github.com/RUB-SysSec/syntia).

Using ESIL emulation also bounds the tool to the limits of ESIL emulation (syscall support is incomplete ATM, which means that r2deob is very likely to break if you are trying to deobfuscate a code section containing syscalls). That's the main reason I'm considering to add support for generating input/output behaviour by directly executing the binary.

TODOs
- Solving expression with constant
- Improving scoring
- Add Libevoasm backend?
- Add user interface (or add to r2pm) and provide some documentation
- Add support for using custom r2 script during initialization
- Experimental: Trying automated function deobfuscation based on calling conventions
