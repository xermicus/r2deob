pub mod engine;
pub mod synth_tree;
pub mod synth_evoasm;

pub enum R2Error {
	PipeFail,
	AnalFail,
	CmdFail,
}

