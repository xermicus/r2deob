use std::{
	fmt,
	fs::File,
	io::{BufReader, Read},
	path::Path,
	sync::Mutex,
	time::Duration,
};

use polkavm::{
	Config, Engine, InterruptKind, Module, ModuleConfig, ProgramCounter, RawInstance, Reg,
};

use crate::{
	checkpoint,
	eval::{SAMPLES, Samples, Word},
	expr::{Expression, Grammar, Token},
	mcts::{self, Problem, SearchConfig, Term},
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
	pub exploration: f64,
	pub best: Option<Expression>,
	/// Definitions of the compressed leaves that `best` refers to, directly or through other
	/// definitions, and no earlier status included, with their indices in `M0`, `M1`, ...
	pub macros: Vec<(usize, Expression)>,
}

#[derive(Debug)]
pub struct Synthesis {
	/// The first expression found that reproduces the search samples.
	pub expression: Option<Expression>,
	/// Whether `expression` also reproduces the verification samples.
	pub verified: bool,
	/// Definitions of the compressed leaves that `expression` refers to, directly or through
	/// other definitions, with their indices in `M0`, `M1`, ...
	pub macros: Vec<(usize, Expression)>,
	pub exhausted: bool,
	pub playouts: u64,
	pub elapsed: Duration,
}

/// `term` with leaves named after the inputs and constants in `base`, then `M{term.first}`
/// up to the library entry before `end`.
fn expression(base: &[String], term: &Term, end: usize) -> Expression {
	Expression {
		tokens: term.tokens.clone(),
		leaves: base
			.iter()
			.cloned()
			.chain((term.first..end).map(|entry| format!("M{entry}")))
			.collect(),
	}
}

/// Indices of the `library` entries that `term` refers to, directly or through other entries.
fn referenced(term: &Term, base_leaves: usize, library: &[Term]) -> Vec<usize> {
	let mark = |term: &Term, referenced: &mut [bool]| {
		for &token in &term.tokens {
			if let Token::Leaf(leaf) = token
				&& let Some(offset) = usize::from(leaf).checked_sub(base_leaves)
			{
				referenced[term.first + offset] = true;
			}
		}
	};
	let mut referenced = vec![false; library.len()];
	mark(term, &mut referenced);
	for entry in (0..library.len()).rev() {
		if referenced[entry] {
			mark(&library[entry], &mut referenced);
		}
	}
	(0..library.len())
		.filter(|&entry| referenced[entry])
		.collect()
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
		let base_leaves = self.input_regs.len() + grammar.constants.len();
		let (problem, version) = match &mut checkpoint {
			Some(input) => {
				let header = checkpoint::read_header::<W>(input)?;
				let problem = header.problem;
				let base: Vec<Token> = problem
					.productions
					.iter()
					.copied()
					.filter(
						|token| !matches!(token, Token::Leaf(leaf) if usize::from(*leaf) >= base_leaves),
					)
					.collect();
				if base != productions || header.max_len != config.max_len {
					return Err(Error::Checkpoint(
						"it was made with different --ops, --consts or --max-len",
					));
				}
				let macros = problem.library.len().checked_sub(problem.first);
				if macros.is_none_or(|macros| problem.samples.columns.len() != base_leaves + macros)
				{
					return Err(Error::Checkpoint("its compressed leaves are inconsistent"));
				}
				self.check(&problem.samples)?;
				self.check(&problem.verification)?;
				(problem, header.version)
			}
			None => {
				let problem = Problem {
					productions,
					samples: self.sample::<W>(&grammar.constants, false)?,
					verification: self.sample::<W>(&grammar.constants, true)?,
					library: Vec::new(),
					first: 0,
				};
				(problem, 0)
			}
		};
		let base: Vec<String> = self
			.fcn_config
			.input_regs
			.into_iter()
			.chain(grammar.constants.iter().map(u64::to_string))
			.collect();
		drop(self.instance);

		let shown = Mutex::new(Vec::new());
		let report = |p: &mcts::Progress| {
			let mut shown = shown.lock().unwrap();
			shown.resize(p.library.len(), false);
			let macros = referenced(p.best, base_leaves, p.library)
				.into_iter()
				.filter(|&entry| !std::mem::replace(&mut shown[entry], true))
				.map(|entry| (entry, expression(&base, &p.library[entry], entry)))
				.collect();
			progress(&Status {
				elapsed: p.elapsed,
				playouts: p.playouts,
				nodes: p.nodes,
				best_score: p.best_score,
				exploration: p.exploration,
				best: (!p.best.tokens.is_empty())
					.then(|| expression(&base, p.best, p.library.len())),
				macros,
			})
		};
		let resume = checkpoint
			.as_mut()
			.map(|input| (input as &mut dyn Read, version));
		let outcome = mcts::search(problem, config, resume, &report)?;
		let library = &outcome.library;
		let macros = outcome
			.solution
			.as_ref()
			.map_or_else(Vec::new, |solution| {
				referenced(solution, base_leaves, library)
			})
			.into_iter()
			.map(|entry| (entry, expression(&base, &library[entry], entry)))
			.collect();
		Ok(Synthesis {
			expression: outcome
				.solution
				.as_ref()
				.map(|solution| expression(&base, solution, library.len())),
			macros,
			verified: outcome.verified,
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
	use crate::expr::BinaryOp;

	#[test]
	fn test_macro_references() {
		let (add, leaf) = (Token::Binary(BinaryOp::Add), Token::Leaf);
		let term = |tokens: &[Token], first| Term {
			tokens: tokens.to_vec(),
			first,
		};
		let library = [
			term(&[add, leaf(0), leaf(1)], 0),
			term(&[add, leaf(2), leaf(0)], 0),
			term(&[add, leaf(2), leaf(2)], 1),
			term(&[add, leaf(1), leaf(1)], 1),
		];
		let best = term(&[add, leaf(3), leaf(0)], 1);
		assert_eq!(referenced(&best, 2, &library), [0, 1, 2]);
		assert_eq!(referenced(&term(&[leaf(4)], 1), 2, &library), [3]);
		let base = ["a0".to_string(), "a1".to_string()];
		assert_eq!(expression(&base, &best, 4).to_string(), "(M2 + a0)");
		assert_eq!(expression(&base, &library[1], 1).to_string(), "(M0 + a0)");
	}

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
