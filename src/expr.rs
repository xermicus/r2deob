use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
	Neg,
	Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
	Add,
	Sub,
	Mul,
	MulHU,
	DivU,
	RemU,
	And,
	Or,
	Xor,
	Shl,
	ShrL,
	ShrA,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
	Leaf(u8),
	Unary(UnaryOp),
	Binary(BinaryOp),
}

impl Token {
	pub const fn arity(self) -> usize {
		match self {
			Token::Leaf(_) => 0,
			Token::Unary(_) => 1,
			Token::Binary(_) => 2,
		}
	}
}

impl UnaryOp {
	pub const ALL: [UnaryOp; 2] = [UnaryOp::Neg, UnaryOp::Not];

	pub const fn name(self) -> &'static str {
		match self {
			UnaryOp::Neg => "neg",
			UnaryOp::Not => "not",
		}
	}

	const fn symbol(self) -> &'static str {
		match self {
			UnaryOp::Neg => "-",
			UnaryOp::Not => "~",
		}
	}
}

impl BinaryOp {
	pub const ALL: [BinaryOp; 12] = [
		BinaryOp::Add,
		BinaryOp::Sub,
		BinaryOp::Mul,
		BinaryOp::MulHU,
		BinaryOp::DivU,
		BinaryOp::RemU,
		BinaryOp::And,
		BinaryOp::Or,
		BinaryOp::Xor,
		BinaryOp::Shl,
		BinaryOp::ShrL,
		BinaryOp::ShrA,
	];

	pub const fn name(self) -> &'static str {
		match self {
			BinaryOp::Add => "add",
			BinaryOp::Sub => "sub",
			BinaryOp::Mul => "mul",
			BinaryOp::MulHU => "mulhu",
			BinaryOp::DivU => "divu",
			BinaryOp::RemU => "remu",
			BinaryOp::And => "and",
			BinaryOp::Or => "or",
			BinaryOp::Xor => "xor",
			BinaryOp::Shl => "shl",
			BinaryOp::ShrL => "shrl",
			BinaryOp::ShrA => "shra",
		}
	}

	const fn symbol(self) -> &'static str {
		match self {
			BinaryOp::Add => "+",
			BinaryOp::Sub => "-",
			BinaryOp::Mul => "*",
			BinaryOp::MulHU => "*h",
			BinaryOp::DivU => "/u",
			BinaryOp::RemU => "%u",
			BinaryOp::And => "&",
			BinaryOp::Or => "|",
			BinaryOp::Xor => "^",
			BinaryOp::Shl => "<<",
			BinaryOp::ShrL => ">>u",
			BinaryOp::ShrA => ">>",
		}
	}
}

/// The search space: leaves are the input registers followed by `constants`.
#[derive(Debug, Clone)]
pub struct Grammar {
	pub unary: Vec<UnaryOp>,
	pub binary: Vec<BinaryOp>,
	pub constants: Vec<u64>,
}

impl Grammar {
	/// Builds a grammar from operation names such as `add`, `mulhu` or `not`.
	pub fn new<S: AsRef<str>>(ops: &[S], constants: Vec<u64>) -> Result<Grammar, String> {
		let mut grammar = Grammar {
			unary: Vec::new(),
			binary: Vec::new(),
			constants,
		};
		for op in ops {
			let op = op.as_ref();
			if let Some(&unary) = UnaryOp::ALL.iter().find(|candidate| candidate.name() == op) {
				grammar.unary.push(unary);
			} else if let Some(&binary) = BinaryOp::ALL
				.iter()
				.find(|candidate| candidate.name() == op)
			{
				grammar.binary.push(binary);
			} else {
				return Err(format!("unknown operation '{op}'"));
			}
		}
		Ok(grammar)
	}

	/// All productions, ordered by arity.
	pub fn productions(&self, inputs: usize) -> Vec<Token> {
		let leaves = inputs + self.constants.len();
		assert!(leaves <= usize::from(u8::MAX), "too many leaves");
		(0..leaves as u8)
			.map(Token::Leaf)
			.chain(self.unary.iter().copied().map(Token::Unary))
			.chain(self.binary.iter().copied().map(Token::Binary))
			.collect()
	}
}

/// A complete expression in prefix notation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expression {
	pub tokens: Vec<Token>,
	pub leaves: Vec<String>,
}

impl Expression {
	fn fmt_at(&self, f: &mut fmt::Formatter, at: usize) -> Result<usize, fmt::Error> {
		match self.tokens[at] {
			Token::Leaf(leaf) => {
				f.write_str(&self.leaves[usize::from(leaf)])?;
				Ok(at + 1)
			}
			Token::Unary(op) => {
				f.write_str(op.symbol())?;
				self.fmt_at(f, at + 1)
			}
			Token::Binary(op) => {
				f.write_str("(")?;
				let next = self.fmt_at(f, at + 1)?;
				write!(f, " {} ", op.symbol())?;
				let next = self.fmt_at(f, next)?;
				f.write_str(")")?;
				Ok(next)
			}
		}
	}
}

impl fmt::Display for Expression {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		self.fmt_at(f, 0).map(|_| ())
	}
}

#[test]
fn test_grammar_new() {
	let grammar = Grammar::new(&["add", "not", "remu"], vec![1]).unwrap();
	assert_eq!(grammar.unary, [UnaryOp::Not]);
	assert_eq!(grammar.binary, [BinaryOp::Add, BinaryOp::RemU]);
	assert_eq!(
		grammar.productions(2),
		[
			Token::Leaf(0),
			Token::Leaf(1),
			Token::Leaf(2),
			Token::Unary(UnaryOp::Not),
			Token::Binary(BinaryOp::Add),
			Token::Binary(BinaryOp::RemU)
		]
	);
	assert!(Grammar::new(&["pow"], Vec::new()).is_err());
}

#[test]
fn test_display() {
	let expression = Expression {
		tokens: vec![
			Token::Binary(BinaryOp::Sub),
			Token::Binary(BinaryOp::Mul),
			Token::Leaf(0),
			Token::Unary(UnaryOp::Not),
			Token::Leaf(1),
			Token::Leaf(2),
		],
		leaves: vec!["a0".into(), "a1".into(), "1".into()],
	};
	assert_eq!(expression.to_string(), "((a0 * ~a1) - 1)");
}
