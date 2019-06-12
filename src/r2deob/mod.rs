pub mod engine;
pub mod synth_tree;
pub mod synth_evoasm;
pub mod sat_interface;
pub mod ast;
pub mod score;
pub mod calc;

pub type OP_T = i64;

pub enum R2Error {
	PipeFail,
	CmdFail,
}
