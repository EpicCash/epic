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
use crate::chain_test_helper::{clean_output_dir, init_chain, prepare_block, process_block};
use epic_core as core;
use epic_keychain as keychain;
use epic_util as util;

use self::core::core::hash::Hashed;
use self::core::global::ChainTypes;
use self::core::{global, pow};
use self::keychain::{ExtKeychain, Keychain};

#[test]
fn test_store_indices() {
	util::init_test_logger();

	let chain_dir = ".epic_idx_1";
	clean_output_dir(chain_dir);

	global::set_mining_mode(ChainTypes::AutomatedTesting);
	let project_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
	let foundation_path = format!("{}/../debian/foundation_floonet.json", project_dir);
	global::set_foundation_path(foundation_path);

	let genesis = pow::mine_genesis_block().unwrap();
	let chain = init_chain(chain_dir, genesis);

	let kc = ExtKeychain::from_random_seed(false).unwrap();
	let mut prev = chain.head_header().unwrap();

	// Mine 3 more blocks after genesis (total height should be 3)
	for n in 1..=3 {
		let b = prepare_block(&kc, &prev, &chain, n + 1, vec![], 1);
		prev = b.header.clone();
		process_block(&chain, &b);
	}

	// Check head exists in the db.
	assert_eq!(chain.head().unwrap().height, 3);

	// Check the header exists in the db.
	assert_eq!(chain.head_header().unwrap().height, 3);

	// Check header_by_height index.
	let block_header = chain.get_header_by_height(3).unwrap();
	let block_hash = block_header.hash();
	assert_eq!(block_hash, chain.head().unwrap().last_block_h);

	{
		// Block exists in the db.
		assert_eq!(chain.get_block(&block_hash).unwrap().hash(), block_hash);

		// Check we have block_sums in the db.
		assert!(chain.get_block_sums(&block_hash).is_ok());

		{
			// Start a new batch and delete the block.
			let store = chain.store();
			let batch = store.batch().unwrap();
			assert!(batch.delete_block(&block_hash).is_ok());

			// Block is deleted within this batch.
			assert!(batch.get_block(&block_hash).is_err());
		}

		// Check the batch did not commit any changes to the store.
		assert!(chain.get_block(&block_hash).is_ok());
	}

	// Cleanup chain directory
	clean_output_dir(chain_dir);
}
