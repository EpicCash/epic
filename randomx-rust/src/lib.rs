extern crate bigint;
extern crate byteorder;
extern crate libc;

pub mod ffi;
pub mod types;
pub mod utils;

use bigint::uint::U256;
use byteorder::{BigEndian, ByteOrder};
use libc::c_void;

use ffi::randomx_calculate_hash;

pub use types::{RxState, RxVM};

pub fn calculate(vm: &RxVM, input: &mut [u8], nonce: u64) -> U256 {
	let mut result: [u8; 32] = [0; 32];
	let input_size = input.len();

	let mut nonce_bytes = [0; 8];
	BigEndian::write_u64(&mut nonce_bytes, nonce);

	// first example
	for i in 0..nonce_bytes.len() {
		input[input_size - (nonce_bytes.len() - i)] = nonce_bytes[i];
	}

	// after test it
	// let mut s_input: Vec<u8> = input.into_iter()
	//	.take(input_size - 8)
	//	.chain(&mut nonce_bytes)
	//	.collect::<Vec<u8>>();

	unsafe {
		randomx_calculate_hash(
			vm.vm,
			input.as_ptr() as *const c_void,
			input_size,
			result.as_mut_ptr() as *mut c_void,
		);
	}

	result.into()
}

pub fn slow_hash(state: &mut RxState, data: &[u8], seed: &[u8; 32]) -> U256 {
	let vm = {
		state.init_cache(seed, false).expect("seed not initialized");
		state.create_vm().expect("vm not initialized")
	};

	let hash_target = unsafe {
		let mut hash: [u8; 32] = [0; 32];

		ffi::randomx_calculate_hash(
			vm.vm,
			data.as_ptr() as *const c_void,
			data.len(),
			hash.as_mut_ptr() as *mut c_void,
		);

		hash.into()
	};

	hash_target
}

#[cfg(test)]
mod test {
	use super::utils::*;
	use super::*;
	use crate::utils::*;

	#[test]
	fn test_verify() {
		let hash: U256 = [
			58, 219, 87, 205, 58, 5, 219, 157, 210, 19, 148, 114, 219, 191, 100, 122, 49, 51, 224,
			67, 83, 184, 50, 73, 105, 255, 58, 230, 35, 20, 232, 244,
		]
		.into();
		let block_template: [u8; 128] = [0; 128];
		let seed: [u8; 32] = [0; 32];

		let mut rx_state = RxState::new();

		assert_eq!(hash, slow_hash(&mut rx_state, &block_template, &seed));
	}
}
