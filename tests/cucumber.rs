#[macro_use]
extern crate cucumber_rust;

use epic_chain::Chain;
use epic_core::core::block::feijoada::PoWType as FType;
use epic_core::core::block::feijoada::{
	get_bottles_default, Deterministic, Feijoada, Policy, PolicyConfig,
};
use epic_core::core::Block;
use epic_core::global;
use epic_core::global::set_policy_config;
use epic_keychain::keychain::ExtKeychain;
use epic_util as util;

pub struct EdnaWorld {
	pub output_dir: String,
	pub genesis: Option<Block>,
	pub keychain: Option<ExtKeychain>,
	pub keychain_foundation: Option<ExtKeychain>,
	pub chain: Option<Chain>,
	pub policy: Policy,
	pub bottles: Policy,
}

impl cucumber_rust::World for EdnaWorld {}
impl std::default::Default for EdnaWorld {
	fn default() -> EdnaWorld {
		EdnaWorld {
			output_dir: ".epic".to_string(),
			genesis: None,
			keychain: None,
			keychain_foundation: None,
			chain: None,
			policy: get_bottles_default(),
			bottles: get_bottles_default(),
		}
	}
}

mod mine_chain {
	use cucumber_rust::steps;
	use epic_chain as chain;
	use epic_core as core;
	use epic_util as util;

	use epic_chain::store::BottleIter;
	use epic_chain::types::NoopAdapter;
	use epic_chain::Chain;
	use epic_core::core::block::feijoada::PoWType as FType;
	use epic_core::core::block::feijoada::{
		count_beans, get_bottles_default, next_block_bottles, Deterministic, Feijoada, Policy,
		PolicyConfig,
	};
	use epic_core::core::foundation::load_foundation_output;
	use epic_core::core::hash::{Hash, Hashed};
	use epic_core::core::verifier_cache::LruVerifierCache;
	use epic_core::core::{Block, BlockHeader, Output, OutputIdentifier, Transaction, TxKernel};
	use epic_core::global::{
		add_allowed_policy, get_emitted_policy, get_policies, set_emitted_policy,
		set_policy_config, ChainTypes,
	};
	use epic_core::libtx::{self, build, reward};
	use epic_core::pow::{
		new_cuckaroo_ctx, new_cuckatoo_ctx, new_md5_ctx, new_progpow_ctx, new_randomx_ctx,
		Difficulty, EdgeType, Error, PoWContext,
	};
	use epic_core::{consensus, pow};
	use epic_core::{genesis, global};
	use epic_keychain::{Identifier, Keychain};

	use chrono::prelude::{DateTime, NaiveDateTime, Utc};
	use chrono::Duration;

	use epic_util::{Mutex, RwLock, StopState};
	use std::collections::HashMap;
	use std::fs;
	use std::sync::Arc;

	use serde::{Deserialize, Serialize};
	use serde_json;

	const MAX_SOLS: u32 = 10;

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

	steps!(crate::EdnaWorld => {
		given regex "I have a <([a-zA-Z]+)> chain" |_world, matches, _step| {
			match matches[1].to_lowercase().as_str() {
				"testing" => global::set_mining_mode(ChainTypes::AutomatedTesting),
				"mainnet" => global::set_mining_mode(ChainTypes::Mainnet),
				"floonet" => global::set_mining_mode(ChainTypes::Floonet),
				_ => panic!("Unknown chain type"),
			};
		};

		given regex "I define my output dir as <(.*)>" |world, matches, _step| {
			world.output_dir = matches[1].clone();
		};

		then "mine an empty keychain" |world, _step| {
			let keychain = epic_keychain::ExtKeychain::from_random_seed(false).unwrap();
			{
				mine_some_on_top(&world.output_dir, pow::mine_genesis_block().unwrap(), &keychain, &PoWType::Cuckatoo);
			}
		};

		then "I mine" |world, _step| {
			let genesis = world.genesis.as_ref().unwrap();
			mine_some_on_top(&world.output_dir, genesis.clone(), world.keychain.as_ref().unwrap(), &PoWType::Cuckatoo);
		};

		then regex "I mine <([a-zA-Z0-9]+)>" |world, matches, _step| {
			let chain = world.chain.as_ref().unwrap();
			//let prev_header = chain.head_header().unwrap();
			//let prev = chain.get_block(&prev_header.hash()).unwrap();
			let pow_type = match matches[1].as_str() {
				"cuckatoo" => PoWType::Cuckatoo,
				"cuckaroo" => PoWType::Cuckaroo,
				"md5" => PoWType::MD5,
				"randomx" => PoWType::RandomX,
				"progpow" => PoWType::ProgPow,
				_ => panic!("Non supported PoW Type"),
			};
			let genesis = world.genesis.as_ref().unwrap();
			mine_some_on_top(&world.output_dir, genesis.clone(), world.keychain.as_ref().unwrap(), &pow_type);
		};

		then "clean output dir" |world, _step| {
			clean_output_dir(&world.output_dir);
		};

		given "I add coinbase data from the dev genesis block" |world, _step| {
			let genesis = genesis::genesis_dev();
			world.keychain = Some(epic_keychain::ExtKeychain::from_random_seed(false).unwrap());
			let key_id = epic_keychain::ExtKeychain::derive_key_id(0, 1, 0, 0, 0);
			let reward = reward::output(world.keychain.as_ref().unwrap(), &key_id, 0, false, 0).unwrap();
			world.genesis = Some(genesis.with_reward(reward.0, reward.1));
			let genesis_ref = world.genesis.as_mut().unwrap();

			let tmp_chain_dir = ".epic.tmp";
			{
				let tmp_chain = setup(tmp_chain_dir, pow::mine_genesis_block().unwrap());
				tmp_chain.set_txhashset_roots(genesis_ref).unwrap();
				genesis_ref.header.output_mmr_size = 2;
				genesis_ref.header.kernel_mmr_size = 2;
			}
		};

		then "Refuse a foundation commit invalid" |world, _step| {
			let chain = world.chain.as_ref().unwrap();
			let kc = world.keychain.as_ref().unwrap();
			let prev = chain.head_header().unwrap();

			let key_id = epic_keychain::ExtKeychain::derive_key_id(1, 3, 0, 0, 0);
			let reward = reward::output(kc, &key_id, 0, false, 0).unwrap();
			let foundation = libtx::reward::output_foundation(kc, &key_id, true).unwrap();

			let hash = chain.txhashset().read().get_header_hash_by_height(pow::randomx::rx_current_seed_height(prev.height + 1)).unwrap();
			let mut block = prepare_block_with_coinbase(&prev, 2, vec![], reward, foundation, hash);
			chain.set_txhashset_roots(&mut block).unwrap();
			let emitted_policy = get_emitted_policy();
			let policy = get_policies(emitted_policy).unwrap();
			// Mining
			let algo = Deterministic::choose_algo(&policy, &prev.bottles);
			block.header.bottles = next_block_bottles(algo, &prev.bottles);
			block.header.pow.proof = get_pow_type(&algo, prev.height);
			block.header.policy = emitted_policy;

			if let Ok(_) = chain.process_block(block, chain::Options::SKIP_POW) {
				panic!("Block need to be refused with foundation invalid!");
			}
		};

		then "clean tmp chain dir" |_world, _step| {
			clean_output_dir(".epic.tmp");
		};

		then "I get a valid PoW" |world, _step| {
			let genesis = world.genesis.as_mut().unwrap();
			pow::pow_size(
				&mut genesis.header,
				Difficulty::unit(),
				global::proofsize(),
				global::min_edge_bits(),
			).unwrap();
		};

		then regex "I get a valid <([a-z0-9]+)> PoW" |world, matches, _step| {
			let pow_type = match matches[1].as_str() {
				"cuckatoo" => PoWType::Cuckatoo,
				"cuckaroo" => PoWType::Cuckaroo,
				"md5" => PoWType::MD5,
				"randomx" => PoWType::RandomX,
				"progpow" => PoWType::ProgPow,
				_ => panic!("Non supported PoW Type"),
			};
			let genesis = world.genesis.as_mut().unwrap();
			pow_size_custom(
				&mut genesis.header,
				Difficulty::unit(),
				global::proofsize(),
				global::min_edge_bits(),
				&pow_type,
			).unwrap();
		};

		given "I setup a chain" |world, _step| {
			world.genesis = Some(pow::mine_genesis_block().unwrap());
			world.chain = Some(setup(&world.output_dir, world.genesis.as_ref().unwrap().clone()));
			world.keychain = Some(epic_keychain::ExtKeychain::from_seed(&[2,0,0], false).unwrap());
		};

		then "I mine and add a few blocks" |world, _step| {
			let chain = world.chain.as_ref().unwrap();
			let kc = world.keychain.as_ref().unwrap();
			for n in 1..4 {
				let prev = chain.head_header().unwrap();
				let b1 = prepare_block(kc, &prev, chain, 3 * n);
				let b2 = prepare_block(kc, &prev, &chain, 3 * n + 1);

				let bhash = b1.hash();

				chain.process_block(b1, chain::Options::SKIP_POW).unwrap();

				let head = chain.head().unwrap();
				assert_eq!(head.height, (n + 1) as u64);
				assert_eq!(head.last_block_h, bhash);
				assert_eq!(head.prev_block_h, prev.hash());

				let bhash = b2.hash();
				chain.process_block(b2, chain::Options::SKIP_POW).unwrap();

				let head = chain.head().unwrap();
				assert_eq!(head.height, (n + 1) as u64);
				assert_eq!(head.last_block_h, bhash);
				assert_eq!(head.prev_block_h, prev.hash());
			}
		};

		given regex "I make <([0-9]+)> blocks" |world, matches, _step| {
			let num: u64 = matches[1].parse().unwrap();
			let chain = world.chain.as_ref().unwrap();
			let kc = world.keychain.as_ref().unwrap();
			let height = chain.head_header().unwrap().height;

			for i in 0..num {
				let prev = chain.head_header().unwrap();
				let block = prepare_block(kc, &prev, &chain, height + i);
				chain.process_block(block, chain::Options::SKIP_POW).unwrap();
			};
		};

		then regex "I make <([0-9]+)> blocks forked in the height <([0-9]+)>" |world, matches, _step| {
			let chain = world.chain.as_ref().unwrap();
			let kc = world.keychain.as_ref().unwrap();

			let n: u64 = matches[1].parse().unwrap();
			let h: u64 = matches[2].parse().unwrap();

			let forked_block =  chain.get_header_by_height(h).unwrap();
			let height = chain.head_header().unwrap().height;

			assert_eq!(forked_block.height, h);

			let mut prev = forked_block;

			for i in 1..(n+1) {
				let block = prepare_fork_block(kc, &prev, &chain, 2 * i + height);
				prev = block.header.clone();
				chain.process_block(block, chain::Options::SKIP_POW).unwrap();
			};

			assert_eq!(chain.head_header().unwrap().hash(), prev.hash());
		};

		then regex "I refuse a block with <([a-z0-9]+)> invalid" |world, matches, _step| {
			let chain = world.chain.as_ref().unwrap();
			let kc = epic_keychain::ExtKeychain::from_mnemonic(
				"legal winner thank year wave sausage worth useful legal winner thank year wave sausage worth useful legal winner thank year wave sausage worth title",
				"", false).unwrap();
			let head = chain.head_header().unwrap();
			let height = head.height + 1;

			let proof_name = matches[1].as_str();

			let proof = match proof_name {
				"progpow" => pow::Proof::ProgPowProof {
					mix: [0; 32],
				},
				"randomx" => pow::Proof::RandomXProof {
					hash: [0; 32]
				},
				"md5" => pow::Proof::MD5Proof {
					edge_bits: 10,
					proof: "teste".to_string()
				},
				"cuckoo" => pow::Proof::CuckooProof {
					edge_bits: 10,
					nonces: vec![0,0,0]
				},
				_ => panic!("invalid type proof")
			};

			let mut diff = HashMap::new();
			diff.insert(FType::Cuckaroo, 3);
			diff.insert(FType::Cuckatoo, 1);
			diff.insert(FType::RandomX, 1);
			diff.insert(FType::ProgPow, 1);

			let hash = chain.txhashset().read().get_header_hash_by_height(pow::randomx::rx_current_seed_height(head.height + 1)).unwrap();
			let mut block = prepare_block_pow(&kc, &head, Difficulty::from_dic_number(diff), vec![], proof, hash);
			chain.set_txhashset_roots(&mut block).unwrap();

			if let Ok(_) = chain.process_block(block, chain::Options::MINE) {
				panic!("Proof can't be valid");
			}
		};

		then regex "I accept a block with a pow <([a-z0-9]+)> valid" |world, matches, _step| {
			let chain = world.chain.as_ref().unwrap();

			let kc = epic_keychain::ExtKeychain::from_mnemonic(
				"legal winner thank year wave sausage worth useful legal winner thank year wave sausage worth useful legal winner thank year wave sausage worth title",
				"", false).unwrap();

			let head = chain.head_header().unwrap();
			let proof_name = matches[1].as_str();

			let proof = match proof_name {
				"progpow" => pow::Proof::ProgPowProof{
					mix: [
						204, 253, 208, 53, 233, 98, 187, 32, 229, 142, 50, 69,
						170, 226, 63, 69, 127, 38, 212, 17, 238, 233, 94, 168,
						78, 147, 193, 55, 67, 18, 17, 85],
				},
				"randomx" => pow::Proof::RandomXProof {
					hash: [
						194, 140, 183, 217, 242, 141, 158, 252, 145, 102, 137, 0,
						207, 230, 90, 238, 198, 138, 199, 156, 102, 117, 127, 252,
						183, 28, 62, 140, 184, 21, 198, 152],
				},
				"md5" => pow::Proof::MD5Proof {
					edge_bits: 10,
					proof: "cf216727c5ed84a9f8baccfba715da2b".to_string()
				},
				"cuckoo" => pow::Proof::CuckooProof {
					edge_bits: 9,
					nonces: vec![27, 209, 166, 497]
				},
				_ => {panic!("invalid type proof")},
			};

			let mut diff = HashMap::new();
			diff.insert(FType::Cuckaroo, 3);
			diff.insert(FType::Cuckatoo, 1);
			diff.insert(FType::RandomX, 1);
			diff.insert(FType::ProgPow, 1);

			let hash = chain.txhashset().read().get_header_hash_by_height(pow::randomx::rx_current_seed_height(head.height + 1)).unwrap();
			let mut block = prepare_block_pow(&kc, &head, Difficulty::from_dic_number(diff), vec![], proof, hash);
			chain.set_txhashset_roots(&mut block);

			if let Err(e) = chain.process_block(block, chain::Options::MINE) {
				panic!("The proof need to be valid: {}", e);
			}
		};

		then regex "the chain need to be on the height <([0-9]+)>" |world, matches, _step| {
			let chain = world.chain.as_ref().unwrap();
			let chain_height = chain.head_header().unwrap().height;
			let height_choosed: u64 = matches[1].parse().unwrap();
			assert_eq!(chain_height, height_choosed);
		};

		then "I fork and mine in the chain lost" |world, _step| {
			let chain = world.chain.as_ref().unwrap();
			let kc = world.keychain.as_ref().unwrap();
			let prev = chain.head_header().unwrap();

			// add a first block we'll be forking from
			// prepare the 2 successor, sibling blocks, one with lower diff
			let b2 = prepare_block(kc, &prev, &chain, 4);
			let b2head = b2.header.clone();
			let bfork = prepare_block(kc, &prev, &chain, 3);

			// add higher difficulty first, prepare its successor, then fork
			// with lower diff
			chain.process_block(b2, chain::Options::SKIP_POW).unwrap();
			assert_eq!(chain.head_header().unwrap().hash(), b2head.hash());
			let b3 = prepare_block(kc, &b2head, &chain, 5);
			chain
				.process_block(bfork, chain::Options::SKIP_POW)
				.unwrap();

			// adding the successor
			let b3head = b3.header.clone();
			chain.process_block(b3, chain::Options::SKIP_POW).unwrap();
			assert_eq!(chain.head_header().unwrap().hash(), b3head.hash());
		};

		then "I spend in different forks" |world, _step| {
			let chain = world.chain.as_ref().unwrap();
			let kc = world.keychain.as_ref().unwrap();
			let mut fork_head = chain.head_header().unwrap();

			// mine the first block and keep track of the block_hash
			// so we can spend the coinbase later
			let b = prepare_block(kc, &fork_head, &chain, 2);
			let out_id = OutputIdentifier::from_output(&b.outputs()[0]);
			assert!(out_id.features.is_coinbase());
			fork_head = b.header.clone();
			chain
				.process_block(b.clone(), chain::Options::SKIP_POW)
				.unwrap();

			// now mine three further blocks
			for n in 3..6 {
				let b = prepare_block(kc, &fork_head, &chain, n);
				fork_head = b.header.clone();
				chain.process_block(b, chain::Options::SKIP_POW).unwrap();
			}

			// Check the height of the "fork block".
			assert_eq!(fork_head.height, 4);
			let key_id2 = epic_keychain::ExtKeychainPath::new(1, 2, 0, 0, 0).to_identifier();
			let key_id30 = epic_keychain::ExtKeychainPath::new(1, 30, 0, 0, 0).to_identifier();
			let key_id31 = epic_keychain::ExtKeychainPath::new(1, 31, 0, 0, 0).to_identifier();

			let tx1 = build::transaction(
				vec![
					build::coinbase_input(consensus::REWARD, key_id2.clone()),
					build::output(consensus::REWARD - 20000, key_id30.clone()),
					build::with_fee(20000),
				],
				kc,
			)
			.unwrap();

			let next = prepare_block_tx(kc, &fork_head, &chain, 7, vec![&tx1]);
			let prev_main = next.header.clone();
			chain
				.process_block(next.clone(), chain::Options::SKIP_POW)
				.unwrap();
			chain.validate(false).unwrap();

			let tx2 = build::transaction(
				vec![
					build::input(consensus::REWARD - 20000, key_id30.clone()),
					build::output(consensus::REWARD - 40000, key_id31.clone()),
					build::with_fee(20000),
				],
				kc,
			)
			.unwrap();

			let next = prepare_block_tx(kc, &prev_main, &chain, 9, vec![&tx2]);
			let prev_main = next.header.clone();
			chain.process_block(next, chain::Options::SKIP_POW).unwrap();

			// Full chain validation for completeness.
			chain.validate(false).unwrap();

			// mine 2 forked blocks from the first
			let fork = prepare_fork_block_tx(kc, &fork_head, &chain, 6, vec![&tx1]);
			let prev_fork = fork.header.clone();
			chain.process_block(fork, chain::Options::SKIP_POW).unwrap();

			let fork_next = prepare_fork_block_tx(kc, &prev_fork, &chain, 8, vec![&tx2]);
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
			let fork_next = prepare_fork_block(kc, &prev_fork, &chain, 10);
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

			chain.validate(false).unwrap();

			if let Err(e) = chain.compact() {
				panic!("Error compacting chain: {:?}", e);
			}
			if let Err(e) = chain.validate(false) {
				panic!("Validation error after compacting chain: {:?}", e);
			}
		};

		then "I check outputs in the header" |world, _step| {
			let mut reward_outputs = vec![];

			let chain = world.chain.as_ref().unwrap();
			let kc = world.keychain.as_ref().unwrap();

			for n in 1..15 {
				let prev = chain.head_header().unwrap();
				let next_header_info = consensus::next_difficulty(
					1,
					(&prev.pow.proof).into(),
					chain.difficulty_iter().unwrap());
				let pk = epic_keychain::ExtKeychainPath::new(1, n as u32, 0, 0, 0).to_identifier();
				let reward = libtx::reward::output(kc, &pk, 0, false, n).unwrap();
				reward_outputs.push(reward.0.clone());
				let mut b =
					core::core::Block::new(&prev, vec![], next_header_info.clone().difficulty, reward)
						.unwrap();
				b.header.timestamp = prev.timestamp + Duration::seconds(60);
				b.header.pow.secondary_scaling = next_header_info.secondary_scaling;
				b.header.bottles = next_block_bottles(FType::Cuckatoo, &prev.bottles);

				let hash = chain.txhashset().read().get_header_hash_by_height(pow::randomx::rx_current_seed_height(prev.height + 1)).unwrap();
				let mut seed = [0u8; 32];
				seed.copy_from_slice(&hash.as_bytes()[0..32]);

				b.header.pow.seed = seed;

				chain.set_txhashset_roots(&mut b).unwrap();

				let edge_bits = if n == 2 {
					global::min_edge_bits() + 1
				} else {
					global::min_edge_bits()
				};

				if let pow::Proof::CuckooProof { edge_bits: ref mut bits, .. } = b.header.pow.proof {
					*bits = edge_bits;
				}
				pow::pow_size(
					&mut b.header,
					next_header_info.difficulty,
					global::proofsize(),
					edge_bits,
				)
				.unwrap();
				if let pow::Proof::CuckooProof { edge_bits: ref mut bits, .. } = b.header.pow.proof {
					*bits = edge_bits;
				}

				chain.process_block(b, chain::Options::MINE).unwrap();

				let header_for_output = chain
					.get_header_for_output(&OutputIdentifier::from_output(&reward_outputs[(n - 1) as usize]))
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
		};

		given regex "I have the policy <([0-9]+)> with <([a-zA-Z0-9]+)> equals <([0-9]+)>" |world, matches, _step| {
			// let index: usize = matches[1].parse().unwrap();
			let algorithm = matches[2].as_str();
			let value: u32 = matches[3].parse().unwrap();
			match algorithm {
				"progpow" => {
					world.policy.insert(FType::ProgPow, value);
				},
				"randomx" => {
					world.policy.insert(FType::RandomX, value);
				},
				"cuckaroo" => {
					world.policy.insert(FType::Cuckaroo, value);
				},
				"cuckatoo" => {
					world.policy.insert(FType::Cuckatoo, value);
				}
				_ => {panic!("algorithm not supported")}
			};
		};

		given "I setup all the policies" |world, _step|{
			set_policy_config(PolicyConfig {
				policies: vec![world.policy.clone()],
				..Default::default()
			});
			// reset the policy
			world.policy = get_bottles_default();
		};

		then regex "Next block needs to be <([a-zA-Z0-9]+)>" |world, matches, _step| {
			let chain = world.chain.as_ref().unwrap();
			let prev = chain.head_header().unwrap();
			let algo = get_fw_type(matches[1].as_str());
			let emitted_policy = get_emitted_policy();
			let policy = get_policies(emitted_policy).unwrap();
			assert_eq!(Deterministic::choose_algo(&policy, &prev.bottles), algo);
		};

		then regex "Check the next algorithm <([a-zA-Z0-9]+)>" |world, matches, _step| {
			let algo = get_fw_type(matches[1].as_str());
			let emitted_policy = get_emitted_policy();
			let policy = get_policies(emitted_policy).unwrap();
			assert_eq!(Deterministic::choose_algo(&policy, &world.bottles), algo);
		};

		then regex "Increase bottles <([A-Za-z0-9]+)>" |world, matches, _step| {
			let algo = get_fw_type(matches[1].as_str());
			world.bottles = next_block_bottles(algo, &world.bottles);
		};

		given regex "I setup a chain with genesis block mined with <([a-zA-Z0-9]+)>" |world, matches, _step| {
			let algo = get_fw_type(matches[1].as_str());
			let mut genesis = pow::mine_genesis_block().unwrap();
			genesis.header.bottles = next_block_bottles(algo, &world.bottles);
			world.genesis = Some(genesis);
			world.chain = Some(setup(&world.output_dir, world.genesis.as_ref().unwrap().clone()));
			world.keychain = Some(epic_keychain::ExtKeychain::from_seed(&[2,0,0], false).unwrap());
		};

		given regex "I add a genesis block with coinbase and mined with <([a-zA-Z0-9]+)>" |world, matches, _step| {
			let algo = get_fw_type(matches[1].as_str());
			let key_id = epic_keychain::ExtKeychain::derive_key_id(0, 1, 0, 0, 0);

			let reward = reward::output(world.keychain.as_ref().unwrap(), &key_id, 0, false, 0).unwrap();
			// creating a placeholder for the genesis block
			let mut genesis = genesis::genesis_dev();
			// creating the block with the desired reward
			genesis = genesis.with_reward(reward.0, reward.1);
			genesis.header.bottles = next_block_bottles(algo, &world.bottles);

			// mining "manually" the genesis
			let genesis_difficulty = genesis.header.pow.total_difficulty.clone();
			let sz = global::min_edge_bits();
			let proof_size = global::proofsize();

			pow::pow_size(&mut genesis.header, genesis_difficulty, proof_size, sz).unwrap();
			world.genesis = Some(genesis);
		};

		given "I setup the chain for coinbase test" |world, _step| {
			let genesis_ref = world.genesis.as_mut().unwrap();

			// WIP: maybe we need to change this, since are 2 outputs ?
			//genesis_ref.header.output_mmr_size = 2;
			//genesis_ref.header.kernel_mmr_size = 2;
			let chain = setup(&world.output_dir, genesis_ref.clone());
			world.chain = Some(chain);
		};

		given "I add foundation wallet pubkeys" |world, _step| {
			// WIP: Add your personalized keychain here
			world.keychain = Some(epic_keychain::ExtKeychain::from_seed(&[2,0,0], false).unwrap());

			// WIP: maybe use this ?
			let kc = epic_keychain::ExtKeychain::from_mnemonic(
				"shop dignity online camera various front stay prosper bench dash learn chimney huge crush develop rack beauty prison wear manual harbor theory bachelor exile",
				"", false).unwrap();
			world.keychain_foundation = Some(kc);
		};

		given regex "I set the allowed policy on the height <([0-9]+)> with value <([0-9]+)>" |world, matches, _step| {
			let height: u64 = matches[1].parse().unwrap();
			let value: u64 = matches[2].parse().unwrap();
			add_allowed_policy(height, value);
		};

		given "I set default policy config" |world, _step| {
			let policies = [
				(FType::Cuckaroo, 100),
				(FType::Cuckatoo, 0),
				(FType::RandomX, 0),
				(FType::ProgPow, 0),
			]
			.into_iter()
			.map(|&x| x)
			.collect();

			let policies2 = [
				(FType::Cuckaroo, 0),
				(FType::Cuckatoo, 0),
				(FType::RandomX, 100),
				(FType::ProgPow, 0),
			]
			.into_iter()
			.map(|&x| x)
			.collect();

			let policies3 = [
				(FType::Cuckaroo, 0),
				(FType::Cuckatoo, 0),
				(FType::RandomX, 0),
				(FType::ProgPow, 100),
			]
			.into_iter()
			.map(|&x| x)
			.collect();

			set_policy_config(PolicyConfig {
				policies: vec![policies, policies2, policies3],
				..Default::default()
			});
		};

		then regex "I add block with foundation reward following the policy <([0-9]+)>" |world, matches, _step| {
			let num: u64 = matches[1].parse().unwrap();
			// The policy index is ignored for now, as we are only using a unique policy.
			// index = matches[2].parse().unwrap();
			let chain = world.chain.as_ref().unwrap();
			let kc_foundation = world.keychain_foundation.as_ref().unwrap();
			let kc = world.keychain.as_ref().unwrap();
			// println!("Keychain: {:?}", kc);

			let prev = chain.head_header().unwrap();
			let diff = prev.height + 1;
			let transactions: Vec<&Transaction>  = vec![];
			let key_id = epic_keychain::ExtKeychainPath::new(3, 0, 0, diff as u32, 0).to_identifier();
			println!("Key_id: {:?}\n", key_id);
			let fees = transactions.iter().map(|tx| tx.fee()).sum();
			let mining_reward = libtx::reward::output(kc, &key_id, fees, false, 0).unwrap();
			let foundation_reward = load_foundation_output(prev.height + 1);
			println!("\nFoundation Reward:{:?}\n", foundation_reward);
			// Creating the block
			let hash = chain.txhashset().read().get_header_hash_by_height(pow::randomx::rx_current_seed_height(prev.height + 1)).unwrap();
			let mut block = prepare_block_with_coinbase(&prev, diff, transactions, mining_reward, (foundation_reward.output, foundation_reward.kernel), hash);
			chain.set_txhashset_roots(&mut block).unwrap();
			// Mining
			let emitted_policy = get_emitted_policy();
			let policy = get_policies(emitted_policy).unwrap();
			let algo = Deterministic::choose_algo(&policy, &prev.bottles);
			block.header.bottles = next_block_bottles(algo, &prev.bottles);
			block.header.pow.proof = get_pow_type(&algo, prev.height);
			block.header.policy = emitted_policy;
			chain.process_block(block, chain::Options::SKIP_POW).unwrap();
		};

		given "I have a hardcoded coinbase" |_world, _step|{
			let data = "{\"output\":{\"features\":\"Coinbase\",\"commit\":\"093a6ce3a05a7fdf810508eb5cf8611db8c60453df595f76016c6969bfcd5a60f7\",\"proof\":\"5a036cda20f01721621e0b8ba38b62791a3450c6d5155f76eb43622d14213f3ec25069e03a47d68cf8ffe69150cf3d13bd98a7bc83a09b647c99f740291f24f30b6797c5a1d8f4beb4c99c8a45092fbbf1624b027323e5bdd1169c0bbf3b2e393a9812cf2b61b7ff30e53ea8cfa16f18377b05c87ec77359f796c3531465cc069b6295d504640c5bea894062d99c47dfa66dfce518cb527e9d706e333f3e4ae3bb0a242131ffe95e9e6e7e087f8b05cbde4e3c7ecb1a4b9f6c19eabd01323acba029103d0c291f028b9cd8bbf79259e49dec1d291278bf934a0229c8909f1b743184e76b31fd72e3e808448fa5874b070628e8f0184c49e7ddf43ed07d74fddc945f7a1a518dd2ccce85403c5348a19292550eb3ffd0ba65ef9bb2a5d73646dd02d253c1bc7c84d4b1498fc87f022be7a15c472e728ec6429a50e5ec1722b34881c0b0829d8712a77044210871e38509dcb4aa4534f923cb06db471d902676e308730066dbad7ed503613407d3510982e9d1ec9ce0264ab45ed2e1fb80a3222be2ddc0b19a0445f7228300c0013e7a336bb846bc232c2d18b08db9bbf743dc18cdcb415a8253b2ef10ba13ab20567483e5bbd31a116f68447701b51e0ce7f59b8052de09a959e0b80164ea0a6cc9f696380d7dd3b341856f42969086e467c17c6152555b722ade0dbfc20b3b786330afc8f61ea1804a357b1f15b7d339d653a2dfaf363e5fba23b296259eb93c0cdc4963e852f2bc596717a1add6dafbb26bae1b5c367c6200252166cfe5fc5c0f903b80e31eb2cc3f361e2f8a45f48a5adccd56f6e867f5cc6b46bddca515936011eb5bfc1ad1c451ec373b88cb81a5af63179515c3723394a09e75b0a738ead9ad15b1e0702548c1a9b5b4b33128c9167f382a802f971ccd9c2e9922f2c38f68a45782e916fc32d808fe9e7eea1a54992dfe371950\"},\"kernel\":{\"features\":\"Coinbase\",\"fee\":\"0\",\"lock_height\":\"0\",\"excess\":\"085987148fb0808195e9cf593164068f2783128d33c30d3dbc3115244771f9aab2\",\"excess_sig\":\"072570eca6b8744d06bae9937e6fd8911c27f6dcf1aabf595277359b309a0ef094c300d376026131dd21ac1f9a4c68872188cca850950ef16b8b85df94b551fb\"},\"key_id\":\"0300000000000000000000000500000000\"}";
			let coinbase: CbData = serde_json::from_str(&data).unwrap();
			println!("Coinbase: {:?}\n", coinbase);
			coinbase.kernel.verify().unwrap();
		};

		then regex "I add <([0-9]+)> blocks following the policy <([0-9]+)>" |world, matches, _step| {
			let num: u64 = matches[1].parse().unwrap();
			let emitted_policy: u8 = matches[2].parse().unwrap();
			let chain = world.chain.as_ref().unwrap();
			let height = chain.head_header().unwrap().height;

			for i in 0..num {
				let kc = epic_keychain::ExtKeychain::from_seed(&[i as u8], false).unwrap().clone();
				let prev = chain.head_header().unwrap();
				let mut block = prepare_block(&kc, &prev, &chain, height + i);
				let policy = get_policies(emitted_policy).unwrap();
				let cursor = chain.bottles_iter(emitted_policy).unwrap();
				let (algo, bottles) = consensus::next_policy(emitted_policy, cursor);
				block.header.bottles = bottles;
				block.header.pow.proof = get_pow_type(&algo, prev.height);
				block.header.policy = emitted_policy;
				chain.process_block(block, chain::Options::SKIP_POW).unwrap();
			};
		};

		then regex "I add <([0-9]+)> blocks mined with <([a-zA-Z0-9]+)> and accept <([0-9]+)>" |world, matches, _step| {
			let num: u64 = matches[1].parse().unwrap();
			let algo = get_fw_type(matches[2].as_str());
			let num_accepted: u64 = matches[3].parse().unwrap();
			let chain = world.chain.as_ref().unwrap();
			let mut count : u64 = 0;

			for i in 0..num {
				let kc = epic_keychain::ExtKeychain::from_seed(&[i as u8], false).unwrap().clone();
				let prev = chain.head_header().unwrap();
				let mut block = prepare_block(&kc, &prev, &chain, prev.height + 1);
				block.header.bottles = next_block_bottles(algo, &prev.bottles);
				block.header.pow.proof = get_pow_type(&algo, prev.height);
				count = match chain.process_block(block, chain::Options::SKIP_POW){
					Err(_) => count,
					_ => count + 1,
				};
			};
			assert_eq!(count, num_accepted);
		};

		then "I check if the bottle matches the policy" |world, _step| {
			let chain = world.chain.as_ref().unwrap();
			let bottles = chain.head_header().unwrap().bottles;
			let emitted_policy = get_emitted_policy();
			let policy = get_policies(emitted_policy).unwrap();
			assert_eq!(bottles, policy);
		};

		then "I check if the bottle is being emptied" |world, _step| {
			let chain = world.chain.as_ref().unwrap();
			let bottles = chain.head_header().unwrap().bottles;
			assert_eq!(count_beans(&bottles), 1);
		};
	});

	fn get_pow_type(ftype: &FType, seed: u64) -> pow::Proof {
		match ftype {
			FType::Cuckaroo => pow::Proof::CuckooProof {
				edge_bits: 29,
				nonces: vec![seed; 3],
			},
			FType::RandomX => pow::Proof::RandomXProof {
				hash: [seed as u8; 32],
			},
			FType::Cuckatoo => pow::Proof::CuckooProof {
				edge_bits: 31,
				nonces: vec![seed; 3],
			},
			_ => panic!("algorithm not supported"),
		}
	}

	fn get_fw_type(s: &str) -> FType {
		match s {
			"progpow" => FType::ProgPow,
			"randomx" => FType::RandomX,
			"cuckaroo" => FType::Cuckaroo,
			"cuckatoo" => FType::Cuckatoo,
			_ => panic!("algorithm not supported"),
		}
	}

	fn clean_output_dir(dir_name: &str) {
		let _ = fs::remove_dir_all(dir_name);
	}

	fn setup(dir_name: &str, genesis: Block) -> Chain {
		util::init_test_logger();
		clean_output_dir(dir_name);
		let verifier_cache = Arc::new(RwLock::new(LruVerifierCache::new()));
		chain::Chain::init(
			dir_name.to_string(),
			Arc::new(NoopAdapter {}),
			genesis,
			pow::verify_size,
			verifier_cache,
			false,
			//Arc::new(Mutex::new(StopState::new())),
		)
		.unwrap()
	}

	enum PoWType {
		Cuckaroo,
		Cuckatoo,
		MD5,
		RandomX,
		ProgPow,
	}

	fn pow_size_custom(
		bh: &mut BlockHeader,
		diff: Difficulty,
		proof_size: usize,
		sz: u8,
		pow_type: &PoWType,
	) -> Result<(), Error> {
		let start_nonce = bh.pow.nonce;
		// set the nonce for faster solution finding in user testing
		if bh.height == 0 && global::is_user_testing_mode() {
			bh.pow.nonce = global::get_genesis_nonce();
		}

		// try to find a cuckoo cycle on that header hash
		loop {
			// if we found a cycle (not guaranteed) and the proof hash is higher that the
			// diff, we're all good
			let mut ctx = create_pow_context_custom::<u32>(
				bh.height,
				sz,
				proof_size,
				MAX_SOLS,
				pow_type,
				bh.pow.seed,
			)?;

			if let pow::Proof::CuckooProof { .. } = bh.pow.proof {
				ctx.set_header_nonce(bh.pre_pow(), None, Some(bh.height), true)?;
			} else {
				ctx.set_header_nonce(bh.pre_pow(), Some(bh.pow.nonce), Some(bh.height), true)?;
			}

			if let Ok(proofs) = ctx.pow_solve() {
				bh.pow.proof = proofs[0].clone();
				if bh.pow.to_difficulty(&bh.pre_pow(), bh.height, bh.pow.nonce) >= diff {
					return Ok(());
				}
			}

			// otherwise increment the nonce
			let (res, _) = bh.pow.nonce.overflowing_add(1);
			bh.pow.nonce = res;

			// and if we're back where we started, update the time (changes the hash as
			// well)
			if bh.pow.nonce == start_nonce {
				bh.timestamp = DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(0, 0), Utc);
			}
		}
	}

	fn create_pow_context_custom<T>(
		_height: u64,
		edge_bits: u8,
		proof_size: usize,
		max_sols: u32,
		pow_type: &PoWType,
		seed: [u8; 32],
	) -> Result<Box<dyn PoWContext<T>>, pow::Error>
	where
		T: EdgeType + 'static,
	{
		match pow_type {
			// Mainnet has Cuckaroo29 for AR and Cuckatoo30+ for AF
			PoWType::Cuckaroo => new_cuckaroo_ctx(edge_bits, proof_size),
			PoWType::Cuckatoo => new_cuckatoo_ctx(edge_bits, proof_size, max_sols),
			PoWType::MD5 => new_md5_ctx(edge_bits, proof_size, max_sols),
			PoWType::RandomX => new_randomx_ctx(seed),
			PoWType::ProgPow => new_progpow_ctx(),
		}
	}

	fn mine_some_on_top<K>(dir: &str, genesis: Block, keychain: &K, pow_type: &PoWType)
	where
		K: Keychain,
	{
		let chain = setup(dir, genesis);

		for n in 1..4 {
			let prev = chain.head_header().unwrap();
			let next_header_info = consensus::next_difficulty(
				1,
				(&prev.pow.proof).into(),
				chain.difficulty_iter().unwrap(),
			);
			let pk = epic_keychain::ExtKeychainPath::new(1, n as u32, 0, 0, 0).to_identifier();
			let reward = libtx::reward::output(keychain, &pk, 0, false, 0).unwrap();
			let mut b =
				core::core::Block::new(&prev, vec![], next_header_info.clone().difficulty, reward)
					.unwrap();
			b.header.timestamp = prev.timestamp + Duration::seconds(60);
			b.header.pow.secondary_scaling = next_header_info.secondary_scaling;

			b.header.bottles = next_block_bottles(
				match pow_type {
					PoWType::RandomX => FType::RandomX,
					PoWType::ProgPow => FType::ProgPow,
					PoWType::Cuckatoo => FType::Cuckatoo,
					PoWType::Cuckaroo => FType::Cuckaroo,
					PoWType::MD5 => FType::Cuckatoo,
				},
				&prev.bottles,
			);

			let hash = chain
				.txhashset()
				.read()
				.get_header_hash_by_height(pow::randomx::rx_current_seed_height(prev.height + 1))
				.unwrap();
			let mut seed = [0u8; 32];
			seed.copy_from_slice(&hash.as_bytes()[0..32]);

			b.header.pow.seed = seed;
			chain.set_txhashset_roots(&mut b).unwrap();

			let edge_bits = if n == 2 {
				global::min_edge_bits() + 1
			} else {
				global::min_edge_bits()
			};
			match b.header.pow.proof {
				pow::Proof::CuckooProof {
					edge_bits: ref mut bits,
					..
				} => *bits = edge_bits,
				pow::Proof::MD5Proof {
					edge_bits: ref mut bits,
					..
				} => *bits = edge_bits,
				_ => {}
			};
			pow_size_custom(
				&mut b.header,
				next_header_info.difficulty,
				global::proofsize(),
				edge_bits,
				pow_type,
			)
			.unwrap();
			match b.header.pow.proof {
				pow::Proof::CuckooProof {
					edge_bits: ref mut bits,
					..
				} => *bits = edge_bits,
				pow::Proof::MD5Proof {
					edge_bits: ref mut bits,
					..
				} => *bits = edge_bits,
				_ => {}
			};

			let bhash = b.hash();
			chain.process_block(b, chain::Options::MINE).unwrap();

			// checking our new head
			let head = chain.head().unwrap();
			assert_eq!(head.height, n);
			assert_eq!(head.last_block_h, bhash);

			// now check the block_header of the head
			let header = chain.head_header().unwrap();
			assert_eq!(header.height, n);
			assert_eq!(header.hash(), bhash);

			// now check the block itself
			let block = chain.get_block(&header.hash()).unwrap();
			assert_eq!(block.header.height, n);
			assert_eq!(block.hash(), bhash);
			assert_eq!(block.outputs().len(), 1);

			// now check the block height index
			let header_by_height = chain.get_header_by_height(n).unwrap();
			assert_eq!(header_by_height.hash(), bhash);

			chain.validate(false).unwrap();
		}
	}

	fn prepare_block<K>(kc: &K, prev: &BlockHeader, chain: &Chain, diff: u64) -> Block
	where
		K: Keychain,
	{
		let hash = chain
			.txhashset()
			.read()
			.get_header_hash_by_height(pow::randomx::rx_current_seed_height(prev.height + 1))
			.unwrap();
		let mut b = prepare_block_nosum(kc, prev, diff, vec![], hash);
		chain.set_txhashset_roots(&mut b).unwrap();
		b
	}

	fn prepare_fork_block<K>(kc: &K, prev: &BlockHeader, chain: &Chain, diff: u64) -> Block
	where
		K: Keychain,
	{
		let hash = chain
			.txhashset()
			.read()
			.get_header_hash_by_height(pow::randomx::rx_current_seed_height(prev.height + 1))
			.unwrap();
		let mut b = prepare_block_nosum(kc, prev, diff, vec![], hash);
		chain.set_txhashset_roots_forked(&mut b, prev).unwrap();
		b
	}

	fn prepare_block_tx<K>(
		kc: &K,
		prev: &BlockHeader,
		chain: &Chain,
		diff: u64,
		txs: Vec<&Transaction>,
	) -> Block
	where
		K: Keychain,
	{
		let hash = chain
			.txhashset()
			.read()
			.get_header_hash_by_height(pow::randomx::rx_current_seed_height(prev.height + 1))
			.unwrap();
		let mut b = prepare_block_nosum(kc, prev, diff, txs, hash);
		chain.set_txhashset_roots(&mut b).unwrap();
		b
	}

	fn prepare_fork_block_tx<K>(
		kc: &K,
		prev: &BlockHeader,
		chain: &Chain,
		diff: u64,
		txs: Vec<&Transaction>,
	) -> Block
	where
		K: Keychain,
	{
		let hash = chain
			.txhashset()
			.read()
			.get_header_hash_by_height(pow::randomx::rx_current_seed_height(prev.height + 1))
			.unwrap();
		let mut b = prepare_block_nosum(kc, prev, diff, txs, hash);
		chain.set_txhashset_roots_forked(&mut b, prev).unwrap();
		b
	}

	fn prepare_block_nosum<K>(
		kc: &K,
		prev: &BlockHeader,
		diff: u64,
		txs: Vec<&Transaction>,
		hash: Hash,
	) -> Block
	where
		K: Keychain,
	{
		let mut seed = [0u8; 32];
		seed.copy_from_slice(&hash.as_bytes()[0..32]);

		let proof_size = global::proofsize();
		let key_id = epic_keychain::ExtKeychainPath::new(1, diff as u32, 0, 0, 0).to_identifier();
		let fees = txs.iter().map(|tx| tx.fee()).sum();
		let reward = libtx::reward::output(kc, &key_id, fees, false, 0).unwrap();
		let mut b = match core::core::Block::new(
			prev,
			txs.into_iter().cloned().collect(),
			Difficulty::from_num(diff),
			reward,
		) {
			Err(e) => panic!("{:?}", e),
			Ok(b) => b,
		};
		b.header.timestamp = prev.timestamp + Duration::seconds(60);
		b.header.pow.total_difficulty = prev.total_difficulty() + Difficulty::from_num(diff);
		b.header.pow.proof = pow::Proof::random(proof_size);
		b.header.pow.seed = seed;
		b.header.bottles = next_block_bottles(FType::Cuckatoo, &prev.bottles);
		b
	}

	fn prepare_block_with_coinbase(
		prev: &BlockHeader,
		diff: u64,
		txs: Vec<&Transaction>,
		reward: (Output, TxKernel),
		foundation: (Output, TxKernel),
		hash: Hash,
	) -> Block {
		let proof_size = global::proofsize();
		let mut b = match core::core::Block::new_with_coinbase(
			prev,
			txs.into_iter().cloned().collect(),
			Difficulty::from_num(diff),
			reward,
			foundation,
		) {
			Err(e) => panic!("{:?}", e),
			Ok(b) => b,
		};
		b.header.timestamp = prev.timestamp + Duration::seconds(60);
		b.header.pow.total_difficulty = prev.total_difficulty() + Difficulty::from_num(diff);
		b.header.pow.proof = pow::Proof::random(proof_size);
		b.header.bottles = next_block_bottles(FType::Cuckatoo, &prev.bottles);

		let mut seed = [0u8; 32];
		seed.copy_from_slice(&hash.as_bytes()[0..32]);
		b.header.pow.seed = seed;

		b
	}

	fn prepare_block_pow<K>(
		kc: &K,
		prev: &BlockHeader,
		diff: Difficulty,
		txs: Vec<&Transaction>,
		proof: pow::Proof,
		hash: Hash,
	) -> Block
	where
		K: Keychain,
	{
		let proof_size = global::proofsize();
		let key_id = epic_keychain::ExtKeychainPath::new(1, 3, 0, 0, 0).to_identifier();

		let fees = txs.iter().map(|tx| tx.fee()).sum();
		let reward = libtx::reward::output(kc, &key_id, fees, true, 0).unwrap();
		let mut b = match core::core::Block::new(
			prev,
			txs.into_iter().cloned().collect(),
			diff.clone(),
			reward,
		) {
			Err(e) => panic!("{:?}", e),
			Ok(b) => b,
		};
		b.header.timestamp = prev.timestamp + Duration::seconds(60);
		b.header.pow.total_difficulty = prev.total_difficulty() + diff;
		b.header.pow.proof = proof;

		let fw_type = match b.header.pow.proof {
			pow::Proof::CuckooProof { ref edge_bits, .. } => {
				if *edge_bits == 29 {
					FType::Cuckaroo
				} else {
					FType::Cuckatoo
				}
			}
			pow::Proof::RandomXProof { .. } => FType::RandomX,
			pow::Proof::ProgPowProof { .. } => FType::ProgPow,
			// just for the test
			pow::Proof::MD5Proof { .. } => FType::Cuckatoo,
		};
		b.header.bottles = next_block_bottles(fw_type, &prev.bottles);

		let mut seed = [0u8; 32];
		seed.copy_from_slice(&hash.as_bytes()[0..32]);
		b.header.pow.seed = seed;

		b
	}
}

// Declares a before handler function named `a_before_fn`
before!(a_before_fn => |_scenario| {
    println!("Test 1");

});

// Declares an after handler function named `an_after_fn`
after!(an_after_fn => |_scenario| {

});

// A setup function to be called before everything else
fn setup() {
	let policies = [
		(FType::Cuckaroo, 100),
		(FType::Cuckatoo, 0),
		(FType::RandomX, 0),
		(FType::ProgPow, 0),
	]
	.into_iter()
	.map(|&x| x)
	.collect();

	let policies2 = [
		(FType::Cuckaroo, 0),
		(FType::Cuckatoo, 0),
		(FType::RandomX, 100),
		(FType::ProgPow, 0),
	]
	.into_iter()
	.map(|&x| x)
	.collect();

	let policies3 = [
		(FType::Cuckaroo, 0),
		(FType::Cuckatoo, 0),
		(FType::RandomX, 0),
		(FType::ProgPow, 100),
	]
	.into_iter()
	.map(|&x| x)
	.collect();

	set_policy_config(PolicyConfig {
		policies: vec![policies, policies2, policies3],
		..Default::default()
	});

	util::init_test_logger();

	global::set_foundation_path("./tests/assets/foundation.json".to_string());
}

cucumber! {
	features: "./features", // Path to our feature files
	world: crate::EdnaWorld, // The world needs to be the same for steps and the main cucumber call
	steps: &[
		mine_chain::steps // the `steps!` macro creates a `steps` function in a module
	],
	setup: setup, // Optional; called once before everything
	before: &[
		a_before_fn // Optional; called before each scenario
	],
	after: &[
		an_after_fn // Optional; called after each scenario
	]
}
