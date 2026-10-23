use std::{path::PathBuf, time::Duration};

use clap::{Parser, ValueEnum};
use r2deob::{
	engine::{FcnConfig, Session, Width},
	expr::Grammar,
	mcts::SearchConfig,
};

#[derive(Clone, Copy, ValueEnum)]
enum Bits {
	#[value(name = "32")]
	W32,
	#[value(name = "64")]
	W64,
}

/// Synthesizes an expression equivalent to an exported PolkaVM function.
#[derive(Parser)]
struct Args {
	/// Function to deobfuscate
	export: String,
	/// Program blob containing the export
	#[arg(long, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/calc.polkavm"))]
	program: PathBuf,
	/// Input registers
	#[arg(long, value_delimiter = ',', default_value = "a0,a1")]
	inputs: Vec<String>,
	/// Output register
	#[arg(long, default_value = "a0")]
	output: String,
	#[arg(long, value_enum, default_value = "64")]
	width: Bits,
	/// Available operations: add sub mul mulhu divu remu and or xor shl shrl shra neg not
	#[arg(long, value_delimiter = ',', default_value = "add,sub,mul,xor")]
	ops: Vec<String>,
	/// Constant leaves
	#[arg(long, value_delimiter = ',')]
	consts: Vec<u64>,
	/// Worker threads [default: all hardware threads]
	#[arg(long)]
	threads: Option<usize>,
	/// Independent search trees the threads are spread over [default: one per thread]
	#[arg(long)]
	trees: Option<usize>,
	/// Visits before a node is expanded
	#[arg(long, default_value_t = SearchConfig::default().expand_after)]
	expand_after: u32,
	/// Random completions evaluated per tree descent
	#[arg(long, default_value_t = SearchConfig::default().playouts)]
	playouts: u32,
	/// Node budget across all trees
	#[arg(long, default_value_t = SearchConfig::default().max_nodes)]
	max_nodes: usize,
	/// Maximum expression length in tokens
	#[arg(long, default_value_t = SearchConfig::default().max_len)]
	max_len: usize,
	#[arg(long, default_value_t = SearchConfig::default().exploration)]
	exploration: f64,
	/// Timeout per run in seconds
	#[arg(long, default_value_t = 60.0)]
	timeout: f64,
	#[arg(long, default_value_t = 0)]
	seed: u64,
	/// Repeat the search on fresh samples and report the median time
	#[arg(long, default_value_t = 1)]
	runs: u64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
	let args = Args::parse();
	let grammar = Grammar::new(&args.ops, args.consts)?;
	let defaults = SearchConfig::default();
	let threads = args.threads.unwrap_or(defaults.threads);
	let target = FcnConfig {
		path: args.program.to_string_lossy().into_owned(),
		export: args.export.clone(),
		input_regs: args.inputs,
		output_reg: args.output,
		width: match args.width {
			Bits::W32 => Width::W32,
			Bits::W64 => Width::W64,
		},
	};

	let mut times = Vec::new();
	let (mut solved, mut playouts) = (0, 0);
	for run in 0..args.runs {
		let config = SearchConfig {
			threads,
			trees: args.trees.unwrap_or(threads),
			max_len: args.max_len,
			exploration: args.exploration,
			expand_after: args.expand_after,
			playouts: args.playouts,
			max_nodes: args.max_nodes,
			timeout: Duration::from_secs_f64(args.timeout),
			seed: args.seed.wrapping_add(run),
		};
		let synthesis = Session::init(target.clone())?.deobfuscate(&grammar, &config)?;
		let rate = synthesis.playouts as f64 / synthesis.elapsed.as_secs_f64() / 1e6;
		print!(
			"{:>9.3?} {:>12} playouts {rate:>6.1} M/s  ",
			synthesis.elapsed, synthesis.playouts
		);
		match synthesis.expression {
			Some(expression) => {
				solved += 1;
				println!("Winner! {expression}");
			}
			None if synthesis.exhausted => println!("Search space exhausted"),
			None => println!("No candidate found"),
		}
		times.push(synthesis.elapsed);
		playouts += synthesis.playouts;
	}
	if args.runs > 1 {
		times.sort();
		let total: Duration = times.iter().sum();
		println!(
			"solved {solved}/{}, median {:.3?}, {:.1} M playouts/s overall",
			args.runs,
			times[times.len() / 2],
			playouts as f64 / total.as_secs_f64() / 1e6
		);
	}
	Ok(())
}
