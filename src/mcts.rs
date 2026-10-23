use std::{
	fs::{self, File},
	io::{self, BufWriter, Read, Write},
	path::{Path, PathBuf},
	ptr,
	sync::{
		Mutex, OnceLock,
		atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, AtomicUsize, Ordering},
	},
	thread,
	time::{Duration, Instant},
};

use crate::{
	checkpoint::{read_u8, read_u32, read_u64, write_header, write_u32, write_u64},
	eval::{Evaluator, Samples, Word},
	expr::{BinaryOp, Token},
};

#[derive(Debug, Clone)]
pub struct SearchConfig {
	pub threads: usize,
	pub trees: usize,
	pub max_len: usize,
	pub exploration: f64,
	/// Time without a new best after which exploration doubles, cycling back to the base value
	/// after [`MAX_EXPLORATION_SCALE`]; every new best halves it.
	pub stall: Duration,
	/// Weight of a node's best playout score against its mean in UCT, from 0 to 1.
	pub max_weight: f32,
	pub expand_after: u32,
	/// Random completions per descent.
	pub playouts: u32,
	pub max_nodes: usize,
	pub timeout: Option<Duration>,
	pub seed: u64,
	pub progress_every: Duration,
	pub save: Option<PathBuf>,
	pub save_every: Duration,
	/// Stops the search like a timeout when set, e.g. from a Ctrl-C handler.
	pub interrupt: Option<&'static AtomicBool>,
}

impl Default for SearchConfig {
	fn default() -> Self {
		SearchConfig {
			threads: thread::available_parallelism().map_or(1, usize::from),
			trees: thread::available_parallelism().map_or(1, usize::from),
			max_len: 15,
			exploration: 1.0,
			stall: Duration::from_secs(1),
			max_weight: 1.0,
			expand_after: 512,
			playouts: 16,
			max_nodes: 1 << 25,
			timeout: None,
			seed: rand::random(),
			progress_every: Duration::from_secs(10),
			save: None,
			save_every: Duration::from_secs(600),
			interrupt: None,
		}
	}
}

#[derive(Debug)]
pub struct Outcome {
	pub tokens: Option<Vec<Token>>,
	pub exhausted: bool,
	pub playouts: u64,
	pub elapsed: Duration,
}

pub struct Progress<'a> {
	pub elapsed: Duration,
	pub playouts: u64,
	pub nodes: usize,
	/// Best playout score so far, from 0 to 1.
	pub best_score: f64,
	pub exploration: f64,
	pub best: &'a [Token],
}

/// Leftmost derivations in prefix order: the derived tokens are always a prefix of the
/// final expression, followed by `holes` unexpanded nonterminals.
struct Shape<'a> {
	productions: &'a [Token],
	leaves: usize,
	unary: usize,
	max_len: usize,
	playout: [Vec<Token>; 3],
}

const MAX_EXPLORATION_SCALE: f64 = 32.0;

/// Leaf, unary and binary weights; keeps random expansions subcritical.
const PLAYOUT_WEIGHTS: [usize; 3] = [3, 1, 2];

impl<'a> Shape<'a> {
	fn new(productions: &'a [Token], max_len: usize) -> Shape<'a> {
		assert!(
			productions.is_sorted_by_key(|token| token.arity()),
			"productions must be ordered by arity"
		);
		let classes: [Vec<Token>; 3] = std::array::from_fn(|arity| {
			productions
				.iter()
				.copied()
				.filter(|token| token.arity() == arity)
				.collect()
		});
		assert!(!classes[0].is_empty(), "grammar has no leaves");
		let playout = std::array::from_fn(|room| {
			let classes = &classes[..=room];
			let product: usize = classes
				.iter()
				.filter(|class| !class.is_empty())
				.map(Vec::len)
				.product();
			classes
				.iter()
				.zip(PLAYOUT_WEIGHTS)
				.flat_map(|(class, weight)| {
					let copies = if class.is_empty() {
						0
					} else {
						weight * product / class.len()
					};
					class
						.iter()
						.flat_map(move |&token| std::iter::repeat_n(token, copies))
				})
				.collect()
		});
		Shape {
			productions,
			leaves: classes[0].len(),
			unary: classes[1].len(),
			max_len,
			playout,
		}
	}

	#[inline]
	fn random(&self, rng: &mut WyRand, len: usize, holes: usize) -> Token {
		let table = &self.playout[(self.max_len - len - holes).min(2)];
		table[rng.below(table.len())]
	}

	/// Allowed productions are a prefix of `productions`, which is ordered by arity.
	#[inline]
	fn choices(&self, len: usize, holes: usize) -> usize {
		if holes == 0 {
			return 0;
		}
		match self.max_len - len - holes {
			0 => self.leaves,
			1 => self.leaves + self.unary,
			_ => self.productions.len(),
		}
	}
}

#[derive(Default)]
struct Node {
	visits: AtomicU32,
	/// Best playout score below this node, or'ed with [`EXHAUSTED`].
	best: AtomicU32,
	score: AtomicU64,
	children: AtomicPtr<Node>,
}

const EXHAUSTED: u32 = 1 << 31;

/// Statistics use plain loads and stores; updates may be lost when threads share a tree.
impl Node {
	#[inline]
	fn visit(&self, count: u32) -> u32 {
		let visits = self.visits.load(Ordering::Relaxed).saturating_add(count);
		self.visits.store(visits, Ordering::Relaxed);
		visits
	}

	#[inline]
	fn exhaust(&self) {
		self.best.fetch_or(EXHAUSTED, Ordering::Relaxed);
	}

	#[inline]
	fn is_exhausted(&self) -> bool {
		self.best.load(Ordering::Relaxed) & EXHAUSTED != 0
	}

	#[inline]
	fn best_score(&self) -> u32 {
		self.best.load(Ordering::Relaxed) & !EXHAUSTED
	}

	#[inline]
	fn record(&self, score: u32) {
		if score > self.best.load(Ordering::Relaxed) {
			self.best.fetch_max(score, Ordering::Relaxed);
		}
	}

	#[inline]
	fn children(&self, n: usize) -> Option<&[Node]> {
		let children = self.children.load(Ordering::Acquire);
		(!children.is_null()).then(|| unsafe { std::slice::from_raw_parts(children, n) })
	}

	fn expand(&self, n: usize) -> &[Node] {
		let fresh =
			Box::into_raw((0..n).map(|_| Node::default()).collect::<Box<[Node]>>()).cast::<Node>();
		let children = match self.children.compare_exchange(
			ptr::null_mut(),
			fresh,
			Ordering::AcqRel,
			Ordering::Acquire,
		) {
			Ok(_) => fresh,
			Err(existing) => {
				drop(unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(fresh, n)) });
				existing
			}
		};
		unsafe { std::slice::from_raw_parts(children, n) }
	}

	/// Writes the subtree below a root in preorder: visits, score sum, best score, flags
	/// (exhausted, has children), then the children.
	fn save(&self, out: &mut impl Write, shape: &Shape) -> io::Result<()> {
		let mut pending = vec![(self, 0, 1)];
		while let Some((node, len, holes)) = pending.pop() {
			let children = node.children(shape.choices(len, holes));
			write_u32(out, node.visits.load(Ordering::Relaxed))?;
			write_u64(out, node.score.load(Ordering::Relaxed))?;
			write_u32(out, node.best_score())?;
			out.write_all(&[u8::from(node.is_exhausted()) | u8::from(children.is_some()) << 1])?;
			let children = children.unwrap_or_default();
			for (child, token) in children
				.iter()
				.zip(&shape.productions[..children.len()])
				.rev()
			{
				pending.push((child, len + 1, holes - 1 + token.arity()));
			}
		}
		Ok(())
	}

	/// Reads a subtree written by [`Node::save`]; version 1 checkpoints have no best score.
	fn load(
		input: &mut impl Read,
		version: u8,
		shape: &Shape,
		nodes: &mut usize,
	) -> io::Result<Node> {
		let mut root = Node::default();
		let mut pending: Vec<(*mut Node, usize, usize)> = vec![(&mut root, 0, 1)];
		let result = (|| {
			while let Some((node, len, holes)) = pending.pop() {
				let node = unsafe { &mut *node };
				*node.visits.get_mut() = read_u32(input)?;
				*node.score.get_mut() = read_u64(input)?;
				*node.best.get_mut() = if version >= 2 { read_u32(input)? } else { 0 };
				let flags = read_u8(input)?;
				if flags & 1 != 0 {
					*node.best.get_mut() |= EXHAUSTED;
				}
				if flags & 2 == 0 {
					continue;
				}
				let n = shape.choices(len, holes);
				if n == 0 {
					return Err(io::Error::new(
						io::ErrorKind::InvalidData,
						"invalid checkpoint: children below a complete expression",
					));
				}
				let children =
					Box::into_raw((0..n).map(|_| Node::default()).collect::<Box<[Node]>>())
						.cast::<Node>();
				*node.children.get_mut() = children;
				*nodes += n;
				for (i, token) in shape.productions[..n].iter().enumerate().rev() {
					pending.push((
						unsafe { children.add(i) },
						len + 1,
						holes - 1 + token.arity(),
					));
				}
			}
			Ok(())
		})();
		match result {
			Ok(()) => Ok(root),
			Err(error) => {
				root.free(shape);
				Err(error)
			}
		}
	}

	fn free(&mut self, shape: &Shape) {
		let mut pending = vec![(*self.children.get_mut(), 0, 1)];
		while let Some((children, len, holes)) = pending.pop() {
			if children.is_null() {
				continue;
			}
			let n = shape.choices(len, holes);
			let mut children = unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(children, n)) };
			for (child, token) in children.iter_mut().zip(shape.productions) {
				pending.push((
					*child.children.get_mut(),
					len + 1,
					holes - 1 + token.arity(),
				));
			}
		}
	}
}

#[repr(align(64))]
struct Tree<'a> {
	root: Node,
	shape: &'a Shape<'a>,
}

impl Drop for Tree<'_> {
	fn drop(&mut self) {
		self.root.free(self.shape);
	}
}

struct WyRand(u64);

impl WyRand {
	#[inline]
	fn below(&mut self, n: usize) -> usize {
		self.0 = self.0.wrapping_add(0xa076_1d64_78bd_642f);
		let t = u128::from(self.0) * u128::from(self.0 ^ 0xe703_7ed1_a0b4_28db);
		let r = (t >> 64) as u64 ^ t as u64;
		((u128::from(r) * n as u128) >> 64) as usize
	}
}

#[inline]
fn fast_ln(x: u32) -> f32 {
	let bits = (x as f32).to_bits() as f32;
	(bits * (1.0 / (1 << 23) as f32) - 127.0) * std::f32::consts::LN_2
}

/// UCT on rewards normalized to the observed score range `[low, low + 1 / inv_span]`; a
/// child's reward mixes its mean playout score with its best one by `max_weight`.
#[inline]
fn select(
	children: &[Node],
	parent_visits: u32,
	exploration: f32,
	max_weight: f32,
	low: f32,
	inv_span: f32,
) -> Option<usize> {
	let explore = exploration * fast_ln(parent_visits).max(0.0).sqrt();
	let mut best = (f32::NEG_INFINITY, None);
	for (i, child) in children.iter().enumerate() {
		if child.is_exhausted() {
			continue;
		}
		let visits = child.visits.load(Ordering::Relaxed);
		if visits == 0 {
			return Some(i);
		}
		let scale = (visits as f32).sqrt().recip();
		let mean = child.score.load(Ordering::Relaxed) as f32 * scale * scale;
		let reward = (mean + max_weight * (child.best_score() as f32 - mean) - low) * inv_span;
		let uct = reward + explore * scale;
		if uct > best.0 {
			best = (uct, Some(i));
		}
	}
	best.1
}

#[repr(align(64))]
#[derive(Default)]
struct Counter(AtomicU64);

struct Shared<'a, W> {
	samples: &'a Samples<W>,
	verification: &'a Samples<W>,
	config: &'a SearchConfig,
	deadline: Option<Instant>,
	stop: AtomicBool,
	solution: OnceLock<Vec<Token>>,
	nodes: AtomicUsize,
	playouts: Vec<Counter>,
	best_score: AtomicU32,
	exploration: AtomicU32,
	low_score: AtomicU32,
	best: Mutex<Vec<Token>>,
}

/// Stops the search when the thread holding it panics, so the other threads finish.
struct StopOnPanic<'a>(&'a AtomicBool);

impl Drop for StopOnPanic<'_> {
	fn drop(&mut self) {
		if thread::panicking() {
			self.0.store(true, Ordering::Relaxed);
		}
	}
}

fn worker<W: Word, const DIVISION: bool>(
	tree: &Tree,
	shared: &Shared<W>,
	thread: usize,
	seed: u64,
) -> u64 {
	let _stop_on_panic = StopOnPanic(&shared.stop);
	let shape = tree.shape;
	let config = shared.config;
	let max_score = Samples::<W>::max_score();
	let mut rng = WyRand(seed);
	let mut evaluator = Evaluator::new(config.max_len);
	let mut path: Vec<&Node> = Vec::with_capacity(config.max_len + 1);
	let mut tokens = Vec::with_capacity(config.max_len);
	let per_descent = config.playouts.max(1);
	let expand_after = config.expand_after.saturating_mul(per_descent);
	let mut playouts = 0u64;

	for descent in 0u64.. {
		if shared.stop.load(Ordering::Relaxed)
			|| tree.root.is_exhausted()
			|| descent % 256 == 0
				&& shared
					.deadline
					.is_some_and(|deadline| Instant::now() >= deadline)
		{
			break;
		}
		path.clear();
		tokens.clear();
		let mut holes = 1;
		let mut node = &tree.root;
		let mut visits = node.visit(per_descent);
		path.push(node);
		let exploration = f32::from_bits(shared.exploration.load(Ordering::Relaxed));
		let high = shared.best_score.load(Ordering::Relaxed);
		let low = shared.low_score.load(Ordering::Relaxed).min(high);
		let inv_span = 1.0 / (high - low).max(1) as f32;

		loop {
			let n = shape.choices(tokens.len(), holes);
			if n == 0 {
				node.exhaust();
				break;
			}
			let children = match node.children(n) {
				Some(children) => children,
				None if visits > expand_after
					&& shared.nodes.fetch_add(n, Ordering::Relaxed) < config.max_nodes =>
				{
					node.expand(n)
				}
				None => break,
			};
			let Some(i) = select(
				children,
				visits,
				exploration,
				config.max_weight,
				low as f32,
				inv_span,
			) else {
				node.exhaust();
				break;
			};
			let token = shape.productions[i];
			tokens.push(token);
			holes = holes - 1 + token.arity();
			node = &children[i];
			visits = node.visit(per_descent);
			path.push(node);
		}

		let (prefix, prefix_holes) = (tokens.len(), holes);
		let rollouts = if prefix_holes == 0 { 1 } else { per_descent };
		let (mut total, mut descent_best) = (0, 0);
		for _ in 0..rollouts {
			tokens.truncate(prefix);
			holes = prefix_holes;
			while holes > 0 {
				let token = shape.random(&mut rng, tokens.len(), holes);
				tokens.push(token);
				holes = holes - 1 + token.arity();
			}
			let score = shared
				.samples
				.score(evaluator.eval::<DIVISION>(&tokens, shared.samples));
			if score == max_score
				&& shared
					.verification
					.score(evaluator.eval::<DIVISION>(&tokens, shared.verification))
					== max_score
			{
				let _ = shared.solution.set(tokens.clone());
				shared.stop.store(true, Ordering::Relaxed);
			}
			if score > shared.best_score.load(Ordering::Relaxed) {
				let mut best = shared.best.lock().unwrap();
				if score > shared.best_score.load(Ordering::Relaxed) {
					shared.best_score.store(score, Ordering::Relaxed);
					best.clone_from(&tokens);
				}
			}
			if score < shared.low_score.load(Ordering::Relaxed) {
				shared.low_score.fetch_min(score, Ordering::Relaxed);
			}
			descent_best = descent_best.max(score);
			total += u64::from(score);
		}
		total *= u64::from(per_descent / rollouts);
		playouts += u64::from(rollouts);
		shared.playouts[thread].0.store(playouts, Ordering::Relaxed);
		for node in &path {
			node.score.store(
				node.score.load(Ordering::Relaxed) + total,
				Ordering::Relaxed,
			);
			node.record(descent_best);
		}
	}
	playouts
}

fn save<W: Word>(path: &Path, trees: &[Tree], samples: [&Samples<W>; 2]) -> io::Result<()> {
	let shape = trees[0].shape;
	let partial = path.with_extension("partial");
	let mut out = BufWriter::new(File::create(&partial)?);
	write_header(&mut out, shape.productions, shape.max_len, samples)?;
	write_u32(&mut out, trees.len() as u32)?;
	for tree in trees {
		tree.root.save(&mut out, shape)?;
	}
	out.into_inner()?.sync_all()?;
	fs::rename(partial, path)
}

/// Searches for an expression reproducing `samples` and `verification`; `resume` continues
/// from the trees of a checkpoint whose header (and format version) has already been read.
pub fn search<W: Word>(
	productions: &[Token],
	samples: &Samples<W>,
	verification: &Samples<W>,
	config: &SearchConfig,
	resume: Option<(&mut dyn Read, u8)>,
	progress: &(dyn Fn(&Progress) + Sync),
) -> io::Result<Outcome> {
	let shape = Shape::new(productions, config.max_len);
	let division = productions
		.iter()
		.any(|token| matches!(token, Token::Binary(BinaryOp::DivU | BinaryOp::RemU)));
	let mut nodes = 0;
	let trees: Vec<Tree> = match resume {
		Some((mut input, version)) => (0..read_u32(&mut input)?)
			.map(|_| {
				Ok(Tree {
					root: Node::load(&mut input, version, &shape, &mut nodes)?,
					shape: &shape,
				})
			})
			.collect::<io::Result<_>>()?,
		None => (0..config.trees.clamp(1, config.threads.max(1)))
			.map(|_| Tree {
				root: Node::default(),
				shape: &shape,
			})
			.collect(),
	};
	if trees.is_empty() {
		return Err(io::Error::new(
			io::ErrorKind::InvalidData,
			"invalid checkpoint: no trees",
		));
	}

	let threads = config.threads.max(1);
	let start = Instant::now();
	let shared = Shared {
		samples,
		verification,
		config,
		deadline: config.timeout.map(|timeout| start + timeout),
		stop: AtomicBool::new(false),
		solution: OnceLock::new(),
		nodes: AtomicUsize::new(nodes),
		playouts: (0..threads).map(|_| Counter::default()).collect(),
		best_score: AtomicU32::new(
			trees
				.iter()
				.map(|tree| tree.root.best_score())
				.max()
				.unwrap_or(0),
		),
		low_score: AtomicU32::new(u32::MAX),
		exploration: AtomicU32::new((config.exploration as f32).to_bits()),
		best: Mutex::new(Vec::new()),
	};
	let report = || {
		let best = shared.best.lock().unwrap();
		progress(&Progress {
			elapsed: start.elapsed(),
			playouts: shared
				.playouts
				.iter()
				.map(|counter| counter.0.load(Ordering::Relaxed))
				.sum(),
			nodes: shared.nodes.load(Ordering::Relaxed).min(config.max_nodes),
			best_score: f64::from(shared.best_score.load(Ordering::Relaxed))
				/ f64::from(Samples::<W>::max_score()),
			exploration: f64::from(f32::from_bits(shared.exploration.load(Ordering::Relaxed))),
			best: &best,
		});
	};
	let (playouts, elapsed, saved) = thread::scope(|scope| {
		let monitor = scope.spawn(|| -> io::Result<()> {
			let _stop_on_panic = StopOnPanic(&shared.stop);
			let (mut next_progress, mut next_save) =
				(start + config.progress_every, start + config.save_every);
			let (mut best, mut stalled_since, mut scale) = (0, start, 1.0);
			while !shared.stop.load(Ordering::Relaxed) {
				thread::park_timeout(Duration::from_millis(50));
				if config
					.interrupt
					.is_some_and(|interrupt| interrupt.load(Ordering::Relaxed))
				{
					shared.stop.store(true, Ordering::Relaxed);
				}
				let now = Instant::now();
				let current = shared.best_score.load(Ordering::Relaxed);
				if current > best {
					(best, stalled_since, scale) = (current, now, f64::max(scale / 2.0, 1.0));
				} else if now >= stalled_since + config.stall {
					let doubled = scale * 2.0;
					(stalled_since, scale) = (
						now,
						if doubled > MAX_EXPLORATION_SCALE {
							1.0
						} else {
							doubled
						},
					);
				}
				let exploration = (config.exploration * scale) as f32;
				shared
					.exploration
					.store(exploration.to_bits(), Ordering::Relaxed);
				if now >= next_progress {
					report();
					next_progress += config.progress_every;
				}
				if let Some(path) = config.save.as_deref().filter(|_| now >= next_save) {
					save(path, &trees, [samples, verification])
						.inspect_err(|_| shared.stop.store(true, Ordering::Relaxed))?;
					next_save += config.save_every;
				}
			}
			Ok(())
		});
		let handles: Vec<_> = (0..threads)
			.map(|t| {
				let (tree, shared) = (&trees[t % trees.len()], &shared);
				let seed = config.seed ^ (t as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
				scope.spawn(move || match division {
					true => worker::<W, true>(tree, shared, t, seed),
					false => worker::<W, false>(tree, shared, t, seed),
				})
			})
			.collect();
		let playouts: u64 = handles
			.into_iter()
			.map(|handle| handle.join().unwrap())
			.sum();
		let elapsed = start.elapsed();
		shared.stop.store(true, Ordering::Relaxed);
		monitor.thread().unpark();
		(playouts, elapsed, monitor.join().unwrap())
	});
	saved?;
	if let Some(path) = &config.save {
		save(path, &trees, [samples, verification])?;
	}

	let exhausted = trees.iter().any(|tree| tree.root.is_exhausted());
	Ok(Outcome {
		tokens: shared.solution.into_inner(),
		exhausted,
		playouts,
		elapsed,
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{checkpoint::read_header, eval::SAMPLES, expr::Grammar};

	#[test]
	fn test_checkpoint_roundtrip() {
		let lanes = |seed: u64| {
			std::array::from_fn(|i| {
				let x = (seed * SAMPLES as u64 + i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
				(x ^ x >> 31).wrapping_mul(0xbf58_476d_1ce4_e5b9) ^ x >> 29
			})
		};
		let samples = Samples::<u64>::new(vec![lanes(1), lanes(2)], &[], lanes(3));
		let verification = Samples::<u64>::new(vec![lanes(4), lanes(5)], &[], lanes(6));
		let productions = Grammar::new(&["add", "mul", "remu", "not"], Vec::new())
			.unwrap()
			.productions(2);
		let dir = std::env::temp_dir().join(format!("r2deob-test-{}", std::process::id()));
		fs::create_dir_all(&dir).unwrap();
		let (first, second) = (dir.join("first"), dir.join("second"));
		let config = SearchConfig {
			threads: 2,
			trees: 2,
			expand_after: 0,
			timeout: Some(Duration::from_millis(200)),
			save: Some(first.clone()),
			..SearchConfig::default()
		};
		let outcome = search(
			&productions,
			&samples,
			&verification,
			&config,
			None,
			&|_| {},
		)
		.unwrap();
		assert!(outcome.tokens.is_none() && outcome.playouts > 0);

		let mut input = io::BufReader::new(File::open(&first).unwrap());
		let header = read_header::<u64>(&mut input).unwrap();
		assert_eq!(header.productions, productions);
		assert_eq!(header.samples.output, samples.output);
		assert_eq!(header.verification.columns[1], verification.columns[1]);
		let resumed = SearchConfig {
			timeout: Some(Duration::ZERO),
			save: Some(second.clone()),
			..config
		};
		let outcome = search(
			&header.productions,
			&header.samples,
			&header.verification,
			&resumed,
			Some((&mut input, header.version)),
			&|_| {},
		)
		.unwrap();
		assert_eq!(outcome.playouts, 0);
		assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
		assert!(fs::metadata(&first).unwrap().len() > 2_000);
		fs::remove_dir_all(dir).unwrap();
	}
}
