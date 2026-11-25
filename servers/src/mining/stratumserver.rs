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

//! Mining Stratum Server

use crate::util::RwLock;
use chrono::prelude::Utc;
use futures::channel::mpsc;
use futures::pin_mut;
use futures::{SinkExt, StreamExt, TryStreamExt};
use serde;
use serde_json;
use serde_json::Value;
use std::collections::HashMap;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;

use tokio_util::codec::{Framed, LinesCodec};

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use std::{cmp, thread};

use crate::chain::{self, SyncState};
use crate::common::stats::{StratumStats, WorkerStats};
use crate::common::types::StratumServerConfig;

use crate::core::core::hash::Hashed;
use crate::core::core::Block;
use crate::core::pow::{DifficultyNumber, PoWType};
use crate::core::{pow, ser};
use crate::keychain;
use crate::mining::mine_block;

use crate::util;

use crate::ServerTxPool;
use epic_core::pow::Proof;
use epic_core::ser::Writeable;

type Tx = mpsc::UnboundedSender<String>;

// ----------------------------------------
// http://www.jsonrpc.org/specification
// RPC Methods
#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
enum Id {
	Integer(i64),
	String(String),
}

#[derive(Serialize, Deserialize, Debug)]
struct RpcRequest {
	id: Id,
	jsonrpc: String,
	method: String,
	params: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug)]
struct RpcResponse {
	id: Id,
	jsonrpc: String,
	method: String,
	result: Option<Value>,
	error: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug)]
struct RpcError {
	code: i32,
	message: String,
}

impl RpcError {
	pub fn internal_error() -> Self {
		RpcError {
			code: 32603,
			message: "Internal error".to_owned(),
		}
	}
	pub fn node_is_syncing() -> Self {
		RpcError {
			code: -32000,
			message: "Node is syncing - Please wait".to_owned(),
		}
	}
	pub fn method_not_found() -> Self {
		RpcError {
			code: -32601,
			message: "Method not found".to_owned(),
		}
	}
	pub fn too_late() -> Self {
		RpcError {
			code: -32503,
			message: "Solution submitted too late".to_string(),
		}
	}
	pub fn cannot_validate() -> Self {
		RpcError {
			code: -32502,
			message: "Failed to validate solution".to_string(),
		}
	}
	pub fn too_low_difficulty() -> Self {
		RpcError {
			code: -32501,
			message: "Share rejected due to low difficulty".to_string(),
		}
	}
	pub fn invalid_request() -> Self {
		RpcError {
			code: -32600,
			message: "Invalid Request".to_string(),
		}
	}
}

impl From<RpcError> for Value {
	fn from(e: RpcError) -> Self {
		serde_json::to_value(e).unwrap()
	}
}

impl<T> From<T> for RpcError
where
	T: std::error::Error,
{
	fn from(e: T) -> Self {
		error!("Received unhandled error: {}", e);
		RpcError::internal_error()
	}
}

#[derive(Serialize, Deserialize, Debug)]
struct LoginParams {
	login: String,
	pass: String,
	agent: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct JobParams {
	algorithm: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum AlgorithmParams {
	Cuckoo(u64, Vec<u64>),
	RandomX([u8; 32]),
	ProgPow([u8; 32]),
}

#[derive(Serialize, Deserialize, Debug)]
struct SubmitParams {
	height: u64,
	job_id: u64,
	nonce: u64,
	pow: AlgorithmParams,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JobTemplate {
	height: u64,
	job_id: u64,
	difficulty: Vec<(String, u64)>,
	block_difficulty: Vec<(String, u64)>,
	pre_pow: String,
	epochs: Vec<(u64, u64, [u8; 32])>,
	algorithm: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WorkerStatus {
	id: String,
	height: u64,
	difficulty: u64,
	accepted: u64,
	rejected: u64,
	stale: u64,
}

struct State {
	current_block_versions: Vec<(Block, PoWType)>,
	// to prevent the wallet from generating a new HD key derivation for each
	// iteration, we keep the returned derivation to provide it back when
	// nothing has changed. We only want to create a key_id for each new block,
	// and reuse it when we rebuild the current block to add new tx.
	current_key_id: Option<keychain::Identifier>,
	current_difficulty: DifficultyNumber,
	minimum_share_difficulty: DifficultyNumber,
}

impl State {
	pub fn new(minimum_share_difficulty: DifficultyNumber) -> Self {
		let blocks = vec![(Block::default(), PoWType::Cuckaroo)];

		let mut current_difficulty = HashMap::new();

		current_difficulty.insert(PoWType::Cuckatoo, u64::MAX);
		current_difficulty.insert(PoWType::Cuckaroo, u64::MAX);
		current_difficulty.insert(PoWType::RandomX, u64::MAX);
		current_difficulty.insert(PoWType::ProgPow, u64::MAX);

		State {
			current_block_versions: blocks,
			current_key_id: None,
			current_difficulty,
			minimum_share_difficulty,
		}
	}

	pub fn get_minimum_difficulty(&self, pow: PoWType) -> u64 {
		*self.minimum_share_difficulty.get(&pow).unwrap()
	}

	pub fn get_current_difficulty(&self, pow: PoWType) -> u64 {
		*self.current_difficulty.get(&pow).unwrap()
	}
}

struct Handler {
	id: String,
	workers: Arc<WorkersList>,
	sync_state: Arc<SyncState>,
	chain: Arc<chain::Chain>,
	current_state: Arc<RwLock<State>>,
}

impl Handler {
	pub fn new(
		id: String,
		stratum_stats: Arc<RwLock<StratumStats>>,
		sync_state: Arc<SyncState>,
		minimum_share_difficulty: DifficultyNumber,
		chain: Arc<chain::Chain>,
	) -> Self {
		Handler {
			id,
			workers: Arc::new(WorkersList::new(stratum_stats.clone())),
			sync_state,
			chain,
			current_state: Arc::new(RwLock::new(State::new(minimum_share_difficulty))),
		}
	}
	pub fn from_stratum(stratum: &StratumServer) -> Self {
		let mut minimum_share_difficulty = HashMap::new();
		minimum_share_difficulty.insert(
			PoWType::Cuckatoo,
			stratum.config.cuckatoo_minimum_share_difficulty,
		);
		minimum_share_difficulty.insert(
			PoWType::Cuckaroo,
			stratum.config.cuckatoo_minimum_share_difficulty,
		);
		minimum_share_difficulty.insert(
			PoWType::RandomX,
			stratum.config.randomx_minimum_share_difficulty,
		);
		minimum_share_difficulty.insert(
			PoWType::ProgPow,
			stratum.config.progpow_minimum_share_difficulty,
		);
		Handler::new(
			stratum.id.clone(),
			stratum.stratum_stats.clone(),
			stratum.sync_state.clone(),
			minimum_share_difficulty,
			stratum.chain.clone(),
		)
	}
	async fn handle_rpc_requests(&self, request: RpcRequest, worker_id: usize) -> String {
		self.workers.last_seen(worker_id);
		debug!("request: {:?}", request);
		// Call the handler function for requested method
		let response = match request.method.as_str() {
			"login" => self.handle_login(request.params, worker_id),
			"submit" => {
				let res = self.handle_submit(request.params, worker_id);
				// this key_id has been used now, reset
				if let Ok((_, true)) = res {
					self.current_state.write().current_key_id = None;
				}
				res.map(|(v, _)| v)
			}
			"keepalive" => self.handle_keepalive(),
			"getjobtemplate" => {
				if self.sync_state.is_syncing() {
					Err(RpcError::node_is_syncing())
				} else {
					self.handle_getjobtemplate(request.params)
				}
			}
			"status" => self.handle_status(worker_id),
			_ => {
				// Called undefined method
				Err(RpcError::method_not_found())
			}
		};

		// Package the reply as RpcResponse json
		let resp = match response {
			Err(rpc_error) => RpcResponse {
				id: request.id,
				jsonrpc: String::from("2.0"),
				method: request.method,
				result: None,
				error: Some(rpc_error.into()),
			},
			Ok(response) => RpcResponse {
				id: request.id,
				jsonrpc: String::from("2.0"),
				method: request.method,
				result: Some(response),
				error: None,
			},
		};
		serde_json::to_string(&resp).unwrap()
	}
	fn handle_login(&self, params: Option<Value>, worker_id: usize) -> Result<Value, RpcError> {
		let params: LoginParams = parse_params(params)?;
		self.workers
			.login(worker_id, params.login, params.agent)?;
		return Ok("ok".into());
	}

	// Handle KEEPALIVE message
	fn handle_keepalive(&self) -> Result<Value, RpcError> {
		return Ok("ok".into());
	}

	fn handle_status(&self, worker_id: usize) -> Result<Value, RpcError> {
		// Return worker status in json for use by a dashboard or healthcheck.
		let stats = self.workers.get_stats(worker_id)?;
		let status = WorkerStatus {
			id: stats.id.clone(),
			height: self
				.current_state
				.read()
				.current_block_versions
				.last()
				.unwrap()
				.0
				.header
				.height,
			difficulty: stats.pow_difficulty,
			accepted: stats.num_accepted,
			rejected: stats.num_rejected,
			stale: stats.num_stale,
		};
		let response = serde_json::to_value(&status).unwrap();
		return Ok(response);
	}

	// Handle GETJOBTEMPLATE message
	fn handle_getjobtemplate(&self, params: Option<Value>) -> Result<Value, RpcError> {
		let _params: JobParams = parse_params(params)?;
		// Build a JobTemplate from a BlockHeader and return JSON
		let job_template = self.build_block_template();
		let response = serde_json::to_value(&job_template).unwrap();
		debug!(
			"(Server ID: {}) sending block {} with id {} to single worker",
			self.id, job_template.height, job_template.job_id,
		);
		return Ok(response);
	}

	/*fn get_parse_algorithm(&self, algo: &str) -> PoWType {
		match algo {
			"cuckoo" => PoWType::Cuckatoo,
			"randomx" => PoWType::RandomX,
			"progpow" => PoWType::ProgPow,
			_ => panic!("algorithm is not supported"),
		}
	}*/

	// Build and return a JobTemplate for mining the current block
	fn build_block_template(&self) -> JobTemplate {
		let job = self
			.current_state
			.read()
			.current_block_versions
			.last()
			.unwrap()
			.clone();

		let bh = job.0.header.clone();
		let pow = job.1.clone();

		let current_seed_height = pow::randomx::rx_current_seed_height(bh.height);
		let next_seed_height = pow::randomx::rx_next_seed_height(bh.height);

		let current_seed_hash = self
			.chain
			.header_pmmr()
			.read()
			.get_header_hash_by_height(current_seed_height)
			.unwrap();

		let mut current_hash = [0u8; 32];
		current_hash.copy_from_slice(&current_seed_hash.as_bytes()[0..32]);

		let mut epochs = vec![(
			pow::randomx::rx_epoch_start(current_seed_height),
			pow::randomx::rx_epoch_end(current_seed_height),
			current_hash,
		)];

		if let Some(h) = next_seed_height {
			let next_seed_hash = self
				.chain
				.header_pmmr()
				.read()
				.get_header_hash_by_height(h)
				.unwrap();

			let mut next_hash = [0u8; 32];
			next_hash.copy_from_slice(&next_seed_hash.as_bytes()[0..32]);

			epochs.push((
				pow::randomx::rx_epoch_start(h),
				pow::randomx::rx_epoch_end(h),
				next_hash,
			));
		}

		// Serialize the block header into pre and post nonce strings
		let algorithms = vec![PoWType::Cuckatoo, PoWType::RandomX, PoWType::ProgPow];

		let difficulty = {
			let state = self.current_state.read();
			algorithms
				.iter()
				.map(|x| (x.to_str(), state.get_minimum_difficulty(*x)))
				.collect::<Vec<(String, u64)>>()
		};

		let block_difficulty = {
			let state = self.current_state.read();
			algorithms
				.iter()
				.map(|x| (x.to_str(), state.get_current_difficulty(*x)))
				.collect::<Vec<(String, u64)>>()
		};

		let mut header_buf = vec![];
		{
			let mut writer = ser::BinWriter::default(&mut header_buf);
			bh.version.write(&mut writer).unwrap();
			bh.write_pre_pow(&mut writer).unwrap();
			bh.pow.write_pre_pow(&mut writer).unwrap();
		}
		let pre_pow = util::to_hex(header_buf);
		let job_template = JobTemplate {
			height: bh.height,
			job_id: (self.current_state.read().current_block_versions.len() - 1) as u64,
			difficulty,
			block_difficulty,
			pre_pow,
			epochs,
			algorithm: pow.to_str(),
		};

		job_template
	}

	// Handle SUBMIT message
	// params contains a solved block header
	// We accept and log valid shares of all difficulty above configured minimum
	// Accepted shares that are full solutions will also be submitted to the
	// network
	fn handle_submit(
		&self,
		params: Option<Value>,
		worker_id: usize,
	) -> Result<(Value, bool), RpcError> {
		// Validate parameters
		let params: SubmitParams = parse_params(params)?;

		let state = self.current_state.read();
		// Find the correct version of the block to match this header
		let b: Option<&(Block, PoWType)> = state.current_block_versions.get(params.job_id as usize);
		if params.height != state.current_block_versions.last().unwrap().0.header.height
			|| b.is_none()
		{
			// Return error status
			error!(
				"(Server ID: {}) Share at height {}, nonce {}, job_id {} submitted too late",
				self.id, params.height, params.nonce, params.job_id,
			);
			self.workers.update_stats(worker_id, |ws| ws.num_stale += 1);
			return Err(RpcError::too_late());
		}

		let share_difficulty: u64;
		let mut share_is_block = false;

		let (mut b, pow_type) = b.unwrap().clone();
		// Reconstruct the blocks header with this nonce and pow added

		b.header.pow.nonce = params.nonce;
		match params.pow {
			AlgorithmParams::Cuckoo(edge_bits, nonces) => {
				let edge_bits = edge_bits as u8;
				b.header.pow.proof = Proof::CuckooProof { edge_bits, nonces };
			}
			AlgorithmParams::RandomX(hash) => {
				b.header.pow.proof = Proof::RandomXProof { hash };
			}
			AlgorithmParams::ProgPow(mix) => {
				b.header.pow.proof = Proof::ProgPowProof { mix };
			}
		}

		if !b.header.pow.is_primary() && !b.header.pow.is_secondary() {
			// Return error status
			error!(
					"(Server ID: {}) Failed to validate solution at height {}, hash {}, nonce {}, job_id {}: cuckoo size too small",
					self.id, params.height, b.hash(), params.nonce, params.job_id,
				);
			self.workers
				.update_stats(worker_id, |worker_stats| worker_stats.num_rejected += 1);
			return Err(RpcError::cannot_validate());
		}

		// Get share difficulty
		share_difficulty = b
			.header
			.pow
			.to_difficulty(&b.header.pre_pow(), b.header.height, b.header.pow.nonce)
			.to_num((&b.header.pow.proof).into());

		let minimum_share_difficulty = state.get_minimum_difficulty((&b.header.pow.proof).into());
		// If the difficulty is too low its an error
		if share_difficulty < minimum_share_difficulty {
			// Return error status
			error!(
					"(Server ID: {}) Share at height {}, hash {}, nonce {}, job_id {} rejected due to low difficulty: {}/{}",
					self.id, params.height, b.hash(), params.nonce, params.job_id, share_difficulty, minimum_share_difficulty,
				);
			self.workers
				.update_stats(worker_id, |worker_stats| worker_stats.num_rejected += 1);
			return Err(RpcError::too_low_difficulty());
		}

		let b_pow_type: PoWType = (&b.header.pow.proof).into();

		let current_difficulty = state.get_current_difficulty(b_pow_type.clone());
		// If the difficulty is high enough, submit it (which also validates it)
		if share_difficulty >= current_difficulty && pow_type == b_pow_type {
			// This is a full solution, submit it to the network
			let res = self.chain.process_block(b.clone(), chain::Options::MINE);
			if let Err(e) = res {
				// Return error status
				error!(
						"(Server ID: {}) Failed to validate solution at height {}, hash {}, nonce {}, job_id {}, {}",
						self.id,
						params.height,
						b.hash(),
						params.nonce,
						params.job_id,
						e,
						
					);
				self.workers
					.update_stats(worker_id, |worker_stats| worker_stats.num_rejected += 1);
				return Err(RpcError::cannot_validate());
			}
			share_is_block = true;
			self.workers
				.update_stats(worker_id, |worker_stats| worker_stats.num_blocks_found += 1);
			// Log message to make it obvious we found a block
			let stats = self.workers.get_stats(worker_id)?;
			warn!(
					"(Server ID: {}) Solution Found for block {}, hash {} - Yay!!! Worker ID: {}, blocks found: {}, shares: {}",
					self.id, params.height,
					b.hash(),
					stats.id,
					stats.num_blocks_found,
					stats.num_accepted,
				);
		} else {
			// Do some validation but dont submit
			let res = pow::verify_size(&b.header);
			if !res.is_ok() {
				// Return error status
				error!(
						"(Server ID: {}) Failed to validate share at height {}, hash {}, nonce {}, job_id {}. {:?}",
						self.id,
						params.height,
						b.hash(),
						b.header.pow.nonce,
						params.job_id,
						res,
					);
				self.workers
					.update_stats(worker_id, |worker_stats| worker_stats.num_rejected += 1);
				return Err(RpcError::cannot_validate());
			}
		}
		// Log this as a valid share
		let worker = self.workers.get_worker(worker_id)?;
		let submitted_by = match worker.login {
			None => worker.id.to_string(),
			Some(login) => login.clone(),
		};

		info!(
				"(Server ID: {}) Got share at height {}, hash {}, nonce {}, job_id {}, difficulty {}/{}, submitted by {}",
				self.id,
				b.header.height,
				b.hash(),
				b.header.pow.nonce,
				params.job_id,
				share_difficulty,
				current_difficulty,
				submitted_by,
			);
		self.workers
			.update_stats(worker_id, |worker_stats| worker_stats.num_accepted += 1);
		let submit_response;
		if share_is_block {
			submit_response = format!("blockfound - {}", b.hash().to_hex());
		} else {
			submit_response = "ok".to_string();
		}
		return Ok((
			serde_json::to_value(submit_response).unwrap(),
			share_is_block,
		));
	} // handle submit a solution

	fn broadcast_job(&self) {
		debug!("broadcast job");
		// Do not broadcast jobs while the node is syncing
		if self.sync_state.is_syncing() {
			debug!("Not broadcasting job: node is syncing");
			return;
		}
		// Package new block into RpcRequest
		let job_template = self.build_block_template();
		let job_template_json = serde_json::to_string(&job_template).unwrap();
		// Issue #1159 - use a serde_json Value type to avoid extra quoting
		let job_template_value: Value = serde_json::from_str(&job_template_json).unwrap();
		let job_request = RpcRequest {
			id: Id::String(String::from("Stratum")),
			jsonrpc: String::from("2.0"),
			method: String::from("job"),
			params: Some(job_template_value),
		};
		let job_request_json = serde_json::to_string(&job_request).unwrap();
		debug!(
			"(Server ID: {}) sending block {} with id {} to stratum clients",
			self.id, job_template.height, job_template.job_id,
		);
		self.workers.broadcast(job_request_json.clone());
	}

	pub fn run(&self, config: &StratumServerConfig, tx_pool: &ServerTxPool) {
		debug!("Run main loop");
		let mut deadline: i64 = 0;
		let mut head = self.chain.head().unwrap();
		let mut current_hash = head.prev_block_h;

		//let mut d_block = (Block::default(), mine_block::BlockFees{fees: 0, height, key_id: None}, PoWType::Cuckaroo);
		loop {
			// get the latest chain state
			head = self.chain.head().unwrap();
			let latest_hash = head.last_block_h;

			// Build a new block if:
			//    There is a new block on the chain
			// or We are rebuilding the current one to include new transactions
			// and there is at least one worker connected
			if (current_hash != latest_hash || Utc::now().timestamp() >= deadline)
				&& self.workers.count() > 0
			{
				{
					debug!("resend updated block");
					// Prepare values we need without holding the write lock
					let wallet_listener_url: Option<String> = if !config.burn_reward {
						Some(config.wallet_listener_url.clone())
					} else {
						None
					};
					let key_id_for_build = {
						let state_read = self.current_state.read();
						state_read.current_key_id.clone()
					};
					// Build the new block (version) without holding the write lock
					let (new_block, block_fees, pow_type) = mine_block::get_block(
						&self.chain,
						tx_pool,
						key_id_for_build,
						wallet_listener_url,
					);

					head = self.chain.head().unwrap();
					let latest_hash = head.last_block_h;
					let clear_blocks = current_hash != latest_hash;

					if head.total_difficulty > new_block.header.total_difficulty() {
						continue;
					}

					// Now acquire write lock only to update shared state
					let mut state = self.current_state.write();
					state.current_difficulty = (new_block.header.total_difficulty()
						- head.total_difficulty)
						.num
						.clone();

					state.current_key_id = block_fees.key_id();

					let cuckato_m_difficulty =
						*state.current_difficulty.get(&PoWType::Cuckatoo).unwrap();
					current_hash = latest_hash;
					// set the minimum acceptable share difficulty for this block
					state.minimum_share_difficulty.insert(
						PoWType::Cuckatoo,
						cmp::min(
							config.cuckatoo_minimum_share_difficulty,
							cuckato_m_difficulty,
						),
					);

					// set the minimum acceptable share difficulty for this block
					state.minimum_share_difficulty.insert(
						PoWType::Cuckaroo,
						cmp::min(
							config.cuckatoo_minimum_share_difficulty,
							cuckato_m_difficulty,
						),
					);

					let randomx_m_difficulty =
						*state.current_difficulty.get(&PoWType::RandomX).unwrap();
					// set the minimum acceptable share difficulty for this block
					state.minimum_share_difficulty.insert(
						PoWType::RandomX,
						cmp::min(
							config.randomx_minimum_share_difficulty,
							randomx_m_difficulty,
						),
					);

					let progpow_m_difficulty =
						*state.current_difficulty.get(&PoWType::ProgPow).unwrap();
					// set the minimum acceptable share difficulty for this block
					state.minimum_share_difficulty.insert(
						PoWType::ProgPow,
						cmp::min(
							config.progpow_minimum_share_difficulty,
							progpow_m_difficulty,
						),
					);

					// set a new deadline for rebuilding with fresh transactions
					deadline =
						Utc::now().timestamp() + u32::min(config.attempt_time_per_block, 30) as i64;

					self.workers.update_block_height(new_block.header.height);
					self.workers
						.update_network_difficulty(state.current_difficulty.clone());

					if clear_blocks {
						state.current_block_versions.clear();
					}
					state.current_block_versions.push((new_block, pow_type));
					// Send this job to all connected workers
				}
				self.broadcast_job();
			}

			// sleep before restarting loop
			thread::sleep(Duration::from_millis(5));
		} // Main Loop
	}
}

// ----------------------------------------
// Worker Factory Thread Function
async fn handle_worker_respo(h: Arc<Handler>, request: RpcRequest, worker_id: usize) {
	let resp = h.handle_rpc_requests(request, worker_id).await;
	h.workers.send_to(worker_id, resp).await;
}

async fn handle_connection<S, F>(
	stream: S,
	worker_id: usize,
	mut rx: mpsc::UnboundedReceiver<String>,
	workers: Arc<WorkersList>,
	handle_request: F,
) where
	S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
	F: Fn(RpcRequest, usize) + Send + 'static,
{
	let framed = Framed::new(stream, LinesCodec::new());
	let (mut writer, mut reader) = framed.split();

	let read = async move {
		while let Some(line) = reader
			.try_next()
			.await
			.map_err(|e| error!("error reading line: {}", e))?
		{
			let request = serde_json::from_str(&line)
				.map_err(|e| error!("error serializing line: {}", e))?;

			handle_request(request, worker_id);
		}

		Result::<_, ()>::Ok(())
	};

	let write = async move {
		while let Some(line) = rx.next().await {
			writer
				.send(line)
				.await
				.map_err(|e| error!("error writing line: {}", e))?;
		}

		Result::<_, ()>::Ok(())
	};

	pin_mut!(read, write);
	let _ = futures::future::select(read, write).await;
	trace!("Worker {} disconnected", worker_id);
	workers.remove_worker(worker_id);
}

// ----------------------------------------
// Worker Factory Thread Function
#[tokio::main]
async fn accept_connections(listen_addr: SocketAddr, handler: Arc<Handler>) {
	//info!("Start tokio stratum server");

	let listener = TcpListener::bind(&listen_addr)
		.await
		.unwrap_or_else(|_| panic!("Stratum: Failed to bind to listen address {}", listen_addr));

	loop {
		let handler = handler.clone();
		let (socket, _) = listener.accept().await.unwrap();
		// Spawn a task to process the connection
		let (tx, rx) = mpsc::unbounded();

		let worker_id = handler.workers.add_worker(tx).await;
		match socket.peer_addr() {
			Ok(addr) => {
				info!("Worker {}/{:?} connected", worker_id, addr);
			}
			Err(e) => {
				error!("Error on Socket {:?}", e);
			}
		};
		let workers = handler.workers.clone();
		let h = handler.clone();
		tokio::spawn(handle_connection(
			socket,
			worker_id,
			rx,
			workers,
			move |request, worker_id| {
				tokio::task::spawn(handle_worker_respo(h.clone(), request, worker_id));
			},
		));
	}
}

// ----------------------------------------
// Worker Object - a connected stratum client - a miner, pool, proxy, etc...

#[derive(Clone)]
pub struct Worker {
	id: usize,
	agent: String,
	login: Option<String>,
	authenticated: bool,
	tx: Tx,
}

impl Worker {
	/// Creates a new Stratum Worker.
	pub fn new(id: usize, tx: Tx) -> Worker {
		Worker {
			id,
			agent: String::from(""),
			login: None,
			authenticated: false,
			tx,
		}
	}
} // impl Worker

struct WorkersList {
	workers_list: Arc<RwLock<HashMap<usize, Worker>>>,
	stratum_stats: Arc<RwLock<StratumStats>>,
	next_worker_id: AtomicUsize,
}

/// A worker is named only by the component after the final dot in its login.
/// A login without a worker component remains an anonymous `worker_<ID>` session.
fn worker_name_from_login(login: &str) -> Option<&str> {
	login
		.rsplit_once('.')
		.filter(|(_, name)| !name.is_empty())
		.map(|(_, name)| name)
}

impl WorkersList {
	pub fn new(stratum_stats: Arc<RwLock<StratumStats>>) -> Self {
		WorkersList {
			workers_list: Arc::new(RwLock::new(HashMap::new())),
			stratum_stats,
			next_worker_id: AtomicUsize::new(0),
		}
	}

	pub async fn add_worker(&self, tx: Tx) -> usize {
		let worker_id = self.next_worker_id.fetch_add(1, Ordering::Relaxed);
		let worker = Worker::new(worker_id, tx);
		let worker_count = {
			let mut workers_list = self.workers_list.write();
			workers_list.insert(worker_id, worker);
			workers_list.len()
		};

		let mut stratum_stats = self.stratum_stats.write();
		let mut worker_stats = WorkerStats::default();
		worker_stats.session_id = worker_id;
		worker_stats.is_connected = true;
		worker_stats.id = format!("worker_{}", worker_id);
		worker_stats.pow_difficulty = 1; // XXX TODO
		stratum_stats.worker_stats.push(worker_stats);
		stratum_stats.num_workers = worker_count;
		worker_id
	}
	pub fn remove_worker(&self, worker_id: usize) {
		let worker_count = {
			let mut workers_list = self.workers_list.write();
			if workers_list.remove(&worker_id).is_none() {
				warn!("Stratum: remove_worker called for unknown id {}", worker_id);
				return;
			}
			workers_list.len()
		};

		let mut stratum_stats = self.stratum_stats.write();
		let retain_stats = stratum_stats
			.worker_stats
			.iter()
			.find(|ws| ws.session_id == worker_id)
			.map(|ws| ws.retain_on_disconnect)
			.unwrap_or(false);
		if retain_stats {
			if let Some(worker_stats) = stratum_stats
				.worker_stats
				.iter_mut()
				.find(|ws| ws.session_id == worker_id)
			{
				worker_stats.is_connected = false;
			}
		} else {
			stratum_stats
				.worker_stats
				.retain(|ws| ws.session_id != worker_id);
		}
		stratum_stats.num_workers = worker_count;
	}

	pub fn login(
		&self,
		worker_id: usize,
		login: String,
		agent: String,
	) -> Result<(), RpcError> {
		let worker_name = worker_name_from_login(&login).map(str::to_string);
		{
			let mut wl = self.workers_list.write();
			let worker = wl.get_mut(&worker_id).ok_or(RpcError::internal_error())?;
			worker.login = Some(login);
			// XXX TODO Future - Validate password?
			worker.agent = agent;
			worker.authenticated = true;
		}

		let worker_name = match worker_name {
			Some(worker_name) => worker_name,
			None => return Ok(()),
		};

		let mut stratum_stats = self.stratum_stats.write();
		if let Some(existing_worker) = stratum_stats
			.worker_stats
			.iter()
			.position(|ws| ws.id == worker_name && ws.retain_on_disconnect && !ws.is_connected)
		{
			let mut existing_worker = stratum_stats.worker_stats.remove(existing_worker);
			stratum_stats
				.worker_stats
				.retain(|ws| ws.session_id != worker_id);
			existing_worker.session_id = worker_id;
			existing_worker.is_connected = true;
			existing_worker.last_seen = SystemTime::now();
			stratum_stats.worker_stats.push(existing_worker);
		} else if let Some(worker_stats) = stratum_stats
			.worker_stats
			.iter_mut()
			.find(|ws| ws.session_id == worker_id)
		{
			worker_stats.id = worker_name;
			worker_stats.retain_on_disconnect = true;
		}
		Ok(())
	}

	pub fn get_worker(&self, worker_id: usize) -> Result<Worker, RpcError> {
		self.workers_list
			.read()
			.get(&worker_id)
			.ok_or_else(|| {
				error!("Worker {} not found", worker_id);
				RpcError::internal_error()
			})
			.map(|w| w.clone())
	}

	pub fn get_stats(&self, worker_id: usize) -> Result<WorkerStats, RpcError> {
		self.stratum_stats
			.read()
			.worker_stats
			.iter()
			.find(|ws| ws.session_id == worker_id && ws.is_connected)
			.ok_or(RpcError::internal_error())
			.map(|ws| ws.clone())
	}

	pub fn last_seen(&self, worker_id: usize) {
		//self.stratum_stats.write().worker_stats[worker_id].last_seen = SystemTime::now();
		self.update_stats(worker_id, |ws| ws.last_seen = SystemTime::now());
	}

	pub fn update_stats(&self, worker_id: usize, f: impl FnOnce(&mut WorkerStats) -> ()) {
		let mut stratum_stats = self.stratum_stats.write();
		if let Some(worker_stats) = stratum_stats
			.worker_stats
			.iter_mut()
			.find(|ws| ws.session_id == worker_id && ws.is_connected)
		{
			f(worker_stats);
		} else {
			debug!("update_stats: worker {} not found (likely disconnected)", worker_id);
		}
	}

	pub async fn send_to(&self, worker_id: usize, msg: String) {
		if let Some(worker) = self.workers_list.read().get(&worker_id) {
			let _ = worker.tx.unbounded_send(msg);
		} else {
			debug!("send_to: worker {} not found (likely disconnected)", worker_id);
		}
	}

	pub fn broadcast(&self, msg: String) {
		for worker in self.workers_list.read().values() {
			let _ = worker.tx.unbounded_send(msg.clone());
		}
	}

	pub fn count(&self) -> usize {
		self.workers_list.read().len()
	}

	pub fn update_block_height(&self, height: u64) {
		let mut stratum_stats = self.stratum_stats.write();
		stratum_stats.block_height = height;
	}

	pub fn update_network_difficulty(&self, difficulty: DifficultyNumber) {
		let mut stratum_stats = self.stratum_stats.write();
		stratum_stats.network_difficulty = difficulty;
	}
}

// ----------------------------------------
// Epic Stratum Server

pub struct StratumServer {
	id: String,
	config: StratumServerConfig,
	chain: Arc<chain::Chain>,
	pub tx_pool: ServerTxPool,
	sync_state: Arc<SyncState>,
	stratum_stats: Arc<RwLock<StratumStats>>,
}

impl StratumServer {
	/// Creates a new Stratum Server.
	pub fn new(
		config: StratumServerConfig,
		chain: Arc<chain::Chain>,
		tx_pool: ServerTxPool,
		stratum_stats: Arc<RwLock<StratumStats>>,
	) -> StratumServer {
		StratumServer {
			id: String::from("0"),
			config,
			chain,
			tx_pool,
			sync_state: Arc::new(SyncState::new()),
			stratum_stats,
		}
	}

	/// "main()" - Starts the stratum-server.  Creates a thread to Listens for
	/// a connection, then enters a loop, building a new block on top of the
	/// existing chain anytime required and sending that to the connected
	/// stratum miner, proxy, or pool, and accepts full solutions to
	/// be submitted.
	pub fn run_loop(&mut self, edge_bits: u32, proof_size: usize, sync_state: Arc<SyncState>) {
		info!(
			"(Server ID: {}) Starting stratum server with edge_bits = {}, proof_size = {}",
			self.id, edge_bits, proof_size
		);

		self.sync_state = sync_state;

		let listen_addr = self
			.config
			.stratum_server_addr
			.clone()
			.unwrap()
			.parse()
			.expect("Stratum: Incorrect address ");

		let handler = Arc::new(Handler::from_stratum(&self));
		let h = handler.clone();

		let _listener_th = thread::spawn(move || {
			accept_connections(listen_addr, h);
		});

		// We have started
		{
			let mut stratum_stats = self.stratum_stats.write();
			stratum_stats.is_running = true;
			stratum_stats.edge_bits = edge_bits as u16;
		}

		info!(
			"Stratum server started on {}",
			self.config.stratum_server_addr.clone().unwrap()
		);

		// Initial Loop. Waiting node complete syncing
		while self.sync_state.is_syncing() {
			thread::sleep(Duration::from_millis(50));
		}

		handler.run(&self.config, &self.tx_pool);
	} // fn run_loop()
} // StratumServer

#[cfg(test)]
mod test {
	use super::*;
	use std::io;
	use std::pin::Pin;
	use std::task::{Context, Poll};
	use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadBuf};
	use tokio::net::TcpStream;
	use tokio::time::timeout;

	fn test_workers() -> (Arc<WorkersList>, Arc<RwLock<StratumStats>>) {
		let stratum_stats = Arc::new(RwLock::new(StratumStats::default()));
		(
			Arc::new(WorkersList::new(stratum_stats.clone())),
			stratum_stats,
		)
	}

	async fn run_test_connection<S>(
		stream: S,
		workers: Arc<WorkersList>,
	) -> tokio::task::JoinHandle<()>
	where
		S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
	{
		let (tx, rx) = mpsc::unbounded();
		let worker_id = workers.add_worker(tx).await;
		tokio::spawn(handle_connection(
			stream,
			worker_id,
			rx,
			workers,
			|_, _| {},
		))
	}

	fn assert_worker_cleaned(
		workers: &WorkersList,
		stratum_stats: &RwLock<StratumStats>,
	) {
		assert_eq!(workers.count(), 0);
		assert_eq!(stratum_stats.read().num_workers, 0);
		assert!(stratum_stats.read().worker_stats.is_empty());
	}

	async fn assert_invalid_input_cleans_worker(input: &[u8]) {
		let (server, mut client) = tokio::io::duplex(4096);
		let (workers, stratum_stats) = test_workers();
		let task = run_test_connection(server, workers.clone()).await;

		client.write_all(input).await.unwrap();
		client.shutdown().await.unwrap();
		timeout(Duration::from_secs(5), task)
			.await
			.expect("connection task did not stop")
			.unwrap();
		assert_worker_cleaned(&workers, &stratum_stats);
	}

	struct FailingWriter;

	impl AsyncRead for FailingWriter {
		fn poll_read(
			self: Pin<&mut Self>,
			_cx: &mut Context<'_>,
			_buf: &mut ReadBuf<'_>,
		) -> Poll<io::Result<()>> {
			Poll::Pending
		}
	}

	impl AsyncWrite for FailingWriter {
		fn poll_write(
			self: Pin<&mut Self>,
			_cx: &mut Context<'_>,
			_buf: &[u8],
		) -> Poll<io::Result<usize>> {
			Poll::Ready(Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed")))
		}

		fn poll_flush(
			self: Pin<&mut Self>,
			_cx: &mut Context<'_>,
		) -> Poll<io::Result<()>> {
			Poll::Ready(Ok(()))
		}

		fn poll_shutdown(
			self: Pin<&mut Self>,
			_cx: &mut Context<'_>,
		) -> Poll<io::Result<()>> {
			Poll::Ready(Ok(()))
		}
	}

	#[tokio::test]
	async fn tcp_eof_cleans_worker_and_stops_job_fanout() {
		let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
		let client = TcpStream::connect(listener.local_addr().unwrap())
			.await
			.unwrap();
		let (server, _) = listener.accept().await.unwrap();
		let (workers, stratum_stats) = test_workers();
		let task = run_test_connection(server, workers.clone()).await;
		let (mut client_reader, mut client_writer) = client.into_split();

		client_writer.shutdown().await.unwrap();
		timeout(Duration::from_secs(5), task)
			.await
			.expect("connection task did not stop after EOF")
			.unwrap();
		assert_worker_cleaned(&workers, &stratum_stats);

		workers.broadcast("stale job".to_string());
		let mut byte = [0; 1];
		assert_eq!(
			timeout(Duration::from_secs(5), client_reader.read(&mut byte))
				.await
				.expect("client did not observe the closed connection")
				.unwrap(),
			0
		);
	}

	#[tokio::test]
	async fn malformed_json_cleans_worker() {
		assert_invalid_input_cleans_worker(b"{]\n").await;
	}

	#[tokio::test]
	async fn line_codec_failure_cleans_worker() {
		assert_invalid_input_cleans_worker(b"\xff\n").await;
	}

	#[tokio::test]
	async fn writer_failure_cleans_worker() {
		let (workers, stratum_stats) = test_workers();
		let task = run_test_connection(FailingWriter, workers.clone()).await;

		workers.broadcast("job".to_string());
		timeout(Duration::from_secs(5), task)
			.await
			.expect("connection task did not stop after writer failure")
			.unwrap();
		assert_worker_cleaned(&workers, &stratum_stats);
	}

	#[test]
	fn worker_name_uses_the_last_login_component() {
		assert_eq!(worker_name_from_login("username"), None);
		assert_eq!(worker_name_from_login("username.myworker"), Some("myworker"));
		assert_eq!(
			worker_name_from_login("esXsdfsdf@epiccash.com.myworker"),
			Some("myworker")
		);
	}

	#[tokio::test]
	async fn login_names_resume_stats_and_anonymous_workers_are_removed() {
		let stratum_stats = Arc::new(RwLock::new(StratumStats::default()));
		let workers = WorkersList::new(stratum_stats.clone());
		let (tx, _rx) = mpsc::unbounded();
		let worker_id = workers.add_worker(tx).await;

		assert_eq!(stratum_stats.read().num_workers, 1);
		assert_eq!(stratum_stats.read().worker_stats.len(), 1);
		assert_eq!(stratum_stats.read().worker_stats[0].id, "worker_0");

		workers
			.login(
				worker_id,
				"esXsdfsdf@epiccash.com.myworker".to_string(),
				"test-miner".to_string(),
			)
			.unwrap();
		assert_eq!(workers.get_stats(worker_id).unwrap().id, "myworker");
		workers.update_stats(worker_id, |ws| ws.num_accepted += 1);

		workers.remove_worker(worker_id);
		assert_eq!(workers.count(), 0);
		assert_eq!(stratum_stats.read().num_workers, 0);
		assert_eq!(stratum_stats.read().worker_stats.len(), 1);
		assert!(!stratum_stats.read().worker_stats[0].is_connected);
		assert_eq!(stratum_stats.read().worker_stats[0].num_accepted, 1);

		workers.remove_worker(worker_id);
		assert_eq!(workers.count(), 0);
		assert_eq!(stratum_stats.read().num_workers, 0);

		let (tx, _rx) = mpsc::unbounded();
		let next_worker_id = workers.add_worker(tx).await;
		assert_eq!(next_worker_id, worker_id + 1);
		workers
			.login(
				next_worker_id,
				"username.myworker".to_string(),
				"test-miner".to_string(),
			)
			.unwrap();
		assert_eq!(stratum_stats.read().worker_stats.len(), 1);
		assert_eq!(
			stratum_stats.read().worker_stats[0].id,
			"myworker"
		);
		assert!(stratum_stats.read().worker_stats[0].is_connected);
		assert_eq!(stratum_stats.read().worker_stats[0].num_accepted, 1);

		workers.remove_worker(next_worker_id);
		let (tx, _rx) = mpsc::unbounded();
		let anonymous_worker_id = workers.add_worker(tx).await;
		assert_eq!(anonymous_worker_id, next_worker_id + 1);
		workers
			.login(
				anonymous_worker_id,
				"myusername".to_string(),
				"test-miner".to_string(),
			)
			.unwrap();
		assert_eq!(workers.get_stats(anonymous_worker_id).unwrap().id, "worker_2");
		workers.remove_worker(anonymous_worker_id);
		assert_eq!(stratum_stats.read().worker_stats.len(), 1);
		assert_eq!(stratum_stats.read().worker_stats[0].id, "myworker");
	}
}

// Utility function to parse a JSON RPC parameter object, returning a proper
// error if things go wrong.
fn parse_params<T>(params: Option<Value>) -> Result<T, RpcError>
where
	for<'de> T: serde::Deserialize<'de>,
{
	params
		.and_then(|v| serde_json::from_value(v).ok())
		.ok_or(RpcError::invalid_request())
}
