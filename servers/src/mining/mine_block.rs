// Copyright 2018 The Grin Developers
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

//! Build a block to mine: gathers transactions from the pool, assembles
//! them into a block and returns it.

use crate::api;
use crate::chain;
use crate::common::types::Error;
use crate::core::consensus::is_foundation_height;
use crate::core::core::foundation::load_foundation_output;
pub use crate::core::core::foundation::CbData;
use crate::core::global::get_emitted_policy;
use crate::core::libtx::ProofBuilder;
use crate::core::pow::randomx::rx_current_seed_height;
use crate::core::pow::PoWType;
use crate::core::{consensus, core, global};
use crate::keychain::{ExtKeychain, Identifier, Keychain};
use chrono::prelude::{DateTime, NaiveDateTime, Utc};
use rand::{thread_rng, Rng};
use serde_json::{json, Value};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::core::core::block_fees::BlockFees;
use crate::ServerTxPool;

// Ensure a block suitable for mining is built and returned
// If a wallet listener URL is not provided the reward will be "burnt"
// Warning: This call does not return until/unless a new block can be built
pub fn get_block(
	chain: &Arc<chain::Chain>,
	tx_pool: &ServerTxPool,
	key_id: Option<Identifier>,
	wallet_listener_url: Option<String>,
) -> (core::Block, BlockFees, PoWType) {
	let wallet_retry_interval = 5;
	// get the latest chain state and build a block on top of it
	let mut result = build_block(chain, tx_pool, key_id.clone(), wallet_listener_url.clone());
	while let Err(e) = result {
		let mut new_key_id = key_id.to_owned();
		match e {
			self::Error::Chain(c) => match c.kind() {
				chain::ErrorKind::DuplicateCommitment(_) => {
					debug!(
						"Duplicate commit for potential coinbase detected. Trying next derivation."
					);
					// use the next available key to generate a different coinbase commitment
					new_key_id = None;
				}
				_ => {
					error!("Chain Error: {}", c);
				}
			},
			self::Error::WalletComm(_) => {
				error!(
					"Error building new block: Can't connect to wallet listener at {:?}; will retry",
					wallet_listener_url.as_ref().unwrap()
				);
				thread::sleep(Duration::from_secs(wallet_retry_interval));
			}
			ae => {
				warn!("Error building new block: {:?}. Retrying.", ae);
			}
		}

		// only wait if we are still using the same key: a different coinbase commitment is unlikely
		// to have duplication
		if new_key_id.is_some() {
			thread::sleep(Duration::from_millis(100));
		}

		result = build_block(chain, tx_pool, new_key_id, wallet_listener_url.clone());
	}
	return result.unwrap();
}

/// Builds a new block with the chain head as previous and eligible
/// transactions from the pool.
fn build_block(
	chain: &Arc<chain::Chain>,
	tx_pool: &ServerTxPool,
	key_id: Option<Identifier>,
	wallet_listener_url: Option<String>,
) -> Result<(core::Block, BlockFees, PoWType), Error> {
	let head = chain.head_header()?;
	let seed = chain
		.header_pmmr()
		.read()
		.get_header_hash_by_height(rx_current_seed_height(head.height + 1))?;

	// prepare the block header timestamp
	let mut now_sec = Utc::now().timestamp();
	let head_sec = head.timestamp.timestamp();
	if now_sec <= head_sec {
		now_sec = head_sec + 1;
	}

	// Determine the difficulty our block should be at.
	// Note: do not keep the difficulty_iter in scope (it has an active batch).
	let difficulty = if head.height < consensus::difficultyfix_height() - 1 {
		consensus::next_difficulty(
			head.height + 1,
			(&head.pow.proof).into(),
			chain.difficulty_iter()?,
		)
	} else {
		consensus::next_difficulty_era1(
			head.height + 1,
			(&head.pow.proof).into(),
			chain.difficulty_iter()?,
		)
	};

	// Extract current "mineable" transactions from the pool.
	// If this fails for *any* reason then fallback to an empty vec of txs.
	// This will allow us to mine an "empty" block if the txpool is in an
	// invalid (and unexpected) state.
	let txs = match tx_pool.read().prepare_mineable_transactions() {
		Ok(txs) => txs,
		Err(e) => {
			error!(
				"build_block: Failed to prepare mineable txs from txpool: {:?}",
				e
			);
			warn!("build_block: Falling back to mining empty block.");
			vec![]
		}
	};

	// build the coinbase and the block itself
	let fees = txs.iter().map(|tx| tx.fee()).sum();
	let height = head.height + 1;
	let block_fees = BlockFees {
		fees,
		key_id,
		height,
	};

	let (output, kernel, block_fees) = get_coinbase(wallet_listener_url, block_fees, height)?;

	let mut b = if is_foundation_height(height) {
		let cb_data = load_foundation_output(height);
		core::Block::from_coinbases(
			&head,
			txs,
			(output, kernel),
			(cb_data.output, cb_data.kernel),
			difficulty.difficulty.clone(),
		)?
	} else {
		core::Block::from_reward(&head, txs, output, kernel, difficulty.difficulty.clone())?
	};

	// making sure we're not spending time mining a useless block

	b.validate(&head.total_kernel_offset)?;

	let mut seed_u8 = [0u8; 32];
	seed_u8.copy_from_slice(&seed.as_bytes()[0..32]);

	b.header.pow.seed = seed_u8;
	b.header.pow.nonce = thread_rng().gen();
	b.header.pow.secondary_scaling = difficulty.secondary_scaling;
	b.header.timestamp = TimeZone::from_utc_datetime(
		&Utc,
		&NaiveDateTime::from_timestamp_opt(now_sec, 0).unwrap(),
	);
	b.header.policy = get_emitted_policy(b.header.height);

	let bottle_cursor = chain.bottles_iter(get_emitted_policy(b.header.height))?;
	let (pow_type, bottles) = consensus::next_policy(b.header.policy, bottle_cursor);
	b.header.bottles = bottles;

	debug!(
		"Built new block with {} inputs and {} outputs, block difficulty: {:?}, cumulative difficulty {:?}",
		b.inputs().len(),
		b.outputs().len(),
		difficulty.difficulty,
		b.header.total_difficulty().num,
	);

	// Now set txhashset roots and sizes on the header of the block being built.
	match chain.set_txhashset_roots(&mut b) {
		Ok(_) => Ok((b, block_fees, pow_type)),
		Err(e) => {
			match e.kind() {
				// If this is a duplicate commitment then likely trying to use
				// a key that hass already been derived but not in the wallet
				// for some reason, allow caller to retry.
				chain::ErrorKind::DuplicateCommitment(e) => Err(Error::Chain(
					chain::ErrorKind::DuplicateCommitment(e).into(),
				)),

				// Some other issue, possibly duplicate kernel
				_ => {
					error!("Error setting txhashset root to build a block: {:?}", e);
					Err(Error::Chain(
						chain::ErrorKind::Other(format!("{:?}", e)).into(),
					))
				}
			}
		}
	}
}

///
/// Probably only want to do this when testing.
///
fn burn_reward(
	block_fees: BlockFees,
	height: u64,
) -> Result<(core::Output, core::TxKernel, BlockFees), Error> {
	warn!("Burning block fees: {:?}", block_fees);
	let keychain = ExtKeychain::from_random_seed(global::is_floonet())?;
	let key_id = ExtKeychain::derive_key_id(1, 1, 0, 0, 0);
	let (out, kernel) = crate::core::libtx::reward::output(
		&keychain,
		&ProofBuilder::new(&keychain),
		&key_id,
		block_fees.fees,
		false,
		height,
	)
	.unwrap();
	Ok((out, kernel, block_fees))
}

// Connect to the wallet listener and get coinbase.
// Warning: If a wallet listener URL is not provided the reward will be "burnt"
fn get_coinbase(
	wallet_listener_url: Option<String>,
	block_fees: BlockFees,
	height: u64,
) -> Result<(core::Output, core::TxKernel, BlockFees), Error> {
	match wallet_listener_url {
		None => {
			// Burn it
			return burn_reward(block_fees, height);
		}
		Some(wallet_listener_url) => {
			let res = create_coinbase(&wallet_listener_url, &block_fees)?;
			let output = res.output;
			let kernel = res.kernel;
			let key_id = res.key_id;
			let block_fees = BlockFees {
				key_id,
				..block_fees
			};

			debug!("get_coinbase: {:?}", block_fees);
			return Ok((output, kernel, block_fees));
		}
	}
}

/// Call the wallet API to create a coinbase output for the given block_fees.
/// Will retry based on default "retry forever with backoff" behavior.
fn build_coinbase(dest: &str, block_fees: &BlockFees, method: &str) -> Result<CbData, Error> {
	let url = format!("{}/v2/foreign", dest);
	let req_body = json!({
		"jsonrpc": "2.0",
		"method": method,
		"id": 1,
		"params": {
			"block_fees": block_fees
		}
	});

	trace!("Sending build_coinbase request: {}", req_body);
	let req = api::client::create_post_request(url.as_str(), None, &req_body)?;
	let timeout = api::client::TimeOut::default();
	let res: String = api::client::send_request(req, timeout).map_err(|e| {
		let report = format!(
			"Failed to get coinbase from {}. Is the wallet listening? {}",
			dest, e
		);
		error!("{}", report);
		Error::WalletComm(report)
	})?;

	let res: Value = serde_json::from_str(&res).unwrap();
	trace!("Response: {}", res);
	if res["error"] != json!(null) {
		let report = format!(
			"Failed to get coinbase from {}: Error: {}, Message: {}",
			dest, res["error"]["code"], res["error"]["message"]
		);
		error!("{}", report);
		return Err(Error::WalletComm(report));
	}

	let cb_data = res["result"]["Ok"].clone();
	trace!("cb_data: {}", cb_data);
	let ret_val = match serde_json::from_value::<CbData>(cb_data) {
		Ok(r) => r,
		Err(e) => {
			let report = format!("Couldn't deserialize CbData: {}", e);
			error!("{}", report);
			return Err(Error::WalletComm(report));
		}
	};

	Ok(ret_val)
}

fn create_coinbase(dest: &str, block_fees: &BlockFees) -> Result<CbData, Error> {
	build_coinbase(dest, block_fees, "build_coinbase")
}
/// Call the wallet API to create a foundation coinbase output for the given block_fees.
/// Will retry based on default "retry forever with backoff" behavior.
pub fn create_foundation(dest: &str, block_fees: &BlockFees) -> Result<CbData, Error> {
	build_coinbase(dest, block_fees, "build_foundation")
}
