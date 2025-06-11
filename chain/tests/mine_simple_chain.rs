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
use crate::chain_test_helper::{
	build_output_negative, clean_output_dir, init_chain, prepare_block, process_block,
	process_header, setup_with_status_adapter, StatusAdapter,
};

use self::chain::types::Tip;
use self::core::core::{block, transaction, KernelFeatures, OutputIdentifier};
use self::core::global::ChainTypes;
use self::core::libtx::build::{self};
use self::core::libtx::ProofBuilder;
use self::core::{consensus, global, pow};
use self::keychain::{ExtKeychain, ExtKeychainPath, Keychain};
use self::util::RwLock;
use epic_chain as chain;
use epic_chain::BlockStatus;
use epic_core as core;
use epic_core::core::hash::Hashed;
use epic_keychain as keychain;
use epic_util as util;
use std::sync::Arc;
mod chain_test_helper;

#[test]
fn mine_empty_chain() {
	let chain_dir = ".epic.empty";
	clean_output_dir(chain_dir);

	// Set up chain in AutomatedTesting mode
	global::set_mining_mode(ChainTypes::AutomatedTesting);
	let project_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
	let foundation_path = format!("{}/../debian/foundation_floonet.json", project_dir);
	global::set_foundation_path(foundation_path);

	let genesis = pow::mine_genesis_block().unwrap();
	let chain = init_chain(chain_dir, genesis);

	// The chain should only contain the genesis block
	let head = chain.head().unwrap();
	assert_eq!(head.height, 0, "Chain head should be at genesis (height 0)");

	clean_output_dir(chain_dir);
}

#[test]
fn mine_short_chain() {
	let chain_dir = ".epic.genesis";
	clean_output_dir(chain_dir);

	// Set up chain in AutomatedTesting mode
	global::set_mining_mode(ChainTypes::AutomatedTesting);
	let project_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
	let foundation_path = format!("{}/../debian/foundation_floonet.json", project_dir);
	global::set_foundation_path(foundation_path);

	let genesis = pow::mine_genesis_block().unwrap();
	let chain = init_chain(chain_dir, genesis);

	// Mine 3 more blocks after genesis (total height should be 3)
	let kc = ExtKeychain::from_random_seed(false).unwrap();
	let mut prev = chain.head_header().unwrap();
	for n in 1..=3 {
		let b = prepare_block(&kc, &prev, &chain, n + 1, vec![], 1);
		prev = b.header.clone();
		chain.process_block(b, chain::Options::SKIP_POW).unwrap();
	}

	let head = chain.head().unwrap();
	assert_eq!(
		head.height, 3,
		"Chain head should be at height 3 after mining 3 blocks"
	);

	clean_output_dir(chain_dir);
}

//
// a - b - c
//  \
//   - b'
//
// Process in the following order -
// 1. block_a
// 2. block_b
// 3. block_b'
// 4. header_c
// 5. block_c
//
#[test]
fn test_block_a_block_b_block_b_fork_header_c_fork_block_c() {
	let chain_dir = ".epic.block_a_block_b_block_b_fork_header_c_fork_block_c";
	clean_output_dir(chain_dir);
	global::set_mining_mode(ChainTypes::AutomatedTesting);
	let project_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
	let foundation_path = format!("{}/../debian/foundation_floonet.json", project_dir);
	global::set_foundation_path(foundation_path);

	let kc = ExtKeychain::from_random_seed(false).unwrap();
	let genesis = pow::mine_genesis_block().unwrap();
	let last_status = RwLock::new(None);
	let adapter = Arc::new(StatusAdapter::new(last_status));
	let chain = setup_with_status_adapter(chain_dir, genesis.clone(), adapter.clone());

	let block_a = prepare_block(&kc, &chain.head_header().unwrap(), &chain, 1, vec![], 1);
	process_block(&chain, &block_a);

	let block_b = prepare_block(&kc, &block_a.header, &chain, 2, vec![], 2);
	let block_b_fork = prepare_block(&kc, &block_a.header, &chain, 2, vec![], 3);

	println!("block_b      hash: {:?}", block_b.hash());
	println!("block_b_fork hash: {:?}", block_b_fork.hash());
	// Assert that the hashes are different to avoid duplicate block error
	assert_ne!(
		block_b.hash(),
		block_b_fork.hash(),
		"block_b and block_b_fork must have different hashes!"
	);

	process_block(&chain, &block_b);
	process_block(&chain, &block_b_fork);

	let block_c = prepare_block(&kc, &block_b.header, &chain, 4, vec![], 4);
	process_header(&chain, &block_c.header);

	assert_eq!(chain.head().unwrap(), Tip::from_header(&block_b.header));
	assert_eq!(
		chain.header_head().unwrap(),
		Tip::from_header(&block_c.header)
	);

	process_block(&chain, &block_c);

	assert_eq!(chain.head().unwrap(), Tip::from_header(&block_c.header));
	assert_eq!(
		chain.header_head().unwrap(),
		Tip::from_header(&block_c.header)
	);

	clean_output_dir(chain_dir);
}

//
// a - b
//  \
//   - b' - c'
//

// Process in the following order -
// 1. block_a
// 2. block_b
// 3. block_b'
// 4. header_c'
// 5. block_c'
//
#[test]
fn test_block_a_block_b_block_b_fork_header_c_fork_block_c_fork() {
	let chain_dir = ".epic.block_a_block_b_block_b_fork_header_c_fork_block_c_fork";
	clean_output_dir(chain_dir);
	global::set_mining_mode(ChainTypes::AutomatedTesting);
	let project_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
	let foundation_path = format!("{}/../debian/foundation_floonet.json", project_dir);
	global::set_foundation_path(foundation_path);

	let kc = ExtKeychain::from_random_seed(false).unwrap();
	let genesis = pow::mine_genesis_block().unwrap();
	let last_status = RwLock::new(None);
	let adapter = Arc::new(StatusAdapter::new(last_status));
	let chain = setup_with_status_adapter(chain_dir, genesis.clone(), adapter.clone());

	let block_a = prepare_block(&kc, &chain.head_header().unwrap(), &chain, 1, vec![], 1);
	process_block(&chain, &block_a);

	let block_b = prepare_block(&kc, &block_a.header, &chain, 2, vec![], 2);
	let block_b_fork = prepare_block(&kc, &block_a.header, &chain, 2, vec![], 3);

	process_block(&chain, &block_b);
	process_block(&chain, &block_b_fork);

	let block_c_fork = prepare_block(&kc, &block_b_fork.header, &chain, 3, vec![], 4);
	process_header(&chain, &block_c_fork.header);

	assert_eq!(chain.head().unwrap(), Tip::from_header(&block_b.header));
	assert_eq!(
		chain.header_head().unwrap(),
		Tip::from_header(&block_c_fork.header)
	);

	process_block(&chain, &block_c_fork);

	assert_eq!(
		chain.head().unwrap(),
		Tip::from_header(&block_c_fork.header)
	);
	assert_eq!(
		chain.header_head().unwrap(),
		Tip::from_header(&block_c_fork.header)
	);

	clean_output_dir(chain_dir);
}

//
// a - b - c
//  \
//   - b'
//
// Process in the following order -
// 1. block_a
// 2. header_b
// 3. header_b_fork
// 4. block_b_fork
// 5. block_b
// 6. block_c
//
#[test]
fn test_block_a_header_b_header_b_fork_block_b_fork_block_b_block_c() {
	let chain_dir = ".epic.test_block_a_header_b_header_b_fork_block_b_fork_block_b_block_c";
	clean_output_dir(chain_dir);
	global::set_mining_mode(ChainTypes::AutomatedTesting);
	let project_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
	let foundation_path = format!("{}/../debian/foundation_floonet.json", project_dir);
	global::set_foundation_path(foundation_path);

	let kc = ExtKeychain::from_random_seed(false).unwrap();
	let genesis = pow::mine_genesis_block().unwrap();
	let last_status = RwLock::new(None);
	let adapter = Arc::new(StatusAdapter::new(last_status));
	let chain = setup_with_status_adapter(chain_dir, genesis.clone(), adapter.clone());

	let block_a = prepare_block(&kc, &chain.head_header().unwrap(), &chain, 1, vec![], 1);
	process_block(&chain, &block_a);

	let block_b = prepare_block(&kc, &block_a.header, &chain, 2, vec![], 2);
	let block_b_fork = prepare_block(&kc, &block_a.header, &chain, 2, vec![], 3);

	process_header(&chain, &block_b.header);
	process_header(&chain, &block_b_fork.header);
	process_block(&chain, &block_b_fork);
	process_block(&chain, &block_b);

	assert_eq!(
		chain.header_head().unwrap(),
		Tip::from_header(&block_b.header)
	);
	assert_eq!(
		chain.head().unwrap(),
		Tip::from_header(&block_b_fork.header)
	);

	let block_c = prepare_block(&kc, &block_b.header, &chain, 3, vec![], 4);
	process_block(&chain, &block_c);

	assert_eq!(chain.head().unwrap(), Tip::from_header(&block_c.header));
	assert_eq!(
		chain.header_head().unwrap(),
		Tip::from_header(&block_c.header)
	);

	clean_output_dir(chain_dir);
}

//
// a - b
//  \
//   - b' - c'
//
// Process in the following order -
// 1. block_a
// 2. header_b
// 3. header_b_fork
// 4. block_b_fork
// 5. block_b
// 6. block_c_fork
//
#[test]
fn test_block_a_header_b_header_b_fork_block_b_fork_block_b_block_c_fork() {
	let chain_dir = ".epic.test_block_a_header_b_header_b_fork_block_b_fork_block_b_block_c_fork";
	clean_output_dir(chain_dir);
	global::set_mining_mode(ChainTypes::AutomatedTesting);
	let project_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
	let foundation_path = format!("{}/../debian/foundation_floonet.json", project_dir);
	global::set_foundation_path(foundation_path);

	let kc = ExtKeychain::from_random_seed(false).unwrap();
	let genesis = pow::mine_genesis_block().unwrap();
	let last_status = RwLock::new(None);
	let adapter = Arc::new(StatusAdapter::new(last_status));
	let chain = setup_with_status_adapter(chain_dir, genesis.clone(), adapter.clone());

	let block_a = prepare_block(&kc, &chain.head_header().unwrap(), &chain, 1, vec![], 1);
	process_block(&chain, &block_a);

	let block_b = prepare_block(&kc, &block_a.header, &chain, 2, vec![], 2);
	let block_b_fork = prepare_block(&kc, &block_a.header, &chain, 2, vec![], 3);

	process_header(&chain, &block_b.header);
	process_header(&chain, &block_b_fork.header);
	process_block(&chain, &block_b_fork);
	process_block(&chain, &block_b);

	assert_eq!(
		chain.header_head().unwrap(),
		Tip::from_header(&block_b.header)
	);
	assert_eq!(
		chain.head().unwrap(),
		Tip::from_header(&block_b_fork.header)
	);

	let block_c_fork = prepare_block(&kc, &block_b_fork.header, &chain, 3, vec![], 4);
	process_block(&chain, &block_c_fork);

	assert_eq!(
		chain.head().unwrap(),
		Tip::from_header(&block_c_fork.header)
	);
	assert_eq!(
		chain.header_head().unwrap(),
		Tip::from_header(&block_c_fork.header)
	);

	clean_output_dir(chain_dir);
}

// This test creates a reorg at REORG_DEPTH by mining a block with difficulty that
// exceeds original chain total difficulty.
//
// Illustration of reorg with NUM_BLOCKS_MAIN = 6 and REORG_DEPTH = 5:
//
// difficulty:    1        2        3        4        5        6
//
//                       / [ 2  ] - [ 3  ] - [ 4  ] - [ 5  ] - [ 6  ] <- original chain
// [ Genesis ] -[ 1 ]- *
//                     ^ \ [ 2' ] - ................................  <- reorg chain with depth 5
//                     |
// difficulty:    1    |   24
//                     |
//                     \----< Fork point and chain reorg
#[test]
fn mine_reorg() {
	// Test configuration
	const NUM_BLOCKS_MAIN: u64 = 6; // Number of blocks to mine in main chain
	const REORG_DEPTH: u64 = 5; // Number of blocks to be discarded from main chain after reorg

	const DIR_NAME: &str = ".epic_reorg";
	clean_output_dir(DIR_NAME);

	global::set_mining_mode(ChainTypes::AutomatedTesting);
	let project_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
	let foundation_path = format!("{}/../debian/foundation_floonet.json", project_dir);
	global::set_foundation_path(foundation_path);

	let kc = ExtKeychain::from_random_seed(false).unwrap();

	let genesis = pow::mine_genesis_block().unwrap();
	{
		// Create chain that reports last block status
		let last_status = RwLock::new(None);
		let adapter = Arc::new(StatusAdapter::new(last_status));
		let chain = setup_with_status_adapter(DIR_NAME, genesis.clone(), adapter.clone());

		// Add blocks to main chain with gradually increasing difficulty
		let mut prev = chain.head_header().unwrap();
		for n in 1..=NUM_BLOCKS_MAIN {
			let b = prepare_block(&kc, &prev, &chain, n, vec![], 1);
			prev = b.header.clone();
			chain.process_block(b, chain::Options::SKIP_POW).unwrap();
		}

		let head = chain.head_header().unwrap();
		assert_eq!(head.height, NUM_BLOCKS_MAIN);
		assert_eq!(head.hash(), prev.hash());

		// Reorg chain should exceed main chain's total difficulty to be considered
		let reorg_difficulty = head.total_difficulty().num.iter().map(|(_, &i)| i).sum();

		// Create one block for reorg chain forking off NUM_BLOCKS_MAIN - REORG_DEPTH height
		let fork_head = chain
			.get_header_by_height(NUM_BLOCKS_MAIN - REORG_DEPTH)
			.unwrap();
		let b = prepare_block(&kc, &fork_head, &chain, reorg_difficulty, vec![], 2);
		let reorg_head = b.header.clone();
		chain.process_block(b, chain::Options::SKIP_POW).unwrap();

		// Check that reorg is correctly reported in block status
		assert_eq!(
			*adapter.last_status.read(),
			Some(BlockStatus::Reorg(REORG_DEPTH))
		);

		// Chain should be switched to the reorganized chain
		let head = chain.head_header().unwrap();
		assert_eq!(head.height, NUM_BLOCKS_MAIN - REORG_DEPTH + 1);
		assert_eq!(head.hash(), reorg_head.hash());
	}

	// Cleanup chain directory
	clean_output_dir(DIR_NAME);
}

#[test]
fn mine_forks() {
	global::set_mining_mode(ChainTypes::AutomatedTesting);
	let project_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
	let foundation_path = format!("{}/../debian/foundation_floonet.json", project_dir);
	global::set_foundation_path(foundation_path);
	{
		let chain = init_chain(".epic2", pow::mine_genesis_block().unwrap());
		let kc = ExtKeychain::from_random_seed(false).unwrap();

		// add a first block to not fork genesis
		let prev = chain.head_header().unwrap();
		let b = prepare_block(&kc, &prev, &chain, 2, vec![], 1);
		chain.process_block(b, chain::Options::SKIP_POW).unwrap();

		// mine and add a few blocks

		for n in 1..4 {
			// first block for one branch
			let prev = chain.head_header().unwrap();
			let b1 = prepare_block(&kc, &prev, &chain, 3 * n, vec![], 2);

			// process the first block to extend the chain
			let bhash = b1.hash();
			chain.process_block(b1, chain::Options::SKIP_POW).unwrap();

			// checking our new head
			let head = chain.head().unwrap();
			assert_eq!(head.height, (n + 1) as u64);
			assert_eq!(head.last_block_h, bhash);
			assert_eq!(head.prev_block_h, prev.hash());

			// 2nd block with higher difficulty for other branch
			let b2 = prepare_block(&kc, &prev, &chain, 3 * n + 1, vec![], 3);

			// process the 2nd block to build a fork with more work
			let bhash = b2.hash();
			chain.process_block(b2, chain::Options::SKIP_POW).unwrap();

			// checking head switch
			let head = chain.head().unwrap();
			assert_eq!(head.height, (n + 1) as u64);
			assert_eq!(head.last_block_h, bhash);
			assert_eq!(head.prev_block_h, prev.hash());
		}
	}
	// Cleanup chain directory
	clean_output_dir(".epic2");
}

#[test]
fn mine_losing_fork() {
	global::set_mining_mode(ChainTypes::AutomatedTesting);
	let project_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
	let foundation_path = format!("{}/debian/floonet_foundation.json", project_dir);
	global::set_foundation_path(foundation_path);
	clean_output_dir(".epic3");
	let kc = ExtKeychain::from_random_seed(false).unwrap();
	{
		let chain = init_chain(".epic3", pow::mine_genesis_block().unwrap());

		// add a first block we'll be forking from
		let prev = chain.head_header().unwrap();
		let b1 = prepare_block(&kc, &prev, &chain, 2, vec![], 1);
		let b1head = b1.header.clone();
		chain.process_block(b1, chain::Options::SKIP_POW).unwrap();

		// prepare the 2 successor, sibling blocks, one with lower diff
		let b2 = prepare_block(&kc, &b1head, &chain, 4, vec![], 2);
		let b2head = b2.header.clone();
		let bfork = prepare_block(&kc, &b1head, &chain, 3, vec![], 3);

		// add higher difficulty first, prepare its successor, then fork
		// with lower diff
		chain.process_block(b2, chain::Options::SKIP_POW).unwrap();
		assert_eq!(chain.head_header().unwrap().hash(), b2head.hash());
		let b3 = prepare_block(&kc, &b2head, &chain, 5, vec![], 4);
		chain
			.process_block(bfork, chain::Options::SKIP_POW)
			.unwrap();

		// adding the successor
		let b3head = b3.header.clone();
		chain.process_block(b3, chain::Options::SKIP_POW).unwrap();
		assert_eq!(chain.head_header().unwrap().hash(), b3head.hash());
	}
	// Cleanup chain directory
	clean_output_dir(".epic3");
}

#[test]
fn longer_fork() {
	global::set_mining_mode(ChainTypes::AutomatedTesting);
	let project_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
	let foundation_path = format!("{}/../debian/foundation_floonet.json", project_dir);
	global::set_foundation_path(foundation_path);

	clean_output_dir(".epic4");
	let kc = ExtKeychain::from_random_seed(false).unwrap();
	// to make it easier to compute the txhashset roots in the test, we
	// prepare 2 chains, the 2nd will be have the forked blocks we can
	// then send back on the 1st
	let genesis = pow::mine_genesis_block().unwrap();
	{
		let chain = init_chain(".epic4", genesis.clone());

		// add blocks to both chains, 20 on the main one, only the first 5
		// for the forked chain
		let mut prev = chain.head_header().unwrap();
		for n in 0..10 {
			let b = prepare_block(&kc, &prev, &chain, 2 * n + 2, vec![], 1);
			prev = b.header.clone();
			chain.process_block(b, chain::Options::SKIP_POW).unwrap();
		}

		let forked_block = chain.get_header_by_height(5).unwrap();

		let head = chain.head_header().unwrap();
		assert_eq!(head.height, 10);
		assert_eq!(head.hash(), prev.hash());

		let mut prev = forked_block;
		for n in 0..7 {
			let b = prepare_block(&kc, &prev, &chain, 2 * n + 11, vec![], 2);
			prev = b.header.clone();
			chain.process_block(b, chain::Options::SKIP_POW).unwrap();
		}

		let new_head = prev;

		// After all this the chain should have switched to the fork.
		let head = chain.head_header().unwrap();
		assert_eq!(head.height, 12);
		assert_eq!(head.hash(), new_head.hash());
	}
	// Cleanup chain directory
	clean_output_dir(".epic4");
}

#[test]
fn spend_in_fork_and_compact() {
	global::set_mining_mode(ChainTypes::AutomatedTesting);
	let project_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
	let foundation_path = format!("{}/../debian/foundation_floonet.json", project_dir);
	global::set_foundation_path(foundation_path);
	util::init_test_logger();
	// Cleanup chain directory
	clean_output_dir(".epic6");

	{
		let chain = init_chain(".epic6", pow::mine_genesis_block().unwrap());
		let prev = chain.head_header().unwrap();
		let kc = ExtKeychain::from_random_seed(false).unwrap();
		let pb = ProofBuilder::new(&kc);

		let mut fork_head = prev;

		// mine the first block and keep track of the block_hash
		// so we can spend the coinbase later
		let b = prepare_block(&kc, &fork_head, &chain, 2, vec![], 1);
		let out_id = OutputIdentifier::from_output(&b.outputs()[0]);
		assert!(out_id.features.is_coinbase());
		fork_head = b.header.clone();
		chain
			.process_block(b.clone(), chain::Options::SKIP_POW)
			.unwrap();

		// now mine three further blocks
		for n in 3..6 {
			let b = prepare_block(&kc, &fork_head, &chain, n, vec![], 1);
			fork_head = b.header.clone();
			chain.process_block(b, chain::Options::SKIP_POW).unwrap();
		}

		// Check the height of the "fork block".
		assert_eq!(fork_head.height, 4);
		let key_id2 = ExtKeychainPath::new(1, 2, 0, 0, 0).to_identifier();
		let key_id30 = ExtKeychainPath::new(1, 30, 0, 0, 0).to_identifier();
		let key_id31 = ExtKeychainPath::new(1, 31, 0, 0, 0).to_identifier();

		let tx1 = build::transaction(
			KernelFeatures::Plain { fee: 20000 },
			vec![
				build::coinbase_input(consensus::reward_at_height(1), key_id2.clone()),
				build::output(consensus::reward_at_height(1) - 20000, key_id30.clone()),
			],
			&kc,
			&pb,
		)
		.unwrap();

		let next = prepare_block(&kc, &fork_head, &chain, 7, vec![&tx1], 4);
		let prev_main = next.header.clone();
		chain
			.process_block(next.clone(), chain::Options::SKIP_POW)
			.unwrap();
		chain.validate(false).unwrap();

		let tx2 = build::transaction(
			KernelFeatures::Plain { fee: 20000 },
			vec![
				build::input(consensus::reward_at_height(1) - 20000, key_id30.clone()),
				build::output(consensus::reward_at_height(1) - 40000, key_id31.clone()),
			],
			&kc,
			&pb,
		)
		.unwrap();

		let next = prepare_block(&kc, &prev_main, &chain, 9, vec![&tx2], 4);
		let prev_main = next.header.clone();
		chain.process_block(next, chain::Options::SKIP_POW).unwrap();

		// Full chain validation for completeness.
		chain.validate(false).unwrap();

		// mine 2 forked blocks from the first
		let fork = prepare_block(&kc, &fork_head, &chain, 6, vec![&tx1], 1);
		let prev_fork = fork.header.clone();
		chain.process_block(fork, chain::Options::SKIP_POW).unwrap();

		let fork_next = prepare_block(&kc, &prev_fork, &chain, 8, vec![&tx2], 1);
		let prev_fork = fork_next.header.clone();
		chain
			.process_block(fork_next, chain::Options::SKIP_POW)
			.unwrap();

		chain.validate(false).unwrap();

		// check state
		let head = chain.head_header().unwrap();
		assert_eq!(head.height, 6);
		assert_eq!(head.hash(), prev_main.hash());
		assert!(chain
			.is_unspent(&OutputIdentifier::from_output(&tx2.outputs()[0]))
			.is_ok());
		assert!(chain
			.is_unspent(&OutputIdentifier::from_output(&tx1.outputs()[0]))
			.is_err());

		// make the fork win
		let fork_next = prepare_block(&kc, &prev_fork, &chain, 10, vec![], 1);
		let prev_fork = fork_next.header.clone();
		chain
			.process_block(fork_next, chain::Options::SKIP_POW)
			.unwrap();
		chain.validate(false).unwrap();

		// check state
		let head = chain.head_header().unwrap();
		assert_eq!(head.height, 7);
		assert_eq!(head.hash(), prev_fork.hash());
		assert!(chain
			.is_unspent(&OutputIdentifier::from_output(&tx2.outputs()[0]))
			.is_ok());
		assert!(chain
			.is_unspent(&OutputIdentifier::from_output(&tx1.outputs()[0]))
			.is_err());

		// add 20 blocks to go past the test horizon
		let mut prev = prev_fork;
		for n in 0..20 {
			let next = prepare_block(&kc, &prev, &chain, 11 + n, vec![], 1);
			prev = next.header.clone();
			chain.process_block(next, chain::Options::SKIP_POW).unwrap();
		}

		chain.validate(false).unwrap();
		if let Err(e) = chain.compact() {
			panic!("Error compacting chain: {:?}", e);
		}
		if let Err(e) = chain.validate(false) {
			panic!("Validation error after compacting chain: {:?}", e);
		}
	}

	// Cleanup chain directory
	clean_output_dir(".epic6");
}

/// Test ability to retrieve block headers for a given output
#[test]
fn output_header_mappings() {
	global::set_mining_mode(ChainTypes::AutomatedTesting);

	let project_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
	let foundation_path = format!("{}/../debian/foundation_floonet.json", project_dir);
	global::set_foundation_path(foundation_path);

	{
		let chain = init_chain(
			".epic_header_for_output",
			pow::mine_genesis_block().unwrap(),
		);
		let kc = ExtKeychain::from_random_seed(false).unwrap();
		let mut reward_outputs = vec![];

		for n in 1..15 {
			let prev = chain.head_header().unwrap();
			let block = prepare_block(&kc, &prev, &chain, n as u64, vec![], 1);
			reward_outputs.push(block.outputs()[0].clone());
			process_block(&chain, &block);

			let header_for_output = chain
				.get_header_for_output(&OutputIdentifier::from_output(
					&reward_outputs[(n - 1) as usize],
				))
				.unwrap();
			assert_eq!(header_for_output.height, n as u64);

			chain.validate(false).unwrap();
		}

		// Check all output positions are as expected
		for n in 1..15 {
			let header_for_output = chain
				.get_header_for_output(&OutputIdentifier::from_output(&reward_outputs[n - 1]))
				.unwrap();
			assert_eq!(header_for_output.height, n as u64);
		}
	}
	// Cleanup chain directory
	clean_output_dir(".epic_header_for_output");
}

/// Test the duplicate rangeproof bug
#[test]
fn test_overflow_cached_rangeproof() {
	clean_output_dir(".epic_overflow");
	global::set_mining_mode(ChainTypes::AutomatedTesting);
	let project_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
	let foundation_path = format!("{}/../debian/foundation_floonet.json", project_dir);
	global::set_foundation_path(foundation_path);

	util::init_test_logger();
	{
		let chain = init_chain(".epic_overflow", pow::mine_genesis_block().unwrap());
		let prev = chain.head_header().unwrap();
		let kc = ExtKeychain::from_random_seed(false).unwrap();
		let pb = ProofBuilder::new(&kc);

		let mut head = prev;

		// mine the first block and keep track of the block_hash
		// so we can spend the coinbase later
		let b = prepare_block(&kc, &head, &chain, 2, vec![], 1);

		assert!(b.outputs()[0].is_coinbase());
		head = b.header.clone();
		process_block(&chain, &b);

		// now mine three further blocks
		for n in 3..6 {
			let b = prepare_block(&kc, &head, &chain, n, vec![], 1);
			head = b.header.clone();
			process_block(&chain, &b);
		}

		// create a few keys for use in txns
		let key_id2 = ExtKeychainPath::new(1, 2, 0, 0, 0).to_identifier();
		let key_id30 = ExtKeychainPath::new(1, 30, 0, 0, 0).to_identifier();
		let key_id31 = ExtKeychainPath::new(1, 31, 0, 0, 0).to_identifier();
		let key_id32 = ExtKeychainPath::new(1, 32, 0, 0, 0).to_identifier();

		// build a regular transaction so we have a rangeproof to copy
		let tx1 = build::transaction(
			KernelFeatures::Plain { fee: 20000 },
			vec![
				build::coinbase_input(consensus::reward_at_height(1), key_id2.clone()),
				build::output(consensus::reward_at_height(1) - 20000, key_id30.clone()),
			],
			&kc,
			&pb,
		)
		.unwrap();

		// mine block with tx1
		let next = prepare_block(&kc, &head, &chain, 7, vec![&tx1.clone()], 4);
		let prev_main = next.header.clone();
		process_block(&chain, &next.clone());

		chain.validate(false).unwrap();

		// create a second tx that contains a negative output
		// and a positive output for 1m epic
		let mut tx2 = build::transaction(
			KernelFeatures::Plain { fee: 0 },
			vec![
				build::input(consensus::reward_at_height(1) - 20000, key_id30.clone()),
				build::output(
					consensus::reward_at_height(1) - 20000 + 1_000_000_000_000_000,
					key_id31.clone(),
				),
				build_output_negative(1_000_000_000_000_000, key_id32.clone()),
			],
			&kc,
			&pb,
		)
		.unwrap();

		// make sure tx1 only has one output as expected
		assert_eq!(tx1.body.outputs.len(), 1);
		let last_rp = tx1.body.outputs[0].proof;

		// overwrite all our rangeproofs with the rangeproof from last block
		for i in 0..tx2.body.outputs.len() {
			tx2.body.outputs[i].proof = last_rp;
		}

		let next = prepare_block(&kc, &prev_main, &chain, 8, vec![&tx2.clone()], 4);
		// process_block fails with verifier_cache disabled or with correct verifier_cache
		// implementations
		let res = chain.process_block(next.clone(), chain::Options::SKIP_POW);

		assert!(matches!(
			res.unwrap_err(),
			chain::Error::InvalidBlockProof(block::Error::Transaction(transaction::Error::Secp(
				util::secp::Error::InvalidRangeProof
			)))
		));
	}
	clean_output_dir(".epic_overflow");
}
