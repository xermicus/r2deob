use std::{fmt, time::Duration};

use polkavm::{
	Config, CorePinning, Engine, InterruptKind, Module, ModuleConfig, ProgramCounter, RawInstance,
	Reg,
};

use crate::{
	eval::{SAMPLES, Samples, Word},
	expr::{Expression, Grammar},
	mcts::{self, SearchConfig},
};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Width {
	W32,
	W64,
}

/// The function to deobfuscate: an export of a `.polkavm` program blob.
pub struct FcnConfig {
	pub path: String,
	pub export: String,
	pub input_regs: Vec<String>,
	pub output_reg: String,
	pub width: Width,
}

#[derive(Debug)]
pub struct Synthesis {
	pub expression: Option<Expression>,
	pub exhausted: bool,
	pub playouts: u64,
	pub elapsed: Duration,
}

pub struct Session {
	instance: RawInstance,
	entry: ProgramCounter,
	input_regs: Vec<Reg>,
	output_reg: Reg,
	fcn_config: FcnConfig,
}

impl Session {
	/// Loads the program with PolkaVM's core pinning disabled; pinning the calling thread would
	/// confine every search thread spawned from it to that single core.
	pub fn init(fcn: FcnConfig) -> Result<Session, Error> {
		let mut config = Config::from_env()?;
		config.set_core_pinning(CorePinning::Disabled);
		let engine = Engine::new(&config)?;
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
		})
	}

	pub fn call(&mut self, args: &[u64]) -> Result<u64, Error> {
		self.instance.prepare_call_untyped(self.entry, &[]);
		for (&reg, &value) in self.input_regs.iter().zip(args) {
			self.instance.set_reg(reg, value);
		}
		match self.instance.run()? {
			InterruptKind::Finished => Ok(self.instance.reg(self.output_reg)),
			interrupt => Err(Error::Interrupted(interrupt)),
		}
	}

	/// Runs the target function on [`SAMPLES`] random inputs, led by edge cases if `edge_cases`.
	pub fn sample<W: Word>(
		&mut self,
		constants: &[u64],
		edge_cases: bool,
	) -> Result<Samples<W>, Error> {
		let edges = [
			0,
			1,
			2,
			u64::MAX,
			1 << (W::BITS - 1),
			u64::MAX >> (65 - W::BITS),
			0x5555_5555_5555_5555,
		];
		let mut inputs = vec![[W::default(); SAMPLES]; self.input_regs.len()];
		let mut output = [W::default(); SAMPLES];
		let mut args = vec![0; self.input_regs.len()];
		for i in 0..SAMPLES {
			for (j, (arg, input)) in args.iter_mut().zip(&mut inputs).enumerate() {
				let value = if edge_cases && i < edges.len() {
					edges[(i + 3 * j) % edges.len()]
				} else {
					rand::random()
				};
				input[i] = W::from_reg(value);
				*arg = input[i].to_reg();
			}
			output[i] = W::from_reg(self.call(&args)?);
		}
		Ok(Samples::new(inputs, constants, output))
	}

	pub fn deobfuscate(
		&mut self,
		grammar: &Grammar,
		config: &SearchConfig,
	) -> Result<Synthesis, Error> {
		match self.fcn_config.width {
			Width::W32 => self.synthesize::<u32>(grammar, config),
			Width::W64 => self.synthesize::<u64>(grammar, config),
		}
	}

	fn synthesize<W: Word>(
		&mut self,
		grammar: &Grammar,
		config: &SearchConfig,
	) -> Result<Synthesis, Error> {
		let samples = self.sample::<W>(&grammar.constants, false)?;
		let verification = self.sample::<W>(&grammar.constants, true)?;
		let outcome = mcts::search(
			&grammar.productions(self.input_regs.len()),
			&samples,
			&verification,
			config,
		);
		let leaves = self
			.fcn_config
			.input_regs
			.iter()
			.cloned()
			.chain(grammar.constants.iter().map(u64::to_string))
			.collect();
		Ok(Synthesis {
			expression: outcome.tokens.map(|tokens| Expression { tokens, leaves }),
			exhausted: outcome.exhausted,
			playouts: outcome.playouts,
			elapsed: outcome.elapsed,
		})
	}
}

fn parse_reg(name: &str) -> Result<Reg, Error> {
	Reg::ALL
		.into_iter()
		.find(|reg| reg.name() == name)
		.ok_or_else(|| Error::UnknownRegister(name.to_string()))
}

#[cfg(test)]
mod tests {
	use super::*;

	fn session(export: &str, input_regs: &[&str], width: Width) -> Session {
		Session::init(FcnConfig {
			path: concat!(env!("CARGO_MANIFEST_DIR"), "/calc.polkavm").to_string(),
			export: export.to_string(),
			input_regs: input_regs.iter().map(|reg| reg.to_string()).collect(),
			output_reg: "a0".to_string(),
			width,
		})
		.unwrap()
	}

	#[test]
	fn test_sample_calc() {
		let samples = session("calc", &["a0", "a1"], Width::W32)
			.sample::<u32>(&[], true)
			.unwrap();
		for i in 0..SAMPLES {
			let (a, b) = (samples.columns[0][i], samples.columns[1][i]);
			assert_eq!(samples.output[i], a.wrapping_add(b.wrapping_mul(2)));
		}
	}

	#[test]
	fn test_sample_mba() {
		let samples = session("mba", &["a0", "a1", "a2"], Width::W64)
			.sample::<u64>(&[], true)
			.unwrap();
		for i in 0..SAMPLES {
			let (a, b, c) = (
				samples.columns[0][i],
				samples.columns[1][i],
				samples.columns[2][i],
			);
			assert_eq!(
				samples.output[i],
				a.wrapping_add(b).wrapping_mul(c).wrapping_sub(a ^ b)
			);
		}
	}
}
