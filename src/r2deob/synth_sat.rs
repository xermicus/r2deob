extern crate calc;
use calc::eval;

extern crate decimal;
use decimal::d128;

use rsmt2::SmtRes;
//use rsmt2::SmtConf;
use rsmt2::Solver;
use rsmt2::parse::IdentParser;
use rsmt2::parse::ModelParser;

use std::collections::HashMap;

use super::engine::Traces;

/// Empty parser structure, we will not maintain any context.
#[derive(Clone, Copy)]
pub struct Parser;
impl<'a> IdentParser<String, String, &'a str> for Parser {
	fn parse_ident(self, input: &'a str) -> SmtRes<String> {
		Ok(input.to_string())
	}
	fn parse_type(self, input: &'a str) -> SmtRes<String> {
		match input {
		"Int" => Ok("Int".into()),
		"Bool" => Ok("Bool".into()),
		_sort => Ok("Bool".into())//println!("unexpected sort `{}`", sort),
		}
	}
}

impl<'a> ModelParser<String, String, Cst, &'a str> for Parser {
    fn parse_value(
        self,
        input: &'a str,
        _: &String,
        _: &[(String, String)],
        _: &String,
    ) -> SmtRes<Cst> {
        match input.trim() {
            "true" => Ok(Cst::B(true)),
            "false" => Ok(Cst::B(false)),
            int => {
                use std::str::FromStr;
                let s = int.trim();
                if let Ok(res) = isize::from_str(s) {
                    return Ok(Cst::I(res));
                } else if s.len() >= 4 && &s[0..1] == "(" && &s[s.len() - 1..] == ")" {
                    let s = &s[1..s.len() - 1].trim();
                    if &s[0..1] == "-" {
                        let s = &s[1..].trim();
                        if let Ok(res) = isize::from_str(s) {
                            return Ok(Cst::I(-res));
                        }
                    }
                }
                Ok(Cst::B(false))//println!("unexpected value `{}`", int)
            }
        }
    }
}

/// A constant.
#[derive(Clone, Copy)]
pub enum Cst {
    /// Boolean constant.
    B(bool),
    /// Integer constant.
    I(isize),
}
impl ::std::fmt::Display for Cst {
    fn fmt(&self, w: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        match *self {
            Cst::B(b) => write!(w, "{}", b),
            Cst::I(i) if i >= 0 => write!(w, "{}", i),
            Cst::I(i) => write!(w, "(- {})", -i),
        }
    }
}

//impl From<bool> for Cst {
//    fn from(b: bool) -> Self {
//        Cst::B(b)
//    }
//}
//impl From<isize> for Cst {
//    fn from(i: isize) -> Self {
//        Cst::I(i)
//    }
//}
///// An example of expression.
//pub enum Expr {
//    /// A constant.
//    C(Cst),
//    /// Variable.
//    V(String),
//    /// Operator application.
//    O(Op, Vec<Expr>),
//}
//impl Expr {
//    pub fn cst<C: Into<Cst>>(c: C) -> Self {
//        Expr::C(c.into())
//    }
//}

#[derive(Copy, Clone)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Conj,
    Disj,
    Eql,
    Ge,
    Gt,
    Lt,
    Le,
}
impl ::std::fmt::Display for Op {
    fn fmt(&self, w: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        w.write_str(match *self {
            Op::Add => "+",
            Op::Sub => "-",
            Op::Mul => "*",
            Op::Conj => "and",
            Op::Disj => "or",
            Op::Eql => "=",
            Op::Ge => ">=",
            Op::Gt => ">",
            Op::Lt => "<",
            Op::Le => "<=",
        })
    }
}

#[derive(Copy, Clone)]
enum Symbol {
	non_terminal,
	constant,
	intermediate,
	candidate
}
impl std::fmt::Display for Symbol {
    fn fmt(&self, w: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        w.write_str(match *self {
			Symbol::non_terminal => "U",
			Symbol::constant => "C",
			Symbol::intermediate => "Intermediate",
			Symbol::candidate => "Candidate",
        })
    }
}

impl ::std::fmt::Debug for Symbol {
    fn fmt(&self, w: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        w.write_str(match *self {
			Symbol::non_terminal => "U",
			Symbol::constant => "C",
			Symbol::intermediate => "Intermediate",
			Symbol::candidate => "Candidate",
        })
    }
}

#[derive(Clone, Debug)]
struct Node {
	exp: String,
	typ: Symbol,
	next: Vec<usize>,
	prev: usize,
	score: d128
}

impl Node {
}

impl std::fmt::Display for Node {
    fn fmt(&self, w: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
		w.write_str("\texpression:\t"); w.write_str(&self.exp);
		w.write_str("\n\ttype:\t\t"); w.write_str(&self.typ.to_string());
		w.write_str("\n\tscore:\t\t"); w.write_str(&self.score.to_string());
		w.write_str("\n\tparent:\t\t"); w.write_str(&self.prev.to_string());
		w.write_str("\n\tn childs:\t"); w.write_str(&self.next.len().to_string());
		Ok(())
    }
}

#[derive(Debug)]
struct Tree {
	nodes: Vec<Node>,
}

impl Tree {
	fn add_node(&mut self, c: usize, e: String, t: Symbol) {
		self.nodes.push(Node {
			exp: e,
			typ: t,
			next: Vec::new(),
			prev: c,
			score: d128::from(0)
		});
		let index = self.nodes.len()-1;
		self.nodes[c].next.push(index);
	}

	fn init() -> Tree {
		Tree {
			nodes: vec![ Node {
				exp: "U".to_string(),
				typ: Symbol::intermediate,
				next: Vec::new(),
				prev: 0,
				score: d128::from(0)
			}]
		}
	}

	// TODO: Add function to solve intermediate with a constant C
	fn derive_node(&mut self, current_node: usize, inputs: Vec<String>) {
		//					U
		//		 .----------+----------.
		//		/ \					  / \	
		// input,  input x U, U x input, U x U 
		match self.nodes.get(current_node).unwrap().typ {
			Symbol::candidate => return,
			Symbol::constant => return,
			_ => {},
		};

		let expressions = enum_expressions(inputs);
		let current_exp = &self.nodes.get(current_node).unwrap().exp;
		let index_exp: Vec<(_,_)> = current_exp.char_indices().collect();
		for (exp,typ) in expressions {
			let mut cand = String::new();
			for (i, c) in &index_exp {
				if c == &'U' { 
					//cand.push(' ');
					match typ {
						Symbol::candidate => { cand.push_str(&exp); },
						_ => { cand.push_str("("); cand.push_str(&exp); cand.push_str(")"); }
					};
				}
				else { cand.push(c.clone()); };
			};
			self.add_node(current_node, cand, typ);
		};
	}

	fn score_node(&mut self, n: usize, inputs: Vec<HashMap<String,String>>, outputs: Vec<u64>) {
		let mut node = if let Some(val) = self.nodes.get(n) { val } else { return };
		match node.typ { Symbol::candidate => { },_ => return };
		let mut result: d128 = d128::from(1);
		
		for i in 0..inputs.len() {
			let mut expression = node.exp.clone();
			for (register, value) in inputs[i].clone().iter() {
				expression = expression.replace(register, value);
			}
			println!("{}", expression);
			if let Ok(val) = eval(&expression) {
				result /= eval_score(val.as_float(), d128::from(outputs[i]));
			} else {
				result = d128::from(0);
			};//result /= eval_score(eval(&expression).unwrap().as_float(), d128::from(outputs[i]));
		}
		self.nodes[n].score = result;
	}

	fn update_parents(&mut self, n: usize) {
		let mut parent = self.nodes[n].prev;
		let mut current = n;
		loop {
			if parent == 0 {
				break;
			}
			else if self.nodes[parent].score > d128::from(0) {
				self.nodes[parent].score *= self.nodes[current].score;
			}
			else {
				self.nodes[parent].score = d128::from(1) * self.nodes[current].score;
			};
			current = parent;
			parent = self.nodes[parent].prev;
		};
	}
}

impl std::fmt::Display for Tree {
    fn fmt(&self, w: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
		for n in 0..self.nodes.len() {
			w.write_str("Node #"); w.write_str(&n.to_string()); w.write_str("\n");
			w.write_str(&self.nodes[n.clone() as usize].to_string()); w.write_str("\n");
		}
		Ok(())
    }
}

pub struct Synthesis {

}

impl Synthesis {
	pub fn walk_tree(inputs: Vec<HashMap<String,String>>, outputs: Vec<u64>, registers: Vec<String>) {
		let mut tree = Tree::init();
		tree.derive_node(0 as usize, registers.clone());
		for i in 0..1000 {
			tree.derive_node(i, registers.clone());
		}
		for i in 0..tree.nodes.len() {
			tree.score_node(i, inputs.clone(), outputs.clone());
		}
		//tree.derive_node(0 as usize, registers.clone());
		//tree.derive_node(3 as usize, registers.clone());
		//tree.derive_node(13 as usize, registers.clone());
		//tree.score_node(22, inputs, outputs);
		//tree.update_parents(22);
		//tree.score_node(41999, inputs, outputs);
		//tree.update_parents(41999);
		println!("{}", tree);
	}
}

fn enum_expressions(inputs: Vec<String>) -> Vec<(String,Symbol)> {
	let operators = vec!["+","-","*","/","&","|","^"];
	let mut expressions: Vec<(String,Symbol)> = Vec::new();
	for i in inputs.iter() {
		expressions.push((i.to_string(), Symbol::candidate));
		for o in operators.iter() {
			expressions.push(("U".to_string() + o + "U", Symbol::intermediate));
			expressions.push((i.to_string() +  o + "U", Symbol::intermediate));
			expressions.push(("U".to_string() + o + &i, Symbol::intermediate));
		};
	};
	expressions
}

fn eval_score(result_test: d128, result_true: d128) -> d128 {
	// TODO for now just the difference
	result_true / result_test
}

// ------------------------
pub fn demo() {
	let mut solver = Solver::default(Parser).unwrap();
	
	solver.declare_const("n", "Int").unwrap();
	solver.declare_const("m", "Int").unwrap();
	let expression = "(= (+ (* n n) (* m m)) 9)";
	solver.assert(&expression).unwrap();
	solver.check_sat().expect("expected true expression");
	
	let model = solver.get_model_const().expect("while getting model");

	println!("Model for expression: {}", &expression);
	for (ident, typ, value) in model {
		println!("{}: {} = {}",ident,typ,value);
	}
}
// ------------------------
