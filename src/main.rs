extern crate rsmt2;
mod r2deob;

use rsmt2::SmtRes;
use rsmt2::SmtConf;
use rsmt2::Solver;

fn main() {
	let target = r2deob::engine::FcnConfig {
		path: "/home/cyrill/r2deob/a.out".to_string(),
		loc: "sym.calc".to_string(),
		len: "12".to_string(),
		input_regs: vec!["esi".to_string(),"edi".to_string()],
		output_reg: "rax".to_string()
	};

	let mut session = r2deob::engine::Session::init(target).unwrap();

	println!("{:?}", session.add_trace());
	println!("{:?}", session.add_trace());
	println!("{:?}", session.add_trace());
	println!("{:?}", session.add_trace());
	println!("{:?}", session.add_trace());
	println!("{:?}", session.add_trace());
	println!("{:?}", session.add_trace());
	println!("{:?}", session.add_trace());
	session.deobfuscate();

	let mut solver = Solver::default(()).unwrap();
	
	solver.declare_const("n", "Int").unwrap();
	//     ^^^^^^^^^^^^^~~~ same as `declare-fun` for a nullary symbol
	solver.declare_const("m", "Int").unwrap();
	solver.assert("(= (+ (* n n) (* m m)) 7)").unwrap();
	
	let is_sat = solver.check_sat().unwrap();
	assert! { ! is_sat };
	//let model = solver.get_model_const();
	//println!("{:?}", solver);
}
