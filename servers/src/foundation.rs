use crate::core::core::{KernelFeatures, OutputFeatures};
use crate::core::global::{get_foundation_path};
use crate::mining::mine_block::create_coinbase;
use crate::mining::mine_block::{BlockFees, CbData};
use std::error::Error;
use std::fs::File;
use std::io::prelude::*;
use std::io::SeekFrom;
use std::path::Path;

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
		match create_coinbase(&wallet_listener_url, &block_fees) {
			Err(_) => {
				panic!(format!(
					"Failed to get coinbase from {}. Is the wallet listening?",
					wallet_listener_url
				));
			}
			Ok(foundation) => {
				//foundation.output.features = OutputFeatures::Coinbase;
				//foundation.kernel.features = KernelFeatures::Coinbase;
				result.push(foundation);
			}
		}
	}
	result
}
