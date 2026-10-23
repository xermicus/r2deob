use r2deob::engine::{FcnConfig, Session, Synthesiser};

fn main() {
	let target = FcnConfig {
		path: concat!(env!("CARGO_MANIFEST_DIR"), "/calc.polkavm").to_string(),
		export: "calc".to_string(),
		input_regs: vec!["a0".to_string(), "a1".to_string()],
		output_reg: "a0".to_string(),
	};

	let mut session = match Session::init(target) {
		Ok(session) => session,
		Err(error) => {
			eprintln!("{error}");
			std::process::exit(1);
		}
	};
	for _ in 0..16 {
		if let Err(error) = session.add_trace() {
			eprintln!("{error}");
			std::process::exit(1);
		}
	}
	match session.deobfuscate(Synthesiser::Tree) {
		Some(expression) => println!("Winner! {}", expression.math_notation()),
		None => println!("No candidate found"),
	}
}
