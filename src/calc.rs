use wide::{i32x16, i64x8};

use crate::BaseT;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Operator {
	Add,
	Sub,
	Mul,
	Div,
}

impl std::fmt::Display for Operator {
	fn fmt(&self, w: &mut std::fmt::Formatter) -> std::fmt::Result {
		w.write_str(match *self {
			Operator::Add => "+",
			Operator::Sub => "-",
			Operator::Mul => "*",
			Operator::Div => "/",
		})
	}
}

impl Operator {
	pub fn perform(&self, a: &[BaseT], b: &[BaseT]) -> Option<Vec<BaseT>> {
		Some(match self {
			Operator::Add => Operator::simd_add(a, b),
			Operator::Sub => Operator::simd_sub(a, b),
			Operator::Mul => Operator::simd_mul(a, b),
			Operator::Div => Operator::simd_div(a, b),
		})
	}
}

pub trait SimdOperator<T> {
	fn simd_add(a: &[T], b: &[T]) -> Vec<T>;
	fn sisd_add(a: &[T], b: &[T]) -> Vec<T>;
	fn simd_sub(a: &[T], b: &[T]) -> Vec<T>;
	fn sisd_sub(a: &[T], b: &[T]) -> Vec<T>;
	fn simd_mul(a: &[T], b: &[T]) -> Vec<T>;
	fn sisd_mul(a: &[T], b: &[T]) -> Vec<T>;
	fn simd_div(a: &[T], b: &[T]) -> Vec<T>;
	fn sisd_div(a: &[T], b: &[T]) -> Vec<T>;
}

/// Applies `simd` lane-wise on full 8-element chunks and `scalar` on the remainder.
fn zip_i64x8(
	a: &[i64],
	b: &[i64],
	simd: impl Fn(i64x8, i64x8) -> i64x8,
	scalar: impl Fn(i64, i64) -> i64,
) -> Vec<i64> {
	let n = a.len().min(b.len());
	let (a, b) = (&a[..n], &b[..n]);
	let mut result = Vec::with_capacity(n);
	let ((a_chunks, a_rest), (b_chunks, b_rest)) = (a.as_chunks::<8>(), b.as_chunks::<8>());
	for (x, y) in a_chunks.iter().zip(b_chunks) {
		result.extend_from_slice(&simd(i64x8::from(*x), i64x8::from(*y)).to_array());
	}
	result.extend(a_rest.iter().zip(b_rest).map(|(x, y)| scalar(*x, *y)));
	result
}

/// Applies `simd` lane-wise on full 16-element chunks and `scalar` on the remainder.
fn zip_i32x16(
	a: &[i32],
	b: &[i32],
	simd: impl Fn(i32x16, i32x16) -> i32x16,
	scalar: impl Fn(i32, i32) -> i32,
) -> Vec<i32> {
	let n = a.len().min(b.len());
	let (a, b) = (&a[..n], &b[..n]);
	let mut result = Vec::with_capacity(n);
	let ((a_chunks, a_rest), (b_chunks, b_rest)) = (a.as_chunks::<16>(), b.as_chunks::<16>());
	for (x, y) in a_chunks.iter().zip(b_chunks) {
		result.extend_from_slice(&simd(i32x16::from(*x), i32x16::from(*y)).to_array());
	}
	result.extend(a_rest.iter().zip(b_rest).map(|(x, y)| scalar(*x, *y)));
	result
}

impl SimdOperator<i64> for Operator {
	fn simd_add(a: &[i64], b: &[i64]) -> Vec<i64> {
		zip_i64x8(a, b, |x, y| x + y, i64::wrapping_add)
	}

	fn sisd_add(a: &[i64], b: &[i64]) -> Vec<i64> {
		a.iter().zip(b).map(|(x, y)| x + y).collect()
	}

	fn simd_sub(a: &[i64], b: &[i64]) -> Vec<i64> {
		zip_i64x8(a, b, |x, y| x - y, i64::wrapping_sub)
	}

	fn sisd_sub(a: &[i64], b: &[i64]) -> Vec<i64> {
		a.iter().zip(b).map(|(x, y)| x - y).collect()
	}

	fn simd_mul(a: &[i64], b: &[i64]) -> Vec<i64> {
		zip_i64x8(a, b, |x, y| x * y, i64::wrapping_mul)
	}

	fn sisd_mul(a: &[i64], b: &[i64]) -> Vec<i64> {
		a.iter().zip(b).map(|(x, y)| x * y).collect()
	}

	fn simd_div(a: &[i64], b: &[i64]) -> Vec<i64> {
		a.iter()
			.zip(b)
			.map(|(x, y)| x.checked_div(*y).unwrap_or(0))
			.collect()
	}

	fn sisd_div(a: &[i64], b: &[i64]) -> Vec<i64> {
		a.iter()
			.zip(b)
			.map(|(x, y)| x.checked_div(*y).unwrap())
			.collect()
	}
}

impl SimdOperator<i32> for Operator {
	fn simd_add(a: &[i32], b: &[i32]) -> Vec<i32> {
		zip_i32x16(a, b, |x, y| x + y, i32::wrapping_add)
	}

	fn sisd_add(a: &[i32], b: &[i32]) -> Vec<i32> {
		a.iter().zip(b).map(|(x, y)| x + y).collect()
	}

	fn simd_sub(a: &[i32], b: &[i32]) -> Vec<i32> {
		zip_i32x16(a, b, |x, y| x - y, i32::wrapping_sub)
	}

	fn sisd_sub(a: &[i32], b: &[i32]) -> Vec<i32> {
		a.iter().zip(b).map(|(x, y)| x - y).collect()
	}

	fn simd_mul(a: &[i32], b: &[i32]) -> Vec<i32> {
		zip_i32x16(a, b, |x, y| x * y, i32::wrapping_mul)
	}

	fn sisd_mul(a: &[i32], b: &[i32]) -> Vec<i32> {
		a.iter().zip(b).map(|(x, y)| x * y).collect()
	}

	fn simd_div(a: &[i32], b: &[i32]) -> Vec<i32> {
		a.iter()
			.zip(b)
			.map(|(x, y)| x.checked_div(*y).unwrap_or(0))
			.collect()
	}

	fn sisd_div(a: &[i32], b: &[i32]) -> Vec<i32> {
		a.iter()
			.zip(b)
			.map(|(x, y)| x.checked_div(*y).unwrap())
			.collect()
	}
}

#[test]
fn test_simd_add_i64() {
	let result = Operator::simd_add(
		&[1i64, 2i64, 3i64, 4i64, 5i64, 6i64, 7i64, 8i64],
		&[8i64, 7i64, 6i64, 5i64, 4i64, 3i64, 2i64, 1i64],
	);
	assert_eq!(result, [9, 9, 9, 9, 9, 9, 9, 9]);
}

#[test]
fn test_simd_sub_i64() {
	let result = Operator::simd_sub(
		&[1i64, 2i64, 3i64, 4i64, 5i64, 6i64, 7i64, 8i64],
		&[1i64, 2i64, 3i64, 4i64, 5i64, 6i64, 7i64, 8i64],
	);
	assert_eq!(result, [0, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn test_sisd_mul_i64() {
	let result = Operator::simd_mul(
		&[1i64, 2i64, 3i64, 4i64, 5i64, 6i64, 7i64, 8i64],
		&[1i64, 2i64, 3i64, 4i64, 5i64, 6i64, 7i64, 8i64],
	);
	assert_eq!(result, [1, 4, 9, 16, 25, 36, 49, 64]);
}

#[test]
fn test_sisd_div_i64() {
	let result = Operator::sisd_div(
		&[1i64, 2i64, 3i64, 4i64, 5i64, 6i64, 7i64, 8i64],
		&[1i64, 2i64, 3i64, 4i64, 5i64, 6i64, 7i64, 8i64],
	);
	assert_eq!(result, [1, 1, 1, 1, 1, 1, 1, 1]);
}

#[test]
fn test_simd_add_i64_remainder() {
	let a: Vec<i64> = (0..11).collect();
	assert_eq!(
		Operator::simd_add(&a, &a),
		(0..11).map(|x| x * 2).collect::<Vec<i64>>()
	);
}
