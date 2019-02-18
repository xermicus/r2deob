#[derive(Debug)]
pub enum Expression {
	Terminal(u64),
	NonTerminal,
	Constant,
	Operation(Operator, Box<Expression>, Box<Expression>)
}

#[derive(Debug)]
pub enum Operator {
	Add,
	Sub,
	Mul,
	Div,
	Pow,
	Mod,
	//LShift,
	//RShift,
	//Xor,
	//And,
	//Or,
	//Not,
	Eql
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

impl ::std::fmt::Display for Operator {
    fn fmt(&self, w: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        w.write_str(match *self {
            Operator::Add => "+",
            Operator::Sub => "-",
            Operator::Mul => "*",
            Operator::Div => "/",
            Operator::Pow => "^",
            Operator::Mod => "%",
            Operator::Eql => "=",
        })
    }
}

#[test]
fn ast() {
	let ast = Expression::Operation(
		Operator::Add,
		Box::new(Expression::Terminal(1)),
		Box::new(Expression::Operation(
			Operator::Sub,
			Box::new(Expression::NonTerminal),
			Box::new(Expression::Constant)
		))
	);
    assert_eq!("(+ 1 (- U x))", format!("{}", ast));
}
