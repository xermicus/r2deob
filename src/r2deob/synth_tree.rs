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
use super::sat_interface::Op;
use super::sat_interface::Sat;

#[derive(Debug)]
struct WorkerResult {
	score: Score,
	node: usize,
	model: HashMap<String,u64>
}

#[derive(Debug,Clone)]
struct WorkerTask {
	expression: Expression,
	node: usize
}

#[derive(Debug)]
struct AtomicWorker {
	tx: Sender<WorkerTask>,
	rx: Receiver<WorkerResult>,
	handle: JoinHandle<()>
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
	max_runs: usize,
	threads: usize,
	tree: Vec<Node>,
	queue: Vec<usize>,
	terms: Vec<Expression>,
	scoring: Score,
	solver: Sat
}

impl Synthesis {
	pub fn default(registers: &Vec<String>) -> Synthesis {
		let mut result = Synthesis {
			threads: 1,
			max_runs: 3,
			tree: vec![Node {
				expression: Expression::NonTerminal,
				score: Score::UnSat,
				index: 0,
				prev: 0,
				next: Vec::new(),
				sat_model: Vec::new()
			}],
			queue: vec![],
			terms: Expression::combinations(registers),
			scoring: Score::Combined(0.0),
			solver: Sat::init(),
		};
		result
	}

	pub fn synthesize(&mut self, inputs: Vec<HashMap<String,String>>, outputs: Vec<u64>) {
		
	}
	
	fn setup_workers(n_runs: usize, n_workers: usize) -> Vec<AtomicWorker> {
		let mut result: Vec<AtomicWorker> = Vec::new();
		for _ in 0..n_workers {
			let (task_tx, task_rx) = channel::<WorkerTask>();
			let (result_tx, result_rx) = channel::<WorkerResult>();
			let handle = thread::spawn(move|| {
				//tx.send(1).expect("channel will be there waiting for the pool");
			});
			result.push(AtomicWorker {
				tx: task_tx,
				rx: result_rx,
				handle: handle
			});
		}
		return result
	}
}

/*
impl Node {
	fn score_node(&mut self, inputs: &Vec<HashMap<String,String>>, outputs: &Vec<u64>, scoring: &Score) {
		let mut result: Vec<u64> = Vec::new();
		match self.expression {
			Expression::Operation(_,_,_) => {
				for i in 0..inputs.len() {
					let mut exp: String = format!("(= {} n)", &self.expression);
					for (register, value) in inputs[i].iter() {
						exp = exp.replace(register, value);
					}
					//println!("{}",exp);
					let mut model: Vec<(String,u64)> = Vec::new();
					if Expression::is_finite(&mut self.expression) {
						// TODO: Maybe a simple evaluator will be faster
						model = Sat::init().eval(&exp);
					} else {
						// TODO: Slow
						//model = Sat::init().eval(exp);
					}
					if model.len() > 0 {
						//self.update_score(model, outputs[i], scoring);
						self.sat_model = model.clone();
						let mut yielded: Option<u64> = None;
						for p in model.iter() {
							match p.0.as_ref() {
								"n" => yielded = Some(p.1),
								_ => {}
							} 
						}
						if let Some(y) = yielded {
							let real = outputs[i];
							match scoring {
								Score::Combined(_) => self.score = Score::combined(y, real),
								Score::HammingDistance(_)=> self.score = Score::hamming_distance(y, real),
								Score::AbsDistance(_) => self.score = Score::abs_distance(y, real),
								Score::RangeDistance(_) => self.score = Score::range_distance(y, real),
								_ => {}
							}
						} else {
							self.score = Score::UnSat;
						}
					} else {
						self.score = Score::UnSat;
					}
				}
			},
			_ => return
		}
	}
}

impl Synthesis {
	pub fn default(registers: &Vec<String>) -> Synthesis {
		let mut result = Synthesis {
			max_runs: 3,
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
			solver: Sat::init(),
		};
		result.derive_node(0);
		result
	}

	pub fn hamming_score_async(&mut self, inputs: Vec<HashMap<String,String>>, outputs: Vec<u64>) {
		for _ in 0..self.max_runs {
			//println!("{:?}", self.queue);
			//println!("{:?}", self.tree);
			//if let Some(i) = self.queue.get(0) {
			//	if self.tree.get(i.clone()).unwrap().next.len() < 1 {
			//		self.derive_node(*i);
			//	}
			//}
			for i in self.queue.clone().iter() {
				
			}
			self.update_queue();
			for n in self.tree[self.queue[0]].next.clone().iter() {
				
				self.tree[n.clone()].score_node(&inputs, &outputs, &self.scoring);
				//self.update_parents(n.clone());
			}
		}
	}

	pub fn brute_force(&mut self, inputs: Vec<HashMap<String,String>>, outputs: Vec<u64>) {
	}

	pub fn hamming_score(&mut self, inputs: Vec<HashMap<String,String>>, outputs: Vec<u64>) {
	}

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

	fn update_queue(&mut self) {
		let mut copy: Vec<(usize,f32)> = Vec::new();
		for n in self.tree.iter() {
			match &n.score {
				Score::HammingDistance(s) => copy.push((n.index, *s)),
				Score::AbsDistance(s) => copy.push((n.index, *s)),
				Score::RangeDistance(s) => copy.push((n.index, *s)),
				Score::Combined(s) => copy.push((n.index, *s)),
				_ => copy.push((n.index, 0.0))
			}
		}
		copy.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
		self.queue = copy.iter().map(|s| s.0).collect();
	}

	fn update_parents(&mut self, node: usize) {
		
	}
}*/
