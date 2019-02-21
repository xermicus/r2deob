use std::cmp;

#[derive(Debug, PartialEq)]
pub enum Score {
	HammingDistance(f32),
	AbsDistance(f32),
	RangeDistance(f32),
	Combined(f32),
	UnSat,
}

impl Score {
	pub fn hamming_distance(result_test: u64, result_true: u64) -> Score {
		Score::HammingDistance(1.0 - (result_test ^ result_true).count_ones() as f32 / 64.0)
	}

	pub fn abs_distance(result_test: u64, result_true: u64) -> Score {
		Score::AbsDistance((cmp::min(result_test, result_true) as f64 / cmp::max(result_test, result_true) as f64) as f32)
	}

	pub fn range_distance(result_test: u64, result_true: u64) -> Score {
		let bytes_test = result_test.to_le_bytes();
		let bytes_true = result_true.to_le_bytes();
		let mut result = 0;
		for i in 0..bytes_test.len() {
			result += 1;
			if bytes_test[i] != bytes_true[i] {
				break
			}
		}
		Score::RangeDistance(1.0 - result as f32 / 8.0)
	}

	pub fn combined(result_test: u64, result_true: u64) -> Score {
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
}

#[test]
fn score_test() {
	assert_eq!(Score::HammingDistance(0.96875), Score::hamming_distance(3, 5));
	assert_eq!(Score::AbsDistance(0.6), Score::abs_distance(3, 5));
	assert_eq!(Score::RangeDistance(0.875), Score::range_distance(3, 5));
	assert_eq!(Score::Combined(0.8145833), Score::combined(3, 5));
}
