pub mod engine;
pub mod synth_tree;
pub mod synth_evoasm;
pub mod sat_interface;
pub mod ast;

pub enum R2Error {
	PipeFail,
	CmdFail,
}
