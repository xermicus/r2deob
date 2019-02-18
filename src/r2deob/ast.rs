use super::sat_interface::Op;

#[derive(Debug)]
pub enum Expression {
	Terminal(u64),
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

#[test]
fn ast_test() {
	let ast = Expression::Operation(
		Op::Add,
		Box::new(Expression::Terminal(1)),
		Box::new(Expression::Operation(
			Op::Sub,
			Box::new(Expression::NonTerminal),
			Box::new(Expression::Constant)
		))
	);
    assert_eq!("(+ 1 (- U x))", format!("{}", ast));
}
