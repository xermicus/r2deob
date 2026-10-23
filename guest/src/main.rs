#![no_std]
#![no_main]

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
	unsafe { core::arch::asm!("unimp", options(noreturn)) }
}

#[polkavm_derive::polkavm_export]
extern "C" fn calc(a: i32, b: i32) -> i32 {
	let d = b.wrapping_mul(2);
	a.wrapping_add(d)
}

#[polkavm_derive::polkavm_export]
extern "C" fn add(a: i32, b: i32) -> i32 {
	a.wrapping_add(b)
}
