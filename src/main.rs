extern crate rsmt2;
mod r2deob;

use rsmt2::SmtRes;
use rsmt2::SmtConf;
use rsmt2::Solver;
use rsmt2::parse::IdentParser;
use rsmt2::parse::ModelParser;

fn main() {
	let target = r2deob::engine::FcnConfig {
		path: "/home/cyrill/r2deob/a.out".to_string(),
		loc: "sym.calc".to_string(),
		len: "12".to_string(),
		input_regs: vec!["esi".to_string(),"edi".to_string()],
		output_reg: "rax".to_string()
	};

	let mut session = r2deob::engine::Session::init(target).unwrap();

	println!("{:?}", session.add_trace());
	println!("{:?}", session.add_trace());
	println!("{:?}", session.add_trace());
	println!("{:?}", session.add_trace());
	println!("{:?}", session.add_trace());
	println!("{:?}", session.add_trace());
	println!("{:?}", session.add_trace());
	println!("{:?}", session.add_trace());
	session.deobfuscate();

	let mut solver = Solver::default(Parser).unwrap();
	
	solver.declare_const("n", "Int").unwrap();
	//     ^^^^^^^^^^^^^~~~ same as `declare-fun` for a nullary symbol
	solver.declare_const("m", "Int").unwrap();
	solver.assert("(= (+ (* n n) (* m m)) 9)").unwrap();
	println!("{:?}",solver.check_sat().unwrap());
	
	let model = solver.get_model_const().expect("while getting model");

	for (ident, typ, value) in model {
		println!("{}",ident);
		println!("{}",typ);
		println!("{}",value);
	}
}

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
		sort => Ok("Bool".into())//println!("unexpected sort `{}`", sort),
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
impl From<bool> for Cst {
    fn from(b: bool) -> Self {
        Cst::B(b)
    }
}
impl From<isize> for Cst {
    fn from(i: isize) -> Self {
        Cst::I(i)
    }
}
/// An example of expression.
pub enum Expr {
    /// A constant.
    C(Cst),
    /// Variable.
    V(String),
    /// Operator application.
    O(Op, Vec<Expr>),
}
impl Expr {
    pub fn cst<C: Into<Cst>>(c: C) -> Self {
        Expr::C(c.into())
    }
}

