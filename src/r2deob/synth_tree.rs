extern crate calc;
use calc::eval;

extern crate decimal;
use decimal::d128;

extern crate simhash;
use simhash::hamming_distance;

use std::collections::HashMap;

#[derive(Copy, Clone)]
enum Symbol {
	Intermediate,
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
	queue: Vec<usize>
}

impl Tree {
	fn init(registers: &Vec<String>) -> Tree {
		let mut tree: Tree = Tree {
			nodes: vec![ Node {
				exp: "U".to_string(),
				typ: Symbol::Intermediate,
				index: 0,
				next: Vec::new(),
				prev: 0,
				score: None//d128::from(0)
			}],
			queue: Vec::new()
		};
		tree.derive_node(0 as usize, registers.clone());
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
			score: None//d128::from(0)
		});
		self.nodes[c].next.push(pos);
	}

	fn derive_node(&mut self, current_node: usize, inputs: Vec<String>) {
		//					U
		//		 .----------+----------.
		//		/ \					  / \	
		// input,  input x U, U x input, U x U 
		match self.nodes.get(current_node).unwrap().typ {
			Symbol::Candidate => return,
			_ => {},
		};

		let expressions = enum_expressions(inputs);
		let current_exp = &self.nodes.get(current_node).unwrap().exp;
		let index_exp: Vec<(_,_)> = current_exp.char_indices().collect();
		for (exp,typ) in expressions {
			let mut cand = String::new();
			for (_, c) in &index_exp {
				if c == &'U' { 
					match typ {
						Symbol::Candidate => { cand.push_str(&exp); },
						_ => { cand.push_str("("); cand.push_str(&exp); cand.push_str(")"); }
					};
				}
				else { cand.push(c.clone()); };
			};
			self.add_node(current_node, cand, typ);
		};
	}

	// TODO: Add function to solve Intermediate with a Constant C
	fn score_node(&mut self, n: usize, inputs: Vec<HashMap<String,String>>, outputs: Vec<u64>) {
		let node = if let Some(val) = self.nodes.get(n) { val } else { return };
		match node.typ { Symbol::Candidate => { },_ => return };
		let mut result: d128 = d128::from(0);
		
		for i in 0..inputs.len() {
			let mut expression = node.exp.clone();
			for (register, value) in inputs[i].clone().iter() {
				expression = expression.replace(register, value);
			}
			if let Ok(val) = eval(&expression) {
				result += eval_score(val.as_float(), d128::from(outputs[i]));
			} else {
				println!("error: failed to eval expression {}", expression);
				return // TODO obviously something to handle
			};
		}
		self.nodes[n].score = Some(result);
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
		};
	}

	fn update_queue(&mut self) {
		let mut copy: Vec<(usize,u32)> = Vec::new();
		for i in self.nodes.iter() {
			if let Some(score) = i.score {
				copy.push((i.index.clone(), score.into()));
			} else if i.index > 0 {
				copy.push((i.index.clone(), 10000));
			}
		}
		copy.sort_by(|a, b| a.1.cmp(&b.1));
		self.queue = copy.iter().map(|s| s.0).collect();
	}
}

pub struct Synthesis {

}

impl Synthesis {
	pub fn brute_force(inputs: Vec<HashMap<String,String>>, outputs: Vec<u64>, registers: Vec<String>, 
	iterations: usize) {
		let mut tree = Tree::init(&registers);
		for i in 1..iterations {
			tree.derive_node(i, registers.clone());
		}
		for i in 1..tree.nodes.len() {
			tree.score_node(i, inputs.clone(), outputs.clone());
			tree.update_parents(i);
			if let Some(score) = tree.nodes[i].score {
				if score < d128::from(1) {
					println!("Winner! {}", tree.nodes[i].exp);
					//println!("{:?}", tree.queue);
					//println!("{}", tree.nodes[i]);
					return
				};
			};
		}
	}

	pub fn hamming_score(inputs: Vec<HashMap<String,String>>, outputs: Vec<u64>, registers: Vec<String>) {
		let mut tree = Tree::init(&registers);
		loop {
			tree.update_queue();
			for i in tree.queue.clone().iter() {
				tree.derive_node(*i, registers.clone());
				for n in tree.nodes[*i].next.clone().iter() {
					tree.score_node(*n, inputs.clone(), outputs.clone());
					tree.update_parents(*n);
					if let Some(score) = tree.nodes[*n].score {
						if score < d128::from(1) {
							println!("Winner! {}", tree.nodes[*n].exp);
							//println!("{:?}", tree.queue);
							//println!("{}", tree.nodes[*n]);
							return
						};
					};
				}
			}
		}
	}
}

fn enum_expressions(inputs: Vec<String>) -> Vec<(String,Symbol)> {
	let mut expressions: Vec<(String,Symbol)> = Vec::new();
	for input in inputs.iter() {
		expressions.push((input.to_string(), Symbol::Candidate));
		for operation in ["+","-","*","/","&","|","^","%"].iter() {
			expressions.push(("U".to_string() + operation + "U", Symbol::Intermediate));
			expressions.push((input.to_string() +  operation + "U", Symbol::Intermediate));
			expressions.push(("U".to_string() + operation + &input, Symbol::Intermediate));
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

impl std::fmt::Display for Symbol { fn fmt(&self, w: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
	w.write_str(match *self {
			Symbol::Intermediate => "Intermediate",
			Symbol::Candidate => "Candidate",
        })
    }
}

impl ::std::fmt::Debug for Symbol {
    fn fmt(&self, w: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        w.write_str(match *self {
			Symbol::Intermediate => "Intermediate",
			Symbol::Candidate => "Candidate",
        })
    }
}
