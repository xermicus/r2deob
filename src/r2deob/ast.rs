use std::collections::HashMap;

use super::{
	calc::Operator,
	BaseT,
};

#[derive(Debug, Clone)]
pub enum Expression {
	Terminal(String),
	NonTerminal,
	Operation(Operator, Box<Expression>, Box<Expression>)
}

impl ::std::fmt::Display for Expression {
    fn fmt(
		&self,
		w: &mut ::std::fmt::Formatter)
		-> ::std::fmt::Result {

		match self {
			Expression::Terminal(x) => write!(w, "{}", x),
			Expression::NonTerminal => write!(w, "U"),
			Expression::Operation(op, a, b) => write!(w, "({} {} {})", op, a, b)
		}
	}
}

impl Expression {
	pub fn math_notation(
		&self)
		-> String {

		match self {
			Expression::Terminal(x) => return x.clone(),
			Expression::NonTerminal => return "U".to_string(),
			Expression::Operation(op, a, b) =>
				return format!("({} {} {})",
					Expression::math_notation(a),
					op,
					Expression::math_notation(b)
				)
		}
	}
	
	pub fn eval(
		&self,
		input: &HashMap<String,Vec<BaseT>>)
		-> Option<Vec<BaseT>> {

		match &self {
			Expression::Terminal(x) => return parse_registers(&x, input),
			Expression::Operation(op, a, b) => {
				let x: Vec<BaseT>;
				if let Some(value) = Expression::eval(a, input) {
					x = value;
				} else { return None }
				let y: Vec<BaseT>;
				if let Some(value) = Expression::eval(b, input) {
					y = value;
				} else { return None }
				return op.perform(&x[..], &y[..])
			},
			_ => return None
		}
	}

	pub fn combinations(
		registers: &Vec<String>,
		operators: &Vec<Operator>)
		-> Vec<Expression> {

		let mut result: Vec<Expression> = Vec::new();

		for reg in registers {
			result.push(Expression::Terminal(reg.clone()));

			for op in operators {
				result.push(
					Expression::Operation(
						*op,
						Box::new(Expression::Terminal(reg.clone())),
						Box::new(Expression::NonTerminal)
					)
				);

				result.push(
					Expression::Operation(
						*op,
						Box::new(Expression::NonTerminal),
						Box::new(Expression::Terminal(reg.clone()))
					)
				);
			}
		}

		result
	}

	pub fn derive(
		&self,
		derivates: &Vec<Expression>)
		-> Vec<Expression> {

		let mut result: Vec<Expression> = Vec::new();

		match &self {
			Expression::Operation(op, a, b) => {
				for e in Expression::derive(a, derivates).iter() {
					result.push(
						Expression::Operation(
							*op,
							Box::new(e.clone()),
							Box::new(*b.clone())
						)
					);
				}
				for e in Expression::derive(b, derivates).iter() {
					result.push(
						Expression::Operation(
							*op, Box::new(
							*a.clone()),
							Box::new(e.clone())
						)
					);
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

fn parse_registers(
	register: &String,
	inputs: &HashMap<String,Vec<BaseT>>)
	-> Option<Vec<BaseT>> {

	if !inputs.contains_key(register) {
		return None
	}
	Some(inputs[register].clone())
}

#[test]
fn test_derive() {
	let combinations = Expression::combinations(&vec!["rax".to_string(), "rbx".to_string()], &vec![Operator::Add, Operator::Sub]);
	let _ = Expression::derive(&mut Expression::NonTerminal, &combinations);
}

#[test]
fn test_format() {
	let ast = Expression::Operation(
	Operator::Add,
		Box::new(Expression::Terminal("rax".to_string())),
		Box::new(Expression::Operation(
			Operator::Sub,
			Box::new(Expression::NonTerminal),
			Box::new(Expression::NonTerminal)
		))
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
			Box::new(Expression::NonTerminal)
		))
	);
    assert_eq!("(rax + (U - U))", ast.math_notation());
}

#[test]
fn test_eval_easy() {
	let ast = Expression::Terminal("rax".to_string());
	let mut inputs: HashMap<String,Vec<BaseT>> = HashMap::new();
	inputs.insert("rax".to_string(), vec![1,2,3]);
	let result = ast.eval(&inputs).unwrap();
	assert!(result == vec![1,2,3], format!("Test result was: {:?}", result));
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
		))
	);
	let mut inputs: HashMap<String,Vec<BaseT>> = HashMap::new();
	inputs.insert("rax".to_string(), vec![1,2,3,1,1,1,1,1]);
	inputs.insert("rbx".to_string(), vec![1,4,9,1,1,1,1,1]);
	inputs.insert("rcx".to_string(), vec![1,2,3,1,1,1,1,1]);
	let result = ast.eval(&inputs).unwrap();
	assert!(result == vec![1,4,9,1,1,1,1,1], format!("Test result was: {:?}", result));
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
		))
	);
	let mut inputs: HashMap<String,Vec<BaseT>> = HashMap::new();
	inputs.insert("rax".to_string(), vec![1,2,3,1,1,1,1,1]);
	inputs.insert("rbx".to_string(), vec![1,4,9,1,1,1,1,1]);
	inputs.insert("rcx".to_string(), vec![1,2,3,1,1,1,1,1]);
	let result = ast.eval(&inputs).unwrap();
	assert!(result == vec![1,4,9,1,1,1,1,1], format!("Test result was: {:?}", result));
}
