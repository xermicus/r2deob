extern crate r2pipe;
use r2pipe::R2Pipe;

extern crate rand;
use rand::prelude::random;
pub struct Session {
	r2: R2Pipe,
	fcn_config: FcnConfig,
	traces: Traces,
}

pub struct FcnConfig {
	pub loc: String,
	pub len: String,
	pub input_regs: Vec<String>,
	pub output_reg: String
}

pub struct Traces {
	pub inputs: Vec<Vec<u64>>,
	pub outputs: Vec<u64>
}

impl Traces {
	pub fn push_strings(&mut self, input: Vec<String>, output: String) -> Result<(), String> {
		self.inputs.push(input.iter().map(|x| x.parse().unwrap()).collect());
		self.outputs.push(output.parse().unwrap());
		//println!("{:?};{:?}", self.inputs, self.outputs);
		Ok(())
	}
}

impl Session {
	// Spawn r2pipe, init esil
	pub fn init(path: String, fcn: FcnConfig) -> Result<Session, String> {
		let mut r2pipe = R2Pipe::spawn(&path, None)?;
		let _anal = r2pipe.cmd("aaa")?;

		// Init esil
		let _res = r2pipe.cmd("aei;aeim");
		
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
		let input = get_random_input(2);
		for n in self.fcn_config.input_regs.iter() {
			
		};
		let _res = self.r2.cmd(&("aer edi = ".to_string() + &input[0])); // TODO
		let _res = self.r2.cmd(&("aer esi = ".to_string() + &input[1])); // TODO
		// Run
		let cmd = self.fcn_config.len.clone() + &"aes".to_string();
		let _res = self.r2.cmd(&cmd);
		// Fetch result
		let reg: &str = &self.fcn_config.output_reg;
		let output = self.r2.cmdj("aerj")?[reg].to_string();
		let result = output.clone();

		if let Ok(_) = self.traces.push_strings(input, output) { return Ok(result) };
		Err(String::new())
	}

	pub fn deobfuscate(self) {
		
	}
}

pub fn get_random_input(n: u8) -> Vec<String> {
	let mut result: Vec<String> = Vec::new();
	for _i in 0..n {
		result.push(random::<u8>().to_string()); // TODO: Support multiple random input
	};
	result
}
