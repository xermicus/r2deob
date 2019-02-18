extern crate calc;
use calc::eval;

extern crate decimal;
use decimal::d128;

extern crate simhash;
use simhash::hamming_distance;

extern crate rayon;
use rayon::prelude::*;

use std::collections::HashMap;

use super::ast::Expression;

#[derive(Copy, Clone)]
enum Symbol {
	Intermediate,
	Constant,
	Candidate
}

#[derive(Clone, Debug)]
struct Node {
	exp: String,
	typ: Symbol,
	index: usize,
	next: Vec<usize>,
	prev: usize,
	score: Option<d128>
}

#[derive(Debug)]
struct Tree {
	nodes: Vec<Node>,
	queue: Vec<usize>,
	terms: Vec<(String,Symbol)>
}

pub struct Synthesis {
	max_runs: usize,
	tree: Tree,
}

impl Tree {
	fn init(terms: Vec<(String,Symbol)>) -> Tree {
		let mut tree: Tree = Tree {
			nodes: vec![ Node {
				exp: "U".to_string(),
				typ: Symbol::Intermediate,
				index: 0,
				next: Vec::new(),
				prev: 0,
				score: None
			}],
			queue: Vec::new(),
			terms: terms
		};
		tree.derive_node(0 as usize);
		tree
	}

	fn add_node(&mut self, c: usize, e: String, t: Symbol) {
		let pos = self.nodes.len();
		self.nodes.push(Node {
			exp: e,
			typ: t,
			index: pos,
			next: Vec::new(),
			prev: c,
			score: None
		});
		self.nodes[c].next.push(pos);
	}

	fn derive_node(&mut self, current_node: usize) {
		//					U
		//		 .----------+----------.
		//		/ \					  / \	
		// input,  input x U, U x input, U x U 
		match self.nodes.get(current_node).unwrap().typ {
			Symbol::Candidate => return,
			Symbol::Constant => return,
			_ => {},
		}

		let current_exp = &self.nodes.get(current_node).unwrap().exp;
		let index_exp: Vec<(_,_)> = current_exp.char_indices().collect();
		for (exp,typ) in self.terms.clone() {
			let mut cand = String::new();
			for (_, c) in &index_exp {
				if c == &'U' { 
					match typ {
						Symbol::Candidate => { cand.push_str(&exp); },
						_ => { cand.push_str("("); cand.push_str(&exp); cand.push_str(")"); }
					}
				}
				else {
					cand.push(*c);
				};
			}
			self.add_node(current_node, cand, typ);
		}
	}

	// TODO: Add function to solve Intermediate with a Constant C
	fn score_node(expr: String, inputs: &Vec<HashMap<String,String>>, outputs: &Vec<u64>) -> Option<d128> {
		let mut result: d128 = d128::from(0);

		for i in 0..inputs.len() {
			let mut expression = expr.clone();
			for (register, value) in inputs[i].iter() {
				expression = expression.replace(register, value);
			}
			if let Ok(val) = eval(&expression) {
				result += eval_score(val.as_float(), d128::from(outputs[i]));
			} else {
				println!("error: failed to eval expression {}", expression);
				return None // TODO obviously something to handle
			}
		}
		Some(result)
	}

	fn update_parents(&mut self, n: usize) {
		let mut parent = self.nodes[n].prev;
		let mut current = n;
		loop {
			if parent == 0 {
				break;
			}
			if let Some(score_current) = self.nodes[current].score {
				if let Some(score_prev) = self.nodes[parent].score {
					self.nodes[parent].score = Some(score_prev + score_current);
				}
				else {
					self.nodes[parent].score = Some(score_current);
				}
			}
			current = parent;
			parent = self.nodes[parent].prev;
		}
	}

	fn update_queue(&mut self) {
		let mut copy: Vec<(usize,u32)> = Vec::new();
		for i in self.nodes.iter() {
			match i.typ { Symbol::Candidate => { continue }, _=>{} }
			if let Some(score) = i.score {
				copy.push((i.index, score.into()));
			} else if i.index > 0 {
				copy.push((i.index, 10000));
			}
		}
		copy.sort_by(|a, b| a.1.cmp(&b.1));
		self.queue = copy.iter().map(|s| s.0).collect();
	}
}

impl Synthesis {
	pub fn default(registers: &Vec<String>) -> Synthesis {
		let operators: Vec<char> = vec!['+','-','*','/','&','|','^','%'];
		Synthesis {
			max_runs: 1000,
			tree: Tree::init(enum_expressions(registers.clone(), operators))
		}
	}

	pub fn brute_force(&mut self, inputs: Vec<HashMap<String,String>>, outputs: Vec<u64>) {
		for i in 1..self.max_runs {
			self.tree.derive_node(i);
		}
		for i in 1..self.tree.nodes.len() {
			if self.tree.nodes[i].next.len() < 1 {
				match self.tree.nodes[i].typ {
					Symbol::Candidate => {
						if let Some(score) = Tree::score_node(
						self.tree.nodes[i].exp.clone(),
						&inputs,
						&outputs) {
							self.tree.nodes[i].score = Some(score);
							if score < d128::from(1) {
								println!("Winner! {}", self.tree.nodes[i].exp);
								//println!("iterations: {}", i);
								return
							}
							self.tree.update_parents(i);
						}
					},
					_ => {}
				}
			}
		}
	}

	pub fn hamming_score(&mut self, inputs: Vec<HashMap<String,String>>, outputs: Vec<u64>) {
		for _ in 0..self.max_runs {
			self.tree.update_queue();
			for i in self.tree.queue.clone().iter() {
				if self.tree.nodes[*i].next.len() < 1 {
					self.tree.derive_node(*i);
					for n in self.tree.nodes[*i].next.clone().iter() {
						match self.tree.nodes[*n].typ {
							Symbol::Candidate => {
								if let Some(score) = Tree::score_node(
								self.tree.nodes[*n].exp.clone(),
								&inputs,
								&outputs) {
									self.tree.nodes[*n].score = Some(score);
									if score < d128::from(1) {
										println!("{:?}", &self.tree);
										return
									}
									self.tree.update_parents(*n);
								}
							},
							_ => {}
						}
					}
				}
			}
		}
	}

	// TODO solve with constant
	pub fn hamming_score_async(&mut self, inputs: Vec<HashMap<String,String>>, outputs: Vec<u64>) {
		for _ in 0..self.max_runs {
			self.tree.update_queue();
			for i in self.tree.queue.clone().iter() {
				if self.tree.nodes[*i].next.len() < 1 {
					self.tree.derive_node(*i);
					// TODO this is dangerous
					let l1: usize = self.tree.nodes.len();
					let l2: usize = self.tree.nodes[*i].next.len();
					self.tree.nodes[l1-l2..l1].par_iter_mut().for_each(|n| {
						match n.typ {
							Symbol::Candidate => {
								n.score = Tree::score_node(n.exp.clone(), &inputs, &outputs);
							},
							_ => {}
						}
					});
					for n in self.tree.nodes[*i].next.clone().iter() {
						if let Some(score) = self.tree.nodes[*n].score {
							if score < d128::from(1) {
								//println!("{:?}", &self.tree.nodes);
								println!("Winner! {}", self.tree.nodes[*n].exp);
								println!("iterations: {}", i);
								return
							}
						}
						self.tree.update_parents(*n);
					}
				}
			}
		}
	}
}

fn enum_expressions(registers: Vec<String>, operators: Vec<char>) -> Vec<(String,Symbol)> {
	let mut expressions: Vec<(String,Symbol)> = Vec::new();
	for input in registers.iter() {
		expressions.push((input.to_string(), Symbol::Candidate));
		for operation in operators.iter() {
			expressions.push(("U".to_string() + &operation.to_string() + "U", Symbol::Intermediate));
			expressions.push((input.to_string() +  &operation.to_string() + "U", Symbol::Intermediate));
			expressions.push(("U".to_string() + &operation.to_string() + &input, Symbol::Intermediate));
			expressions.push(("C".to_string() + &operation.to_string() + "C", Symbol::Constant));
			expressions.push((input.to_string() +  &operation.to_string() + "C", Symbol::Constant));
			expressions.push(("C".to_string() + &operation.to_string() + &input, Symbol::Constant));
		};
	};
	expressions
}

fn eval_score(result_test: d128, result_true: d128) -> d128 {
	let mut result: u32 = 0;
	let bytes_test = result_test.to_raw_bytes();
	let bytes_true = result_true.to_raw_bytes();
	for i in 0..8 { // TODO for now we only care about 64b
		result += hamming_distance(bytes_test[i] as u64, bytes_true[i] as u64);
	}
	d128::from(result)
}

impl std::fmt::Display for Tree {
    fn fmt(&self, w: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
		for n in 0..self.nodes.len() {
			w.write_str("Node #")?; w.write_str(&n.to_string())?; w.write_str("\n")?;
			w.write_str(&self.nodes[n.clone() as usize].to_string())?; w.write_str("\n")?;
		}
		Ok(())
    }
}

impl std::fmt::Display for Node {
    fn fmt(&self, w: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
		w.write_str("\texpression:\t")?; w.write_str(&self.exp)?;
		w.write_str("\n\ttype:\t\t")?; w.write_str(&self.typ.to_string())?;
		w.write_str("\n\tindex:\t\t")?; w.write_str(&self.index.to_string())?;
		w.write_str("\n\tscore:\t\t")?; if let Some(score) = &self.score { w.write_str(&score.to_string())?; }
		w.write_str("\n\tparent:\t\t")?; w.write_str(&self.prev.to_string())?;
		w.write_str("\n\tn childs:\t")?; w.write_str(&self.next.len().to_string())?;
		Ok(())
    }
}

impl std::fmt::Display for Symbol {
	fn fmt(&self, w: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
		w.write_str(match *self {
				Symbol::Intermediate => "Intermediate",
				Symbol::Constant => "Constant",
				Symbol::Candidate => "Candidate",
	        })
	    }
}

impl ::std::fmt::Debug for Symbol {
    fn fmt(&self, w: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        w.write_str(match *self {
			Symbol::Intermediate => "Intermediate",
			Symbol::Constant => "Constant",
			Symbol::Candidate => "Candidate",
        })
    }
}
