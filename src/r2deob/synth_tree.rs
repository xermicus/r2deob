use std::{
	collections::HashMap,
	thread,
	thread::JoinHandle,
	sync::mpsc::channel,
	sync::mpsc::Sender,
	sync::mpsc::Receiver,
};

use super::{
	ast::Expression,
	score::Score,
	sat_interface::Sat,
	BaseT,
};

#[derive(Debug,Default)]
struct WorkerResult {
	score: Score,
	node: usize,
	model: HashMap<String,BaseT>
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
	sat_model: Vec<(String,BaseT)>
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
	pub fn work(_sat: Option<&mut Sat>, inputs: &HashMap<String,Vec<BaseT>>, outputs: &Vec<BaseT>, exp: &Expression) -> WorkerResult {
		let mut result =  WorkerResult::default();
		if let Some(results) = exp.eval(inputs) {
			result.score = Score::get(&results, outputs);
		} else {
			result.score = Score::UnSat;
		}
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

	pub fn synthesize(&mut self, inputs: &HashMap<String,Vec<BaseT>>, outputs: &Vec<BaseT>) {
		let _workers = AtomicWorker::setup_workers(self.n_threads, inputs, outputs);
	}
}

impl AtomicWorker {	
	fn setup_workers(n_workers: usize, inputs: &HashMap<String,Vec<BaseT>>, outputs: &Vec<BaseT>) -> Vec<AtomicWorker> {
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
	use super::calc::Operator;
	let ast = Expression::Operation(
		Operator::Add,
		Box::new(Expression::Terminal("rax".to_string())),
		Box::new(Expression::Operation(
			Operator::Sub,
			Box::new(Expression::Terminal("rbx".to_string())),
			Box::new(Expression::Terminal("rcx".to_string()))
		))
	);
	let mut inputs = HashMap::new();
	inputs.insert("rax".to_string(), vec![1,2,3,4,5,6,7,8]);
	inputs.insert("rbx".to_string(), vec![1,2,3,4,5,6,7,8]);
	inputs.insert("rcx".to_string(), vec![1,2,3,4,5,6,7,8]);
	let result = WorkerTask::work(None, &inputs, &vec![1,2,3,4,5,6,7,8], &ast);
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
