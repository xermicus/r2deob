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
	eval::{Evaluator, Lanes, Samples, Word},
	expr::{BinaryOp, Token},
};

#[derive(Debug, Clone)]
pub struct SearchConfig {
	pub threads: usize,
	pub trees: usize,
	pub max_len: usize,
	pub exploration: f64,
	/// Time without a new best after which exploration doubles, cycling back to the base value
	/// after `MAX_EXPLORATION_SCALE`; every new best halves it.
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
	/// Time without a new best after which the best expressions of the trees become leaves,
	/// the oldest compressed ones beyond `MACRO_LEAVES` drop out, and the search restarts
	/// over the new leaf set.
	pub compress_after: Option<Duration>,
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
			compress_after: Some(Duration::from_secs(30)),
		}
	}
}

/// Tokens whose leaves after the inputs and constants are the library entries `first`,
/// `first + 1`, ...
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Term {
	pub tokens: Vec<Token>,
	pub first: usize,
}

/// Compressed expressions, oldest first; each refers only to entries before it.
pub type Library = Vec<Term>;

/// What a search runs on: productions ordered by arity, whose leaves are the columns of both
/// sample batches, the inputs and constants followed by the library entries from `first` on.
#[derive(Debug, Clone)]
pub struct Problem<W> {
	pub productions: Vec<Token>,
	pub samples: Samples<W>,
	pub verification: Samples<W>,
	pub library: Library,
	/// The library entry of the first leaf after the inputs and constants.
	pub first: usize,
}

#[derive(Debug)]
pub struct Outcome {
	pub solution: Option<Term>,
	pub exhausted: bool,
	pub playouts: u64,
	pub elapsed: Duration,
	pub library: Library,
}

pub struct Progress<'a> {
	pub elapsed: Duration,
	pub playouts: u64,
	pub nodes: usize,
	/// Best playout score so far, from 0 to 1.
	pub best_score: f64,
	pub exploration: f64,
	pub best: &'a Term,
	pub library: &'a [Term],
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

/// Expressions compressed per stall, and the size of each tree's elite.
const COMPRESSED_PER_ROUND: usize = 4;

/// The newest library entries that are leaves; older ones live on in the entries built from
/// them.
const MACRO_LEAVES: usize = 32;

/// Leaf indices are `u8`, with `u8::MAX` reserved by the evaluator.
const MAX_LEAVES: usize = u8::MAX as usize;

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

/// An expression with its values on the search samples.
struct Candidate<W> {
	score: u32,
	column: Lanes<W>,
	tokens: Vec<Token>,
}

#[repr(align(64))]
struct Tree<'a, W> {
	root: Node,
	shape: &'a Shape<'a>,
	/// The best expressions of this tree whose values differ from each other and from all
	/// leaves, best first; candidates for compression.
	elite: Mutex<Vec<Candidate<W>>>,
	elite_min: AtomicU32,
}

impl<'a, W: Word> Tree<'a, W> {
	fn new(root: Node, shape: &'a Shape<'a>) -> Tree<'a, W> {
		Tree {
			root,
			shape,
			elite: Mutex::new(Vec::new()),
			elite_min: AtomicU32::new(0),
		}
	}

	#[inline]
	fn offer(&self, score: u32, column: &Lanes<W>, tokens: &[Token], leaves: &[Lanes<W>]) {
		if score <= self.elite_min.load(Ordering::Relaxed) || leaves.contains(column) {
			return;
		}
		let mut elite = self.elite.lock().unwrap();
		if elite.iter().any(|candidate| candidate.column == *column) {
			return;
		}
		let position = elite.partition_point(|candidate| candidate.score >= score);
		elite.insert(
			position,
			Candidate {
				score,
				column: *column,
				tokens: tokens.to_vec(),
			},
		);
		elite.truncate(COMPRESSED_PER_ROUND);
		if elite.len() == COMPRESSED_PER_ROUND {
			self.elite_min
				.store(elite[COMPRESSED_PER_ROUND - 1].score, Ordering::Relaxed);
		}
	}
}

impl<W> Drop for Tree<'_, W> {
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

/// State that outlives a round.
struct Global {
	start: Instant,
	deadline: Option<Instant>,
	best_score: AtomicU32,
	best: Mutex<Term>,
	playouts: AtomicU64,
}

impl<W: Word> Problem<W> {
	/// Adds the best distinct `candidates` to the library and as leaves, keeping the newest
	/// [`MACRO_LEAVES`] entries as leaves; returns how many were added.
	fn compress<const DIVISION: bool>(
		&mut self,
		mut candidates: Vec<Candidate<W>>,
		max_len: usize,
	) -> usize {
		let base = self.samples.columns.len() - (self.library.len() - self.first);
		let window = MACRO_LEAVES.min(MAX_LEAVES - base);
		candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.score));
		let mut evaluator = Evaluator::new(max_len);
		let mut added = 0;
		for candidate in candidates {
			if added == COMPRESSED_PER_ROUND.min(window) {
				break;
			}
			if self.samples.columns.contains(&candidate.column) {
				continue;
			}
			let verification = *evaluator.eval::<DIVISION>(&candidate.tokens, &self.verification);
			self.samples.push(candidate.column);
			self.verification.push(verification);
			self.library.push(Term {
				tokens: candidate.tokens,
				first: self.first,
			});
			added += 1;
		}
		let evicted = (self.samples.columns.len() - base).saturating_sub(window);
		self.samples.remove(base..base + evicted);
		self.verification.remove(base..base + evicted);
		self.first += evicted;
		let operations: Vec<Token> = self
			.productions
			.iter()
			.copied()
			.filter(|token| token.arity() > 0)
			.collect();
		self.productions = (0..self.samples.columns.len() as u8)
			.map(Token::Leaf)
			.chain(operations)
			.collect();
		added
	}
}

struct Shared<'a, W> {
	problem: &'a Problem<W>,
	config: &'a SearchConfig,
	global: &'a Global,
	stop: AtomicBool,
	stalled: AtomicBool,
	solution: OnceLock<Vec<Token>>,
	nodes: AtomicUsize,
	playouts: Vec<Counter>,
	exploration: AtomicU32,
	low_score: AtomicU32,
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
	tree: &Tree<W>,
	shared: &Shared<W>,
	thread: usize,
	seed: u64,
) -> u64 {
	let _stop_on_panic = StopOnPanic(&shared.stop);
	let shape = tree.shape;
	let config = shared.config;
	let (samples, verification) = (&shared.problem.samples, &shared.problem.verification);
	let global = shared.global;
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
				&& global
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
		let high = global.best_score.load(Ordering::Relaxed);
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
			let column = evaluator.eval::<DIVISION>(&tokens, samples);
			let score = samples.score(column);
			tree.offer(score, column, &tokens, &samples.columns);
			if score == max_score
				&& verification.score(evaluator.eval::<DIVISION>(&tokens, verification))
					== max_score
			{
				let _ = shared.solution.set(tokens.clone());
				shared.stop.store(true, Ordering::Relaxed);
			}
			if score > global.best_score.load(Ordering::Relaxed) {
				let mut best = global.best.lock().unwrap();
				if score > global.best_score.load(Ordering::Relaxed) {
					global.best_score.store(score, Ordering::Relaxed);
					best.tokens.clone_from(&tokens);
					best.first = shared.problem.first;
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

fn save<W: Word>(path: &Path, trees: &[Tree<W>], problem: &Problem<W>) -> io::Result<()> {
	let shape = trees[0].shape;
	let partial = path.with_extension("partial");
	let mut out = BufWriter::new(File::create(&partial)?);
	write_header(&mut out, shape.max_len, problem)?;
	write_u32(&mut out, trees.len() as u32)?;
	for tree in trees {
		tree.root.save(&mut out, shape)?;
	}
	out.into_inner()?.sync_all()?;
	fs::rename(partial, path)
}

struct RoundOutcome<W> {
	solution: Option<Vec<Token>>,
	/// Ended by a solution, the deadline or an interrupt.
	finished: bool,
	exhausted: bool,
	stalled: bool,
	candidates: Vec<Candidate<W>>,
}

/// Searches over a fixed leaf set until a solution, exhaustion, the deadline, an interrupt or a
/// stall of [`SearchConfig::compress_after`].
fn run_round<W: Word, const DIVISION: bool>(
	problem: &Problem<W>,
	round: u64,
	config: &SearchConfig,
	global: &Global,
	resume: Option<(&mut dyn Read, u8)>,
	progress: &(dyn Fn(&Progress) + Sync),
) -> io::Result<RoundOutcome<W>> {
	let shape = Shape::new(&problem.productions, config.max_len);
	let mut nodes = 0;
	let mut trees: Vec<Tree<W>> = match resume {
		Some((mut input, version)) => (0..read_u32(&mut input)?)
			.map(|_| {
				Ok(Tree::new(
					Node::load(&mut input, version, &shape, &mut nodes)?,
					&shape,
				))
			})
			.collect::<io::Result<_>>()?,
		None => (0..config.trees.clamp(1, config.threads.max(1)))
			.map(|_| Tree::new(Node::default(), &shape))
			.collect(),
	};
	if trees.is_empty() {
		return Err(io::Error::new(
			io::ErrorKind::InvalidData,
			"invalid checkpoint: no trees",
		));
	}
	let resumed_best = trees
		.iter()
		.map(|tree| tree.root.best_score())
		.max()
		.unwrap_or(0);
	global.best_score.fetch_max(resumed_best, Ordering::Relaxed);

	let threads = config.threads.max(1);
	let shared = Shared {
		problem,
		config,
		global,
		stop: AtomicBool::new(false),
		stalled: AtomicBool::new(false),
		solution: OnceLock::new(),
		nodes: AtomicUsize::new(nodes),
		playouts: (0..threads).map(|_| Counter::default()).collect(),
		exploration: AtomicU32::new((config.exploration as f32).to_bits()),
		low_score: AtomicU32::new(u32::MAX),
	};
	let report = || {
		let best = global.best.lock().unwrap();
		progress(&Progress {
			elapsed: global.start.elapsed(),
			playouts: global.playouts.load(Ordering::Relaxed)
				+ shared
					.playouts
					.iter()
					.map(|counter| counter.0.load(Ordering::Relaxed))
					.sum::<u64>(),
			nodes: shared.nodes.load(Ordering::Relaxed).min(config.max_nodes),
			best_score: f64::from(global.best_score.load(Ordering::Relaxed))
				/ f64::from(Samples::<W>::max_score()),
			exploration: f64::from(f32::from_bits(shared.exploration.load(Ordering::Relaxed))),
			best: &best,
			library: &problem.library,
		});
	};
	let round_start = Instant::now();
	let (playouts, saved) = thread::scope(|scope| {
		let monitor = scope.spawn(|| -> io::Result<()> {
			let _stop_on_panic = StopOnPanic(&shared.stop);
			let elapsed = global.start.elapsed();
			let next = |every: Duration| {
				let periods = (elapsed.as_secs_f64() / every.as_secs_f64()).floor() + 1.0;
				global.start + every.mul_f64(periods)
			};
			let (mut next_progress, mut next_save) =
				(next(config.progress_every), next(config.save_every));
			let mut best = global.best_score.load(Ordering::Relaxed);
			let (mut improved_at, mut stalled_since, mut scale) = (round_start, round_start, 1.0);
			while !shared.stop.load(Ordering::Relaxed) {
				thread::park_timeout(Duration::from_millis(50));
				if config
					.interrupt
					.is_some_and(|interrupt| interrupt.load(Ordering::Relaxed))
				{
					shared.stop.store(true, Ordering::Relaxed);
				}
				let now = Instant::now();
				let current = global.best_score.load(Ordering::Relaxed);
				if current > best {
					(best, improved_at, stalled_since) = (current, now, now);
					scale = f64::max(scale / 2.0, 1.0);
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
				if config
					.compress_after
					.is_some_and(|after| now >= improved_at + after)
				{
					shared.stalled.store(true, Ordering::Relaxed);
					shared.stop.store(true, Ordering::Relaxed);
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
					save(path, &trees, problem)
						.inspect_err(|_| shared.stop.store(true, Ordering::Relaxed))?;
					next_save += config.save_every;
				}
			}
			Ok(())
		});
		let handles: Vec<_> = (0..threads)
			.map(|t| {
				let (tree, shared) = (&trees[t % trees.len()], &shared);
				let seed = config.seed
					^ (t as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
					^ round.wrapping_mul(0xbf58_476d_1ce4_e5b9);
				scope.spawn(move || worker::<W, DIVISION>(tree, shared, t, seed))
			})
			.collect();
		let playouts: u64 = handles
			.into_iter()
			.map(|handle| handle.join().unwrap())
			.sum();
		shared.stop.store(true, Ordering::Relaxed);
		monitor.thread().unpark();
		(playouts, monitor.join().unwrap())
	});
	saved?;
	global.playouts.fetch_add(playouts, Ordering::Relaxed);
	let solution = shared.solution.into_inner();
	let finished = solution.is_some()
		|| config
			.interrupt
			.is_some_and(|interrupt| interrupt.load(Ordering::Relaxed))
		|| global
			.deadline
			.is_some_and(|deadline| Instant::now() >= deadline);
	let stalled = shared.stalled.load(Ordering::Relaxed);
	if let Some(path) = config.save.as_deref().filter(|_| finished || !stalled) {
		save(path, &trees, problem)?;
	}

	Ok(RoundOutcome {
		solution,
		finished,
		exhausted: trees.iter().any(|tree| tree.root.is_exhausted()),
		stalled,
		candidates: trees
			.iter_mut()
			.flat_map(|tree| std::mem::take(tree.elite.get_mut().unwrap()))
			.collect(),
	})
}

/// Searches for an expression reproducing both sample batches of `problem`; `resume` continues
/// from the trees of a checkpoint whose header (and format version) has already been read.
pub fn search<W: Word>(
	problem: Problem<W>,
	config: &SearchConfig,
	resume: Option<(&mut dyn Read, u8)>,
	progress: &(dyn Fn(&Progress) + Sync),
) -> io::Result<Outcome> {
	match problem
		.productions
		.iter()
		.any(|token| matches!(token, Token::Binary(BinaryOp::DivU | BinaryOp::RemU)))
	{
		true => search_rounds::<W, true>(problem, config, resume, progress),
		false => search_rounds::<W, false>(problem, config, resume, progress),
	}
}

fn search_rounds<W: Word, const DIVISION: bool>(
	mut problem: Problem<W>,
	config: &SearchConfig,
	mut resume: Option<(&mut dyn Read, u8)>,
	progress: &(dyn Fn(&Progress) + Sync),
) -> io::Result<Outcome> {
	let start = Instant::now();
	let global = Global {
		start,
		deadline: config.timeout.map(|timeout| start + timeout),
		best_score: AtomicU32::new(0),
		best: Mutex::new(Term::default()),
		playouts: AtomicU64::new(0),
	};
	let mut round = 0;
	loop {
		let outcome =
			run_round::<W, DIVISION>(&problem, round, config, &global, resume.take(), progress)?;
		let added = match config.compress_after {
			Some(_) if !outcome.finished => {
				problem.compress::<DIVISION>(outcome.candidates, config.max_len)
			}
			_ => 0,
		};
		if added == 0 && (outcome.finished || !outcome.stalled) {
			return Ok(Outcome {
				solution: outcome.solution.map(|tokens| Term {
					tokens,
					first: problem.first,
				}),
				exhausted: outcome.exhausted,
				playouts: global.playouts.load(Ordering::Relaxed),
				elapsed: start.elapsed(),
				library: problem.library,
			});
		}
		round += 1;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{checkpoint::read_header, eval::SAMPLES, expr::Grammar};

	fn lanes(seed: u64) -> Lanes<u64> {
		std::array::from_fn(|i| {
			let x = (seed * SAMPLES as u64 + i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
			(x ^ x >> 31).wrapping_mul(0xbf58_476d_1ce4_e5b9) ^ x >> 29
		})
	}

	fn problem(ops: &[&str]) -> Problem<u64> {
		Problem {
			productions: Grammar::new(ops, Vec::new()).unwrap().productions(2),
			samples: Samples::new(vec![lanes(1), lanes(2)], &[], lanes(3)),
			verification: Samples::new(vec![lanes(4), lanes(5)], &[], lanes(6)),
			library: Vec::new(),
			first: 0,
		}
	}

	/// Evaluates every library entry from the inputs alone and checks the leaves against it.
	fn check_library(problem: &Problem<u64>, inputs: &Samples<u64>, leaves: &Samples<u64>) {
		let mut evaluator = Evaluator::new(64);
		let mut columns = Vec::new();
		for term in &problem.library {
			let visible = inputs.columns.iter().chain(&columns[term.first..]).copied();
			let samples = Samples::from_columns(visible.collect(), inputs.output);
			columns.push(*evaluator.eval::<true>(&term.tokens, &samples));
		}
		assert_eq!(leaves.columns[..2], inputs.columns);
		assert_eq!(leaves.columns[2..], columns[problem.first..]);
		assert_eq!(
			problem
				.productions
				.iter()
				.filter(|token| token.arity() == 0)
				.count(),
			leaves.columns.len()
		);
	}

	#[test]
	fn test_compress_keeps_newest_leaves() {
		let mut problem = problem(&["add"]);
		let inputs = (problem.samples.clone(), problem.verification.clone());
		let mut evaluator = Evaluator::new(3);
		for _ in 0..MACRO_LEAVES + 5 {
			let newest = problem.samples.columns.len() as u8 - 1;
			let tokens = vec![
				Token::Binary(BinaryOp::Add),
				Token::Leaf(newest),
				Token::Leaf(0),
			];
			let column = *evaluator.eval::<false>(&tokens, &problem.samples);
			let candidate = Candidate {
				score: 0,
				column,
				tokens,
			};
			assert_eq!(problem.compress::<false>(vec![candidate], 3), 1);
		}
		assert_eq!(problem.library.len(), MACRO_LEAVES + 5);
		assert_eq!(problem.first, 5);
		check_library(&problem, &inputs.0, &problem.samples);
		check_library(&problem, &inputs.1, &problem.verification);
	}

	#[test]
	fn test_checkpoint_roundtrip() {
		let problem = problem(&["add", "mul", "remu", "not"]);
		let dir = std::env::temp_dir().join(format!("r2deob-test-{}", std::process::id()));
		fs::create_dir_all(&dir).unwrap();
		let (first, second) = (dir.join("first"), dir.join("second"));
		let config = SearchConfig {
			threads: 2,
			trees: 2,
			expand_after: 0,
			timeout: Some(Duration::from_millis(400)),
			save: Some(first.clone()),
			compress_after: Some(Duration::from_millis(100)),
			..SearchConfig::default()
		};
		let outcome = search(problem.clone(), &config, None, &|_| {}).unwrap();
		assert!(outcome.solution.is_none() && outcome.playouts > 0);
		assert!(!outcome.library.is_empty());

		let mut input = io::BufReader::new(File::open(&first).unwrap());
		let header = read_header::<u64>(&mut input).unwrap();
		assert_eq!(header.problem.library, outcome.library);
		assert_eq!(header.problem.samples.output, problem.samples.output);
		check_library(&header.problem, &problem.samples, &header.problem.samples);
		check_library(
			&header.problem,
			&problem.verification,
			&header.problem.verification,
		);
		let resumed = SearchConfig {
			timeout: Some(Duration::ZERO),
			save: Some(second.clone()),
			..config
		};
		let outcome = search(
			header.problem,
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
