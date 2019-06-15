extern crate r2pipe;
use r2pipe::R2Pipe;

extern crate rand;
use rand::prelude::random;

use super::{
	synth_tree,
	R2Error,
	BaseT,
};

use std::collections::HashMap;

pub enum Synthesiser {
	Tree,
}

pub struct Session {
	r2: R2Pipe,
	fcn_config: FcnConfig,
	traces: Traces,
}

pub struct FcnConfig {
	pub path: String,
	pub loc: String,
	pub len: String,
	pub input_regs: Vec<String>,
	pub output_reg: String
}

pub struct Traces {
	pub inputs: HashMap<String,Vec<BaseT>>,
	pub outputs: Vec<BaseT>,
}

impl Traces {
	pub fn push_strings(&mut self, registers: Vec<String>, input: Vec<String>, output: String) -> Result<(), String> {
		if registers.len() != input.len() {
			return Err("expected one input for each register".to_string())
		}
		for (reg, val) in registers.iter().zip(input) {
			self.inputs.get_mut(reg).unwrap().push(val.parse().unwrap());
		}
		self.outputs.push(output.parse().unwrap());
		Ok(())
	}
}

impl Session {
	// Spawn r2pipe, init esil
	pub fn init(fcn: FcnConfig) -> Result<Session, R2Error> {
		let mut r2pipe = if let Ok(pipe) = R2Pipe::spawn(&fcn.path, None) { pipe }
		else { return Err(R2Error::PipeFail) };

		if let Ok(_) = r2pipe.cmd("aaa;aei;aeim") {} 
		else { return Err(R2Error::CmdFail) };

		let mut inputs = HashMap::new();
		for register in fcn.input_regs.iter() {
			inputs.insert(register.to_string(), Vec::new());
		}
		
		Ok(Session {
			r2: r2pipe,
			fcn_config: fcn,
			traces: Traces { inputs: inputs.clone(), outputs: Vec::new() }
		})
	}

	pub fn add_trace(&mut self) -> Result<String, String> {
		// Flush old stuff and seek to target fcn
		let cmd = "aek-;s ".to_string() + &self.fcn_config.loc;
		let _res = self.r2.cmd(&cmd);
		// Init esil
		let cmd = "aei;aeim;aeip";
		let _res = self.r2.cmd(&cmd);
		// Set random input
		let input = get_random_input(self.fcn_config.input_regs.len() as u8);
		for n in 0..self.fcn_config.input_regs.len() {
			let cmd = "aer ".to_string() + &self.fcn_config.input_regs.get(n).unwrap()
				+ &" = ".to_string() + input.get(n).unwrap();
			let _res = self.r2.cmd(&cmd);
		}
		// Run
		let cmd = self.fcn_config.len.clone() + &"aes".to_string();
		let _res = self.r2.cmd(&cmd);
		// Fetch result
		let reg: &str = &self.fcn_config.output_reg;
		let output = self.r2.cmdj("aerj")?[reg].to_string();
		let result = output.clone();

		let registers = self.fcn_config.input_regs.clone();
		if let Ok(_) = self.traces.push_strings(registers, input, output) { return Ok(result) };
		Err(String::new())
	}

	pub fn deobfuscate(self, backend: Synthesiser) {
		let inputs = self.traces.inputs;
		let outputs = self.traces.outputs;
		let registers = self.fcn_config.input_regs.clone();
		match backend {
			Synthesiser::Tree => {
				let mut synthesis = synth_tree::Synthesis::default(&registers);
				synthesis.synthesize(&inputs, &outputs);
			},
		}
	}
}

// TODO: Add types enum to support multiple random input types
fn get_random_input(n: u8) -> Vec<String> {
	let mut result: Vec<String> = Vec::new();
	for _i in 0..n {
		loop {
			let r = random::<u8>();
			if r != 0 {
				result.push(r.to_string());
				break
			}
		}
	}
	result
}
