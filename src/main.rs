mod r2deob;

fn main() {
	let target = r2deob::engine::FcnConfig {
		loc: "sym.calc".to_string(),
		len: "12".to_string(),
		input_regs: vec!["esi".to_string(),"edi".to_string()],
		output_reg: "rax".to_string()
	};

	let mut session = r2deob::engine::Session::init("/home/cyrill/r2deob/a.out".to_string(), target).unwrap();

	println!("{:?}", session.add_trace());
	session.deobfuscate();
}
