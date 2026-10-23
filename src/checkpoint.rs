use std::io::{self, Read, Write};

use crate::{
	eval::{Lanes, SAMPLES, Samples, Word},
	expr::{BinaryOp, Token, UnaryOp},
};

const MAGIC: &[u8; 7] = b"r2deob\0";
const VERSION: u8 = 2;

/// The part of a checkpoint preceding the trees.
pub struct Header<W> {
	pub version: u8,
	pub productions: Vec<Token>,
	pub max_len: usize,
	pub samples: Samples<W>,
	pub verification: Samples<W>,
}

fn invalid(message: &str) -> io::Error {
	io::Error::new(
		io::ErrorKind::InvalidData,
		format!("invalid checkpoint: {message}"),
	)
}

pub fn write_u32(out: &mut impl Write, value: u32) -> io::Result<()> {
	out.write_all(&value.to_le_bytes())
}

pub fn write_u64(out: &mut impl Write, value: u64) -> io::Result<()> {
	out.write_all(&value.to_le_bytes())
}

pub fn read_u8(input: &mut impl Read) -> io::Result<u8> {
	let mut bytes = [0; 1];
	input.read_exact(&mut bytes)?;
	Ok(bytes[0])
}

pub fn read_u32(input: &mut impl Read) -> io::Result<u32> {
	let mut bytes = [0; 4];
	input.read_exact(&mut bytes)?;
	Ok(u32::from_le_bytes(bytes))
}

pub fn read_u64(input: &mut impl Read) -> io::Result<u64> {
	let mut bytes = [0; 8];
	input.read_exact(&mut bytes)?;
	Ok(u64::from_le_bytes(bytes))
}

fn encode(token: Token) -> [u8; 2] {
	let index = |found: Option<usize>| found.expect("operation listed in ALL") as u8;
	match token {
		Token::Leaf(leaf) => [0, leaf],
		Token::Unary(op) => [1, index(UnaryOp::ALL.iter().position(|&x| x == op))],
		Token::Binary(op) => [2, index(BinaryOp::ALL.iter().position(|&x| x == op))],
	}
}

fn decode([kind, value]: [u8; 2]) -> io::Result<Token> {
	match kind {
		0 => Some(Token::Leaf(value)),
		1 => UnaryOp::ALL
			.get(usize::from(value))
			.map(|&op| Token::Unary(op)),
		2 => BinaryOp::ALL
			.get(usize::from(value))
			.map(|&op| Token::Binary(op)),
		_ => None,
	}
	.ok_or_else(|| invalid("unknown token"))
}

fn write_lanes<W: Word>(out: &mut impl Write, lanes: &Lanes<W>) -> io::Result<()> {
	lanes
		.iter()
		.try_for_each(|value| write_u64(out, value.to_reg()))
}

fn read_lanes<W: Word>(input: &mut impl Read) -> io::Result<Lanes<W>> {
	let mut lanes = [W::default(); SAMPLES];
	for lane in &mut lanes {
		*lane = W::from_reg(read_u64(input)?);
	}
	Ok(lanes)
}

pub fn write_header<W: Word>(
	out: &mut impl Write,
	productions: &[Token],
	max_len: usize,
	batches: [&Samples<W>; 2],
) -> io::Result<()> {
	out.write_all(MAGIC)?;
	out.write_all(&[VERSION])?;
	write_u32(out, W::BITS)?;
	write_u32(out, SAMPLES as u32)?;
	write_u32(out, max_len as u32)?;
	write_u32(out, productions.len() as u32)?;
	for &token in productions {
		out.write_all(&encode(token))?;
	}
	for samples in batches {
		for lanes in samples.columns.iter().chain([&samples.output]) {
			write_lanes(out, lanes)?;
		}
	}
	Ok(())
}

pub fn read_header<W: Word>(input: &mut impl Read) -> io::Result<Header<W>> {
	let mut magic = [0; 7];
	input.read_exact(&mut magic)?;
	if &magic != MAGIC {
		return Err(invalid("not an r2deob checkpoint"));
	}
	let version = read_u8(input)?;
	if !(1..=VERSION).contains(&version) {
		return Err(invalid("unsupported version"));
	}
	if read_u32(input)? != W::BITS {
		return Err(invalid("different width"));
	}
	if read_u32(input)? as usize != SAMPLES {
		return Err(invalid("different sample count"));
	}
	let max_len = read_u32(input)? as usize;
	let productions = (0..read_u32(input)?)
		.map(|_| {
			let mut bytes = [0; 2];
			input.read_exact(&mut bytes)?;
			decode(bytes)
		})
		.collect::<io::Result<Vec<_>>>()?;
	let leaves = productions
		.iter()
		.filter(|token| token.arity() == 0)
		.count();
	let mut batch = || -> io::Result<Samples<W>> {
		let columns = (0..leaves)
			.map(|_| read_lanes(input))
			.collect::<io::Result<_>>()?;
		Ok(Samples::from_columns(columns, read_lanes(input)?))
	};
	let samples = batch()?;
	let verification = batch()?;
	Ok(Header {
		version,
		productions,
		max_len,
		samples,
		verification,
	})
}
