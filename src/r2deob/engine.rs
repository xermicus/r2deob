extern crate r2pipe;
use r2pipe::R2Pipe;

extern crate rand;
use rand::prelude::random;

use super::synth_tree;
use super::R2Error;

use std::collections::HashMap;

pub enum Synthesiser {
	BruteForce,
	HammingScore,
	HammingScoreAsync,
	LibEvoasm
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
	pub inputs: Vec<HashMap<String,u64>>,
	pub outputs: Vec<u64>,
}

impl Traces {
	pub fn push_strings(&mut self, registers: Vec<String>, input: Vec<String>, output: String) -> Result<(), String> {
		if registers.len() != input.len() { return Err("expected one input for each register".to_string()) };

		let mut inputs = HashMap::new();
		for i in 0..registers.len() {
			inputs.insert(registers[i].clone(), input[i].parse().unwrap());
		};
		self.inputs.push(inputs);
		self.outputs.push(output.parse().unwrap());
		Ok(())
	}

	pub fn inputs_as_str(&self) -> Vec<HashMap<String,String>> {
		let mut result: Vec<HashMap<String,String>> = Vec::new();
		for i in self.inputs.iter() {
			let mut input: HashMap<String,String> = HashMap::new();
			for (key, val) in i.clone().iter_mut() {
				input.insert(key.to_string(), val.to_string());
			};
			result.push(input);
		};
		result
	}
	
	//pub fn outputs_as_str(&self) -> Vec<String> {
	//	let mut result: Vec<String> = Vec::new();
	//	for i in self.outputs.iter() {
	//		result.push(i.to_string());
	//	};
	//	result
	//}
}

impl Session {
	// Spawn r2pipe, init esil
	pub fn init(fcn: FcnConfig) -> Result<Session, R2Error> {
		let mut r2pipe = if let Ok(pipe) = R2Pipe::spawn(&fcn.path, None) { pipe }
		else { return Err(R2Error::PipeFail) };

		if let Ok(_) = r2pipe.cmd("aaa;aei;aeim") {} 
		else { return Err(R2Error::CmdFail) };
		
		Ok(Session {
			r2: r2pipe,
			fcn_config: fcn,
			traces: Traces { inputs: Vec::new(), outputs: Vec::new() }
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
		let inputs = self.traces.inputs_as_str();
		let outputs = self.traces.outputs;
		let registers = self.fcn_config.input_regs.clone();
		match backend {
			Synthesiser::BruteForce => {
				let mut synthesis = synth_tree::Synthesis::default(&registers);
				synthesis.brute_force(inputs, outputs);
			},
			Synthesiser::HammingScore => {
				let mut synthesis = synth_tree::Synthesis::default(&registers);
				synthesis.hamming_score(inputs, outputs);
			},
			Synthesiser::HammingScoreAsync => {
				let mut synthesis = synth_tree::Synthesis::default(&registers);
				synthesis.hamming_score_async(inputs, outputs);
			},
			Synthesiser::LibEvoasm => {
				println!("not implemented");
			}
		}
	}
}

fn get_random_input(n: u8) -> Vec<String> {
	let mut result: Vec<String> = Vec::new();
	for _i in 0..n {
		result.push(random::<u8>().to_string()); // TODO: Add types enum to support multiple random input
	};
	result
}
