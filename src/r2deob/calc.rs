use enum_iterator::IntoEnumIterator;
use packed_simd::{
	i64x8,
	i32x16,
};

use super::BaseT;

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
	pub fn perform(&self, a: &[BaseT], b: &[BaseT]) -> Option<Vec<BaseT>> {
		match self {
			Operator::Add => return Some(Operator::simd_add(&a[..], &b[..])),
			Operator::Sub => return Some(Operator::simd_sub(&a[..], &b[..])),
			Operator::Mul => return Some(Operator::simd_mul(&a[..], &b[..])),
			Operator::Div => return Some(Operator::simd_div(&a[..], &b[..])),
		}
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

impl SimdOperator<i64> for Operator {
	fn simd_add(a: &[i64], b: &[i64]) -> Vec<i64> {
		let mut result: Vec<i64> = Vec::with_capacity(a.len());
		unsafe { result.set_len(a.len()); }
		let mut result_chunk = result.chunks_exact_mut(8);
		for elem in a.chunks_exact(8)
			.map(i64x8::from_slice_unaligned)
			.zip(b.chunks_exact(8).map(i64x8::from_slice_unaligned))
			.map(|(a, b)| a + b) {
			elem.write_to_slice_unaligned(&mut result_chunk.next().unwrap());
		}
		result
	}

	fn sisd_add(a: &[i64], b: &[i64]) -> Vec<i64> {
		a.iter().zip(b).map(|(x,y)| x + y).collect()
	}

	fn simd_sub(a: &[i64], b: &[i64]) -> Vec<i64> {
		let mut result: Vec<i64> = Vec::with_capacity(a.len());
		unsafe { result.set_len(a.len()); }
		let mut result_chunk = result.chunks_exact_mut(8);
		for elem in a.chunks_exact(8)
			.map(i64x8::from_slice_unaligned)
			.zip(b.chunks_exact(8).map(i64x8::from_slice_unaligned))
			.map(|(a, b)| a - b) {
			elem.write_to_slice_unaligned(&mut result_chunk.next().unwrap());
		}
		result
	}

	fn sisd_sub(a: &[i64], b: &[i64]) -> Vec<i64> {
		a.iter().zip(b).map(|(x,y)| x - y).collect()
	}

	fn simd_mul(a: &[i64], b: &[i64]) -> Vec<i64> {
		let mut result: Vec<i64> = Vec::with_capacity(a.len());
		unsafe { result.set_len(a.len()); }
		let mut result_chunk = result.chunks_exact_mut(8);
		for elem in a.chunks_exact(8)
			.map(i64x8::from_slice_unaligned)
			.zip(b.chunks_exact(8).map(i64x8::from_slice_unaligned))
			.map(|(a, b)| a * b) {
			elem.write_to_slice_unaligned(&mut result_chunk.next().unwrap());
		}
		result
	}

	fn sisd_mul(a: &[i64], b: &[i64]) -> Vec<i64> {
		return a.iter().zip(b).map(|(x, y)| x * y).collect()
	}

	fn simd_div(a: &[i64], b: &[i64]) -> Vec<i64> {
		// TODO how to checked_div ?
		//let mut result: Vec<i64> = Vec::with_capacity(a.len());
		//unsafe { result.set_len(a.len()); }
		//let mut result_chunk = result.chunks_exact_mut(8);
		//for elem in a.chunks_exact(8)
		//	.map(i64x8::from_slice_unaligned)
		//	.zip(b.chunks_exact(8).map(i64x8::from_slice_unaligned))
		//	.map(|(a, b)| a / b) {
		//	elem.write_to_slice_unaligned(&mut result_chunk.next().unwrap());
		//}
		//result
		return a.iter().zip(b).map(|(x, y)| if let Some(v) = x.checked_div(*y) { v } else { 0 }).collect()
	}

	fn sisd_div(a: &[i64], b: &[i64]) -> Vec<i64> {
		return a.iter().zip(b).map(|(x, y)| x.checked_div(*y).unwrap()).collect()
	}
}

impl SimdOperator<i32> for Operator {
	fn simd_add(a: &[i32], b: &[i32]) -> Vec<i32> {
		let mut result: Vec<i32> = Vec::with_capacity(a.len());
		unsafe { result.set_len(a.len()); }
		let mut result_chunk = result.chunks_exact_mut(16);
		for elem in a.chunks_exact(16)
			.map(i32x16::from_slice_unaligned)
			.zip(b.chunks_exact(16).map(i32x16::from_slice_unaligned))
			.map(|(a, b)| a + b) {
			elem.write_to_slice_unaligned(&mut result_chunk.next().unwrap());
		}
		result
	}

	fn sisd_add(a: &[i32], b: &[i32]) -> Vec<i32> {
		a.iter().zip(b).map(|(x,y)| x + y).collect()
	}

	fn simd_sub(a: &[i32], b: &[i32]) -> Vec<i32> {
		let mut result: Vec<i32> = Vec::with_capacity(a.len());
		unsafe { result.set_len(a.len()); }
		let mut result_chunk = result.chunks_exact_mut(16);
		for elem in a.chunks_exact(16)
			.map(i32x16::from_slice_unaligned)
			.zip(b.chunks_exact(16).map(i32x16::from_slice_unaligned))
			.map(|(a, b)| a - b) {
			elem.write_to_slice_unaligned(&mut result_chunk.next().unwrap());
		}
		result
	}

	fn sisd_sub(a: &[i32], b: &[i32]) -> Vec<i32> {
		a.iter().zip(b).map(|(x,y)| x - y).collect()
	}

	fn simd_mul(a: &[i32], b: &[i32]) -> Vec<i32> {
		let mut result: Vec<i32> = Vec::with_capacity(a.len());
		unsafe { result.set_len(a.len()); }
		let mut result_chunk = result.chunks_exact_mut(16);
		for elem in a.chunks_exact(16)
			.map(i32x16::from_slice_unaligned)
			.zip(b.chunks_exact(16).map(i32x16::from_slice_unaligned))
			.map(|(a, b)| a * b) {
			elem.write_to_slice_unaligned(&mut result_chunk.next().unwrap());
		}
		result
	}

	fn sisd_mul(a: &[i32], b: &[i32]) -> Vec<i32> {
		return a.iter().zip(b).map(|(x, y)| x * y).collect()
	}

	fn simd_div(a: &[i32], b: &[i32]) -> Vec<i32> {
		// TODO how to checked_div ?
		return a.iter().zip(b).map(|(x, y)| if let Some(v) = x.checked_div(*y) { v } else { 0 }).collect()
	}

	fn sisd_div(a: &[i32], b: &[i32]) -> Vec<i32> {
		return a.iter().zip(b).map(|(x, y)| x.checked_div(*y).unwrap()).collect()
	}
}

#[test]
fn test_simd_add_i64() {
	let result = Operator::simd_add(&[1i64,2i64,3i64,4i64,5i64,6i64,7i64,8i64], &[8i64,7i64,6i64,5i64,4i64,3i64,2i64,1i64]);
	assert!(result == [9,9,9,9,9,9,9,9], format!("Test result was: {:?}", result));
}

#[test]
fn test_simd_sub_i64() {
	let result = Operator::simd_sub(&[1i64,2i64,3i64,4i64,5i64,6i64,7i64,8i64], &[1i64,2i64,3i64,4i64,5i64,6i64,7i64,8i64]);
	assert!(result == [0,0,0,0,0,0,0,0], format!("Test result was: {:?}", result));
}

#[test]
fn test_sisd_mul_i64() {
	let result = Operator::simd_mul(&[1i64,2i64,3i64,4i64,5i64,6i64,7i64,8i64], &[1i64,2i64,3i64,4i64,5i64,6i64,7i64,8i64]);
	assert!(result == [1,4,9,16,25,36,49,64], format!("Test result was: {:?}", result));
}

#[test]
fn test_sisd_div_i64() {
	let result = Operator::sisd_div(&[1i64,2i64,3i64,4i64,5i64,6i64,7i64,8i64], &[1i64,2i64,3i64,4i64,5i64,6i64,7i64,8i64]);
	assert!(result == [1,1,1,1,1,1,1,1], format!("Test result was: {:?}", result));
}
