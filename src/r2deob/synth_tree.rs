use std::{
	cmp::Ordering,
	collections::HashMap,
	collections::BinaryHeap,
	thread,
	thread::JoinHandle,
	sync::mpsc::channel,
	sync::mpsc::Sender,
	sync::mpsc::Receiver,
};

use super::{
	ast::Expression,
	calc::Operator,
	score::Score,
	sat_interface::Sat,
	BaseT,
};

#[derive(Debug,PartialEq)]
struct QueueScore(f32,usize);

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
	score: f32,
	index: usize,
	prev: usize,
	next: Vec<usize>,
	sat_model: Vec<(String,BaseT)>
}

#[derive(Debug)]
pub struct Synthesis {
	n_runs: usize,
	n_threads: usize,
	n_batchsize: usize,
	tree: Vec<Node>,
	queue: BinaryHeap<QueueScore>,
	terms: Vec<Expression>,
	scoring: Score,
}

impl WorkerTask {
	pub fn work(
		inputs: &HashMap<String,Vec<BaseT>>,
		outputs: &Vec<BaseT>,
		exp: &Expression)
		-> WorkerResult {

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
	pub fn default(
		registers: &Vec<String>)
		-> Synthesis {

		Synthesis {
			n_runs: 8192,
			n_threads: 8,
			n_batchsize: 32,
			tree: vec![Node {
				expression: Expression::NonTerminal,
				score: 0.0,
				index: 0,
				prev: 0,
				next: Vec::new(),
				sat_model: Vec::new()
			}],
			queue: BinaryHeap::from(vec![QueueScore(0.0,0usize)]),
			terms: Expression::combinations(
				registers,
				&vec![Operator::Add, Operator::Sub, Operator::Mul, Operator::Div]
			),
			scoring: Score::Combined(0.0),
		}
	}

	pub fn synthesize(
		&mut self,
		inputs: &HashMap<String,Vec<BaseT>>,
		outputs: &Vec<BaseT>) {

		let workers = AtomicWorker::setup_workers(self.n_threads, inputs, outputs);
		for _ in 0..self.n_runs {
			for w in 0..self.n_threads {
				if let Some(node) = self.queue.pop() {
					let derivates = self.tree[node.1].expression.derive(&self.terms);
					self.create_nodes(inputs, outputs, &workers[w], derivates, node.1);
				}
			}
			//self.update(&workers);
			self.rebuild_queue();
		}
	}

	fn recv_n_results(
		n: usize,
		workers: &Vec<AtomicWorker>)
		-> Vec<(f32,usize)> {

		let mut results = Vec::new();
		for _ in 0..n {
			for w in 0..workers.len() {
				if let Ok(result) = &workers[w].rx.try_recv() {
					match result.score {
						Score::Combined(x) => results.push((x, result.node)),
						_ => results.push((0f32, result.node)),
					}
				}
			}
		}
		results
	}

	fn update(
		&mut self,
		workers: &Vec<AtomicWorker>) {

		for result in Synthesis::recv_n_results(self.n_batchsize, workers) {
			self.tree[result.1].score = result.0;
			if result.0 - 1.0 == 0.0 {
				println!("Candidate found: {}", self.tree[result.1].expression.math_notation());
				//::std::process::exit(0);
			}
		}
	}

	fn rebuild_queue(
		&mut self) {

		self.queue.clear();
		for node in self.tree
			.iter()
			.filter(|x| x.next.len() < 1) {
			self.queue.push(QueueScore(node.score, node.index));
		}
	}

	fn add_node(
		&mut self,
		expression: &Expression,
		parent: usize)
		-> usize {

		let node = self.tree.len();

		self.tree.push(Node {
			expression: expression.clone(),
			score: 0.0,
			index: node,
			prev: parent,
			next: Vec::new(),
			sat_model: Vec::new()
		});

		self.tree[parent].next.push(node);

		node
	}

	fn update_parents(
		&mut self,
		start: usize) {

		let mut current = start;
		let mut current_score = self.tree[start].score.clone();
		while let Some(node) = self.tree.get_mut(current) {
			let len = node.next.len() as f32;
			node.score = (node.score * (len - 1f32) + current_score.clone()) / len;
			//node.score = (node.score + current_score.clone()) / 2f32;
			if node.index == 0 { break; }
			current = node.prev;
			current_score = node.score;
		}
	}

	fn create_nodes(
		&mut self,
		inputs: &HashMap<String,Vec<BaseT>>,
		outputs: &Vec<BaseT>,
		worker: &AtomicWorker,
		derivates: Vec<Expression>,
		parent: usize) {

		for expression in derivates.iter() {
			let last_node = self.add_node(expression, parent);
			if let Some(results) = expression.eval(inputs) {
				if let Score::Combined(x) = Score::get(&results, outputs) {
					self.tree[last_node].score = x;
					self.update_parents(last_node);

					if x - 1.0 == 0.0  {
						println!("Candidate found: {} Node #{}",
							expression.math_notation(),
							self.tree[last_node].index
						);
						::std::process::exit(0);
					}
				}
			}
		}
	}
}

impl AtomicWorker {	
	fn setup_workers(
		n_workers: usize,
		inputs: &HashMap<String,Vec<BaseT>>,
		outputs: &Vec<BaseT>)
		-> Vec<AtomicWorker> {

		let mut result: Vec<AtomicWorker> = Vec::new();
		for _ in 0..n_workers {
			let (task_tx, task_rx) = channel::<WorkerTask>();
			let (result_tx, result_rx) = channel::<WorkerResult>();
			let input = inputs.clone();
			let output = outputs.clone();
			let handle = thread::spawn(move|| {
				loop {
					if let Ok(task) = task_rx.recv() {
						let mut result = WorkerTask::work(&input, &output, &task.expression);
						result.node = task.node;
						if let Err(x) = result_tx.send(result) {
							panic!("worker send failure: {:?}", x);
						}
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
		result
	}
}

impl Eq for QueueScore {}

impl PartialOrd for QueueScore {
	fn partial_cmp(
		&self,
		other: &Self)
		-> Option<Ordering> {

		other.0.partial_cmp(&self.0)
	}
}

impl Ord for QueueScore {
    fn cmp(
		&self,
		other: &QueueScore)
		-> Ordering {

        self.partial_cmp(other).unwrap()
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
	let result = WorkerTask::work(&inputs, &vec![1,2,3,4,5,6,7,8], &ast);
	assert_eq!(result.score, Score::Combined(1.0))
}
