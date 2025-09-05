#[cfg(test)]
mod tests {
	use epic_api::CommitmentWrapper;
	use epic_util as util;
	use util::secp::pedersen::Commitment;

	#[test]
	fn test_commitment_to_hex() {
		// Example commitment data
		let commitment_data = vec![
			0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
			0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x01, 0x02, 0x03, 0x04,
			0x05, 0x06, 0x07, 0x08,
		];

		// Create a commitment from the data
		let commitment = Commitment::from_vec(commitment_data.clone());

		// Wrap the commitment
		let wrapper = CommitmentWrapper(commitment);

		// Convert to hexadecimal
		let hex_string = wrapper.to_hex();

		// Adjust the expected data to match the fixed size of the commitment
		let mut padded_data = vec![0; util::secp::constants::PEDERSEN_COMMITMENT_SIZE];
		for i in 0..commitment_data
			.len()
			.min(util::secp::constants::PEDERSEN_COMMITMENT_SIZE)
		{
			padded_data[i] = commitment_data[i];
		}

		// Expected hexadecimal string
		let expected_hex = util::to_hex(padded_data);

		// Assert the result matches the expected value
		assert_eq!(hex_string, expected_hex);
	}
}
