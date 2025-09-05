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
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use self::chain::types::NoopAdapter;
use self::core::core::{Block, BlockHeader, Output, OutputFeatures, Transaction};
use self::core::{consensus, global, pow};
use self::keychain::{BlindSum, Identifier, Keychain, SwitchCommitmentType};
use self::util::RwLock;
use epic_chain as chain;
use epic_core as core;
use epic_core::core::block::feijoada::PoWType as FType;
use epic_core::core::hash::Hash;
use epic_core::libtx::build::Append;
use epic_keychain as keychain;
use epic_util as util;

use self::chain::Chain;
use self::core::libtx::proof::{self, ProofBuild};
use self::core::libtx::{self, Error, ProofBuilder};
use self::core::pow::Difficulty;
use chrono::Duration;
use epic_chain::{BlockStatus, ChainAdapter, Options};
use epic_core::core::block::feijoada::{next_block_bottles, Deterministic, Feijoada};
use epic_core::core::foundation::load_foundation_output;
use epic_core::core::TxKernel;
use epic_core::global::{get_emitted_policy, get_policies};
pub fn clean_output_dir(dir_name: &str) {
	let _ = fs::remove_dir_all(dir_name);
}

/// Adapter to retrieve last status
pub struct StatusAdapter {
	pub last_status: RwLock<Option<BlockStatus>>,
}

impl StatusAdapter {
	#[allow(dead_code)]
	pub fn new(last_status: RwLock<Option<BlockStatus>>) -> Self {
		StatusAdapter { last_status }
	}
}

impl ChainAdapter for StatusAdapter {
	fn block_accepted(&self, _b: &Block, status: BlockStatus, _opts: Options) {
		*self.last_status.write() = Some(status);
	}
}

/// Sets the foundation path for tests, cross-platform and robust for CI.
#[allow(dead_code)]
pub fn set_foundation_path_for_test(filename: &str) {
	global::set_mining_mode(global::ChainTypes::AutomatedTesting);
	let mut dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());

	loop {
		let candidate = dir.join("debian").join(filename);
		if candidate.exists() {
			epic_core::global::set_foundation_path(candidate.to_string_lossy().to_string());
			return;
		}
		if !dir.pop() {
			break;
		}
	}

	panic!(
		"Foundation file '{}' not found in any parent directory's debian/ folder.",
		filename
	);
}

pub fn init_chain(dir_name: &str, genesis: Block) -> Chain {
	let chain = chain::Chain::init(
		dir_name.to_string(),
		Arc::new(NoopAdapter {}),
		genesis.clone(),
		pow::verify_size,
		false,
	)
	.unwrap();
	chain
}

pub fn prepare_block<K>(
	kc: &K,
	prev: &BlockHeader,
	chain: &Chain,
	diff: u64,
	txs: Vec<&Transaction>,
	key_id_branch: u8,
) -> Block
where
	K: Keychain,
{
	let height = prev.height + 1;
	let key_id =
		epic_keychain::ExtKeychainPath::new(key_id_branch, height as u32, 0, 0, 0).to_identifier();

	let fees = txs.iter().map(|tx| tx.fee()).sum();
	let mining_reward =
		libtx::reward::output(kc, &ProofBuilder::new(kc), &key_id, fees, false, height).unwrap();

	let hash = chain
		.header_pmmr()
		.read()
		.get_header_hash_by_height(pow::randomx::rx_current_seed_height(height))
		.unwrap();

	let mut b = if consensus::is_foundation_height(height) {
		let foundation_reward = load_foundation_output(height);
		prepare_block_with_coinbase(
			prev,
			diff,
			txs.clone(),
			mining_reward.clone(),
			(foundation_reward.output, foundation_reward.kernel),
			hash,
		)
	} else {
		prepare_block_with_coinbase(
			prev,
			diff,
			txs.clone(),
			mining_reward.clone(),
			mining_reward.clone(), // pass mining_reward as both reward and "foundation" (ignored)
			hash,
		)
	};

	chain.set_txhashset_roots(&mut b).unwrap();

	let emitted_policy = get_emitted_policy(height);
	let policy = get_policies(emitted_policy).unwrap();
	let algo = Deterministic::choose_algo(&policy, &prev.bottles);
	b.header.bottles = next_block_bottles(algo, &prev.bottles);
	//b.header.pow.proof = get_pow_type(&algo, prev.height);
	b.header.pow.proof = get_pow_type(&algo, prev.height + key_id_branch as u64);
	b.header.policy = emitted_policy;
	b.header.timestamp = prev.timestamp + Duration::seconds(60 + key_id_branch as i64);
	b
}

// Convenience wrapper for processing a full block on the test chain.
// NOTE: This function is used in other integration test files.
#[allow(dead_code)]
pub fn process_header(chain: &Chain, header: &BlockHeader) {
	chain
		.process_block_header(header, chain::Options::SKIP_POW)
		.unwrap();
}

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
		FType::ProgPow => pow::Proof::ProgPowProof {
			mix: [seed as u8; 32],
		},
	}
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
	let mut b = match core::core::Block::from_coinbases(
		prev,
		txs.into_iter().cloned().collect(),
		reward,
		foundation,
		Difficulty::from_num(diff),
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

pub fn process_block(chain: &Chain, block: &Block) {
	chain
		.process_block(block.clone(), chain::Options::SKIP_POW)
		.unwrap();
}

/// Build a negative output. This function must not be used outside of tests.
/// The commitment will be an inversion of the value passed in and the value is
/// subtracted from the sum.
/// NOTE: This function is used in other integration test files.
#[allow(dead_code)]
pub fn build_output_negative<K, B>(value: u64, key_id: Identifier) -> Box<Append<K, B>>
where
	K: Keychain,
	B: ProofBuild,
{
	Box::new(
		move |build, acc| -> Result<(Transaction, BlindSum), Error> {
			let (tx, sum) = acc?;

			// TODO: proper support for different switch commitment schemes
			let switch = SwitchCommitmentType::Regular;

			let commit = build.keychain.commit(value, &key_id, &switch)?;

			// invert commitment
			let commit = build.keychain.secp().commit_sum(vec![], vec![commit])?;

			eprintln!("Building output: {}, {:?}", value, commit);

			// build a proof with a rangeproof of 0 as a placeholder
			// the test will replace this later
			let proof = proof::create(
				build.keychain,
				build.builder,
				0,
				&key_id,
				&switch,
				commit,
				None,
			)?;

			let out = Output {
				features: OutputFeatures::Plain,
				commit: commit,
				proof: proof,
			};

			// we return the output and the value is subtracted instead of added
			Ok((
				tx.with_output(out),
				sum.sub_key_id(key_id.to_value_path(value)),
			))
		},
	)
}

// NOTE: This function is used in other integration test files.
/// Creates a `Chain` instance with `StatusAdapter` attached to it.
#[allow(dead_code)]
pub fn setup_with_status_adapter(
	dir_name: &str,
	genesis: Block,
	adapter: Arc<StatusAdapter>,
) -> Chain {
	util::init_test_logger();
	clean_output_dir(dir_name);

	let chain = chain::Chain::init(
		dir_name.to_string(),
		adapter,
		genesis,
		pow::verify_size,
		false,
	)
	.unwrap();

	chain
}
