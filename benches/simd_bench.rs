use std::{collections::HashMap, hint::black_box};

use criterion::{Criterion, criterion_group, criterion_main};

use r2deob::{
	BaseT,
	ast::Expression,
	calc::{Operator, SimdOperator},
};

fn criterion_benchmark(c: &mut Criterion) {
	let instructions: usize = 32;
	let data: Vec<BaseT> = (0..instructions as BaseT).collect();
	let mut inputs: HashMap<String, Vec<BaseT>> = HashMap::new();
	inputs.insert("rax".to_string(), data.clone());
	inputs.insert("rbx".to_string(), data.clone());

	let ast = |op| {
		Expression::Operation(
			op,
			Box::new(Expression::Terminal("rax".to_string())),
			Box::new(Expression::Terminal("rbx".to_string())),
		)
	};

	let mut group = c.benchmark_group("Add");
	group.bench_function("packed", |b| {
		b.iter(|| Operator::simd_add(black_box(&data), black_box(&data)))
	});
	group.bench_function("SISD", |b| {
		b.iter(|| Operator::sisd_add(black_box(&data), black_box(&data)))
	});
	let add = ast(Operator::Add);
	group.bench_function("AST add", |b| b.iter(|| add.eval(black_box(&inputs))));
	group.finish();

	let mut group = c.benchmark_group("Sub");
	group.bench_function("packed", |b| {
		b.iter(|| Operator::simd_sub(black_box(&data), black_box(&data)))
	});
	group.bench_function("SISD", |b| {
		b.iter(|| Operator::sisd_sub(black_box(&data), black_box(&data)))
	});
	let sub = ast(Operator::Sub);
	group.bench_function("AST sub", |b| b.iter(|| sub.eval(black_box(&inputs))));
	group.finish();

	let mut group = c.benchmark_group("Mul");
	group.bench_function("packed", |b| {
		b.iter(|| Operator::simd_mul(black_box(&data), black_box(&data)))
	});
	group.bench_function("SISD", |b| {
		b.iter(|| Operator::sisd_mul(black_box(&data), black_box(&data)))
	});
	group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
