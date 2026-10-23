use std::{
	fmt,
	fs::File,
	io::{BufReader, Read},
	path::Path,
	time::Duration,
};

use polkavm::{
	Config, Engine, InterruptKind, Module, ModuleConfig, ProgramCounter, RawInstance, Reg,
};

use crate::{
	checkpoint,
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
	Checkpoint(&'static str),
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			Error::Io(error) => write!(f, "I/O error: {error}"),
			Error::Vm(error) => write!(f, "PolkaVM error: {error}"),
			Error::UnknownExport(name) => write!(f, "program has no export named '{name}'"),
			Error::UnknownRegister(name) => write!(f, "unknown register '{name}'"),
			Error::Interrupted(kind) => write!(f, "execution did not finish: {kind:?}"),
			Error::Checkpoint(reason) => write!(f, "cannot resume: {reason}"),
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

#[derive(Debug, Clone)]
pub struct FcnConfig {
	pub path: String,
	pub export: String,
	pub input_regs: Vec<String>,
	pub output_reg: String,
	pub width: Width,
}

pub struct Status {
	pub elapsed: Duration,
	pub playouts: u64,
	pub nodes: usize,
	pub best_score: f64,
	pub best: Option<Expression>,
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

	/// Drops the VM after sampling, before the search starts.
	pub fn deobfuscate(
		self,
		grammar: &Grammar,
		config: &SearchConfig,
		resume: Option<&Path>,
		progress: &(dyn Fn(&Status) + Sync),
	) -> Result<Synthesis, Error> {
		match self.fcn_config.width {
			Width::W32 => self.synthesize::<u32>(grammar, config, resume, progress),
			Width::W64 => self.synthesize::<u64>(grammar, config, resume, progress),
		}
	}

	fn check<W: Word>(&mut self, samples: &Samples<W>) -> Result<(), Error> {
		for i in 0..SAMPLES {
			let args: Vec<u64> = samples.columns[..self.input_regs.len()]
				.iter()
				.map(|column| column[i].to_reg())
				.collect();
			if W::from_reg(self.call(&args)?) != samples.output[i] {
				return Err(Error::Checkpoint("its samples do not match the target"));
			}
		}
		Ok(())
	}

	fn synthesize<W: Word>(
		mut self,
		grammar: &Grammar,
		config: &SearchConfig,
		resume: Option<&Path>,
		progress: &(dyn Fn(&Status) + Sync),
	) -> Result<Synthesis, Error> {
		let productions = grammar.productions(self.input_regs.len());
		let mut checkpoint = resume.map(File::open).transpose()?.map(BufReader::new);
		let (samples, verification, version) = match &mut checkpoint {
			Some(input) => {
				let header = checkpoint::read_header::<W>(input)?;
				if header.productions != productions || header.max_len != config.max_len {
					return Err(Error::Checkpoint(
						"it was made with different --ops, --consts or --max-len",
					));
				}
				self.check(&header.samples)?;
				self.check(&header.verification)?;
				(header.samples, header.verification, header.version)
			}
			None => (
				self.sample::<W>(&grammar.constants, false)?,
				self.sample::<W>(&grammar.constants, true)?,
				0,
			),
		};
		let leaves: Vec<String> = self
			.fcn_config
			.input_regs
			.into_iter()
			.chain(grammar.constants.iter().map(u64::to_string))
			.collect();
		drop(self.instance);

		let report = |p: &mcts::Progress| {
			progress(&Status {
				elapsed: p.elapsed,
				playouts: p.playouts,
				nodes: p.nodes,
				best_score: p.best_score,
				best: (!p.best.is_empty()).then(|| Expression {
					tokens: p.best.to_vec(),
					leaves: leaves.clone(),
				}),
			})
		};
		let resume = checkpoint
			.as_mut()
			.map(|input| (input as &mut dyn Read, version));
		let outcome = mcts::search(
			&productions,
			&samples,
			&verification,
			config,
			resume,
			&report,
		)?;
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
