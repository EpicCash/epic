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

//! Values that should be shared across all modules, without necessarily
//! having to pass them all over the place, but aren't consensus values.
//! should be used sparingly.

use crate::consensus::HeaderInfo;
use crate::consensus::{
	graph_weight, BASE_EDGE_BITS, BLOCK_TIME_SEC, COINBASE_MATURITY, CUT_THROUGH_HORIZON,
	DAY_HEIGHT, DEFAULT_MIN_EDGE_BITS, DIFFICULTY_ADJUST_WINDOW, INITIAL_DIFFICULTY,
	MAX_BLOCK_WEIGHT, PROOFSIZE, SECOND_POW_EDGE_BITS, STATE_SYNC_THRESHOLD,
};
use crate::core::block::feijoada::{AllowPolicy, Policy, PolicyConfig};
use crate::pow::{self, new_cuckaroo_ctx, new_cuckatoo_ctx, EdgeType, PoWContext};
/// An enum collecting sets of parameters used throughout the
/// code wherever mining is needed. This should allow for
/// different sets of parameters for different purposes,
/// e.g. CI, User testing, production values
use crate::util::RwLock;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::path::Path;
/// Define these here, as they should be developer-set, not really tweakable
/// by users

/// The default "local" protocol version for this node.
/// We negotiate compatible versions with each peer via Hand/Shake.
/// Note: We also use a specific (possible different) protocol version
/// for both the backend database and MMR data files.
/// This defines the p2p layer protocol version for this node.
pub const PROTOCOL_VERSION: u32 = 2;

/// Automated testing edge_bits
pub const AUTOMATED_TESTING_MIN_EDGE_BITS: u8 = 9;

/// Automated testing proof size
pub const AUTOMATED_TESTING_PROOF_SIZE: usize = 4;

/// User testing edge_bits
pub const USER_TESTING_MIN_EDGE_BITS: u8 = 15;

/// User testing proof size
pub const USER_TESTING_PROOF_SIZE: usize = 42;

/// Automated testing coinbase maturity
pub const AUTOMATED_TESTING_COINBASE_MATURITY: u64 = 3;

/// User testing coinbase maturity
pub const USER_TESTING_COINBASE_MATURITY: u64 = 3;

/// Foonet coinbase maturity
pub const FLOONET_COINBASE_MATURITY: u64 = 30;

/// Testing cut through horizon in blocks
pub const TESTING_CUT_THROUGH_HORIZON: u32 = 70;

/// Testing state sync threshold in blocks
pub const TESTING_STATE_SYNC_THRESHOLD: u32 = 20;

/// Testing initial graph weight
pub const TESTING_INITIAL_GRAPH_WEIGHT: u32 = 1;

/// Testing initial block difficulty
pub const TESTING_INITIAL_DIFFICULTY: u64 = 1;

/// Testing max_block_weight (artifically low, just enough to support a few txs).
pub const TESTING_MAX_BLOCK_WEIGHT: usize = 150;

/// If a peer's last updated difficulty is 2 hours ago and its difficulty's lower than ours,
/// we're sure this peer is a stuck node, and we will kick out such kind of stuck peers.
pub const STUCK_PEER_KICK_TIME: i64 = 2 * 3600 * 1000;

/// If a peer's last seen time is 2 weeks ago we will forget such kind of defunct peers.
const PEER_EXPIRATION_DAYS: i64 = 7 * 2;

/// Constant that expresses defunct peer timeout in seconds to be used in checks.
pub const PEER_EXPIRATION_REMOVE_TIME: i64 = PEER_EXPIRATION_DAYS * 24 * 3600;

/// Trigger compaction check on average every day for all nodes.
/// Randomized per node - roll the dice on every block to decide.
/// Will compact the txhashset to remove pruned data.
/// Will also remove old blocks and associated data from the database.
/// For a node configured as "archival_mode = true" only the txhashset will be compacted.
pub const COMPACTION_CHECK: u64 = DAY_HEIGHT;

/// Number of blocks to reuse a txhashset zip for (automated testing and user testing).
pub const TESTING_TXHASHSET_ARCHIVE_INTERVAL: u64 = 10;

/// Number of blocks to reuse a txhashset zip for.
pub const TXHASHSET_ARCHIVE_INTERVAL: u64 = 12 * 60;

pub const CURRENT_HEADER_VERSION: u16 = 7;

#[cfg(target_family = "unix")]
pub const FOUNDATION_JSON_SHA256: &str =
	"ddf5ad515d3200d1c9fe2a566b9eb81cff0835690ce7f6f3b2a89ee52636ada0";
#[cfg(target_family = "windows")]
pub const FOUNDATION_JSON_SHA256: &str =
	"4d01ca4134959d19ae1b76058c8d11040b63bd1bd112401b80b36185e7faf94a";

/// Types of chain a server can run with, dictates the genesis block and
/// and mining parameters used.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChainTypes {
	/// For CI testing
	AutomatedTesting,
	/// For User testing
	UserTesting,
	/// Protocol testing network
	Floonet,
	/// Main production network
	Mainnet,
}

impl ChainTypes {
	/// Short name representing the chain type ("floo", "main", etc.)
	pub fn shortname(&self) -> String {
		match *self {
			ChainTypes::AutomatedTesting => "auto".to_owned(),
			ChainTypes::UserTesting => "user".to_owned(),
			ChainTypes::Floonet => "floo".to_owned(),
			ChainTypes::Mainnet => "main".to_owned(),
		}
	}
}

impl Default for ChainTypes {
	fn default() -> ChainTypes {
		ChainTypes::Mainnet
	}
}

/// PoW test mining and verifier context
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PoWContextTypes {
	/// Classic Cuckoo
	Cuckoo,
	/// ASIC-friendly Cuckatoo
	Cuckatoo,
	/// ASIC-resistant Cuckaroo
	Cuckaroo,
}

lazy_static! {
	/// The mining parameter mode
	pub static ref CHAIN_TYPE: RwLock<ChainTypes> =
			RwLock::new(ChainTypes::Mainnet);

	/// PoW context type to instantiate
	pub static ref POW_CONTEXT_TYPE: RwLock<PoWContextTypes> =
			RwLock::new(PoWContextTypes::Cuckoo);

	/// The policy parameters
	pub static ref POLICY_CONFIG : RwLock<PolicyConfig> =
			RwLock::new(PolicyConfig::default());

	/// The path to the file that contains the foundation
	pub static ref FOUNDATION_FILE : RwLock<Option<String>> =
			RwLock::new(None);

	/// Store the current epic version being executed
	pub static ref EPIC_VERSION : RwLock<Option<Version>> =
			RwLock::new(None);

	/// Store the timeout for the header sync
	pub static ref HEADER_SYNC_TIMEOUT : RwLock<i64> =
			RwLock::new(10);
}

/// Get the current Timeout without the verification of the existence of more headers to be synced,
/// after all header were processed
pub fn get_header_sync_timeout() -> i64 {
	let header_sync_timeout = HEADER_SYNC_TIMEOUT.read();
	header_sync_timeout.clone()
}

/// Set the current Timeout without the verification of the existence of more headers to be synced,
/// after all header were processed
pub fn set_header_sync_timeout(timeout: i64) {
	let mut header_sync_timeout = HEADER_SYNC_TIMEOUT.write();
	*header_sync_timeout = if timeout <= 0 { 10 } else { timeout }
}

/// Set the version of the current epic executable
pub fn set_epic_version(version_major: String, version_minor: String) {
	let mut epic_version = EPIC_VERSION.write();
	let major_int: u32 = version_major.parse().expect("The current version of the epic node in the Cargo.toml is not valid! The major release value should be an integer");
	let minor_int: u32 = version_minor.parse().expect("The current version of the epic node in the Cargo.toml is not valid! The minor release value should be an integer");
	*epic_version = Some(Version::new(major_int, minor_int));
}

/// Get the version of the current epic executable
pub fn get_epic_version() -> Option<Version> {
	let epic_version = EPIC_VERSION.read();
	epic_version.clone()
}

/// Set the path to the foundation.json file (file with the foundation wallet outputs/kernels)
pub fn set_foundation_path(path: String) {
	let mut foundation_path = FOUNDATION_FILE.write();
	let path_str = use_alternative_path(path);
	*foundation_path = Some(path_str);
}

///	Check if the foundation.json exists in the directory appointed by the .toml file, if not,
/// use the alternative path ../../debian/foundation.json relative to the folder where the executable is in.
pub fn use_alternative_path(path_str: String) -> String {
	let check_path = Path::new(&path_str);
	if !check_path.exists() {
		let mut p = env::current_exe().expect("Failed to get the executable's directory and no path to the foundation.json was provided!");
		//removing the file from the path and going back 2 directories
		for _ in 0..3 {
			p.pop();
		}
		p.push("debian");
		p.push("foundation.json");
		warn!(
			"The file `{}` was not found! Will try to use the alternative file `{}`!",
			check_path.display(),
			p.display()
		);
		p.to_str().expect("Failed to get the executable's directory and no path to the foundation.json was provided!").to_owned()
	} else {
		path_str
	}
}

/// Get the current path to the foundation.json file (file with the foundation wallet outputs/kernels)
pub fn get_foundation_path() -> Option<String> {
	let foundation_path = FOUNDATION_FILE.read();
	foundation_path.clone()
}

/// Set the policy configuration that will be used by the blockchain
pub fn set_policy_config(policy: PolicyConfig) {
	let mut policy_config = POLICY_CONFIG.write();
	*policy_config = policy;
}

pub fn set_emitted_policy(emitted: u8) {
	let mut policy_config = POLICY_CONFIG.write();
	policy_config.emitted_policy = emitted;
}

pub fn add_allowed_policy(height: u64, value: u64) {
	let mut policy_config = POLICY_CONFIG.write();
	policy_config
		.allowed_policies
		.push(AllowPolicy { height, value });
}

pub fn get_allowed_policies() -> Vec<AllowPolicy> {
	let policy_config = POLICY_CONFIG.read();
	policy_config.allowed_policies.clone()
}

pub fn get_emitted_policy() -> u8 {
	let policy_config = POLICY_CONFIG.read();
	policy_config.emitted_policy
}

pub fn get_policies(index: u8) -> Option<Policy> {
	let policy_config = POLICY_CONFIG.read();

	if (index as usize) < (*policy_config).policies.len() {
		Some(policy_config.policies[index as usize].clone())
	} else {
		None
	}
}

/// Get the policy configuration that is being used by the blockchain
pub fn get_policy_config() -> PolicyConfig {
	let policy_config = POLICY_CONFIG.read();
	policy_config.clone()
}

/// Set the mining mode
pub fn set_mining_mode(mode: ChainTypes) {
	let mut param_ref = CHAIN_TYPE.write();
	*param_ref = mode;
}

/// Return either a cuckoo context or a cuckatoo context
/// Single change point
pub fn create_pow_context<T>(
	_height: u64,
	edge_bits: u8,
	proof_size: usize,
	max_sols: u32,
) -> Result<Box<dyn PoWContext<T>>, pow::Error>
where
	T: EdgeType + 'static,
{
	let chain_type = CHAIN_TYPE.read().clone();
	match chain_type {
		// Mainnet has Cuckaroo29 for AR and Cuckatoo30+ for AF
		ChainTypes::Mainnet => new_cuckatoo_ctx(edge_bits, proof_size, max_sols),
		//ChainTypes::Mainnet => new_cuckaroo_ctx(edge_bits, proof_size),

		// Same for Floonet
		ChainTypes::Floonet => new_cuckatoo_ctx(edge_bits, proof_size, max_sols),
		//ChainTypes::Floonet => new_cuckaroo_ctx(edge_bits, proof_size),

		// Everything else is Cuckatoo only
		_ => new_cuckatoo_ctx(edge_bits, proof_size, max_sols),
	}
}

/// The minimum acceptable edge_bits
pub fn min_edge_bits() -> u8 {
	let param_ref = CHAIN_TYPE.read();
	match *param_ref {
		ChainTypes::AutomatedTesting => AUTOMATED_TESTING_MIN_EDGE_BITS,
		ChainTypes::UserTesting => USER_TESTING_MIN_EDGE_BITS,
		_ => DEFAULT_MIN_EDGE_BITS,
	}
}

/// Reference edge_bits used to compute factor on higher Cuck(at)oo graph sizes,
/// while the min_edge_bits can be changed on a soft fork, changing
/// base_edge_bits is a hard fork.
pub fn base_edge_bits() -> u8 {
	let param_ref = CHAIN_TYPE.read();
	match *param_ref {
		ChainTypes::AutomatedTesting => AUTOMATED_TESTING_MIN_EDGE_BITS,
		ChainTypes::UserTesting => USER_TESTING_MIN_EDGE_BITS,
		_ => BASE_EDGE_BITS,
	}
}

/// The proofsize
pub fn proofsize() -> usize {
	let param_ref = CHAIN_TYPE.read();
	match *param_ref {
		ChainTypes::AutomatedTesting => AUTOMATED_TESTING_PROOF_SIZE,
		ChainTypes::UserTesting => USER_TESTING_PROOF_SIZE,
		_ => PROOFSIZE,
	}
}

/// Coinbase maturity for coinbases to be spent
pub fn coinbase_maturity() -> u64 {
	let param_ref = CHAIN_TYPE.read();
	match *param_ref {
		ChainTypes::AutomatedTesting => AUTOMATED_TESTING_COINBASE_MATURITY,
		ChainTypes::UserTesting => USER_TESTING_COINBASE_MATURITY,
		ChainTypes::Floonet => FLOONET_COINBASE_MATURITY,
		_ => COINBASE_MATURITY,
	}
}

/// Initial mining difficulty
pub fn initial_block_difficulty() -> u64 {
	let param_ref = CHAIN_TYPE.read();
	match *param_ref {
		ChainTypes::AutomatedTesting => TESTING_INITIAL_DIFFICULTY,
		ChainTypes::UserTesting => TESTING_INITIAL_DIFFICULTY,
		ChainTypes::Floonet => INITIAL_DIFFICULTY,
		ChainTypes::Mainnet => INITIAL_DIFFICULTY,
	}
}
/// Initial mining secondary scale
pub fn initial_graph_weight() -> u32 {
	let param_ref = CHAIN_TYPE.read();
	match *param_ref {
		ChainTypes::AutomatedTesting => TESTING_INITIAL_GRAPH_WEIGHT,
		ChainTypes::UserTesting => TESTING_INITIAL_GRAPH_WEIGHT,
		ChainTypes::Floonet => graph_weight(0, SECOND_POW_EDGE_BITS) as u32,
		ChainTypes::Mainnet => graph_weight(0, SECOND_POW_EDGE_BITS) as u32,
	}
}

/// Maximum allowed block weight.
pub fn max_block_weight() -> usize {
	let param_ref = CHAIN_TYPE.read();
	match *param_ref {
		ChainTypes::AutomatedTesting => TESTING_MAX_BLOCK_WEIGHT,
		ChainTypes::UserTesting => TESTING_MAX_BLOCK_WEIGHT,
		ChainTypes::Floonet => MAX_BLOCK_WEIGHT,
		ChainTypes::Mainnet => MAX_BLOCK_WEIGHT,
	}
}

/// Horizon at which we can cut-through and do full local pruning
pub fn cut_through_horizon() -> u32 {
	let param_ref = CHAIN_TYPE.read();
	match *param_ref {
		ChainTypes::AutomatedTesting => TESTING_CUT_THROUGH_HORIZON,
		ChainTypes::UserTesting => TESTING_CUT_THROUGH_HORIZON,
		_ => CUT_THROUGH_HORIZON,
	}
}

/// Threshold at which we can request a txhashset (and full blocks from)
pub fn state_sync_threshold() -> u32 {
	let param_ref = CHAIN_TYPE.read();
	match *param_ref {
		ChainTypes::AutomatedTesting => TESTING_STATE_SYNC_THRESHOLD,
		ChainTypes::UserTesting => TESTING_STATE_SYNC_THRESHOLD,
		_ => STATE_SYNC_THRESHOLD,
	}
}

/// Are we in automated testing mode?
pub fn is_automated_testing_mode() -> bool {
	let param_ref = CHAIN_TYPE.read();
	ChainTypes::AutomatedTesting == *param_ref
}

/// Are we in user testing mode?
pub fn is_user_testing_mode() -> bool {
	let param_ref = CHAIN_TYPE.read();
	ChainTypes::UserTesting == *param_ref
}

/// Number of blocks to reuse a txhashset zip for.
pub fn txhashset_archive_interval() -> u64 {
	let param_ref = CHAIN_TYPE.read();
	match *param_ref {
		ChainTypes::AutomatedTesting => TESTING_TXHASHSET_ARCHIVE_INTERVAL,
		ChainTypes::UserTesting => TESTING_TXHASHSET_ARCHIVE_INTERVAL,
		_ => TXHASHSET_ARCHIVE_INTERVAL,
	}
}

/// Are we in production mode?
/// Production defined as a live public network, testnet[n] or mainnet.
pub fn is_production_mode() -> bool {
	let param_ref = CHAIN_TYPE.read();
	ChainTypes::Floonet == *param_ref || ChainTypes::Mainnet == *param_ref
}

/// Are we in floonet?
/// Note: We do not have a corresponding is_mainnet() as we want any tests to be as close
/// as possible to "mainnet" configuration as possible.
/// We want to avoid missing any mainnet only code paths.
pub fn is_floonet() -> bool {
	let param_ref = CHAIN_TYPE.read();
	ChainTypes::Floonet == *param_ref
}

/// Are we for real?
pub fn is_mainnet() -> bool {
	let param_ref = CHAIN_TYPE.read();
	ChainTypes::Mainnet == *param_ref
}

/// Helper function to get a nonce known to create a valid POW on
/// the genesis block, to prevent it taking ages. Should be fine for now
/// as the genesis block POW solution turns out to be the same for every new
/// block chain at the moment
pub fn get_genesis_nonce() -> u64 {
	let param_ref = CHAIN_TYPE.read();
	match *param_ref {
		// won't make a difference
		ChainTypes::AutomatedTesting => 0,
		// Magic nonce for current genesis block at cuckatoo15
		ChainTypes::UserTesting => 27944,
		// Placeholder, obviously not the right value
		ChainTypes::Floonet => 0,
		// Placeholder, obviously not the right value
		ChainTypes::Mainnet => 0,
	}
}

/// Short name representing the current chain type ("floo", "main", etc.)
pub fn chain_shortname() -> String {
	let param_ref = CHAIN_TYPE.read();
	param_ref.shortname()
}

/// Converts an iterator of block difficulty data to more a more manageable
/// vector and pads if needed (which will) only be needed for the first few
/// blocks after genesis

pub fn difficulty_data_to_vector<T>(cursor: T, needed_block_count: u64) -> Vec<HeaderInfo>
where
	T: IntoIterator<Item = HeaderInfo>,
{
	// Convert iterator to vector, so we can append to it if necessary
	let needed_block_count = needed_block_count as usize + 1;
	let mut last_n: Vec<HeaderInfo> = cursor.into_iter().take(needed_block_count).collect();
	for i in 1..last_n.len() {
		last_n[i].timestamp = last_n[i - 1]
			.timestamp
			.saturating_sub(last_n[i - 1].prev_timespan);
	}
	// Only needed just after blockchain launch... basically ensures there's
	// always enough data by simulating perfectly timed pre-genesis
	// blocks at the genesis difficulty as needed.
	let n = last_n.len();
	if needed_block_count > n {
		let last_ts_delta = if n > 1 {
			last_n[0].timestamp - last_n[1].timestamp
		} else {
			BLOCK_TIME_SEC
		};
		let last_diff = last_n[0].difficulty.clone();

		// fill in simulated blocks with values from the previous real block
		let mut last_ts = last_n.last().unwrap().timestamp;
		for _ in n..needed_block_count {
			last_ts = last_ts.saturating_sub(last_ts_delta);
			last_n.push(HeaderInfo::from_ts_diff(last_ts, last_diff.clone()));
		}
	}

	last_n.reverse();
	last_n
}

/// Strcut that store the major and minor release versions
#[derive(Debug, Clone)]
pub struct Version {
	/// Store the major release number of an application
	pub version_major: u32,
	/// Store the minor release number of an application
	pub version_minor: u32,
}

impl Version {
	/// Create a new Version Struct
	pub fn new(version_major: u32, version_minor: u32) -> Version {
		Version {
			version_major,
			version_minor,
		}
	}
}

pub fn get_file_sha256(path: &str) -> String {
	let mut file = File::open(path).expect(
		format!(
			"Error trying to read the foundation.json. Couldn't find/open the file {}!",
			path
		)
		.as_str(),
	);
	let mut sha256 = Sha256::new();
	std::io::copy(&mut file, &mut sha256).expect(
		format!(
			"Error trying to read the foundation.json. Couldn't find/open the file {}!",
			path
		)
		.as_str(),
	);
	let hash = sha256.result();
	format!("{:x}", hash)
}
