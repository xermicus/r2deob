// Beware this is really a huge mess; ATM not used for anything.
use rsmt2::SmtRes;
use rsmt2::Solver;
use rsmt2::parse::IdentParser;
use rsmt2::parse::ModelParser;

use enum_iterator::IntoEnumIterator;

/// Empty parser structure, we will not maintain any context.
#[derive(Clone, Copy)]
pub struct Parser;

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
    _ident: & String, _params: & [ (String, String) ], _typ: & String,
  ) -> SmtRes<String> {
    Ok(input.into())
  }
}


#[derive(Debug, Copy, Clone, PartialEq, IntoEnumIterator)]
pub enum Op {
	Add,
	Sub,
	Mul,
	Div,
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
