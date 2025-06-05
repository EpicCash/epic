// Copyright 2025 The Epic Developers
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
//

use crate::chain;
use crate::rest::*;
use bytes::Bytes;

use crate::core::consensus;
use crate::core::core::hash::Hashed;
use crate::core::core::{Block, BlockHeader, Output, Transaction, TxKernel};
use crate::core::pow;
use crate::core::pow::randomx::rx_current_seed_height;
use crate::pool;
use crate::router::{Handler, ResponseFuture};
use crate::util::RwLock;
use crate::web::*;
use http_body_util::Full;
use hyper::Request;
use std::sync::Weak;

use super::utils::w;
use crate::chain::Options;
use crate::core::global::get_emitted_policy;
use crate::pool::{BlockChain, PoolAdapter};
use chrono::Duration;
use rand::{rng, Rng};
pub struct MiningHandler<B, P>
where
	B: BlockChain,
	P: PoolAdapter,
{
	pub chain: Weak<chain::Chain>,
	pub tx_pool: Weak<RwLock<pool::TransactionPool<B, P>>>,
}

/// Step 1: Get a block template (without coinbase)
#[derive(Serialize, Deserialize)]
pub struct BlockTemplate {
	pub header: BlockHeader,
	pub transactions: Vec<Transaction>,
	pub height: u64,
	pub algorithm: String,
	pub block_difficulty: epic_core::pow::Difficulty,

	pub epochs: Vec<(u64, u64, [u8; 32])>,
}

#[derive(Serialize, Deserialize)]
pub struct CoinbaseData {
	pub output: Output,
	pub kernel: TxKernel,
}

/// Step 2: Finalized block template (with coinbase, roots, pre_pow)
#[derive(Serialize, Deserialize)]
pub struct FinalizedBlockTemplate {
	pub header: BlockHeader,
	pub pre_pow: String,
	pub height: u64,
	pub transactions: Vec<Transaction>,
	pub coinbase_output: Output,
	pub coinbase_kernel: TxKernel,
}

impl<B, P> MiningHandler<B, P>
where
	B: BlockChain,
	P: PoolAdapter,
{
	/// Step 1: Returns a block template for mining, without coinbase output/kernel.
	pub fn get_block_template(&self) -> Result<BlockTemplate, Error> {
		let chain = w(&self.chain)?;
		let head = chain.head_header()?;
		let height = head.height + 1;
		//let proof_size = global::proofsize();
		let prev = &head;

		let seed = chain
			.header_pmmr()
			.read()
			.get_header_hash_by_height(rx_current_seed_height(head.height + 1))?;

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

		let pool_arc = w(&self.tx_pool)?;
		let pool = pool_arc.read();
		let txs = match pool.prepare_mineable_transactions() {
			Ok(txs) => txs,
			Err(_) => vec![],
		};

		// Build header as in stratum, but without coinbase and roots

		let mut header = head.clone();

		header.height = height;
		header.prev_hash = head.hash();

		let mut seed_u8 = [0u8; 32];
		seed_u8.copy_from_slice(&seed.as_bytes()[0..32]);

		header.pow.seed = seed_u8;
		header.pow.nonce = rng().random();
		header.pow.secondary_scaling = difficulty.secondary_scaling;

		header.timestamp = prev.timestamp + Duration::seconds(60);

		header.policy = get_emitted_policy(header.height);

		let bottle_cursor = chain.bottles_iter(get_emitted_policy(header.height))?;
		let (pow_type, bottles) = consensus::next_policy(header.policy, bottle_cursor);
		header.bottles = bottles;

		//build epochs for RandomX
		let current_seed_height = pow::randomx::rx_current_seed_height(header.height);
		let current_seed_hash = chain
			.header_pmmr()
			.read()
			.get_header_hash_by_height(current_seed_height)
			.unwrap();
		let mut current_hash = [0u8; 32];
		current_hash.copy_from_slice(&current_seed_hash.as_bytes()[0..32]);
		let epochs = vec![(
			pow::randomx::rx_epoch_start(current_seed_height),
			pow::randomx::rx_epoch_end(current_seed_height),
			current_hash,
		)];

		Ok(BlockTemplate {
			header,
			transactions: txs,
			height,
			block_difficulty: difficulty.difficulty.clone(),
			algorithm: pow_type.to_str(),
			epochs,
		})
	}

	/// Step 2: Finalize block with coinbase, set roots, return header+pre_pow
	pub fn finalize_block_template(
		&self,
		coinbase: CoinbaseData,
	) -> Result<FinalizedBlockTemplate, Error> {
		let chain = w(&self.chain)?;
		let head = chain.head_header()?;
		let height = head.height + 1;

		// Get txs from pool again (must match what was given in template)
		let pool_arc = w(&self.tx_pool)?;
		let pool = pool_arc.read();
		let txs = match pool.prepare_mineable_transactions() {
			Ok(txs) => txs,
			Err(_) => vec![],
		};

		// Build block with txs + coinbase
		let mut block = Block::from_reward(
			&head,
			txs.clone(),
			coinbase.output.clone(),
			coinbase.kernel.clone(),
			chain.difficulty_iter()?.next().unwrap().difficulty,
		)
		.unwrap();

		// Set txhashset roots (must be done after coinbase is added)
		chain.set_txhashset_roots(&mut block)?;

		// Serialize pre_pow for mining
		let pre_pow = {
			let mut header_buf = vec![];
			{
				use crate::core::ser::BinWriter;
				let mut writer = BinWriter::default(&mut header_buf);
				block.header.write_pre_pow(&mut writer).unwrap();
			}
			crate::util::to_hex(header_buf)
		};

		Ok(FinalizedBlockTemplate {
			header: block.header.clone(),
			pre_pow,
			height,
			transactions: txs,
			coinbase_output: coinbase.output,
			coinbase_kernel: coinbase.kernel,
		})
	}

	/// Step 3: Submit a mined block (with valid PoW) to the node.
	pub fn submit_block(&self, block: Block) -> Result<(), Error> {
		let chain = w(&self.chain)?;
		// Validate and process the block
		// This typically calls chain.process_block, which does full validation and adds to the chain if valid
		chain.process_block(block, Options::MINE)?;
		Ok(())
	}
}

// Example handler wiring (pseudo, adapt to your router)
impl<B, P> Handler<Full<Bytes>> for MiningHandler<B, P>
where
	B: BlockChain,
	P: PoolAdapter,
{
	fn get(&self, _req: Request<hyper::body::Incoming>) -> ResponseFuture {
		result_to_response(self.get_block_template())
	}
	// Add a POST endpoint for finalize_block_template as needed
}
