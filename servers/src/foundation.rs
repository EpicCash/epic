use crate::core::core::{KernelFeatures, OutputFeatures};
use crate::core::global::{get_foundation_path};
use crate::mining::mine_block::create_coinbase;
use crate::mining::mine_block::{BlockFees, CbData};
use std::error::Error;
use std::fs::File;
use std::io::prelude::*;
use std::io::SeekFrom;
use std::path::Path;

// Size in bytes of each foundation coinbase (Output + Kernel)
pub const FOUNDATION_COINBASE_SIZE: usize = 1807;

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
			Ok(mut foundation) => {
				foundation.output.features = OutputFeatures::Foundation;
				foundation.kernel.features = KernelFeatures::Foundation;
				result.push(foundation);
			}
		}
	}
	result
}

// Serialize a vector of foundation coinbases in a series of json
pub fn serialize_foundation(foundation_coinbases: Vec<CbData>) -> String {
	let mut result = String::new();
	for f_cb in foundation_coinbases {
		let serialized = serde_json::to_string(&f_cb).unwrap();
		result.push_str(&serialized);
		result.push_str("\n"); // to put each json in a line
	}
	result
}

// Save the serialization of the foundation coinbases in the disk with the extension .json
pub fn save_in_disk(serialization: String, path: &Path) {
	let mut path = path.join("foundation");
	path = path.join("foundation.json");
	println!("Saving the file as: {}", path.display());
	let mut file = match File::create(&path) {
		Err(why) => panic!("Couldn't create {}: {}", path.display(), why.description()),
		Ok(file) => file,
	};
	file.write_all(serialization.as_bytes())
		.expect("Couldn't save the serialization in the disk!")
}

// Load the foundation coinbase relative to the height of the chain
pub fn load_from_disk(height: u64) -> CbData {
	let path_str = get_foundation_path().unwrap_or_else(|| panic!("No path to the foundation.json was provided!"));
	let path = Path::new(&path_str);
	println!("path: {:?}", path.display());
	let mut file = match File::open(&path) {
		Err(why) => panic!(
			"Error trying to read the foundation coinbase. Couldn't open the file {}: {}",
			path.display(),
			why.description()
		),
		Ok(file) => file,
	};
	let file_len = file.metadata().unwrap().len();
	assert_eq!(
		file_len % (FOUNDATION_COINBASE_SIZE as u64),
		0,
		"The file {} has an invalid size! The size should be multiple of {}",
		path.display(),
		FOUNDATION_COINBASE_SIZE
	);
	let offset = height * (FOUNDATION_COINBASE_SIZE as u64);
	if offset >= file_len {
		// TODO: What should we do when the foundations blocks ends ?
		panic!("Not implemented yet!");
	};
	let mut buffer = vec![0 as u8; FOUNDATION_COINBASE_SIZE];
	file.seek(SeekFrom::Start(offset)).unwrap();
	file.read_exact(&mut buffer).unwrap();
	let buffer_str = String::from_utf8(buffer).unwrap();
	serde_json::from_str(&buffer_str).unwrap()
}
