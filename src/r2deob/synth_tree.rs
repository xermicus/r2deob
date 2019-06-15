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
			n_runs: 8192,
			n_threads: 1,
			n_batchsize: 32,
			tree: vec![Node {
				expression: Expression::NonTerminal,
				score: 0.0,//Score::UnSat,
				index: 0,
				prev: 0,
				next: Vec::new(),
				sat_model: Vec::new()
			}],
			queue: BinaryHeap::from(vec![QueueScore(0.0,0usize)]),
			terms: Expression::combinations(registers, &vec![Operator::Add, Operator::Sub, Operator::Mul, Operator::Div]),
			scoring: Score::Combined(0.0),
		}
	}

	pub fn synthesize(&mut self, inputs: &HashMap<String,Vec<BaseT>>, outputs: &Vec<BaseT>) {
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

	fn recv_n_results(n: usize, workers: &Vec<AtomicWorker>) -> Vec<(f32,usize)> {
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
		return results
	}

	fn update(&mut self, workers: &Vec<AtomicWorker>) {
		for result in Synthesis::recv_n_results(self.n_batchsize, workers) {
			self.tree[result.1].score = result.0;
			if result.0 == 1.0 {
				println!("Candidate found: {}", self.tree[result.1].expression.math_notation());
				::std::process::exit(0);
			}
		}
	}

	fn rebuild_queue(&mut self) {
		self.queue.clear();
		for node in self.tree.iter().filter(|x| x.next.len() < 1).map(|x| (&x.score, &x.index)) {
			self.queue.push(QueueScore(*node.0, *node.1));
		}
	}

	fn add_node(&mut self, node: usize, expression: &Expression, parent: usize) {
		self.tree.push(Node {
			expression: expression.clone(),
			score: 0.0,
			index: node,
			prev: parent,
			next: Vec::new(),
			sat_model: Vec::new()
		});
		self.tree[parent].next.push(node);
	}

	fn create_nodes(&mut self, inputs: &HashMap<String,Vec<BaseT>>, outputs: &Vec<BaseT>, worker: &AtomicWorker, derivates: Vec<Expression>, parent: usize) {
			for expression in derivates.iter() {
				// eval here
				let last_node = self.tree.len();
				self.add_node(last_node, expression, parent);
				if let Some(results) = expression.eval(inputs) {
					if let Score::Combined(x) = Score::get(&results, outputs) {
						if x == 1.0  { println!("Candidate found: {}", expression.math_notation()); }
						self.tree[last_node].score = x;
					}
				}
			}
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
		return result
	}
}

impl Eq for QueueScore {}

impl PartialOrd for QueueScore {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		other.0.partial_cmp(&self.0)
	}
}

impl Ord for QueueScore {
    fn cmp(&self, other: &QueueScore) -> Ordering {
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
	let result = WorkerTask::work(None, &inputs, &vec![1,2,3,4,5,6,7,8], &ast);
	assert_eq!(result.score, Score::Combined(1.0))
}
