use std::{
	path::PathBuf,
	sync::atomic::{AtomicBool, Ordering},
	time::Duration,
};

use clap::{Parser, ValueEnum};
use r2deob::{
	engine::{FcnConfig, Session, Status, Width},
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
	/// Timeout per run in seconds [default: none]
	#[arg(long)]
	timeout: Option<f64>,
	/// Playout seed [default: random]
	#[arg(long)]
	seed: Option<u64>,
	/// Seconds between progress lines on stderr
	#[arg(long, default_value_t = SearchConfig::default().progress_every.as_secs_f64())]
	stats: f64,
	/// Write the search trees to this checkpoint periodically and at the end
	#[arg(long)]
	save: Option<PathBuf>,
	/// Seconds between checkpoints
	#[arg(long, default_value_t = SearchConfig::default().save_every.as_secs_f64())]
	save_every: f64,
	/// Continue from a checkpoint written by --save
	#[arg(long)]
	resume: Option<PathBuf>,
	/// Repeat the search on fresh samples and report the median time
	#[arg(long, default_value_t = 1)]
	runs: u64,
}

fn print_status(status: &Status) {
	eprint!(
		"{:>9.0?} {:>14} playouts {:>6.1} M/s {:>10} nodes  best {:.4}",
		status.elapsed,
		status.playouts,
		status.playouts as f64 / status.elapsed.as_secs_f64() / 1e6,
		status.nodes,
		status.best_score
	);
	match &status.best {
		Some(best) => eprintln!("  {best}"),
		None => eprintln!(),
	}
}

static INTERRUPT: AtomicBool = AtomicBool::new(false);

fn main() {
	let handler = ctrlc::set_handler(|| {
		if INTERRUPT.swap(true, Ordering::Relaxed) {
			std::process::exit(130);
		}
		eprintln!("interrupted, stopping (Ctrl-C again to exit immediately)");
	});
	if let Err(error) = handler.map_err(Into::into).and_then(|()| run()) {
		eprintln!("error: {error}");
		std::process::exit(1);
	}
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
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
			timeout: args.timeout.map(Duration::from_secs_f64),
			seed: args
				.seed
				.map_or_else(rand::random, |seed| seed.wrapping_add(run)),
			progress_every: Duration::from_secs_f64(args.stats),
			save: args.save.clone(),
			save_every: Duration::from_secs_f64(args.save_every),
			interrupt: Some(&INTERRUPT),
		};
		let synthesis = Session::init(target.clone())?.deobfuscate(
			&grammar,
			&config,
			args.resume.as_deref(),
			&print_status,
		)?;
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
			None if INTERRUPT.load(Ordering::Relaxed) => println!("Interrupted"),
			None if synthesis.exhausted => println!("Search space exhausted"),
			None => println!("No candidate found"),
		}
		times.push(synthesis.elapsed);
		playouts += synthesis.playouts;
		if INTERRUPT.load(Ordering::Relaxed) {
			break;
		}
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
