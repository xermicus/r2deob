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

#[polkavm_derive::polkavm_export]
extern "C" fn mba(a: u64, b: u64, c: u64) -> u64 {
	let x = (a ^ b).wrapping_add((a & b).wrapping_shl(1));
	let y = (x | c)
		.wrapping_mul(x & c)
		.wrapping_add((x & !c).wrapping_mul(!x & c));
	let z = (a | b).wrapping_sub(a & b);
	y.wrapping_sub(z)
}

#[polkavm_derive::polkavm_export]
extern "C" fn rsa(m: u64, n: u64) -> u64 {
	if n == 0 {
		return 0;
	}
	let n = u128::from(n);
	let mut base = u128::from(m) % n;
	let mut exponent = 65537u32;
	let mut result = 1 % n;
	while exponent > 0 {
		if exponent & 1 == 1 {
			result = result * base % n;
		}
		base = base * base % n;
		exponent >>= 1;
	}
	result as u64
}

#[polkavm_derive::polkavm_export]
extern "C" fn mba2(a: u64, b: u64, c: u64, d: u64) -> u64 {
	let sum = (a | b).wrapping_add(a & b);
	let xor = (c | d).wrapping_sub(c & d);
	let and = a.wrapping_add(d).wrapping_sub(a | d);
	let product = (sum | xor)
		.wrapping_mul(sum & xor)
		.wrapping_add((sum & !xor).wrapping_mul(!sum & xor));
	product.wrapping_sub(and)
}
