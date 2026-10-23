use std::collections::HashMap;

use crate::{BaseT, calc::Operator};

#[derive(Debug, Clone)]
pub enum Expression {
	Terminal(String),
	NonTerminal,
	Operation(Operator, Box<Expression>, Box<Expression>),
}

impl std::fmt::Display for Expression {
	fn fmt(&self, w: &mut std::fmt::Formatter) -> std::fmt::Result {
		match self {
			Expression::Terminal(x) => write!(w, "{}", x),
			Expression::NonTerminal => write!(w, "U"),
			Expression::Operation(op, a, b) => write!(w, "({} {} {})", op, a, b),
		}
	}
}

impl Expression {
	pub fn math_notation(&self) -> String {
		match self {
			Expression::Terminal(x) => x.clone(),
			Expression::NonTerminal => "U".to_string(),
			Expression::Operation(op, a, b) => {
				format!("({} {} {})", a.math_notation(), op, b.math_notation())
			}
		}
	}

	pub fn eval(&self, input: &HashMap<String, Vec<BaseT>>) -> Option<Vec<BaseT>> {
		match self {
			Expression::Terminal(x) => input.get(x).cloned(),
			Expression::Operation(op, a, b) => op.perform(&a.eval(input)?, &b.eval(input)?),
			Expression::NonTerminal => None,
		}
	}

	pub fn combinations(registers: &[String], operators: &[Operator]) -> Vec<Expression> {
		let non_terminal = || Box::new(Expression::NonTerminal);
		let mut result: Vec<Expression> = operators
			.iter()
			.map(|op| Expression::Operation(*op, non_terminal(), non_terminal()))
			.collect();
		for reg in registers {
			let terminal = || Box::new(Expression::Terminal(reg.clone()));
			result.push(Expression::Terminal(reg.clone()));
			for op in operators {
				result.push(Expression::Operation(*op, terminal(), non_terminal()));
				result.push(Expression::Operation(*op, non_terminal(), terminal()));
			}
		}
		result
	}

	pub fn derive(&self, derivates: &[Expression]) -> Vec<Expression> {
		match self {
			Expression::Operation(op, a, b) => {
				let left = a
					.derive(derivates)
					.into_iter()
					.map(|e| Expression::Operation(*op, Box::new(e), b.clone()));
				let right = b
					.derive(derivates)
					.into_iter()
					.map(|e| Expression::Operation(*op, a.clone(), Box::new(e)));
				left.chain(right).collect()
			}
			Expression::NonTerminal => derivates.to_vec(),
			Expression::Terminal(_) => Vec::new(),
		}
	}
}

#[test]
fn test_derive() {
	let combinations = Expression::combinations(
		&["rax".to_string(), "rbx".to_string()],
		&[Operator::Add, Operator::Sub],
	);
	let _ = Expression::NonTerminal.derive(&combinations);
}

#[test]
fn test_format() {
	let ast = Expression::Operation(
		Operator::Add,
		Box::new(Expression::Terminal("rax".to_string())),
		Box::new(Expression::Operation(
			Operator::Sub,
			Box::new(Expression::NonTerminal),
			Box::new(Expression::NonTerminal),
		)),
	);
	assert_eq!("(+ rax (- U U))", format!("{}", ast));
}

#[test]
fn test_math_notation() {
	let ast = Expression::Operation(
		Operator::Add,
		Box::new(Expression::Terminal("rax".to_string())),
		Box::new(Expression::Operation(
			Operator::Sub,
			Box::new(Expression::NonTerminal),
			Box::new(Expression::NonTerminal),
		)),
	);
	assert_eq!("(rax + (U - U))", ast.math_notation());
}

#[test]
fn test_eval_easy() {
	let ast = Expression::Terminal("rax".to_string());
	let mut inputs: HashMap<String, Vec<BaseT>> = HashMap::new();
	inputs.insert("rax".to_string(), vec![1, 2, 3]);
	let result = ast.eval(&inputs).unwrap();
	assert_eq!(result, vec![1, 2, 3]);
}

#[test]
fn test_eval_add_sub() {
	let ast = Expression::Operation(
		Operator::Add,
		Box::new(Expression::Terminal("rax".to_string())),
		Box::new(Expression::Operation(
			Operator::Sub,
			Box::new(Expression::Terminal("rbx".to_string())),
			Box::new(Expression::Terminal("rcx".to_string())),
		)),
	);
	let mut inputs: HashMap<String, Vec<BaseT>> = HashMap::new();
	inputs.insert("rax".to_string(), vec![1, 2, 3, 1, 1, 1, 1, 1]);
	inputs.insert("rbx".to_string(), vec![1, 4, 9, 1, 1, 1, 1, 1]);
	inputs.insert("rcx".to_string(), vec![1, 2, 3, 1, 1, 1, 1, 1]);
	let result = ast.eval(&inputs).unwrap();
	assert_eq!(result, vec![1, 4, 9, 1, 1, 1, 1, 1]);
}

#[test]
fn test_eval_mul_div() {
	let ast = Expression::Operation(
		Operator::Mul,
		Box::new(Expression::Terminal("rax".to_string())),
		Box::new(Expression::Operation(
			Operator::Div,
			Box::new(Expression::Terminal("rbx".to_string())),
			Box::new(Expression::Terminal("rcx".to_string())),
		)),
	);
	let mut inputs: HashMap<String, Vec<BaseT>> = HashMap::new();
	inputs.insert("rax".to_string(), vec![1, 2, 3, 1, 1, 1, 1, 1]);
	inputs.insert("rbx".to_string(), vec![1, 4, 9, 1, 1, 1, 1, 1]);
	inputs.insert("rcx".to_string(), vec![1, 2, 3, 1, 1, 1, 1, 1]);
	let result = ast.eval(&inputs).unwrap();
	assert_eq!(result, vec![1, 4, 9, 1, 1, 1, 1, 1]);
}
