// Copyright 2018 The Epic Developers
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Foundation coinbase serialization and loading utilities for Epic.

use crate::consensus::foundation_index;
use crate::core::{Output, TxKernel};
use crate::global::get_foundation_path;
use crate::keychain::Identifier;
use crate::serde::{Deserialize, Serialize};
use serde_json;
use std::fs::{create_dir, File};
// use std::path::Path; // Already imported elsewhere or not needed
use std::io::{prelude::*, BufRead, BufReader};
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
pub const FOUNDATION_COINBASE_SIZE_1: usize = 1775;

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
		Err(why) => panic!("Couldn't create {}: {}", path.display(), why.to_string()),
		Ok(file) => file,
	};
	file.write_all(serialization.as_bytes())
		.expect("Couldn't save the serialization in the disk!")
}

/// Load the foundation coinbase relative to the height of the chain
/// Loads the foundation output for the given height.
/// If the foundation.json file exists, loads from file. Otherwise, uses the embedded mainnet content.
use crate::global::{ChainTypes, CHAIN_TYPE};

/// Load the foundation coinbase relative to the height of the chain
/// Loads the foundation output for the given height.
/// If the foundation.json file exists, loads from file. Otherwise, uses the embedded content for the current chain type.
pub fn load_foundation_output(height: u64) -> CbData {
	let index_foundation = foundation_index(height);

	let path_str = get_foundation_path()
		.unwrap_or_else(|| panic!("No path to the foundation.json was provided!"));

	let path = Path::new(&path_str);
	let lines: Vec<String> = if path.exists() {
		// Load from file
		let file = File::open(&path_str).unwrap_or_else(|why| {
			panic!(
				"Error trying to read the foundation coinbase. Couldn't open the file {}: {}",
				path_str, why
			)
		});
		let reader = BufReader::new(file);
		reader.lines().map(|l| l.unwrap()).collect()
	} else {
		// Use embedded content based on chain type
		let chain_type = CHAIN_TYPE.read().clone();
		match chain_type {
			ChainTypes::Mainnet => {
				static EMBEDDED_FOUNDATION_JSON: &str = include_str!("../../../debian/foundation.json");
				EMBEDDED_FOUNDATION_JSON.lines().map(|l| l.to_string()).collect()
			},
			ChainTypes::AutomatedTesting |
			ChainTypes::UserTesting |
			ChainTypes::Floonet => {
				static EMBEDDED_FOUNDATION_FLOONET_JSON: &str = include_str!("../../../debian/foundation_floonet.json");
				EMBEDDED_FOUNDATION_FLOONET_JSON.lines().map(|l| l.to_string()).collect()
			}
			
		}
	};

	let line = lines
		.get(index_foundation as usize)
		.unwrap_or_else(|| panic!("Index {} out of bounds in foundation.json!", index_foundation));

	serde_json::from_str(line).unwrap()
}
