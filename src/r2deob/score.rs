use std::cmp;
use crate::r2deob::BaseT;

#[derive(Debug, PartialEq)]
pub enum Score {
	HammingDistance(f32),
	AbsDistance(f32),
	RangeDistance(f32),
	Combined(f32),
	Unknown,
	UnSat,
}

impl Default for Score {
	fn default() -> Self {

		Score::Unknown
	}
}

impl Score {
	fn hamming_distance(
		result_test: BaseT,
		result_true: BaseT)
		-> Score {

		Score::HammingDistance(1.0 - (result_test ^ result_true).count_ones() as f32 / 64.0)
	}

	fn abs_distance(
		result_test: BaseT,
		result_true: BaseT)
		-> Score {

		Score::AbsDistance((
			cmp::min(result_test, result_true) as f64 /
			cmp::max(result_test, result_true) as f64
		) as f32)
	}

	fn range_distance(
		result_test: BaseT,
		result_true: BaseT)
		-> Score {

		let bytes_test = result_test.to_le_bytes();
		let bytes_true = result_true.to_le_bytes();

		let mut result = 0;
		for i in 0..bytes_test.len() {
			if bytes_test[i] == bytes_true[i] {
				result += 1;
			}
		}
		Score::RangeDistance(result as f32 / bytes_test.len() as f32)
	}

	fn combined(
		result_test: BaseT,
		result_true: BaseT)
		-> Score {

		let mut result: f32 = 0.0;
		let mut scores: f32 = 0.0;
		if let Score::HammingDistance(x) = Score::hamming_distance(result_test, result_true) {
			result += x;
			scores += 1.0;
		}
		if let Score::AbsDistance(x) = Score::abs_distance(result_test, result_true) {
			result += x;
			scores += 1.0;
		}
		if let Score::RangeDistance(x) = Score::range_distance(result_test, result_true) {
			result += x;
			scores += 1.0;
		}
		Score::Combined(result / scores)
	}

	pub fn get(
		result_test: &Vec<BaseT>,
		result_true: &Vec<BaseT>)
		-> Score {

		let mut result: f32 = 0.0;
		let mut scores: f32 = 0.0;

		for (r_test, r_true) in result_test
			.iter()
			.zip(result_true) {

			if let Score::Combined(x) = Score::combined(*r_test, *r_true) {
				result += x;
				scores += 1.0;
			}
		}
		Score::Combined(result / scores)
	}
}

#[test]
fn score_test() {
	assert_eq!(Score::HammingDistance(0.96875), Score::hamming_distance(3, 5));
	assert_eq!(Score::AbsDistance(0.6), Score::abs_distance(3, 5));
	assert_eq!(Score::RangeDistance(0.875), Score::range_distance(3, 5));
	assert_eq!(Score::Combined(0.8145833), Score::combined(3, 5));
	assert_eq!(Score::Combined(1.0), Score::get(&vec![1,2,3,4,5,6,7,8], &vec![1,2,3,4,5,6,7,8]));
	assert_eq!(Score::RangeDistance(1.0), Score::range_distance(3, 3));
	assert_eq!(Score::HammingDistance(1.0), Score::hamming_distance(3, 3));
	assert_eq!(Score::AbsDistance(1.0), Score::abs_distance(3, 3));
}
