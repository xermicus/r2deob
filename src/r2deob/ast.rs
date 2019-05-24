use enum_iterator::IntoEnumIterator;

use super::sat_interface::Op;

#[derive(Debug, Clone)]
pub enum Expression {
	Terminal(String),
	NonTerminal,
	Constant,
	Operation(Op, Box<Expression>, Box<Expression>)
}

impl ::std::fmt::Display for Expression {
    fn fmt(&self, w: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
		match self {
			Expression::Terminal(x) => write!(w, "{}", x),
			Expression::NonTerminal => write!(w, "U"),
			Expression::Constant => write!(w, "x"),
			Expression::Operation(op, a, b) => write!(w, "({} {} {})", op, a, b)
		}
	}
}

impl Expression {
	pub fn math_notation(&self) -> String {
		match self {
			Expression::Terminal(x) => return x.clone(),
			Expression::NonTerminal => return "U".to_string(),
			Expression::Constant => return "x".to_string(),
			Expression::Operation(op, a, b) => return format!("({} {} {})", Expression::math_notation(a), op, Expression::math_notation(b))
		}
	}

	pub fn is_finite(expression: &mut Expression) -> bool {
		match expression {
			Expression::NonTerminal => return false,
			Expression::Operation(_, a, b) => {
				return Expression::is_finite(&mut *a.clone()) & Expression::is_finite(&mut *b.clone())
			},
			_ => return true
		}
	}

	pub fn combinations(registers: &Vec<String>) -> Vec<Expression> {
		let mut result: Vec<Expression> = Vec::new();
		for reg in registers {
			result.push(Expression::Terminal(reg.clone()));
		}
		for op in Op::into_enum_iter().filter(|x| !x.to_string().contains('=')) {
			result.push(Expression::Operation(op, Box::new(Expression::NonTerminal), Box::new(Expression::NonTerminal)));
		}
		for reg in registers {
			for op in Op::into_enum_iter().filter(|x| !x.to_string().contains('=')) {
				result.push(Expression::Operation(op, Box::new(Expression::Terminal(reg.clone())), Box::new(Expression::NonTerminal)));
				result.push(Expression::Operation(op, Box::new(Expression::NonTerminal), Box::new(Expression::Terminal(reg.clone()))));
			}
		}
		result
	}

	pub fn derive(expression: &mut Expression, derivates: &Vec<Expression>) -> Vec<Expression> {
		let mut result: Vec<Expression> = Vec::new();
		match expression {
			Expression::Operation(op, a, b) => {
				for e in Expression::derive(a, derivates).iter() {
					result.push(Expression::Operation(
						*op,
						Box::new(e.clone()),
						Box::new(*b.clone())
					));
				}
				for e in Expression::derive(b, derivates).iter() {
					result.push(Expression::Operation(
						*op,
						Box::new(*a.clone()),
						Box::new(e.clone())
					));
				}
			},
			Expression::NonTerminal => {
				return derivates.clone();
			},
			_ => {}
		}
		result
	}
}

#[test]
fn ast_test() {
	let ast = Expression::Operation(
		Op::Add,
		Box::new(Expression::Terminal("rax".to_string())),
		Box::new(Expression::Operation(
			Op::Sub,
			Box::new(Expression::NonTerminal),
			Box::new(Expression::Constant)
		))
	);
    assert_eq!("(+ rax (- U x))", format!("{}", ast));
}
