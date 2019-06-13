use simdeez::{
	*,
	avx2::*,
	sse2::*,
	sse41::*,
	scalar::*,
};

use enum_iterator::IntoEnumIterator;

use super::OP_T;

#[derive(Debug, Copy, Clone, PartialEq, IntoEnumIterator)]
pub enum Operator {
	Add,
	Sub,
	Mul,
	Div,
}

impl ::std::fmt::Display for Operator {
	fn fmt(&self, w: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
		w.write_str(match *self {
			Operator::Add => "+",
			Operator::Sub => "-",
			Operator::Mul => "*",
			Operator::Div => "/",
		})
	}
}

impl Operator {
	pub fn perform(&self, a: Vec<OP_T>, b: Vec<OP_T>) -> Option<Vec<OP_T>> {
		match self {
			Operator::Add => return Some(Operator::simd_add(&a[..], &b[..])),
			Operator::Sub => return Some(Operator::simd_sub(&a[..], &b[..])),
			Operator::Mul => return Some(Operator::simd_mul(&a[..], &b[..])),
			Operator::Div => return Some(Operator::simd_div(&a[..], &b[..])),
		}
	}
}

trait SimdOperator<T> {
	fn simd_add(a: &[T], b: &[T]) -> Vec<T>;
	fn simd_sub(a: &[T], b: &[T]) -> Vec<T>;
	fn simd_mul(a: &[T], b: &[T]) -> Vec<T>;
	fn simd_div(a: &[T], b: &[T]) -> Vec<T>;
}

impl SimdOperator<i64> for Operator {
	fn simd_add(a: &[i64], b: &[i64]) -> Vec<i64> {
		return simd_add_i64_compiletime(a, b);
	}
	fn simd_sub(a: &[i64], b: &[i64]) -> Vec<i64> {
		return simd_sub_i64_compiletime(a, b);
	}
	fn simd_mul(a: &[i64], b: &[i64]) -> Vec<i64> {
		return simd_mul_i64_compiletime(a, b);
	}
	fn simd_div(a: &[i64], b: &[i64]) -> Vec<i64> {
		return simd_div_i64_compiletime(a, b);
	}
}

simd_compiletime_generate!(
fn simd_add_i64(a: &[i64], b: &[i64]) -> Vec<i64> {
	let mut result: Vec<i64> = Vec::with_capacity(a.len());
	result.set_len(a.len());
	for i in (0..a.len()).step_by(S::VI64_WIDTH) {
		let mut av = S::loadu_epi64(&a[i]);
		let bv = S::loadu_epi64(&b[i]);
		av += bv;
		S::storeu_epi64(&mut result[i], av);
	}
	return result
});

simd_compiletime_generate!(
fn simd_sub_i64(a: &[i64], b: &[i64]) -> Vec<i64> {
	let mut result: Vec<i64> = Vec::with_capacity(a.len());
	result.set_len(a.len());
	for i in (0..a.len()).step_by(S::VI64_WIDTH) {
		let mut av = S::loadu_epi64(&a[i]);
		let bv = S::loadu_epi64(&b[i]);
		av -= bv;
		S::storeu_epi64(&mut result[i], av);
	}
	return result
});

// No mul/div intrinsics for i64X4 yet ...
simd_compiletime_generate!(
fn simd_mul_i64(a: &[i64], b: &[i64]) -> Vec<i64> {
	return a.iter().zip(b).map(|(x, y)| x * y).collect()
});

simd_compiletime_generate!(
fn simd_div_i64(a: &[i64], b: &[i64]) -> Vec<i64> {
	return a.iter().zip(b).map(|(x, y)| x / y).collect()
});

#[test]
fn test_simd_add_i64() {
	let result = simd_add_i64_compiletime(&[1,2,3,4,5,6,7,8], &[8,7,6,5,4,3,2,1]);
	assert!(result == [9,9,9,9,9,9,9,9], format!("Test result was: {:?}", result));
}

#[test]
fn test_simd_sub_i64() {
	let result = simd_sub_i64_compiletime(&[1,2,3,4,5,6,7,8], &[1,2,3,4,5,6,7,8]);
	assert!(result == [0,0,0,0,0,0,0,0], format!("Test result was: {:?}", result));
}

#[test]
fn test_simd_mul_i64() {
	let result = simd_mul_i64_compiletime(&[1,2,3,4,5,6,7,8], &[1,2,3,4,5,6,7,8]);
	assert!(result == [1,4,9,16,25,36,49,64], format!("Test result was: {:?}", result));
}

#[test]
fn test_simd_div_i64() {
	let result = simd_div_i64_compiletime(&[1,2,3,4,5,6,7,8], &[1,2,3,4,5,6,7,8]);
	assert!(result == [1,1,1,1,1,1,1,1], format!("Test result was: {:?}", result));
}
