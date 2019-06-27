use bigint::uint::U256;
use std::mem::transmute;

pub fn from_u32_to_U256(v: &[u32; 8]) -> U256 {
	let vu64: [u64; 4] = unsafe { transmute(*v) };

	U256(vu64)
}

#[cfg(test)]
mod test {
	use bigint::uint::U256;
	#[test]
	fn test_u32_to_u256() {
		let v = [1; 8];
		let result = crate::utils::from_u32_to_U256(&v);
		let expected: U256 = (0..8).fold(U256::from(0), |acc, i| {
			acc + U256::from(2).pow(U256::from(i * 32))
		});
		assert_eq!(expected, result);
		assert_eq!([1; 8], v);
	}

	#[test]
	fn test_u32_to_u256_simple() {
		let mut v = [0; 8];
		v[0] = 1;
		let result = crate::utils::from_u32_to_U256(&v);
		let expected = U256::from(1);
		assert_eq!(expected, result);
	}
}
