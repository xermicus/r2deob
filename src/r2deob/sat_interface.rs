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
	let mut tokens = get_tokens(expression);
	let mut ast = get_ast(get_tokens(expression));
	for t in tokens.iter() {
		println!("{}", t.expression);
		match t.typ {
			TokenType::Operator => { println!("\tOperator"); },
			TokenType::Value => { println!("\tValue"); },
		}
		println!("\t{}", t.level);
	}
}

fn print_ast(ast: Vec<Token>) {

}

fn get_ast(tokens: Vec<Token>) -> Vec<Token> {
	let mut max_level: u8 = 0;
	for token in tokens { if token.level > max_level { max_level = token.level } }
	Vec::new()
}

fn get_tokens(expression: &str) -> Vec<Token> {
	let mut result: Vec<Token> = Vec::new();
	let mut level: u8 = 0;
	let mut context: bool = false;
	for c in expression.chars() {
		match c {
			'(' => { level += 1; context = false },
			')' => { level -= 1; context = false },
			'+' => { context = false; result.push(Token {
				typ: TokenType::Operator, level: level, expression: c.to_string(), next: None })},
			'-' => { context = false; result.push(Token {
				typ: TokenType::Operator, level: level, expression: c.to_string(), next: None })},
			'*' => { context = false; result.push(Token {
				typ: TokenType::Operator, level: level, expression: c.to_string(), next: None })},
			'/' => { context = false; result.push(Token {
				typ: TokenType::Operator, level: level, expression: c.to_string(), next: None })},
			'=' => { context = false; result.push(Token {
				typ: TokenType::Operator, level: level, expression: c.to_string(), next: None })},
			_ => {	
					if context {
						let i: usize = result.len() - 1;
						result[i].expression.push(c)
					} else {
						context = true; result.push(Token {
							typ: TokenType::Value, level: level, expression: c.to_string(), next: None })
					}
				},
		}
	}
	result
}
