// Copyright 2018 The Epic Foundation
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

//! Epic server implementation, glues the different parts of the system (mostly
//! the peer-to-peer server, the blockchain and the transaction pool) and acts
//! as a facade.

use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use std::sync::Arc;
use std::{
	thread::{self, JoinHandle},
	time::{self, Duration},
};

use fs2::FileExt;

use clokwerk::{ScheduleHandle, Scheduler, TimeUnits};

use crate::api;
use crate::api::TLSConfig;
use crate::chain;
use crate::common::adapters::{
	ChainToPoolAndNetAdapter, NetToChainAdapter, PoolToChainAdapter, PoolToNetAdapter,
};
use crate::common::hooks::{init_chain_hooks, init_net_hooks};
use crate::common::stats::{DiffBlock, DiffStats, PeerStats, ServerStateInfo, ServerStats};
use crate::common::types::{Error, ServerConfig, StratumServerConfig, SyncState, SyncStatus};
use crate::core::core::hash::{Hash, Hashed, ZERO_HASH};
use crate::core::core::verifier_cache::{LruVerifierCache, VerifierCache};
use crate::core::pow::{PoWType, Proof};
use crate::core::{consensus, genesis, global, pow};
use crate::epic::{dandelion_monitor, seed, sync, version};
use crate::mining::stratumserver;
use crate::mining::test_miner::Miner;
use crate::p2p;
use crate::p2p::types::PeerAddr;
use crate::pool;
use crate::util::file::get_first_line;
use crate::util::{RwLock, StopState};

/// Epic server holding internal structures.
pub struct Server {
	/// server config
	pub config: ServerConfig,
	/// handle to our network server
	pub p2p: Arc<p2p::Server>,
	/// data store access
	pub chain: Arc<chain::Chain>,
	/// in-memory transaction pool
	tx_pool: Arc<RwLock<pool::TransactionPool>>,
	/// Shared cache for verification results when
	/// verifying rangeproof and kernel signatures.
	verifier_cache: Arc<RwLock<dyn VerifierCache>>,
	/// Whether we're currently syncing
	sync_state: Arc<SyncState>,
	/// To be passed around to collect stats and info
	state_info: ServerStateInfo,
	/// Stop flag
	pub stop_state: Arc<StopState>,
	/// Maintain a lock_file so we do not run multiple Epic nodes from same dir.
	lock_file: Arc<File>,
	connect_thread: Option<JoinHandle<()>>,
	sync_thread: JoinHandle<()>,
	dandelion_thread: JoinHandle<()>,
	p2p_thread: JoinHandle<()>,
	version_checker_thread: ScheduleHandle,
}

impl Server {
	/// Instantiates and starts a new server. Optionally takes a callback
	/// for the server to send an ARC copy of itself, to allow another process
	/// to poll info about the server status
	pub fn start<F>(config: ServerConfig, mut info_callback: F) -> Result<(), Error>
	where
		F: FnMut(Server),
	{
		/*let policy_config = config.policy_config.clone();
		for i in 0..policy_config.policies.len() {
			if policy_config.policies[i]
				.values()
				.fold(0, |acc, &x| x + acc)
				!= 100
			{
				panic!("Error reading the .toml file!\n The values of the policy number <{}> must sum to 100 and be integers!\n", i);
			};
		}
		// set the policies configs from the .toml file
		global::set_policy_config(policy_config);*/
		global::set_foundation_path(config.foundation_path.clone().to_owned());
		info!(
			"The policy configuration is: {:?}",
			global::get_policy_config()
		);
		info!(
			"The foundation.json is being read from {:?}",
			global::get_foundation_path().unwrap()
		);
		let mining_config = config.stratum_mining_config.clone();
		let enable_test_miner = config.run_test_miner;
		let test_miner_wallet_url = config.test_miner_wallet_url.clone();
		let serv = Server::new(config)?;

		if let Some(c) = mining_config {
			let enable_stratum_server = c.enable_stratum_server;
			if let Some(s) = enable_stratum_server {
				if s {
					{
						let mut stratum_stats = serv.state_info.stratum_stats.write();
						stratum_stats.is_enabled = true;
					}
					serv.start_stratum_server(c.clone());
				}
			}
		}

		if let Some(s) = enable_test_miner {
			if s {
				serv.start_test_miner(test_miner_wallet_url, serv.stop_state.clone());
			}
		}

		info_callback(serv);
		Ok(())
	}

	// Exclusive (advisory) lock_file to ensure we do not run multiple
	// instance of epic server from the same dir.
	// This uses fs2 and should be safe cross-platform unless somebody abuses the file itself.
	fn one_epic_at_a_time(config: &ServerConfig) -> Result<Arc<File>, Error> {
		let path = Path::new(&config.db_root);
		fs::create_dir_all(path.clone())?;
		let path = path.join("epic.lock");
		let lock_file = fs::OpenOptions::new()
			.read(true)
			.write(true)
			.create(true)
			.open(&path)?;
		lock_file.try_lock_exclusive().map_err(|e| {
			let mut stderr = std::io::stderr();
			writeln!(
				&mut stderr,
				"Failed to lock {:?} (epic server already running?)",
				path
			)
			.expect("Could not write to stderr");
			e
		})?;
		Ok(Arc::new(lock_file))
	}

	/// Instantiates a new server associated with the provided future reactor.
	pub fn new(config: ServerConfig) -> Result<Server, Error> {
		// Obtain our lock_file or fail immediately with an error.
		let lock_file = Server::one_epic_at_a_time(&config)?;

		// Defaults to None (optional) in config file.
		// This translates to false here.
		let archive_mode = match config.archive_mode {
			None => false,
			Some(b) => b,
		};

		let stop_state = Arc::new(StopState::new());

		// Shared cache for verification results.
		// We cache rangeproof verification and kernel signature verification.
		let verifier_cache = Arc::new(RwLock::new(LruVerifierCache::new()));

		let pool_adapter = Arc::new(PoolToChainAdapter::new());
		let pool_net_adapter = Arc::new(PoolToNetAdapter::new(config.dandelion_config.clone()));
		let tx_pool = Arc::new(RwLock::new(pool::TransactionPool::new(
			config.pool_config.clone(),
			pool_adapter.clone(),
			verifier_cache.clone(),
			pool_net_adapter.clone(),
		)));

		let sync_state = Arc::new(SyncState::new());

		let chain_adapter = Arc::new(ChainToPoolAndNetAdapter::new(
			tx_pool.clone(),
			init_chain_hooks(&config),
		));

		let genesis = match config.chain_type {
			global::ChainTypes::AutomatedTesting => genesis::genesis_dev(),
			global::ChainTypes::UserTesting => genesis::genesis_dev(),
			global::ChainTypes::Floonet => genesis::genesis_floo(),
			global::ChainTypes::Mainnet => genesis::genesis_main(),
		};

		info!("Starting server, genesis block: {}", genesis.hash());

		let shared_chain = Arc::new(chain::Chain::init(
			config.db_root.clone(),
			chain_adapter.clone(),
			genesis.clone(),
			pow::verify_size,
			verifier_cache.clone(),
			archive_mode,
		)?);

		pool_adapter.set_chain(shared_chain.clone());

		let net_adapter = Arc::new(NetToChainAdapter::new(
			sync_state.clone(),
			shared_chain.clone(),
			tx_pool.clone(),
			verifier_cache.clone(),
			config.clone(),
			init_net_hooks(&config),
		));

		let p2p_server = Arc::new(p2p::Server::new(
			&config.db_root,
			config.p2p_config.capabilities,
			config.p2p_config.clone(),
			net_adapter.clone(),
			genesis.hash(),
			stop_state.clone(),
		)?);

		// Initialize various adapters with our dynamic set of connected peers.
		chain_adapter.init(p2p_server.peers.clone());
		pool_net_adapter.init(p2p_server.peers.clone());
		net_adapter.init(p2p_server.peers.clone());

		let mut connect_thread = None;

		if config.p2p_config.seeding_type != p2p::Seeding::Programmatic {
			let seeder = match config.p2p_config.seeding_type {
				p2p::Seeding::None => {
					warn!("No seed configured, will stay solo until connected to");
					seed::predefined_seeds(vec![])
				}
				p2p::Seeding::List => match &config.p2p_config.seeds {
					Some(seeds) => seed::predefined_seeds(seeds.clone()),
					None => {
						return Err(Error::Configuration(
							"Seeds must be configured for seeding type List".to_owned(),
						));
					}
				},
				p2p::Seeding::DNSSeed => seed::dns_seeds(),
				_ => unreachable!(),
			};

			connect_thread = Some(seed::connect_and_monitor(
				p2p_server.clone(),
				config.p2p_config.capabilities,
				seeder,
				config.p2p_config.peers_preferred.clone(),
				stop_state.clone(),
			)?);
		}

		// Defaults to None (optional) in config file.
		// This translates to false here so we do not skip by default.
		let skip_sync_wait = config.skip_sync_wait.unwrap_or(false);
		sync_state.update(SyncStatus::AwaitingPeers(!skip_sync_wait));

		let sync_thread = sync::run_sync(
			sync_state.clone(),
			p2p_server.peers.clone(),
			shared_chain.clone(),
			stop_state.clone(),
		)?;

		let p2p_inner = p2p_server.clone();
		let p2p_thread = thread::Builder::new()
			.name("p2p-server".to_string())
			.spawn(move || {
				if let Err(e) = p2p_inner.listen() {
					error!("P2P server failed with error: {:?}", e);
				}
			})?;

		info!("Starting rest apis at: {}", &config.api_http_addr);
		let api_secret = get_first_line(config.api_secret_path.clone());

		let tls_conf = match config.tls_certificate_file.clone() {
			None => None,
			Some(file) => {
				let key = match config.tls_certificate_key.clone() {
					Some(k) => k,
					None => {
						let msg = format!("Private key for certificate is not set");
						return Err(Error::ArgumentError(msg));
					}
				};
				Some(TLSConfig::new(file, key))
			}
		};

		// TODO fix API shutdown and join this thread
		api::start_rest_apis(
			config.api_http_addr.clone(),
			shared_chain.clone(),
			tx_pool.clone(),
			p2p_server.peers.clone(),
			api_secret,
			tls_conf,
		);

		info!("Starting dandelion monitor: {}", &config.api_http_addr);
		let dandelion_thread = dandelion_monitor::monitor_transactions(
			config.dandelion_config.clone(),
			tx_pool.clone(),
			pool_net_adapter.clone(),
			verifier_cache.clone(),
			stop_state.clone(),
		)?;

		info!("Starting the version checker monitor!");
		let mut scheduler = Scheduler::new();
		scheduler.every(15.minutes()).run(|| {
			if let Ok(dns_version) = version::get_dns_version() {
				if let Some(our_version) = global::get_epic_version() {
					if !version::is_version_valid(our_version.clone(), dns_version.clone()) {
						error!(
							"Your current epic node version {}.{}.X.X is outdated! Please consider updating your code to the newest version {}.{}.X.X!",
							our_version.version_major,
							our_version.version_minor,
							dns_version.version_major,
							dns_version.version_minor,
						);
						error!("Closing the application!");
						std::process::exit(1);
					}
				} else {
					error!("Failed to retrieve information about the this application's version!");
				}
			} else {
				error!("Unable to get the allowed versions from the dns server!");
			}
		});
		let version_checker_thread = scheduler.watch_thread(Duration::from_millis(100));

		warn!("Epic server started.");
		Ok(Server {
			config,
			p2p: p2p_server,
			chain: shared_chain,
			tx_pool,
			verifier_cache,
			sync_state,
			state_info: ServerStateInfo {
				..Default::default()
			},
			stop_state,
			lock_file,
			connect_thread,
			sync_thread,
			p2p_thread,
			dandelion_thread,
			version_checker_thread,
		})
	}

	/// Asks the server to connect to a peer at the provided network address.
	pub fn connect_peer(&self, addr: PeerAddr) -> Result<(), Error> {
		self.p2p.connect(addr)?;
		Ok(())
	}

	/// Ping all peers, mostly useful for tests to have connected peers share
	/// their heights
	pub fn ping_peers(&self) -> Result<(), Error> {
		let head = self.chain.head()?;
		self.p2p.peers.check_all(head.total_difficulty, head.height);
		Ok(())
	}

	/// Number of peers
	pub fn peer_count(&self) -> u32 {
		self.p2p.peers.peer_count()
	}

	/// Start a minimal "stratum" mining service on a separate thread
	pub fn start_stratum_server(&self, config: StratumServerConfig) {
		let edge_bits = global::min_edge_bits();
		let proof_size = global::proofsize();
		let sync_state = self.sync_state.clone();

		let mut stratum_server = stratumserver::StratumServer::new(
			config.clone(),
			self.chain.clone(),
			self.tx_pool.clone(),
			self.verifier_cache.clone(),
			self.state_info.stratum_stats.clone(),
		);
		let _ = thread::Builder::new()
			.name("stratum_server".to_string())
			.spawn(move || {
				stratum_server.run_loop(edge_bits as u32, proof_size, sync_state);
			});
	}

	/// Start mining for blocks internally on a separate thread. Relies on
	/// internal miner, and should only be used for automated testing. Burns
	/// reward if wallet_listener_url is 'None'
	pub fn start_test_miner(
		&self,
		wallet_listener_url: Option<String>,
		stop_state: Arc<StopState>,
	) {
		info!("start_test_miner - start",);
		let sync_state = self.sync_state.clone();
		let config_wallet_url = match wallet_listener_url.clone() {
			Some(u) => u,
			None => String::from("http://127.0.0.1:13415"),
		};

		let config = StratumServerConfig {
			attempt_time_per_block: 60,
			burn_reward: false,
			enable_stratum_server: None,
			stratum_server_addr: None,
			wallet_listener_url: config_wallet_url,
			cuckatoo_minimum_share_difficulty: 1,
			randomx_minimum_share_difficulty: 1,
			progpow_minimum_share_difficulty: 1,
		};

		let mut miner = Miner::new(
			config.clone(),
			self.chain.clone(),
			self.tx_pool.clone(),
			self.verifier_cache.clone(),
			stop_state,
		);
		miner.set_debug_output_id(format!("Port {}", self.config.p2p_config.port));
		let _ = thread::Builder::new()
			.name("test_miner".to_string())
			.spawn(move || {
				// TODO push this down in the run loop so miner gets paused anytime we
				// decide to sync again
				let secs_5 = time::Duration::from_secs(5);
				while sync_state.is_syncing() {
					thread::sleep(secs_5);
				}
				miner.run_loop(wallet_listener_url);
			});
	}

	/// The chain head
	pub fn head(&self) -> Result<chain::Tip, Error> {
		self.chain.head().map_err(|e| e.into())
	}

	/// The head of the block header chain
	pub fn header_head(&self) -> Result<chain::Tip, Error> {
		self.chain.header_head().map_err(|e| e.into())
	}

	/// Current p2p layer protocol version.
	pub fn protocol_version() -> p2p::msg::ProtocolVersion {
		p2p::msg::ProtocolVersion::default()
	}

	/// Returns a set of stats about this server. This and the ServerStats
	/// structure
	/// can be updated over time to include any information needed by tests or
	/// other
	/// consumers
	pub fn get_server_stats(&self) -> Result<ServerStats, Error> {
		let stratum_stats = self.state_info.stratum_stats.read().clone();

		// Fill out stats on our current difficulty calculation
		// TODO: check the overhead of calculating this again isn't too much
		// could return it from next_difficulty, but would rather keep consensus
		// code clean. This may be handy for testing but not really needed
		// for release
		let diff_stats = {
			let last_blocks: Vec<consensus::HeaderInfo> =
				global::difficulty_data_to_vector(self.chain.difficulty_iter_all()?, 100)
					.into_iter()
					.collect();

			let tip_height = self.head()?.height as i64;
			let mut height = tip_height as i64 - last_blocks.len() as i64 + 1;

			let txhashset = self.chain.txhashset();
			let txhashset = txhashset.read();

			let diff_entries: Vec<DiffBlock> = last_blocks
				.windows(2)
				.map(|pair| {
					let prev = &pair[0];
					let next = &pair[1];

					height += 1;

					// Use header hash if real header.
					// Default to "zero" hash if synthetic header_info.
					let (hash, algo_type): (Hash, Option<Proof>) = if height >= 0 {
						if let Ok(header) = txhashset.get_header_by_height(height as u64) {
							(header.hash(), Some(header.pow.proof.clone()))
						} else {
							(ZERO_HASH, None)
						}
					} else {
						(ZERO_HASH, None)
					};
					let (algo_type, algo_name) = if let Some(proof) = algo_type {
						match proof {
							Proof::CuckooProof { .. } => (PoWType::Cuckatoo, "Cuckatoo"),
							Proof::RandomXProof { .. } => (PoWType::RandomX, "RandomX"),
							Proof::ProgPowProof { .. } => (PoWType::ProgPow, "ProgPow"),
							_ => (PoWType::Cuckatoo, "Cuckatoo"),
						}
					} else {
						(PoWType::Cuckatoo, "Cuckatoo")
					};
					let duration = if height <= 1 {
						60
					} else {
						next.timestamp - prev.timestamp
					};
					DiffBlock {
						block_height: height,
						block_hash: hash,
						difficulty: next.difficulty.to_num(algo_type),
						time: next.timestamp,
						duration: duration,
						secondary_scaling: next.secondary_scaling,
						is_secondary: next.is_secondary,
						algorithm: algo_name.to_owned(),
					}
				})
				.collect();
			let mut block_cuckatoo: Vec<DiffBlock> = Vec::new();
			let mut block_randomx: Vec<DiffBlock> = Vec::new();
			let mut block_progpow: Vec<DiffBlock> = Vec::new();
			for diff_block in &diff_entries {
				match diff_block.algorithm.as_str() {
					"Cuckatoo" => block_cuckatoo.push(diff_block.clone()),
					"RandomX" => block_randomx.push(diff_block.clone()),
					"ProgPow" => block_progpow.push(diff_block.clone()),
					_ => (),
				};
			}
			let (cuckatoo_avg_time, cuckatoo_avg_diff) =
				get_difficulty_info_average(block_cuckatoo);
			let (progpow_avg_time, progpow_avg_diff) = get_difficulty_info_average(block_progpow);
			let (randomx_avg_time, randomx_avg_diff) = get_difficulty_info_average(block_randomx);
			let avg_block_time = format!(
				"Cuckatoo: {} secs, ProgPow: {} secs, RandomX: {} secs",
				cuckatoo_avg_time, progpow_avg_time, randomx_avg_time
			);
			let avg_block_difficulty = format!(
				"Cuckatoo: {}, ProgPow: {}, RandomX: {}",
				cuckatoo_avg_diff, progpow_avg_diff, randomx_avg_diff
			);
			DiffStats {
				height: height as u64,
				last_blocks: diff_entries,
				average_block_time: avg_block_time,
				average_difficulty: avg_block_difficulty,
				window_size: 100,
			}
		};

		let peer_stats = self
			.p2p
			.peers
			.connected_peers()
			.into_iter()
			.map(|p| PeerStats::from_peer(&p))
			.collect();
		Ok(ServerStats {
			peer_count: self.peer_count(),
			head: self.head()?,
			header_head: self.header_head()?,
			sync_status: self.sync_state.status(),
			stratum_stats: stratum_stats,
			peer_stats: peer_stats,
			diff_stats: diff_stats,
		})
	}

	/// Stop the server.
	pub fn stop(self) {
		{
			self.sync_state.update(SyncStatus::Shutdown);
			self.stop_state.stop();

			if let Some(connect_thread) = self.connect_thread {
				match connect_thread.join() {
					Err(e) => error!("failed to join to connect_and_monitor thread: {:?}", e),
					Ok(_) => info!("connect_and_monitor thread stopped"),
				}
			} else {
				info!("No active connect_and_monitor thread")
			}

			match self.sync_thread.join() {
				Err(e) => error!("failed to join to sync thread: {:?}", e),
				Ok(_) => info!("sync thread stopped"),
			}

			match self.dandelion_thread.join() {
				Err(e) => error!("failed to join to dandelion_monitor thread: {:?}", e),
				Ok(_) => info!("dandelion_monitor thread stopped"),
			}
		}
		self.p2p.stop();
		match self.p2p_thread.join() {
			Err(e) => error!("failed to join to p2p thread: {:?}", e),
			Ok(_) => info!("p2p thread stopped"),
		}

		self.version_checker_thread.stop();
		let _ = self.lock_file.unlock();
	}

	/// Pause the p2p server.
	pub fn pause(&self) {
		self.stop_state.pause();
		thread::sleep(time::Duration::from_secs(1));
		self.p2p.pause();
	}

	/// Resume p2p server.
	/// TODO - We appear not to resume the p2p server (peer connections) here?
	pub fn resume(&self) {
		self.stop_state.resume();
	}

	/// Stops the test miner without stopping the p2p layer
	pub fn stop_test_miner(&self, stop: Arc<StopState>) {
		stop.stop();
		info!("stop_test_miner - stop",);
	}
}

fn get_difficulty_info_average(diff_entries: Vec<DiffBlock>) -> (String, String) {
	let num_elements = diff_entries.len() as u64;
	if diff_entries.len() > 0 {
		let block_time_sum: u64 = diff_entries.iter().fold(0, |sum, t| sum + t.duration);
		let block_diff_sum: u64 = diff_entries.iter().fold(0, |sum, d| sum + d.difficulty);
		(
			format!("{}", block_time_sum / num_elements),
			format!("{}", block_diff_sum / num_elements),
		)
	} else {
		("NaN".to_owned(), "NaN".to_owned())
	}
}
