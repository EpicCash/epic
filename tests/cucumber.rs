#[macro_use]
extern crate cucumber_rust;

use epic_chain::Chain;
use epic_core::core::block::feijoada::PoWType as FType;
use epic_core::core::block::feijoada::{
	get_bottles_default, Deterministic, Feijoada, Policy, PolicyConfig,
};
use epic_core::core::Block;
use epic_core::global::set_policy_config;
use epic_keychain::keychain::ExtKeychain;
use epic_util as util;

pub struct EdnaWorld {
	pub output_dir: String,
	pub genesis: Option<Block>,
	pub keychain: Option<ExtKeychain>,
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
			chain: None,
			policy: Policy::new(),
			bottles: get_bottles_default(),
		}
	}
}

mod mine_chain {
	use cucumber_rust::steps;
	use epic_chain as chain;
	use epic_core as core;
	use epic_util as util;

	use epic_chain::types::NoopAdapter;
	use epic_chain::Chain;
	use epic_core::core::block::feijoada::PoWType as FType;
	use epic_core::core::block::feijoada::{
		count_beans, get_bottles_default, next_block_bottles, Deterministic, Feijoada, Policy,
		PolicyConfig,
	};
	use epic_core::core::hash::Hashed;
	use epic_core::core::verifier_cache::LruVerifierCache;
	use epic_core::core::{Block, BlockHeader, Output, OutputIdentifier, Transaction, TxKernel};
	use epic_core::global::{get_policies, set_policy_config, ChainTypes};
	use epic_core::libtx::{self, build, reward};
	use epic_core::pow::{
		new_cuckaroo_ctx, new_cuckatoo_ctx, new_md5_ctx, new_randomx_ctx, Difficulty, EdgeType,
		Error, PoWContext, new_progpow_ctx,
	};
	use epic_core::{consensus, pow};
	use epic_core::{genesis, global};
	use epic_keychain::Keychain;

	use chrono::prelude::{DateTime, NaiveDateTime, Utc};
	use chrono::Duration;

	use epic_util::{Mutex, RwLock, StopState};
	use std::fs;
	use std::sync::Arc;

	const MAX_SOLS: u32 = 10;

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
				genesis_ref.header.output_mmr_size = 1;
				genesis_ref.header.kernel_mmr_size = 1;
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

			let mut block = prepare_block_pow(&kc, &head, 3, vec![], proof);
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
						176, 216, 144, 11, 129, 186, 141, 9, 18, 217, 110,
						192, 237, 240, 176, 184, 159, 21, 216, 180, 18, 130,
						49, 91, 79, 84, 33, 92, 44, 178, 109, 131],
				},
				"randomx" => pow::Proof::RandomXProof {
					hash: [
						14, 249, 121, 13, 43, 74, 180, 213, 122, 194,
						147, 222, 255, 202, 4, 29, 11, 15, 23, 39, 21,
						47, 181, 240, 144, 96, 125, 172, 122, 45, 49, 227
					]
				},
				"md5" => pow::Proof::MD5Proof {
					edge_bits: 10,
					proof: "7ed7d6d5b405274bd11ea6971d1f9890".to_string()
				},
				"cuckoo" => pow::Proof::CuckooProof {
					edge_bits: 9,
					nonces: vec![27, 209, 166, 497]
				},
				_ => {panic!("invalid type proof")},
			};

			let mut block = prepare_block_pow(&kc, &head, 3, vec![], proof);
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
				let next_header_info = consensus::next_difficulty(1, chain.difficulty_iter().unwrap());
				let pk = epic_keychain::ExtKeychainPath::new(1, n as u32, 0, 0, 0).to_identifier();
				let reward = libtx::reward::output(kc, &pk, 0, false, prev.height + 1).unwrap();
				reward_outputs.push(reward.0.clone());
				let mut b =
					core::core::Block::new(&prev, vec![], next_header_info.clone().difficulty, reward)
						.unwrap();
				b.header.timestamp = prev.timestamp + Duration::seconds(60);
				b.header.pow.secondary_scaling = next_header_info.secondary_scaling;
				b.header.bottles = next_block_bottles(FType::Cuckatoo, &prev.bottles);

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
			assert_eq!(Deterministic::choose_algo(&get_policies(), &prev.bottles), algo);
		};

		then regex "Check the next algorithm <([a-zA-Z0-9]+)>" |world, matches, _step| {
			let algo = get_fw_type(matches[1].as_str());
			assert_eq!(Deterministic::choose_algo(&get_policies(), &world.bottles), algo);
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
			let genesis_difficulty = genesis.header.pow.total_difficulty;
			let sz = global::min_edge_bits();
			let proof_size = global::proofsize();
			pow::pow_size(&mut genesis.header, genesis_difficulty, proof_size, sz).unwrap();
			world.genesis = Some(genesis);
		};

		given "I setup the chain for coinbase test" |world, _step| {
			let chain = setup(&world.output_dir, world.genesis.as_ref().unwrap().clone());
			let genesis_ref = world.genesis.as_mut().unwrap();
			world.chain = Some(chain);
			// WIP: maybe we need to change this, since are 2 outputs ?
			genesis_ref.header.output_mmr_size = 1;
			genesis_ref.header.kernel_mmr_size = 1;
		};

		given "I add foundation wallet pubkeys" |world, _step| {
			// WIP: Add your personalized keychain here
			world.keychain = Some(epic_keychain::ExtKeychain::from_seed(&[2,0,0], false).unwrap());
			// WIP: maybe use this ?
			// let kc = epic_keychain::ExtKeychain::from_mnemonic(
			// 	"legal winner thank year wave sausage worth useful legal winner thank year wave sausage worth useful legal winner thank year wave sausage worth title",
			// 	"", false).unwrap();
		};

		then regex "I add <([0-9]+)> blocks with coinbase following the policy <([0-9]+)>" |world, matches, _step| {
			let num: u64 = matches[1].parse().unwrap();
			// The policy index is ignored for now, as we are only using a unique policy.
			// index = matches[2].parse().unwrap();
			let chain = world.chain.as_ref().unwrap();
			let kc = world.keychain.as_ref().unwrap();

			for i in 0..num {
				let prev = chain.head_header().unwrap();
				let diff = prev.height + 1;
				// WIP: This is the critical part to check coinbase and tax ?
				let transactions: Vec<&Transaction>  = vec![];
				let key_id = epic_keychain::ExtKeychainPath::new(1, diff as u32, 0, 0, 0).to_identifier();
				let fees = transactions.iter().map(|tx| tx.fee()).sum();
				let reward = libtx::reward::output(kc, &key_id, fees, false, 0).unwrap();
				// Creating the block
				let mut block = prepare_block_with_coinbase(&prev, diff, transactions, reward);
				chain.set_txhashset_roots(&mut block).unwrap();
				// Mining
				let algo = Deterministic::choose_algo(&get_policies(), &prev.bottles);
				block.header.bottles = next_block_bottles(algo, &prev.bottles);
				block.header.pow.proof = get_pow_type(&algo, prev.height);
				chain.process_block(block, chain::Options::SKIP_POW).unwrap();
			};
		};

		then regex "I add <([0-9]+)> blocks following the policy <([0-9]+)>" |world, matches, _step| {
			let num: u64 = matches[1].parse().unwrap();
			// The policy index is ignored for now, as we are only using a unique policy.
			// index = matches[2].parse().unwrap();
			let chain = world.chain.as_ref().unwrap();
			let height = chain.head_header().unwrap().height;

			for i in 0..num {
				let kc = epic_keychain::ExtKeychain::from_seed(&[i as u8], false).unwrap().clone();
				let prev = chain.head_header().unwrap();
				let mut block = prepare_block(&kc, &prev, &chain, height + i);
				let algo = Deterministic::choose_algo(&get_policies(), &prev.bottles);
				block.header.bottles = next_block_bottles(algo, &prev.bottles);
				block.header.pow.proof = get_pow_type(&algo, prev.height);
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
			assert_eq!(bottles, get_policies());
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
		ProgPow
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
			let mut ctx =
				create_pow_context_custom::<u32>(bh.height, sz, proof_size, MAX_SOLS, pow_type)?;
			ctx.set_header_nonce(bh.pre_pow(), Some(bh.pow.nonce), Some(bh.height), true)?;
			if let Ok(proofs) = ctx.pow_solve() {
				bh.pow.proof = proofs[0].clone();
				if bh.pow.to_difficulty(bh.height) >= diff {
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
	) -> Result<Box<dyn PoWContext<T>>, pow::Error>
	where
		T: EdgeType + 'static,
	{
		match pow_type {
			// Mainnet has Cuckaroo29 for AR and Cuckatoo30+ for AF
			PoWType::Cuckaroo => new_cuckaroo_ctx(edge_bits, proof_size),
			PoWType::Cuckatoo => new_cuckatoo_ctx(edge_bits, proof_size, max_sols),
			PoWType::MD5 => new_md5_ctx(edge_bits, proof_size, max_sols),
			PoWType::RandomX => new_randomx_ctx([0; 32]),
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
			let next_header_info = consensus::next_difficulty(1, chain.difficulty_iter().unwrap());
			let pk = epic_keychain::ExtKeychainPath::new(1, n as u32, 0, 0, 0).to_identifier();
			let reward = libtx::reward::output(keychain, &pk, 0, false, n).unwrap();
			let mut b =
				core::core::Block::new(&prev, vec![], next_header_info.clone().difficulty, reward)
					.unwrap();
			b.header.timestamp = prev.timestamp + Duration::seconds(60);
			b.header.pow.secondary_scaling = next_header_info.secondary_scaling;
			b.header.bottles = next_block_bottles(match pow_type {
				PoWType::RandomX => FType::RandomX,
				PoWType::ProgPow => FType::ProgPow,
				PoWType::Cuckatoo => FType::Cuckatoo,
				PoWType::Cuckaroo => FType::Cuckaroo,
				PoWType::MD5 => FType::Cuckatoo,
			}, &prev.bottles);
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
		let mut b = prepare_block_nosum(kc, prev, diff, vec![]);
		chain.set_txhashset_roots(&mut b).unwrap();
		b
	}

	fn prepare_fork_block<K>(kc: &K, prev: &BlockHeader, chain: &Chain, diff: u64) -> Block
	where
		K: Keychain,
	{
		let mut b = prepare_block_nosum(kc, prev, diff, vec![]);
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
		let mut b = prepare_block_nosum(kc, prev, diff, txs);
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
		let mut b = prepare_block_nosum(kc, prev, diff, txs);
		chain.set_txhashset_roots_forked(&mut b, prev).unwrap();
		b
	}

	fn prepare_block_nosum<K>(
		kc: &K,
		prev: &BlockHeader,
		diff: u64,
		txs: Vec<&Transaction>,
	) -> Block
	where
		K: Keychain,
	{
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
		b.header.bottles = next_block_bottles(FType::Cuckatoo, &prev.bottles);
		b
	}

	fn prepare_block_with_coinbase(
		prev: &BlockHeader,
		diff: u64,
		txs: Vec<&Transaction>,
		reward: (Output, TxKernel),
	) -> Block {
		let proof_size = global::proofsize();
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
		b.header.bottles = next_block_bottles(FType::Cuckatoo, &prev.bottles);
		b
	}

	fn prepare_block_pow<K>(
		kc: &K,
		prev: &BlockHeader,
		diff: u64,
		txs: Vec<&Transaction>,
		proof: pow::Proof,
	) -> Block
	where
		K: Keychain,
	{
		let proof_size = global::proofsize();
		let key_id = epic_keychain::ExtKeychainPath::new(1, diff as u32, 0, 0, 0).to_identifier();

		let fees = txs.iter().map(|tx| tx.fee()).sum();
		let reward = libtx::reward::output(kc, &key_id, fees, true, 0).unwrap();

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
	println!("Test 2");

	let policies = [
		(FType::Cuckaroo, 100),
		(FType::Cuckatoo, 0),
		(FType::RandomX, 0),
		(FType::ProgPow, 0),
	]
	.into_iter()
	.map(|&x| x)
	.collect();

	set_policy_config(PolicyConfig {
		policies: vec![policies],
		..Default::default()
	});

	util::init_test_logger();
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
