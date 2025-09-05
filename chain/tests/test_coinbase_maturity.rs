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
use epic_chain as chain;
use epic_core as core;
use epic_keychain as keychain;
use epic_util as util;

use self::core::consensus;
use self::core::core::KernelFeatures;
use self::core::libtx::{build, ProofBuilder};
use self::core::{global, pow};
use self::keychain::{ExtKeychain, ExtKeychainPath, Keychain};
use crate::chain_test_helper::{
	clean_output_dir, init_chain, prepare_block, process_block, set_foundation_path_for_test,
};

#[test]
fn test_coinbase_maturity() {
	util::init_test_logger();

	let chain_dir = ".epic_coinbase";
	clean_output_dir(chain_dir);

	set_foundation_path_for_test("foundation_floonet.json");

	let genesis = pow::mine_genesis_block().unwrap();
	let chain = init_chain(chain_dir, genesis.clone());
	let keychain = ExtKeychain::from_random_seed(false).unwrap();
	let builder = ProofBuilder::new(&keychain);

	// Mine the first block with coinbase
	let prev = chain.head_header().unwrap();
	let key_id1 = ExtKeychainPath::new(1, 1, 0, 0, 0).to_identifier();
	let key_id2 = ExtKeychainPath::new(1, 2, 0, 0, 0).to_identifier();
	let _key_id3 = ExtKeychainPath::new(1, 3, 0, 0, 0).to_identifier();
	let _key_id4 = ExtKeychainPath::new(1, 4, 0, 0, 0).to_identifier();

	let block = prepare_block(&keychain, &prev, &chain, 1, vec![], 1);
	process_block(&chain, &block);

	let amount = consensus::reward_at_height(1);
	let lock_height = 1 + global::coinbase_maturity();
	assert_eq!(lock_height, 4);

	// Try to spend the coinbase before maturity
	let coinbase_txn = build::transaction(
		KernelFeatures::Plain { fee: 2 },
		vec![
			build::coinbase_input(amount, key_id1.clone()),
			build::output(amount - 2, key_id2.clone()),
		],
		&keychain,
		&builder,
	)
	.unwrap();

	match chain.verify_coinbase_maturity(&coinbase_txn) {
		Ok(_) => {}
		Err(chain::Error::ImmatureCoinbase) => {}
		Err(_) => panic!("Expected transaction error with immature coinbase."),
	}

	// Mine enough blocks for coinbase to mature
	for n in 2..=lock_height {
		let prev = chain.head_header().unwrap();
		let block = prepare_block(&keychain, &prev, &chain, n as u64, vec![], 1);
		process_block(&chain, &block);
	}

	// Now the coinbase should be mature and spendable
	chain.verify_coinbase_maturity(&coinbase_txn).unwrap();

	// Spend the coinbase in a block
	let prev = chain.head_header().unwrap();
	let txs = vec![coinbase_txn];
	let tx_refs: Vec<&core::core::Transaction> = txs.iter().collect();
	let block = prepare_block(&keychain, &prev, &chain, prev.height + 1, tx_refs, 3);
	process_block(&chain, &block);

	clean_output_dir(chain_dir);
}
