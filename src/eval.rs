use std::ops::Range;

use crate::expr::{BinaryOp, Token, UnaryOp};

pub const SAMPLES: usize = 16;

pub type Lanes<W> = [W; SAMPLES];

/// Machine word arithmetic with PolkaVM (RISC-V) semantics.
pub trait Word: Copy + Default + PartialEq + Ord + std::fmt::Debug + Send + Sync + 'static {
	const BITS: u32;
	fn from_reg(value: u64) -> Self;
	fn to_reg(self) -> u64;
	fn add(self, rhs: Self) -> Self;
	fn sub(self, rhs: Self) -> Self;
	fn mul(self, rhs: Self) -> Self;
	fn mulhu(self, rhs: Self) -> Self;
	fn divu(self, rhs: Self) -> Self;
	fn remu(self, rhs: Self) -> Self;
	fn and(self, rhs: Self) -> Self;
	fn or(self, rhs: Self) -> Self;
	fn xor(self, rhs: Self) -> Self;
	fn shl(self, rhs: Self) -> Self;
	fn shrl(self, rhs: Self) -> Self;
	fn shra(self, rhs: Self) -> Self;
	fn neg(self) -> Self;
	fn not(self) -> Self;
	fn count_ones(self) -> u32;
	fn leading_zeros(self) -> u32;
	fn zero_bytes(self) -> u32;
	fn sign_fill(self) -> Self;
	fn divisor(self) -> Divisor<Self>;
	fn div_by(self, divisor: &Divisor<Self>) -> Self;
}

/// Granlund–Montgomery reciprocal of an invariant divisor.
#[derive(Debug, Clone, Copy, Default)]
pub struct Divisor<W> {
	value: W,
	magic: W,
	shift: u32,
	kind: DivisorKind,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum DivisorKind {
	#[default]
	Zero,
	PowerOfTwo,
	General,
}

macro_rules! impl_word {
	($word:ty, $signed:ty, $wide:ty) => {
		impl Word for $word {
			const BITS: u32 = <$word>::BITS;

			#[inline(always)]
			fn from_reg(value: u64) -> Self {
				value as $word
			}
			#[inline(always)]
			fn to_reg(self) -> u64 {
				self as $signed as i64 as u64
			}
			#[inline(always)]
			fn add(self, rhs: Self) -> Self {
				self.wrapping_add(rhs)
			}
			#[inline(always)]
			fn sub(self, rhs: Self) -> Self {
				self.wrapping_sub(rhs)
			}
			#[inline(always)]
			fn mul(self, rhs: Self) -> Self {
				self.wrapping_mul(rhs)
			}
			#[inline(always)]
			fn mulhu(self, rhs: Self) -> Self {
				((self as $wide * rhs as $wide) >> <$word>::BITS) as $word
			}
			#[inline(always)]
			fn divu(self, rhs: Self) -> Self {
				self.checked_div(rhs).unwrap_or(<$word>::MAX)
			}
			#[inline(always)]
			fn remu(self, rhs: Self) -> Self {
				self.checked_rem(rhs).unwrap_or(self)
			}
			#[inline(always)]
			fn and(self, rhs: Self) -> Self {
				self & rhs
			}
			#[inline(always)]
			fn or(self, rhs: Self) -> Self {
				self | rhs
			}
			#[inline(always)]
			fn xor(self, rhs: Self) -> Self {
				self ^ rhs
			}
			#[inline(always)]
			fn shl(self, rhs: Self) -> Self {
				self << (rhs & (<$word>::BITS - 1) as $word)
			}
			#[inline(always)]
			fn shrl(self, rhs: Self) -> Self {
				self >> (rhs & (<$word>::BITS - 1) as $word)
			}
			#[inline(always)]
			fn shra(self, rhs: Self) -> Self {
				((self as $signed) >> (rhs & (<$word>::BITS - 1) as $word)) as $word
			}
			#[inline(always)]
			fn neg(self) -> Self {
				self.wrapping_neg()
			}
			#[inline(always)]
			fn not(self) -> Self {
				!self
			}
			#[inline(always)]
			fn count_ones(self) -> u32 {
				<$word>::count_ones(self)
			}
			#[inline(always)]
			fn leading_zeros(self) -> u32 {
				<$word>::leading_zeros(self)
			}
			fn divisor(self) -> Divisor<Self> {
				if self == 0 {
					return Divisor::default();
				}
				let shift = <$word>::BITS - 1 - self.leading_zeros();
				if self.is_power_of_two() {
					return Divisor {
						value: self,
						magic: 0,
						shift,
						kind: DivisorKind::PowerOfTwo,
					};
				}
				let dividend = 1 << (<$word>::BITS + shift);
				let (quotient, remainder) = (dividend / self as $wide, dividend % self as $wide);
				let twice = 2 * remainder;
				let magic = 2 * quotient + <$wide>::from(twice >= self as $wide) + 1;
				Divisor {
					value: self,
					magic: magic as $word,
					shift,
					kind: DivisorKind::General,
				}
			}
			#[inline(always)]
			fn div_by(self, divisor: &Divisor<Self>) -> Self {
				match divisor.kind {
					DivisorKind::Zero => <$word>::MAX,
					DivisorKind::PowerOfTwo => self >> divisor.shift,
					DivisorKind::General => {
						let high =
							((self as $wide * divisor.magic as $wide) >> <$word>::BITS) as $word;
						(((self - high) >> 1) + high) >> divisor.shift
					}
				}
			}
			#[inline(always)]
			fn sign_fill(self) -> Self {
				((self as $signed) >> (<$word>::BITS - 1)) as $word
			}
			#[inline(always)]
			fn zero_bytes(self) -> u32 {
				let low = <$word>::from_ne_bytes([0x7f; size_of::<$word>()]);
				(!(((self & low).wrapping_add(low)) | self | low)).count_ones()
			}
		}
	};
}

impl_word!(u32, i32, u64);
impl_word!(u64, i64, u128);

/// Observed input/output behaviour; leaf `i` reads `columns[i]`.
#[derive(Debug, Clone)]
pub struct Samples<W> {
	pub columns: Vec<Lanes<W>>,
	pub output: Lanes<W>,
	divisors: Vec<[Divisor<W>; SAMPLES]>,
}

impl<W: Word> Samples<W> {
	pub fn new(inputs: Vec<Lanes<W>>, constants: &[u64], output: Lanes<W>) -> Samples<W> {
		let mut columns = inputs;
		columns.extend(constants.iter().map(|&c| [W::from_reg(c); SAMPLES]));
		Samples::from_columns(columns, output)
	}

	/// Adds a leaf column, e.g. the values of a compressed expression.
	pub fn push(&mut self, column: Lanes<W>) {
		self.divisors.push(column.map(W::divisor));
		self.columns.push(column);
	}

	/// Removes the leaf columns in `range`; the leaves after it move down.
	pub fn remove(&mut self, range: Range<usize>) {
		self.columns.drain(range.clone());
		self.divisors.drain(range);
	}

	pub fn from_columns(columns: Vec<Lanes<W>>, output: Lanes<W>) -> Samples<W> {
		let divisors = columns
			.iter()
			.map(|column| column.map(W::divisor))
			.collect();
		Samples {
			columns,
			output,
			divisors,
		}
	}

	pub const fn max_score() -> u32 {
		3 * W::BITS * SAMPLES as u32
	}

	/// Equals [`Self::max_score`] iff all samples match.
	#[inline]
	pub fn score(&self, value: &Lanes<W>) -> u32 {
		let bitwise: u32 = value
			.iter()
			.zip(&self.output)
			.map(|(&x, &y)| {
				let diff = x.xor(y);
				W::BITS - diff.count_ones() + 8 * diff.zero_bytes()
			})
			.sum();
		bitwise + sign_bits(value, &self.output)
	}
}

/// Kept scalar and out of line: AVX2 has no `lzcnt`, and inlined the SLP vectorizer emulates it.
#[inline(never)]
fn sign_bits<W: Word>(value: &Lanes<W>, output: &Lanes<W>) -> u32 {
	let mut acc = [0; 4];
	for (x, y) in value
		.as_chunks::<4>()
		.0
		.iter()
		.zip(output.as_chunks::<4>().0)
	{
		for ((acc, &x), &y) in acc.iter_mut().zip(x).zip(y) {
			let diff = x.sub(y);
			*acc += diff.sign_fill().xor(diff).leading_zeros();
		}
	}
	acc.iter().sum()
}

pub struct Evaluator<W> {
	stack: Vec<Lanes<W>>,
	origins: Vec<u8>,
}

const COMPUTED: u8 = u8::MAX;

#[inline(always)]
fn divide<W: Word>(
	lhs: &Lanes<W>,
	rhs: &mut Lanes<W>,
	divisors: &[Divisor<W>; SAMPLES],
	remainder: bool,
) {
	for ((x, y), divisor) in lhs.iter().zip(rhs.iter_mut()).zip(divisors) {
		let quotient = x.div_by(divisor);
		*y = match (remainder, divisor.kind) {
			(false, _) => quotient,
			(true, DivisorKind::Zero) => *x,
			(true, _) => x.sub(quotient.mul(divisor.value)),
		};
	}
}

#[inline(always)]
fn unary<W: Word>(value: &mut Lanes<W>, f: impl Fn(W) -> W) {
	for x in value.iter_mut() {
		*x = f(*x);
	}
}

#[inline(always)]
fn binary<W: Word>(lhs: &Lanes<W>, rhs: &mut Lanes<W>, f: impl Fn(W, W) -> W) {
	for (x, y) in lhs.iter().zip(rhs.iter_mut()) {
		*y = f(*x, *y);
	}
}

impl<W: Word> Evaluator<W> {
	pub fn new(max_len: usize) -> Evaluator<W> {
		let depth = max_len.div_ceil(2).max(1);
		Evaluator {
			stack: vec![[W::default(); SAMPLES]; depth],
			origins: vec![COMPUTED; depth],
		}
	}

	/// `DIVISION` enables reciprocals for division by a leaf.
	pub fn eval<const DIVISION: bool>(
		&mut self,
		tokens: &[Token],
		samples: &Samples<W>,
	) -> &Lanes<W> {
		let origins = &mut self.origins;
		let mut top = 0;
		for &token in tokens.iter().rev() {
			match token {
				Token::Leaf(leaf) => {
					self.stack[top] = samples.columns[usize::from(leaf)];
					if DIVISION {
						origins[top] = leaf;
					}
					top += 1;
				}
				Token::Unary(op) => {
					let value = &mut self.stack[top - 1];
					if DIVISION {
						origins[top - 1] = COMPUTED;
					}
					match op {
						UnaryOp::Neg => unary(value, W::neg),
						UnaryOp::Not => unary(value, W::not),
					}
				}
				Token::Binary(op) => {
					top -= 1;
					let (below, above) = self.stack.split_at_mut(top);
					let (lhs, rhs) = (&above[0], &mut below[top - 1]);
					let divisors = DIVISION
						.then(|| samples.divisors.get(usize::from(origins[top - 1])))
						.flatten();
					if DIVISION {
						origins[top - 1] = COMPUTED;
					}
					match op {
						BinaryOp::Add => binary(lhs, rhs, W::add),
						BinaryOp::Sub => binary(lhs, rhs, W::sub),
						BinaryOp::Mul => binary(lhs, rhs, W::mul),
						BinaryOp::MulHU => binary(lhs, rhs, W::mulhu),
						BinaryOp::DivU => match divisors {
							Some(divisors) => divide(lhs, rhs, divisors, false),
							None => binary(lhs, rhs, W::divu),
						},
						BinaryOp::RemU => match divisors {
							Some(divisors) => divide(lhs, rhs, divisors, true),
							None => binary(lhs, rhs, W::remu),
						},
						BinaryOp::And => binary(lhs, rhs, W::and),
						BinaryOp::Or => binary(lhs, rhs, W::or),
						BinaryOp::Xor => binary(lhs, rhs, W::xor),
						BinaryOp::Shl => binary(lhs, rhs, W::shl),
						BinaryOp::ShrL => binary(lhs, rhs, W::shrl),
						BinaryOp::ShrA => binary(lhs, rhs, W::shra),
					}
				}
			}
		}
		&self.stack[0]
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn lanes(f: impl Fn(u64) -> u64) -> Lanes<u64> {
		std::array::from_fn(|i| f((i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)))
	}

	#[test]
	fn test_eval_prefix_order() {
		let a = lanes(|x| x);
		let b = lanes(|x| x.rotate_left(17));
		let samples = Samples::new(vec![a, b], &[3], lanes(|_| 0));
		let tokens = [
			Token::Binary(BinaryOp::Sub),
			Token::Binary(BinaryOp::Shl),
			Token::Leaf(0),
			Token::Leaf(2),
			Token::Unary(UnaryOp::Not),
			Token::Leaf(1),
		];
		let expected: Vec<u64> = (0..SAMPLES)
			.map(|i| (a[i] << 3).wrapping_sub(!b[i]))
			.collect();
		let mut evaluator = Evaluator::new(tokens.len());
		assert_eq!(
			evaluator.eval::<false>(&tokens, &samples).to_vec(),
			expected
		);
		assert_eq!(evaluator.eval::<true>(&tokens, &samples).to_vec(), expected);
	}

	#[test]
	fn test_riscv_semantics() {
		assert_eq!(7u64.divu(0), u64::MAX);
		assert_eq!(7u64.remu(0), 7);
		assert_eq!(1u64.shl(65), 2);
		assert_eq!(u64::MAX.mulhu(2), 1);
		assert_eq!(0x8000_0000u32.shra(31), u32::MAX);
		assert_eq!(0x8000_0000u32.to_reg(), 0xffff_ffff_8000_0000);
		assert_eq!(0x0012_0034u32.zero_bytes(), 2);
	}

	#[test]
	fn test_divisor() {
		let mut state = 0x1234_5678_9abc_def0u64;
		let mut next = || {
			state ^= state << 13;
			state ^= state >> 7;
			state ^= state << 17;
			state
		};
		let mut divisors = vec![
			0u64,
			1,
			2,
			3,
			5,
			7,
			10,
			1 << 63,
			(1 << 63) + 1,
			u64::MAX,
			u64::MAX - 1,
		];
		divisors.extend((0..2000).map(|i| next() >> (i % 64)));
		for d in divisors {
			let divisor = d.divisor();
			for x in [
				0,
				1,
				d,
				d.wrapping_sub(1),
				d.wrapping_add(1),
				u64::MAX,
				next(),
				next() >> 32,
			] {
				assert_eq!(x.div_by(&divisor), x.divu(d), "{x} / {d}");
				let narrow = (d as u32).divisor();
				assert_eq!(
					(x as u32).div_by(&narrow),
					(x as u32).divu(d as u32),
					"{x} / {d} (u32)"
				);
			}
		}
	}

	#[test]
	fn test_eval_leaf_division() {
		let a = lanes(|x| x);
		let n = lanes(|x| x >> (x % 61));
		let samples = Samples::new(vec![a, n], &[0], lanes(|_| 0));
		let mut evaluator = Evaluator::new(5);
		for (op, f) in [
			(BinaryOp::RemU, u64::remu as fn(u64, u64) -> u64),
			(BinaryOp::DivU, u64::divu),
		] {
			for divisor in [1, 2] {
				let tokens = [Token::Binary(op), Token::Leaf(0), Token::Leaf(divisor)];
				let expected: Vec<u64> = (0..SAMPLES)
					.map(|i| f(a[i], samples.columns[usize::from(divisor)][i]))
					.collect();
				assert_eq!(evaluator.eval::<true>(&tokens, &samples).to_vec(), expected);
				assert_eq!(
					evaluator.eval::<false>(&tokens, &samples).to_vec(),
					expected
				);
			}
		}
	}

	#[test]
	fn test_score() {
		let output = lanes(|x| x);
		let samples = Samples::new(Vec::new(), &[], output);
		assert_eq!(samples.score(&output), Samples::<u64>::max_score());
		assert!(samples.score(&lanes(|x| x ^ 1)) < Samples::<u64>::max_score());
	}
}
