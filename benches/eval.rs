use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use r2deob::{
	eval::{Evaluator, Lanes, SAMPLES, Samples, Word},
	expr::{BinaryOp, Token},
};

fn lanes<W: Word>(seed: u64) -> Lanes<W> {
	std::array::from_fn(|i| W::from_reg((i as u64 + seed).wrapping_mul(0x9e37_79b9_7f4a_7c15)))
}

fn mba_truth() -> [Token; 9] {
	[
		Token::Binary(BinaryOp::Sub),
		Token::Binary(BinaryOp::Mul),
		Token::Binary(BinaryOp::Add),
		Token::Leaf(0),
		Token::Leaf(1),
		Token::Leaf(2),
		Token::Binary(BinaryOp::Xor),
		Token::Leaf(0),
		Token::Leaf(1),
	]
}

fn bench_eval<W: Word>(c: &mut Criterion, name: &str) {
	let samples = Samples::<W>::new(vec![lanes(1), lanes(2), lanes(3)], &[], lanes(4));
	let tokens = mba_truth();
	let mut evaluator = Evaluator::new(15);
	c.bench_function(&format!("eval/{name}/9 tokens x {SAMPLES}"), |b| {
		b.iter(|| *evaluator.eval::<false>(black_box(&tokens), &samples))
	});
	let value = lanes::<W>(5);
	c.bench_function(&format!("score/{name}/{SAMPLES}"), |b| {
		b.iter(|| samples.score(black_box(&value)))
	});
}

fn criterion_benchmark(c: &mut Criterion) {
	bench_eval::<u64>(c, "u64");
	bench_eval::<u32>(c, "u32");
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
