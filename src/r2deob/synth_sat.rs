use rsmt2::SmtRes;
//use rsmt2::SmtConf;
use rsmt2::Solver;
use rsmt2::parse::IdentParser;
use rsmt2::parse::ModelParser;

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

enum Symbol {
	non_terminal,
	input,
	constant,
	intermediate,
	candidate
}

struct Node {
	exp: String,
	typ: Symbol,
	next: Option<usize>,
	prev: Option<usize>,
	score: f32
}

struct Tree {
	nodes: Vec<Node>,
	grammar: Grammar,
}

impl Tree {
	fn get_next(current: usize) {

	}
	fn get_prev(current: usize) {

	}

	// TODO: Add function to solve intermediate with a constant C

	fn derive_node(&mut self, current_node: usize, n_inputs: u8) {
		//					U
		//		 .----------+----------.
		//		/ \					  / \	
		// input,  input x U, U x input, U x U 
		let mut nodes: Vec<Node>;
		let operators = 2;
		for n in 0..n_inputs {
			//nodes.push("input".to_string());
			for o in 0..operators {
				//nodes.push(U + op + U);
				//nodes.push("input".to_string() + op + U);
				//nodes.push(U + op + "input";
			};
		};

		let node = Node { exp: "".to_string(), typ: Symbol::non_terminal, next: None, prev: None, score: 0.0 };
		self.nodes.push(node);

	}
}

struct Grammar {
	symbols: Vec<Symbol>,
	operators: Vec<Op>
}

impl Grammar { }

pub struct Synthesis {

}

impl Synthesis {
	pub fn next_expr(&mut self) {
		
	}

	pub fn solve_expr(&mut self, trace: Traces) {
		for n in 0..trace.inputs.len() {
			println!("{:?} ; {:?}", trace.inputs[n], trace.outputs[n]);
		};
		demo();
	}
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
