use crate::mining::mine_block::create_foundation as c_foundation;
use crate::mining::mine_block::{BlockFees, CbData};
use crate::core::consensus;

/// Call the wallet API to create a given number of foundations coinbases (output/kernel)
pub fn create_foundation(
	wallet_listener_url: &str,
	num_to_generate: u64,
	height_gen: u64,
) -> Vec<CbData> {
	let fees = 0;
	let key_id = None;
	let height = height_gen;
	let mut block_fees = BlockFees {
		fees,
		key_id,
		height,
	};
	let mut result: Vec<CbData> = vec![];
	for _ in 0..num_to_generate {
		println!("Generating a foundation reward at height of: {:?}", block_fees.height);
		match c_foundation(&wallet_listener_url, &block_fees) {
			Err(_) => {
				panic!(format!(
					"Failed to get coinbase from {}. Is the wallet listening?",
					wallet_listener_url
				));
			}
			Ok(foundation) => {
				result.push(foundation);
			}
		}
		block_fees.height += consensus::foundation_height();
	}
	result
}
