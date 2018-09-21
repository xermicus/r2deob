extern crate rsmt2;
mod r2deob;

fn main() {
	let target = r2deob::engine::FcnConfig {
		path: "/home/cyrill/r2deob/calc".to_string(),
		loc: "sym.calc".to_string(),
		len: "12".to_string(),
		input_regs: vec!["esi".to_string(),"edi".to_string()],
		output_reg: "rax".to_string()
	};

	let mut session = r2deob::engine::Session::init(target).unwrap();

	for _ in 0..20 { session.add_trace(); };
	
	session.deobfuscate(r2deob::engine::Synthesiser::Tree);
}
