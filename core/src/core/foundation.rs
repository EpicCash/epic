use crate::consensus::{first_fork_height, foundation_height, foundation_index, header_version};
use crate::core::{HeaderVersion, Output, TxKernel};
use crate::global::get_foundation_path;
use crate::keychain::Identifier;
use crate::serde::{Deserialize, Serialize};
use serde_json;
use std::error::Error;
use std::fs::{create_dir, File};
use std::io::prelude::*;
use std::io::SeekFrom;
use std::path::Path;

/// Response to build a coinbase output.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CbData {
	/// Output
	pub output: Output,
	/// Kernel
	pub kernel: TxKernel,
	/// Key Id
	pub key_id: Option<Identifier>,
}

/// Size in bytes of each foundation coinbase (Output + Kernel)
pub const FOUNDATION_COINBASE_SIZE_1: usize = 1803;
pub const FOUNDATION_COINBASE_SIZE_2: usize = 1775;

// TODO-FOUNDATION : Create a function to verify if the file exists if the height is different form 0 in the CLI

/// Serialize a vector of foundation coinbases in a series of json
pub fn serialize_foundation(foundation_coinbases: Vec<CbData>) -> String {
	let mut result = String::new();
	for f_cb in foundation_coinbases {
		let serialized = serde_json::to_string(&f_cb).unwrap();
		result.push_str(&serialized);
		result.push_str("\n"); // to put each json in a line
	}
	result
}

/// Save the serialization of the foundation coinbases in the disk with the extension .json
pub fn save_in_disk(serialization: String, path: &Path) {
	let mut path = path.join("foundation");
	if path.exists() == false {
		create_dir(path.clone())
			.expect(format!("Was not possible to create the file {:?}", path).as_str());
	};
	path = path.join("foundation.json");
	println!("Saving the file as: {}", path.display());
	let mut file = match File::create(&path) {
		Err(why) => panic!("Couldn't create {}: {}", path.display(), why.description()),
		Ok(file) => file,
	};
	file.write_all(serialization.as_bytes())
		.expect("Couldn't save the serialization in the disk!")
}

fn get_tx_version_size(version: HeaderVersion) -> usize {
	match version {
		HeaderVersion(6) => FOUNDATION_COINBASE_SIZE_1,
		HeaderVersion(7) => FOUNDATION_COINBASE_SIZE_2,
		HeaderVersion(_) => panic!("YOU NEED UPDATE YOUR NODE!"),
	}
}

fn get_tx_offset(index: u64, version: HeaderVersion) -> u64 {
	let size = match version {
		HeaderVersion(6) => index * (FOUNDATION_COINBASE_SIZE_1 as u64),
		HeaderVersion(7) => {
			let fork_height = first_fork_height();
			let fork_index = foundation_index(fork_height);
			let index = index.saturating_sub(fork_index);

			let size_1 = fork_index * (FOUNDATION_COINBASE_SIZE_1 as u64);
			let size_2 = index * (FOUNDATION_COINBASE_SIZE_2 as u64);

			size_1 + size_2
		}
		HeaderVersion(_) => panic!("YOU NEED UPDATE YOUR NODE!"),
	};

	if cfg!(windows) {
		size + index
	} else {
		size
	}
}

/// Load the foundation coinbase relative to the height of the chain
pub fn load_foundation_output(height: u64) -> CbData {
	let height_version = header_version(height);
	let height = foundation_index(height);

	let path_str = get_foundation_path()
		.unwrap_or_else(|| panic!("No path to the foundation.json was provided!"));

	let path = Path::new(&path_str);
	let mut file = match File::open(&path) {
		Err(why) => panic!(
			"Error trying to read the foundation coinbase. Couldn't open the file {}: {}",
			path.display(),
			why.description()
		),
		Ok(file) => file,
	};
	let file_len = file.metadata().unwrap().len();

	// Checks if the file has its size multiple of 1 json
	// Each json has to have a fixed size in bytes (FOUNDATION_COINBASE_SIZE) for the reading occurs successfully.
	let offset = get_tx_offset(height, height_version);

	if offset >= file_len {
		// TODO: What should we do when the foundations blocks ends ?
		panic!("Not implemented yet!");
	};

	let mut buffer = vec![0 as u8; get_tx_version_size(height_version)];
	file.seek(SeekFrom::Start(offset)).unwrap();
	file.read_exact(&mut buffer).unwrap();
	let buffer_str = String::from_utf8(buffer).unwrap();
	serde_json::from_str(&buffer_str).unwrap()
}
