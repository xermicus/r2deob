use std::{
	ptr,
	sync::{
		OnceLock,
		atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, AtomicUsize, Ordering},
	},
	thread,
	time::{Duration, Instant},
};

use crate::{
	eval::{Evaluator, Samples, Word},
	expr::{BinaryOp, Token},
};

#[derive(Debug, Clone)]
pub struct SearchConfig {
	pub threads: usize,
	/// Threads are spread round-robin over this many independent trees.
	pub trees: usize,
	pub max_len: usize,
	pub exploration: f64,
	/// Descents through a node before it is expanded.
	pub expand_after: u32,
	/// Random completions evaluated per tree descent; `expand_after` counts descents.
	pub playouts: u32,
	/// Node budget across all trees; once spent, trees stop growing.
	pub max_nodes: usize,
	pub timeout: Duration,
	pub seed: u64,
}

impl Default for SearchConfig {
	fn default() -> Self {
		SearchConfig {
			threads: thread::available_parallelism().map_or(1, usize::from),
			trees: thread::available_parallelism().map_or(1, usize::from),
			max_len: 15,
			exploration: 1.0,
			expand_after: 512,
			playouts: 16,
			max_nodes: 1 << 25,
			timeout: Duration::from_secs(60),
			seed: 0,
		}
	}
}

#[derive(Debug)]
pub struct Outcome {
	pub tokens: Option<Vec<Token>>,
	/// Every expression up to `max_len` was evaluated without a match.
	pub exhausted: bool,
	pub playouts: u64,
	pub elapsed: Duration,
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

/// Relative probability of picking a leaf, unary or binary production during a playout,
/// chosen so that random expansions are subcritical and stay small.
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

	/// Number of productions allowed for the next hole; they are always a prefix of
	/// `productions` because productions are ordered by arity.
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
	exhausted: AtomicBool,
	score: AtomicU64,
	children: AtomicPtr<Node>,
}

/// Statistics use plain loads and stores instead of read-modify-write atomics: exact with
/// one thread per tree, and losing the occasional update when threads share a tree.
impl Node {
	#[inline]
	fn visit(&self, count: u32) -> u32 {
		let visits = self.visits.load(Ordering::Relaxed).saturating_add(count);
		self.visits.store(visits, Ordering::Relaxed);
		visits
	}

	/// Marks a subtree whose expressions have all been evaluated.
	#[inline]
	fn exhaust(&self) {
		self.exhausted.store(true, Ordering::Relaxed);
	}

	#[inline]
	fn is_exhausted(&self) -> bool {
		self.exhausted.load(Ordering::Relaxed)
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

	fn free(&mut self, shape: &Shape, len: usize, holes: usize) {
		let children = *self.children.get_mut();
		if children.is_null() {
			return;
		}
		let n = shape.choices(len, holes);
		let mut children = unsafe { Box::from_raw(ptr::slice_from_raw_parts_mut(children, n)) };
		for (child, token) in children.iter_mut().zip(shape.productions) {
			child.free(shape, len + 1, holes - 1 + token.arity());
		}
	}
}

/// Cache-line aligned so the roots of independent trees do not false-share.
#[repr(align(64))]
struct Tree<'a> {
	root: Node,
	shape: &'a Shape<'a>,
}

impl Drop for Tree<'_> {
	fn drop(&mut self) {
		self.root.free(self.shape, 0, 1);
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

/// `ln(x)` from the float exponent and a linear mantissa, within 0.06 of the exact value.
#[inline]
fn fast_ln(x: u32) -> f32 {
	let bits = (x as f32).to_bits() as f32;
	(bits * (1.0 / (1 << 23) as f32) - 127.0) * std::f32::consts::LN_2
}

#[inline]
fn select(
	children: &[Node],
	parent_visits: u32,
	exploration: f32,
	max_score: f32,
) -> Option<usize> {
	let explore = exploration * fast_ln(parent_visits).max(0.0).sqrt();
	let inv_max = 1.0 / max_score;
	let mut best = (f32::NEG_INFINITY, None);
	for (i, child) in children.iter().enumerate() {
		if child.is_exhausted() {
			continue;
		}
		let visits = child.visits.load(Ordering::Relaxed);
		if visits == 0 {
			return Some(i);
		}
		let visits = visits as f32;
		let score = child.score.load(Ordering::Relaxed) as f32;
		let uct = (score * inv_max + explore * visits.sqrt()) / visits;
		if uct > best.0 {
			best = (uct, Some(i));
		}
	}
	best.1
}

struct Shared<'a, W> {
	samples: &'a Samples<W>,
	verification: &'a Samples<W>,
	config: &'a SearchConfig,
	deadline: Instant,
	stop: AtomicBool,
	solution: OnceLock<Vec<Token>>,
	nodes: AtomicUsize,
}

fn worker<W: Word, const DIVISION: bool>(tree: &Tree, shared: &Shared<W>, seed: u64) -> u64 {
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
			|| descent % 256 == 0 && Instant::now() >= shared.deadline
		{
			break;
		}
		path.clear();
		tokens.clear();
		let mut holes = 1;
		let mut node = &tree.root;
		let mut visits = node.visit(per_descent);
		path.push(node);

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
				config.exploration as f32,
				max_score as f32,
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
		let mut total = 0;
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
			total += u64::from(score);
		}
		total *= u64::from(per_descent / rollouts);
		playouts += u64::from(rollouts);
		for node in &path {
			node.score.store(
				node.score.load(Ordering::Relaxed) + total,
				Ordering::Relaxed,
			);
		}
	}
	playouts
}

/// Searches for an expression over `productions` that reproduces `samples` and
/// `verification` exactly; only `samples` guides the search.
pub fn search<W: Word>(
	productions: &[Token],
	samples: &Samples<W>,
	verification: &Samples<W>,
	config: &SearchConfig,
) -> Outcome {
	let shape = Shape::new(productions, config.max_len);
	let division = productions
		.iter()
		.any(|token| matches!(token, Token::Binary(BinaryOp::DivU | BinaryOp::RemU)));
	let trees: Vec<Tree> = (0..config.trees.clamp(1, config.threads.max(1)))
		.map(|_| Tree {
			root: Node::default(),
			shape: &shape,
		})
		.collect();

	let start = Instant::now();
	let shared = Shared {
		samples,
		verification,
		config,
		deadline: start + config.timeout,
		stop: AtomicBool::new(false),
		solution: OnceLock::new(),
		nodes: AtomicUsize::new(0),
	};
	let playouts = thread::scope(|scope| {
		let handles: Vec<_> = (0..config.threads.max(1))
			.map(|t| {
				let (tree, shared) = (&trees[t % trees.len()], &shared);
				let seed = config.seed ^ (t as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
				scope.spawn(move || match division {
					true => worker::<W, true>(tree, shared, seed),
					false => worker::<W, false>(tree, shared, seed),
				})
			})
			.collect();
		handles
			.into_iter()
			.map(|handle| handle.join().unwrap())
			.sum()
	});

	let exhausted = trees.iter().any(|tree| tree.root.is_exhausted());
	Outcome {
		tokens: shared.solution.into_inner(),
		exhausted,
		playouts,
		elapsed: start.elapsed(),
	}
}
