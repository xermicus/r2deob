use std::collections::HashMap;

use rsmt2::SmtRes;
//use rsmt2::SmtConf;
use rsmt2::Solver;
use rsmt2::parse::IdentParser;
use rsmt2::parse::ModelParser;

use enum_iterator::IntoEnumIterator;

use super::ast::Expression;
use super::score::Score;

/// Empty parser structure, we will not maintain any context.
#[derive(Clone, Copy)]
pub struct Parser;
/*impl<'a> IdentParser<String, String, &'a str> for Parser {
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
				Ok(Cst::B(false))//println!("unexpected Value `{}`", int)
			}
		}
	}
}*/

impl<'a> IdentParser<String, String, & 'a str> for Parser {
  // Idents ~~~~~~~~~^^^^^^          ^^^^^^^^~~~~ Input
  fn parse_ident(self, input: & 'a str) -> SmtRes<String> {
    Ok(input.into())
  }
  fn parse_type(self, input: & 'a str) -> SmtRes<String> {
    Ok(input.into())
  }
}

  // Types ~~~~~~~~~~~~~~~~~~vvvvvv  vvvvvv~~~~~~~~~~~~~~ Values
impl<'a> ModelParser<String, String, String, & 'a str> for Parser {
  // Idents ~~~~~~~~~^^^^^^                  ^^^^^^^^~~~~ Input
  fn parse_value(
    self, input: & 'a str,
    ident: & String, params: & [ (String, String) ], typ: & String,
  ) -> SmtRes<String> {
    Ok(input.into())
  }
}

/// A constant.
#[derive(Debug, Clone, Copy)]
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

#[derive(Debug, Copy, Clone, PartialEq, IntoEnumIterator)]
pub enum Op {
	Add,
	Sub,
	Mul,
	Div,
	//Conj,
	//Disj,
	Pow,
	Eql,
}

impl ::std::fmt::Display for Op {
	fn fmt(&self, w: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
		w.write_str(match *self {
			Op::Add => "+",
			Op::Sub => "-",
			Op::Mul => "*",
			Op::Div => "/",
			//Op::Conj => "and",
			//Op::Disj => "or",
			Op::Pow => "^",
			Op::Eql => "=",
		})
	}
}

pub struct Sat {
	pub solver: Solver<Parser>,
}

impl Sat {
	pub fn init() -> Sat {
		let mut result = Sat {
			solver: Solver::default(Parser).expect("during Solver::default")
		};
		result.solver.declare_const("U", "BitVec").expect("during constant declaration in Sat::init");
		result
	}

	pub fn eval(&mut self, expression: &String) -> Vec<(String,u64)> {
		let mut result: Vec<(String,u64)> = Vec::new();
		self.solver.assert(&expression).unwrap();
		if let Ok(sat) = self.solver.check_sat() {
			if let Ok(model) = self.solver.get_model_const() {
				for (ident, typ, value) in model {
					result.push((ident,0));
				}
			}
		}
		result
	}

	pub fn check_sat(&mut self, exp: String, inputs: &Vec<HashMap<String,u64>>) -> Option<HashMap<String,u64>> {
		let mut constraints: Vec<String> = Vec::new();
		for i in inputs.iter() {
			let mut constraint = exp.clone();
			for (reg, value) in i.iter() {
				constraint = constraint.replace(reg, &value.to_string());
			}
			constraints.push(constraint);
		}
		println!("{:?}", constraints);
		for c in constraints.iter() {
			self.solver.assert(c).expect("during constraint assertion in Sat::check_sat");
		}
		if !self.solver.check_sat().expect("during satisfiability checking in Sat::check_sat") {
			return None
		}
		None
	}
}

impl std::fmt::Debug for Sat {
	fn fmt(&self, w: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
		write!(w, "Sat")
	}
}

#[test]
pub fn sat_test() {
	let mut solver = Solver::default(Parser).unwrap();
	
	solver.declare_const("n", "Int").unwrap();
	let expression = " (= (+ 2 (/ 6 (* 1 3))) n)";
	solver.assert(&expression).unwrap();
	solver.check_sat().expect("expected true expression");
	
	let model = solver.get_model_const().expect("while getting model");

	println!("Model for expression: {}", &expression);
	for (ident, typ, value) in model {
		println!("{}: {} = {}",ident,typ,value);
	}
}
