// Copyright 2019 The Grin Developers
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

mod chain_test_helper;
use epic_core as core;
use epic_keychain as keychain;
use epic_util as util;

use self::core::global::ChainTypes;
use self::core::{global, pow};
use self::keychain::{ExtKeychain, Keychain};
use crate::chain_test_helper::{clean_output_dir, init_chain, prepare_block, process_block};

#[test]
fn data_files() {
	util::init_test_logger();

	let chain_dir = ".epic_df";
	clean_output_dir(chain_dir);

	// Set up chain in AutomatedTesting mode
	global::set_mining_mode(ChainTypes::AutomatedTesting);
	global::set_foundation_path("../debian/floonet_foundation.json".to_string());

	let genesis = pow::mine_genesis_block().unwrap();

	// Mine a few blocks on a new chain.
	{
		let chain = init_chain(chain_dir, genesis.clone());
		let kc = ExtKeychain::from_random_seed(false).unwrap();
		let mut prev = chain.head_header().unwrap();
		for n in 1..=3 {
			let b = prepare_block(&kc, &prev, &chain, n + 1, vec![], 1);
			prev = b.header.clone();
			process_block(&chain, &b);
		}
		chain.validate(false).unwrap();
		assert_eq!(chain.head().unwrap().height, 3);
	};

	// Now reload the chain from existing data files and check it is valid.
	{
		let chain = init_chain(chain_dir, genesis);
		chain.validate(false).unwrap();
		assert_eq!(chain.head().unwrap().height, 3);
	}

	// Cleanup chain directory
	clean_output_dir(chain_dir);
}
