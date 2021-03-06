#[macro_use]
extern crate criterion;
use std::collections::HashMap;
#[allow(dead_code)]
mod r2deob;
use criterion::{
	Criterion,
	black_box,
	ParameterizedBenchmark,
};
use r2deob::{
	BaseT,
	calc::{
		Operator,
		SimdOperator,
	},
	ast::Expression,
};

fn criterion_benchmark(c: &mut Criterion) {
	let instructions: usize = 32;
	let mut data: Vec<BaseT> = Vec::new();
	for i in 0..instructions {
		data.push(i as BaseT);
	}
	let mut ast = Expression::Operation(
		Operator::Add,
		Box::new(Expression::Terminal("rax".to_string())),
		Box::new(Expression::Terminal("rbx".to_string())),
	);
	let mut inputs: HashMap<String,Vec<BaseT>> = HashMap::new();
	inputs.insert("rax".to_string(), data.clone());
	inputs.insert("rbx".to_string(), data.clone());
	let inputs2 = inputs.clone();
	c.bench(
		"Add",
		ParameterizedBenchmark::new("packed", |b, x| b.iter(|| Operator::simd_add(&x[..], &x[..])), vec![black_box(data.clone())])
		.with_function("SISD", |b, x| b.iter(|| Operator::sisd_add(&x[..], &x[..])))
		.with_function("AST add", move |b, _| b.iter(|| ast.eval(black_box(&inputs)))),
	);
	ast = Expression::Operation(
		Operator::Sub,
		Box::new(Expression::Terminal("rax".to_string())),
		Box::new(Expression::Terminal("rbx".to_string())),
	);
	c.bench(
		"Sub",
		ParameterizedBenchmark::new("packed", |b, x| b.iter(|| Operator::simd_sub(&x[..], &x[..])), vec![black_box(data.clone())])
		.with_function("SISD", |b, x| b.iter(|| Operator::sisd_sub(&x[..], &x[..])))
		.with_function("AST sub", move |b, _| b.iter(|| ast.eval(black_box(&inputs2)))),
	);
	c.bench(
		"Mul",
		ParameterizedBenchmark::new("packed", |b, x| b.iter(|| Operator::simd_mul(&x[..], &x[..])), vec![black_box(data)])
		.with_function("SISD", |b, x| b.iter(|| Operator::simd_mul(&x[..], &x[..]))),
	);
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
