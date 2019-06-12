extern crate rayon;
use rayon::prelude::*;

use rsmt2::SmtRes;

use std::collections::HashMap;
use std::thread;
use std::thread::JoinHandle;
use std::sync::mpsc::channel;
use std::sync::mpsc::Sender;
use std::sync::mpsc::Receiver;

use super::ast::Expression;
use super::score::Score;
//use super::sat_interface::Op;
use super::sat_interface::Sat;
use super::calc::Operator;

#[derive(Debug,Default)]
struct WorkerResult {
	score: Score,
	node: usize,
	model: HashMap<String,u64>
}

#[derive(Debug)]
struct WorkerTask {
	expression: Expression,
	node: usize
}

#[derive(Debug)]
struct AtomicWorker {
	tx: Sender<WorkerTask>,
	rx: Receiver<WorkerResult>,
	handle: JoinHandle<()>,
}

#[derive(Debug)]
struct Node {
	expression: Expression,
	score: Score,
	index: usize,
	prev: usize,
	next: Vec<usize>,
	sat_model: Vec<(String,u64)>
}

#[derive(Debug)]
pub struct Synthesis {
	n_runs: usize,
	n_threads: usize,
	tree: Vec<Node>,
	queue: Vec<usize>,
	terms: Vec<Expression>,
	scoring: Score,
}

impl WorkerTask {
	pub fn work(sat: Option<&mut Sat>, inputs: &Vec<HashMap<String,u64>>, outputs: &Vec<u64>, exp: &Expression) -> WorkerResult {
		let mut result =  WorkerResult::default();

		if !exp.is_finite() {
			result.score = Score::UnSat;
			return result
		}

		let result_tests: Vec<u64> = Vec::new();
		result.score = Score::get(result_tests, outputs.to_vec());
		result
	}
}

impl Synthesis {
	pub fn default(registers: &Vec<String>) -> Synthesis {
		Synthesis {
			n_runs: 3,
			n_threads: 1,
			tree: vec![Node {
				expression: Expression::NonTerminal,
				score: Score::UnSat,
				index: 0,
				prev: 0,
				next: Vec::new(),
				sat_model: Vec::new()
			}],
			queue: vec![0],
			terms: Expression::combinations(registers),
			scoring: Score::Combined(0.0),
		}
	}

	pub fn synthesize(&mut self, inputs: &Vec<HashMap<String,u64>>, outputs: &Vec<u64>) {
		let workers = AtomicWorker::setup_workers(self.n_threads, inputs, outputs);
	}
}

impl AtomicWorker {	
	fn setup_workers(n_workers: usize, inputs: &Vec<HashMap<String,u64>>, outputs: &Vec<u64>) -> Vec<AtomicWorker> {
		let mut result: Vec<AtomicWorker> = Vec::new();
		for _ in 0..n_workers {
			let (task_tx, task_rx) = channel::<WorkerTask>();
			let (result_tx, result_rx) = channel::<WorkerResult>();
			let input = inputs.clone();
			let output = outputs.clone();
			let handle = thread::spawn(move|| {
				let mut sat = Sat::init();
				loop {
					if let Ok(task) = task_rx.recv() {
						let mut result = WorkerTask::work(Some(&mut sat), &input, &output, &task.expression);
						result.node = task.node;
						result_tx.send(result).unwrap();
					} else {
						break;
					}
				}
			});
			result.push(AtomicWorker {
				tx: task_tx,
				rx: result_rx,
				handle: handle,
			});
		}
		return result
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
			Box::new(Expression::Terminal("rcx".to_string()))
		))
	);
	let mut inputs = Vec::new();
	let mut input = HashMap::new();
	input.insert("rax".to_string(), 20);
	input.insert("rbx".to_string(), 2);
	input.insert("rcx".to_string(), 2);
	inputs.push(input);
	let mut input = HashMap::new();
	input.insert("rax".to_string(), 10);
	input.insert("rbx".to_string(), 2);
	input.insert("rcx".to_string(), 2);
	inputs.push(input);

	let result = WorkerTask::work(None, &inputs, &vec![20, 10], &ast);
	assert_eq!(result.score, Score::Combined(1.0))
}

/*
	pub fn derive_node(&mut self, node: usize) {
		let expression = &self.tree.get(node).unwrap().expression;
		let expressions: Vec<Expression> = Expression::derive(&mut expression.clone(), &self.terms);
		for e in expressions.iter() {
			let index = self.tree.len();
			self.tree.push(Node {
				expression: e.clone(),
				score: Score::UnSat,
				index: index,
				prev: node,
				next: Vec::new(),
				sat_model: Vec::new()
			});
			self.tree[node].next.push(index);
		}
	}
*/
