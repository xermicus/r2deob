use std::{collections::HashMap, fmt};

use polkavm::{
	Config, Engine, InterruptKind, Module, ModuleConfig, ProgramCounter, RawInstance, Reg,
};

use crate::{BaseT, ast::Expression, synth_tree};

pub enum Synthesiser {
	Tree,
}

#[derive(Debug)]
pub enum Error {
	Io(std::io::Error),
	Vm(polkavm::Error),
	UnknownExport(String),
	UnknownRegister(String),
	Interrupted(InterruptKind),
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			Error::Io(error) => write!(f, "failed to read program: {error}"),
			Error::Vm(error) => write!(f, "PolkaVM error: {error}"),
			Error::UnknownExport(name) => write!(f, "program has no export named '{name}'"),
			Error::UnknownRegister(name) => write!(f, "unknown register '{name}'"),
			Error::Interrupted(kind) => write!(f, "execution did not finish: {kind:?}"),
		}
	}
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
	fn from(error: std::io::Error) -> Self {
		Error::Io(error)
	}
}

impl From<polkavm::Error> for Error {
	fn from(error: polkavm::Error) -> Self {
		Error::Vm(error)
	}
}

impl From<polkavm::CompileError> for Error {
	fn from(error: polkavm::CompileError) -> Self {
		Error::Vm(error.into())
	}
}

/// The function to deobfuscate: an export of a `.polkavm` program blob.
pub struct FcnConfig {
	pub path: String,
	pub export: String,
	pub input_regs: Vec<String>,
	pub output_reg: String,
}

#[derive(Default)]
pub struct Traces {
	pub inputs: HashMap<String, Vec<BaseT>>,
	pub outputs: Vec<BaseT>,
}

impl Traces {
	pub fn push(&mut self, registers: &[String], input: &[BaseT], output: BaseT) {
		for (register, value) in registers.iter().zip(input) {
			self.inputs
				.entry(register.clone())
				.or_default()
				.push(*value);
		}
		self.outputs.push(output);
	}
}

pub struct Session {
	instance: RawInstance,
	entry: ProgramCounter,
	input_regs: Vec<Reg>,
	output_reg: Reg,
	fcn_config: FcnConfig,
	traces: Traces,
}

impl Session {
	pub fn init(fcn: FcnConfig) -> Result<Session, Error> {
		let engine = Engine::new(&Config::from_env()?)?;
		let module = Module::new(
			&engine,
			&ModuleConfig::new(),
			std::fs::read(&fcn.path)?.into(),
		)?;
		let entry = module
			.exports()
			.find(|export| export.symbol() == fcn.export.as_str())
			.map(|export| export.program_counter())
			.ok_or_else(|| Error::UnknownExport(fcn.export.clone()))?;
		let input_regs = fcn
			.input_regs
			.iter()
			.map(|name| parse_reg(name))
			.collect::<Result<_, _>>()?;
		let output_reg = parse_reg(&fcn.output_reg)?;

		Ok(Session {
			instance: module.instantiate()?,
			entry,
			input_regs,
			output_reg,
			fcn_config: fcn,
			traces: Traces::default(),
		})
	}

	/// Runs the target function once on random inputs and records the observed output.
	pub fn add_trace(&mut self) -> Result<BaseT, Error> {
		let input = get_random_input(self.input_regs.len());
		self.instance.prepare_call_untyped(self.entry, &[]);
		for (reg, value) in self.input_regs.iter().zip(&input) {
			self.instance.set_reg(*reg, *value as u64);
		}

		match self.instance.run()? {
			InterruptKind::Finished => {}
			interrupt => return Err(Error::Interrupted(interrupt)),
		}

		let output = self.instance.reg(self.output_reg) as BaseT;
		self.traces
			.push(&self.fcn_config.input_regs, &input, output);
		Ok(output)
	}

	pub fn deobfuscate(self, backend: Synthesiser) -> Option<Expression> {
		match backend {
			Synthesiser::Tree => synth_tree::Synthesis::new(&self.fcn_config.input_regs)
				.synthesize(&self.traces.inputs, &self.traces.outputs),
		}
	}
}

fn parse_reg(name: &str) -> Result<Reg, Error> {
	Reg::ALL
		.into_iter()
		.find(|reg| reg.name() == name)
		.ok_or_else(|| Error::UnknownRegister(name.to_string()))
}

fn get_random_input(n: usize) -> Vec<BaseT> {
	(0..n)
		.map(|_| BaseT::from(rand::random_range(1..=u8::MAX)))
		.collect()
}

#[test]
fn test_trace_calc() {
	let mut session = Session::init(FcnConfig {
		path: concat!(env!("CARGO_MANIFEST_DIR"), "/calc.polkavm").to_string(),
		export: "calc".to_string(),
		input_regs: vec!["a0".to_string(), "a1".to_string()],
		output_reg: "a0".to_string(),
	})
	.unwrap();
	for _ in 0..8 {
		session.add_trace().unwrap();
	}
	let traces = &session.traces;
	for (i, output) in traces.outputs.iter().enumerate() {
		assert_eq!(*output, traces.inputs["a0"][i] + 2 * traces.inputs["a1"][i]);
	}
}
