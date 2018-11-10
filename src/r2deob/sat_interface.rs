use rsmt2::SmtRes;
//use rsmt2::SmtConf;
use rsmt2::Solver;
use rsmt2::parse::IdentParser;
use rsmt2::parse::ModelParser;

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
                Ok(Cst::B(false))//println!("unexpected Value `{}`", int)
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

pub fn demo() {
	let mut solver = Solver::default(Parser).unwrap();
	
	solver.declare_const("n", "Int").unwrap();
	//solver.declare_const("m", "Int").unwrap();
	//let expression = "(= (+ (* n n) (* m m)) 9)";
	//let expression = "(= (+ (/ 6 (* 1 3)) n) 2)";
	let expression = " (= (+ n (/ 6 (* 1 3))) 2)";
	solver.assert(&expression).unwrap();
	solver.check_sat().expect("expected true expression");
	
	let model = solver.get_model_const().expect("while getting model");

	println!("Model for expression: {}", &expression);
	for (ident, typ, value) in model {
		println!("{}: {} = {}",ident,typ,value);
	}
}

enum TokenType {
	Operator,
	Value
}

struct Token {
	typ: TokenType,
	level: u8,
	expression: String,
	next: Option<Box<Token>>
}

pub fn build_stack(expression:&str) {
	let mut ast: Vec<Token> = Vec::new();
	let mut level: u8 = 0;
	let mut context: bool = false;
	for c in expression.chars() {
		match c {
			'(' => { level += 1; context = false },
			')' => { level -= 1; context = false },
			'+' => { context = false; ast.push(Token {
				typ: TokenType::Operator, level: level, expression: c.to_string(), next: None })},
			'-' => { context = false; ast.push(Token {
				typ: TokenType::Operator, level: level, expression: c.to_string(), next: None })},
			'*' => { context = false; ast.push(Token {
				typ: TokenType::Operator, level: level, expression: c.to_string(), next: None })},
			'/' => { context = false; ast.push(Token {
				typ: TokenType::Operator, level: level, expression: c.to_string(), next: None })},
			_ => {	
					if context {
						let i: usize = ast.len() - 1;
						ast[i].expression.push(c)
					} else {
						context = true; ast.push(Token {
							typ: TokenType::Value, level: level, expression: c.to_string(), next: None })
					}
				},
		}
	}
	
	for t in ast.iter() {
		println!("{}", t.expression);
		match t.typ {
			TokenType::Operator => { println!("\tOperator"); },
			TokenType::Value => { println!("\tValue"); },
		}
		println!("\t{}", t.level);
	}
}

//enum TokenType {
//	ParLeft,
//	ParRightt,
//	Operator,
//	Value,
//	Variable
//}
//
//enum Position {
//	Left,
//	Mid,
//	Right
//}
//
//pub struct Expression {
//	tokentype: TokenType,
//	level: u8,
//	value: String,
//}
//
//struct Node {
//	expression: Expression,
//	position: Position,
//	next: Box<Node>,
//}
//
//pub fn build_stack(expression: &str) {
//	let mut result: Vec<Node> = Vec::new();
//	//let mut result: String = String::new();
//	let mut max_level: u8 = 0;
//	if let Ok(tokens) = parse_expression(expression) {
//		for token in tokens.iter() {
//			// Find innest node
//			if token.level > max_level { max_level = token.level }
//		}
//		for level in 0..max_level {
//			for token in tokens.iter() {
//				let mut switch = false;
//				let mut side = false;
//				let mut ident: usize = 0;
//				if token.level == level { 
//					match token.tokentype {
//						TokenType::ParLeft => { 
//							result.insert_str(0, "( ");
//						},
//						TokenType::ParRightt => {
//							result.push_str(" )");
//						},
//						TokenType::Operator => {
//							result.insert(1, token.symbol);
//							result.insert(2, ' ');
//							switch = true;
//						},
//						_ => {
//							if switch { result.push(' '); switch = false; side = true; }
//							if side { result.push(token.symbol); }
//							else { result.insert(0 + ident, token.symbol); ident +=1; }
//						}
//					}
//				}
//			}
//		}
//	}
//	println!("{}", result);
//}
//
//pub fn parse_expression(expression: &str) -> Result<Vec<Expression>,&str> {
//	let mut tokens: Vec<Expression> = Vec::new();
//	let mut level: u8 = 0;
//	for symbol in expression.chars() {
//		match symbol {
//			'(' => {
//				tokens.push(Expression {
//					tokentype: TokenType::ParLeft,
//					level: level,
//					symbol: symbol
//				});
//				level += 1;
//			},
//			')' => {
//				tokens.push(Expression {
//					tokentype: TokenType::ParRightt,
//					level: level,
//					symbol: symbol
//				});
//				level -= 1;
//			},
//			'=' => {
//				tokens.push(Expression {
//					tokentype: TokenType::Operator,
//					level: level,
//					symbol: symbol
//				});
//		 	},
//			'+' => {
//				tokens.push(Expression {
//					tokentype: TokenType::Operator,
//					level: level,
//					symbol: symbol
//				});
//		 	},
//			'-' => {
//				tokens.push(Expression {
//					tokentype: TokenType::Operator,
//					level: level,
//					symbol: symbol
//				});
//		 	},
//			'*' => {
//				tokens.push(Expression {
//					tokentype: TokenType::Operator,
//					level: level,
//					symbol: symbol
//				});
//		 	},
//			'/' => {
//				tokens.push(Expression {
//					tokentype: TokenType::Operator,
//					level: level,
//					symbol: symbol
//				});
//		 	},
//			'C' => {
//				tokens.push(Expression {
//					tokentype: TokenType::Variable,
//					level: level,
//					symbol: symbol
//				});
//		 	},
//			_ => {
//				tokens.push(Expression {
//					tokentype: TokenType::Value,
//					level: level,
//					symbol: symbol
//				});
//		 	},
//		}
//	}
//	if tokens.len() > 0 {
//		Ok(tokens)
//	} else {
//		Err("failed to parse expression")
//	}
//}
