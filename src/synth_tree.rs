use std::{
	cmp::Ordering,
	collections::{BinaryHeap, HashMap},
	sync::mpsc::{Receiver, Sender, channel},
	thread::{self, JoinHandle},
};

use crate::{BaseT, ast::Expression, calc::Operator, score::Score};

#[derive(Debug, PartialEq)]
struct QueueScore(f32, usize);

#[derive(Debug)]
struct WorkerResult {
	score: Score,
	node: usize,
}

#[derive(Debug)]
struct WorkerTask {
	expression: Expression,
	node: usize,
}

#[derive(Debug)]
struct Worker {
	tx: Sender<WorkerTask>,
	rx: Receiver<WorkerResult>,
	handle: JoinHandle<()>,
}

#[derive(Debug)]
struct Node {
	expression: Expression,
	score: f32,
	next: Vec<usize>,
}

#[derive(Debug)]
pub struct Synthesis {
	n_runs: usize,
	n_threads: usize,
	tree: Vec<Node>,
	queue: BinaryHeap<QueueScore>,
	terms: Vec<Expression>,
}

impl WorkerTask {
	fn work(
		inputs: &HashMap<String, Vec<BaseT>>,
		outputs: &[BaseT],
		expression: &Expression,
	) -> Score {
		match expression.eval(inputs) {
			Some(results) => Score::get(&results, outputs),
			None => Score::UnSat,
		}
	}
}

impl Synthesis {
	pub fn new(registers: &[String]) -> Synthesis {
		Synthesis {
			n_runs: 8192,
			n_threads: 8,
			tree: vec![Node {
				expression: Expression::NonTerminal,
				score: 0.0,
				next: Vec::new(),
			}],
			queue: BinaryHeap::from(vec![QueueScore(0.0, 0)]),
			terms: Expression::combinations(
				registers,
				&[Operator::Add, Operator::Sub, Operator::Mul, Operator::Div],
			),
		}
	}

	/// Searches for an expression matching the observed input/output behaviour.
	pub fn synthesize(
		&mut self,
		inputs: &HashMap<String, Vec<BaseT>>,
		outputs: &[BaseT],
	) -> Option<Expression> {
		let workers = Worker::spawn(self.n_threads, inputs, outputs);
		let mut winner = None;
		for _ in 0..self.n_runs {
			let mut pending = vec![0; workers.len()];
			for (worker, pending) in workers.iter().zip(&mut pending) {
				if let Some(QueueScore(_, node)) = self.queue.pop() {
					let derivates = self.tree[node].expression.derive(&self.terms);
					*pending = self.create_nodes(worker, derivates, node);
				}
			}
			winner = self.update(&workers, &pending);
			if winner.is_some() {
				break;
			}
			self.rebuild_queue();
		}
		for worker in workers {
			drop(worker.tx);
			worker.handle.join().expect("worker thread panicked");
		}
		winner
	}

	fn update(&mut self, workers: &[Worker], pending: &[usize]) -> Option<Expression> {
		let mut winner = None;
		for (worker, &n) in workers.iter().zip(pending) {
			for result in worker.rx.iter().take(n) {
				let score = match result.score {
					Score::Combined(x) => x,
					_ => 0.0,
				};
				self.tree[result.node].score = score;
				if score == 1.0 && winner.is_none() {
					winner = Some(self.tree[result.node].expression.clone());
				}
			}
		}
		winner
	}

	fn rebuild_queue(&mut self) {
		self.queue = self
			.tree
			.iter()
			.enumerate()
			.filter(|(_, node)| node.next.is_empty())
			.map(|(index, node)| QueueScore(node.score, index))
			.collect();
	}

	fn create_nodes(
		&mut self,
		worker: &Worker,
		derivates: Vec<Expression>,
		parent: usize,
	) -> usize {
		let count = derivates.len();
		for expression in derivates {
			let node = self.tree.len();
			worker
				.tx
				.send(WorkerTask {
					expression: expression.clone(),
					node,
				})
				.expect("worker thread hung up");
			self.tree.push(Node {
				expression,
				score: 0.0,
				next: Vec::new(),
			});
			self.tree[parent].next.push(node);
		}
		count
	}
}

impl Worker {
	fn spawn(
		n_workers: usize,
		inputs: &HashMap<String, Vec<BaseT>>,
		outputs: &[BaseT],
	) -> Vec<Worker> {
		(0..n_workers)
			.map(|_| {
				let (task_tx, task_rx) = channel::<WorkerTask>();
				let (result_tx, result_rx) = channel::<WorkerResult>();
				let inputs = inputs.clone();
				let outputs = outputs.to_vec();
				let handle = thread::spawn(move || {
					for task in task_rx {
						let score = WorkerTask::work(&inputs, &outputs, &task.expression);
						if result_tx
							.send(WorkerResult {
								score,
								node: task.node,
							})
							.is_err()
						{
							break;
						}
					}
				});
				Worker {
					tx: task_tx,
					rx: result_rx,
					handle,
				}
			})
			.collect()
	}
}

impl Eq for QueueScore {}

impl PartialOrd for QueueScore {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for QueueScore {
	fn cmp(&self, other: &Self) -> Ordering {
		other.0.total_cmp(&self.0)
	}
}

#[test]
fn worker_test_finite_perfect_expression() {
	let ast = Expression::Operation(
		Operator::Add,
		Box::new(Expression::Terminal("rax".to_string())),
		Box::new(Expression::Operation(
			Operator::Sub,
			Box::new(Expression::Terminal("rbx".to_string())),
			Box::new(Expression::Terminal("rcx".to_string())),
		)),
	);
	let mut inputs = HashMap::new();
	inputs.insert("rax".to_string(), vec![1, 2, 3, 4, 5, 6, 7, 8]);
	inputs.insert("rbx".to_string(), vec![1, 2, 3, 4, 5, 6, 7, 8]);
	inputs.insert("rcx".to_string(), vec![1, 2, 3, 4, 5, 6, 7, 8]);
	let score = WorkerTask::work(&inputs, &[1, 2, 3, 4, 5, 6, 7, 8], &ast);
	assert_eq!(score, Score::Combined(1.0));
}
