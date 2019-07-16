use crate::mining::mine_block::create_coinbase as c_foundation;
use crate::mining::mine_block::{BlockFees, CbData};

// Call the wallet API to create a given number of foundations coinbases (output/kernel)
pub fn create_foundation(wallet_listener_url: &str, num_to_generate: u32) -> Vec<CbData> {
	let fees = 0;
	let key_id = None;
	let height = 0;
	let mut block_fees = BlockFees {
		fees,
		key_id,
		height,
	};
	let mut result: Vec<CbData> = vec![];
	for _ in 0..num_to_generate {
		block_fees.height += 1;
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
	}
	result
}
