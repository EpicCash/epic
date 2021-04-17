// Copyright 2018 The Grin Developers
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

//! All the rules required for a cryptocurrency to have reach consensus across
//! the whole network are complex and hard to completely isolate. Some can be
//! simple parameters (like block reward), others complex algorithms (like
//! Merkle sum trees or reorg rules). However, as long as they're simple
//! enough, consensus-relevant constants and short functions should be kept
//! here.

use std::cmp::{max, min};
use std::collections::HashMap;

use crate::core::block::feijoada::{
	get_bottles_default, next_block_bottles, Deterministic, Feijoada, Policy,
};
use crate::core::block::HeaderVersion;
use crate::global;
use crate::pow::{Difficulty, DifficultyNumber, PoWType};

/// A epic is divisible to 10^8 like bitcoin
pub const EPIC_BASE: u64 = 100_000_000;
/// Milliepic, a thousand of a epic
pub const MILLI_EPIC: u64 = EPIC_BASE / 1_000;
/// Microepic, a thousand of a milliepic
pub const MICRO_EPIC: u64 = MILLI_EPIC / 1_000;
/// Freeman, smallest unit, takes a hundred million to make a epic
pub const FREEMAN: u64 = 1;

/// Block interval, in seconds, the network will tune its next_target for. Note
/// that we may reduce this value in the future as we get more data on mining
/// with Cuckoo Cycle, networks improve and block propagation is optimized
/// (adjusting the reward accordingly).
pub const BLOCK_TIME_SEC: u64 = 60;

/// Height of the first epic block emission era
pub const BLOCK_ERA_1: u64 = DAY_HEIGHT * 334;
/// Height of the second epic block emission era
pub const BLOCK_ERA_2: u64 = BLOCK_ERA_1 + (DAY_HEIGHT * 470);
/// Height of the third epic block emission era
pub const BLOCK_ERA_3: u64 = BLOCK_ERA_2 + (DAY_HEIGHT * 601);
/// Height of the fourth epic block emission era
pub const BLOCK_ERA_4: u64 = BLOCK_ERA_3 + (DAY_HEIGHT * 800);
/// Height of the fifth epic block emission era
pub const BLOCK_ERA_5: u64 = BLOCK_ERA_4 + (DAY_HEIGHT * 1019);
/// After the epic block emission era 6, each era will last 4 years (approximately 1460 days)
pub const BLOCK_ERA_6_ONWARDS: u64 = DAY_HEIGHT * 1460;
/// Block Reward that will be assigned after we change from era 5 to era 6.
pub const BASE_REWARD_ERA_6_ONWARDS: u64 = (0.15625 * EPIC_BASE as f64) as u64;

/// Mainnet: Compute the total reward generated by each block in a given height.
pub fn mainnet_block_total_reward_at_height(height: u64) -> u64 {
	if height <= BLOCK_ERA_1 {
		16 * EPIC_BASE
	} else if height <= BLOCK_ERA_2 {
		8 * EPIC_BASE
	} else if height <= BLOCK_ERA_3 {
		4 * EPIC_BASE
	} else if height <= BLOCK_ERA_4 {
		2 * EPIC_BASE
	} else if height <= BLOCK_ERA_5 {
		1 * EPIC_BASE
	} else {
		// After the era 6, we reduce the block rewards by half each 1460 days.
		// Minus 1 to include multiples in the same index
		// (i.e changes greater than to greater or equals to)
		let height_with_offset = height - BLOCK_ERA_5 - 1;
		let exp = height_with_offset / BLOCK_ERA_6_ONWARDS;
		BASE_REWARD_ERA_6_ONWARDS / (1 << exp)
	}
}

/// Floonet: Height of the first epic block emission era
pub const FLOONET_BLOCK_ERA_1: u64 = 2880;

/// Floonet: Compute the total reward generated by each block in a given height.
pub fn floonet_block_total_reward_at_height(height: u64) -> u64 {
	if height <= FLOONET_BLOCK_ERA_1 {
		16 * EPIC_BASE
	} else {
		8 * EPIC_BASE
	}
}

/// Compute the total reward generated by each block in a given height.
pub fn block_total_reward_at_height(height: u64) -> u64 {
	let param_ref = global::CHAIN_TYPE.read();
	match *param_ref {
		global::ChainTypes::Floonet => floonet_block_total_reward_at_height(height),
		_ => mainnet_block_total_reward_at_height(height),
	}
}

/// Set the height (and its multiples) where the foundation coinbase will be added to the block.
/// This variable will sparse the blocks that receive the foundation coinbase.
pub const MAINNET_FOUNDATION_HEIGHT: u64 = DAY_HEIGHT;

/// Set the height (and its multiples) where the foundation coinbase will be added to the block.
/// Used in automated tests.
pub const AUTOMATEDTEST_FOUNDATION_HEIGHT: u64 = 5;

/// Set the height (and its multiples) where the foundation coinbase will be added to the block.
/// Used in the Floonet.
pub const FLOONET_FOUNDATION_HEIGHT: u64 = DAY_HEIGHT;

/// Get the height where the foundation coinbase will be added to the block.
pub fn foundation_height() -> u64 {
	let param_ref = global::CHAIN_TYPE.read();
	match *param_ref {
		global::ChainTypes::AutomatedTesting => AUTOMATEDTEST_FOUNDATION_HEIGHT,
		global::ChainTypes::UserTesting => AUTOMATEDTEST_FOUNDATION_HEIGHT,
		global::ChainTypes::Floonet => FLOONET_FOUNDATION_HEIGHT,
		_ => MAINNET_FOUNDATION_HEIGHT,
	}
}

/// Check if the given height is a foundation height. A foundation height is a height where we add
/// the foundation levy. To be a foundation height the height has to be multiple of the foundation height
/// and we have to have a foundation levy (following the schedule) different from zero for that height.
pub fn is_foundation_height(height: u64) -> bool {
	height > 0 && height % foundation_height() == 0 && reward_foundation_at_height(height) != 0
}

/// Get the current position of the foundation coinbase in the file `foundation.json` based on the block's height
pub fn foundation_index(height: u64) -> u64 {
	// The genesis doesn't have a foundation reward.
	// The foundation.json file that stores all the foundation taxes has its index starting in 0. Therefore, we subtract 1.
	if height > 0 {
		(height / foundation_height()) - 1
	} else {
		panic!("Error to get the correct index in the foundation.json file! It was expected a height > 0, it got a height of {:?}", height);
	}
}

/// Check if the current height is a foundation height, if it's, the function returns the cumulative
/// foundation reward value for one DAY_HEIGHT. Otherwise, the function returns 0.
pub fn add_reward_foundation(height: u64) -> u64 {
	if is_foundation_height(height) {
		cumulative_reward_foundation(height)
	} else {
		0
	}
}

/// Sum all the foundation reward, to send one
pub fn cumulative_reward_foundation(height: u64) -> u64 {
	assert!(is_foundation_height(height), "To compute the cumulative foundation reward the height needs to be a foundation height multiple");
	let mut sum: u64 = 0;
	let f_height: u64 = foundation_height();
	let n: u64 = (height - f_height) + 1;
	for iter_height in n..=height {
		sum += reward_foundation_at_height(iter_height);
	}
	sum
}

/// Duration in height of the first foundation levy era
/// NOTE_L: Adjust this before the official launch
pub const FOUNDATION_LEVY_ERA_1: u64 = DAY_HEIGHT * 120;
/// After the first foundation levy era, we decrease the foundation levy each year
pub const FOUNDATION_LEVY_ERA_2_ONWARDS: u64 = DAY_HEIGHT * 365;
/// The foundation levy in each era

pub const FOUNDATION_LEVY_RATIO: u64 = 10000;
pub const FOUNDATION_LEVY: [u64; 9] = [888, 777, 666, 555, 444, 333, 222, 111, 111];

/// Compute the foundation levy for each block.
pub fn reward_foundation_at_height(height: u64) -> u64 {
	if height == 0 {
		return 0;
	} else if height <= FOUNDATION_LEVY_ERA_1 {
		let block_total_reward = block_total_reward_at_height(height);
		return (block_total_reward * FOUNDATION_LEVY[0]) / FOUNDATION_LEVY_RATIO;
	} else {
		// We subtract 1 to include the last block of an era.
		let height_with_offset = height - FOUNDATION_LEVY_ERA_1 - 1;
		// We used the index 0 in the first era, therefore we offset the index by 1
		let index: u64 = (height_with_offset / FOUNDATION_LEVY_ERA_2_ONWARDS) + 1;
		assert!(
			index < std::u32::MAX.into(),
			"Couldn't convert index u64 to usize without lost information!"
		);
		let index = index as usize;
		// After the year of 2028 the foundation levy will be zero
		if index < FOUNDATION_LEVY.len() {
			let block_total_reward = block_total_reward_at_height(height);
			return (block_total_reward * FOUNDATION_LEVY[index]) / FOUNDATION_LEVY_RATIO;
		} else {
			return 0;
		}
	}
}

/// Get the total mining reward (with fee) based on the height
pub fn reward(fee: u64, height: u64) -> u64 {
	let reward = reward_at_height(height);
	return reward.saturating_add(fee);
}

/// Get the mining reward at current height
pub fn reward_at_height(height: u64) -> u64 {
	let total_reward = block_total_reward_at_height(height);
	return total_reward - reward_foundation_at_height(height);
}

/// Get the current value of the mining reward + foundation levy for a given height
pub fn reward_foundation(fees: u64, height: u64) -> u64 {
	reward(fees, height) + add_reward_foundation(height)
}
/// The total overage at a given height. Variable due to changing rewards
/// TODOBG: Make this more efficient by hardcoding reward schedule times
pub fn total_overage_at_height(height: u64, genesis_had_reward: bool) -> i64 {
	let mut sum: i64 = 0;

	if genesis_had_reward {
		sum += reward_at_height(0) as i64;
	}

	for i in 1..=height {
		let reward = reward_at_height(i as u64) as i64;
		sum += reward + add_reward_foundation(i as u64) as i64;
	}

	return sum;
}

/// Nominal height for standard time intervals, hour is 60 blocks
pub const HOUR_HEIGHT: u64 = 3600 / BLOCK_TIME_SEC;
/// A day is 1440 blocks
pub const DAY_HEIGHT: u64 = 24 * HOUR_HEIGHT;
/// A week is 10_080 blocks
pub const WEEK_HEIGHT: u64 = 7 * DAY_HEIGHT;
/// A year is 524_160 blocks
pub const YEAR_HEIGHT: u64 = 52 * WEEK_HEIGHT;

/// Number of blocks before a coinbase matures and can be spent
pub const COINBASE_MATURITY: u64 = DAY_HEIGHT;

/// Ratio the secondary proof of work should take over the primary, as a
/// function of block height (time). Starts at 90% losing a percent
/// approximately every week. Represented as an integer between 0 and 100.
pub fn secondary_pow_ratio(height: u64) -> u64 {
	90u64.saturating_sub(height / (2 * YEAR_HEIGHT / 90))
}

/// The AR scale damping factor to use. Dependent on block height
/// to account for pre HF behavior on testnet4.
fn ar_scale_damp_factor(_height: u64) -> u64 {
	AR_SCALE_DAMP_FACTOR
}

/// Cuckoo-cycle proof size (cycle length)
pub const PROOFSIZE: usize = 42;

/// Default Cuckatoo Cycle edge_bits, used for mining and validating.
pub const DEFAULT_MIN_EDGE_BITS: u8 = 19;

/// Cuckaroo proof-of-work edge_bits, meant to be ASIC resistant.
pub const SECOND_POW_EDGE_BITS: u8 = 31;

/// Original reference edge_bits to compute difficulty factors for higher
/// Cuckoo graph sizes, changing this would hard fork
pub const BASE_EDGE_BITS: u8 = 24;

/// Default number of blocks in the past when cross-block cut-through will start
/// happening. Needs to be long enough to not overlap with a long reorg.
/// Rational
/// behind the value is the longest bitcoin fork was about 30 blocks, so 5h. We
/// add an order of magnitude to be safe and round to 7x24h of blocks to make it
/// easier to reason about.
pub const CUT_THROUGH_HORIZON: u32 = WEEK_HEIGHT as u32;

/// Default number of blocks in the past to determine the height where we request
/// a txhashset (and full blocks from). Needs to be long enough to not overlap with
/// a long reorg.
/// Rational behind the value is the longest bitcoin fork was about 30 blocks, so 5h.
/// We add an order of magnitude to be safe and round to 2x24h of blocks to make it
/// easier to reason about.
pub const STATE_SYNC_THRESHOLD: u32 = 2 * DAY_HEIGHT as u32;

/// Weight of an input when counted against the max block weight capacity
pub const BLOCK_INPUT_WEIGHT: usize = 1;

/// Weight of an output when counted against the max block weight capacity
pub const BLOCK_OUTPUT_WEIGHT: usize = 21;

/// Weight of a kernel when counted against the max block weight capacity
pub const BLOCK_KERNEL_WEIGHT: usize = 3;

/// Total maximum block weight. At current sizes, this means a maximum
/// theoretical size of:
/// * `(674 + 33 + 1) * (40_000 / 21) = 1_348_571` for a block with only outputs
/// * `(1 + 8 + 8 + 33 + 64) * (40_000 / 3) = 1_520_000` for a block with only kernels
/// * `(1 + 33) * 40_000 = 1_360_000` for a block with only inputs
///
/// Regardless of the relative numbers of inputs/outputs/kernels in a block the maximum
/// block size is around 1.5MB
/// For a block full of "average" txs (2 inputs, 2 outputs, 1 kernel) we have -
/// `(1 * 2) + (21 * 2) + (3 * 1) = 47` (weight per tx)
/// `40_000 / 47 = 851` (txs per block)
///
pub const MAX_BLOCK_WEIGHT: usize = 40_000;

/// Mainnet first hard fork height, set to happen around 2020-04-29
pub const MAINNET_FIRST_HARD_FORK: u64 = 700000;

/// Floonet first hard fork height
pub const FLOONET_FIRST_HARD_FORK: u64 = 10080;

/// AutomatedTesting and UserTesting first hard fork height.
pub const TESTING_FIRST_HARD_FORK: u64 = 6;

/// Get the height of the first epic hard fork
pub fn first_fork_height() -> u64 {
	match global::CHAIN_TYPE.read().clone() {
		global::ChainTypes::Mainnet => MAINNET_FIRST_HARD_FORK,
		global::ChainTypes::Floonet => FLOONET_FIRST_HARD_FORK,
		global::ChainTypes::AutomatedTesting | global::ChainTypes::UserTesting => {
			TESTING_FIRST_HARD_FORK
		}
	}
}

/// Compute possible block version at a given height
pub fn header_version(height: u64) -> HeaderVersion {
	if height < first_fork_height() {
		HeaderVersion(6)
	} else {
		HeaderVersion(7)
	}
}

/// Check whether the block version is valid at a given height, implements
/// 6 months interval scheduled hard forks for the first 2 years.
pub fn valid_header_version(height: u64, version: HeaderVersion) -> bool {
	version == header_version(height)
}

///defines the block height at wich the difficulty adjustment era changes for testing
pub const TESTING_DIFFICULTY_ERA: u64 = 50;
///defines the block height at wich the difficulty adjustment era changes for floonet
pub const FLOONET_DIFFICULTY_ERA: u64 = 200;
///defines the block height at wich the difficulty adjustment era changes
pub const MAINNET_DIFFICULTY_ERA: u64 = 501100;

/// Number of blocks used to calculate difficulty adjustments
pub const DIFFICULTY_ADJUST_WINDOW: u64 = HOUR_HEIGHT;

/// Average time span of the difficulty adjustment window
pub const BLOCK_TIME_WINDOW: u64 = DIFFICULTY_ADJUST_WINDOW * BLOCK_TIME_SEC;

/// Clamp factor to use for difficulty adjustment
/// Limit value to within this factor of goal
pub const CLAMP_FACTOR: u64 = 2;

/// Dampening factor to use for difficulty adjustment
pub const DIFFICULTY_DAMP_FACTOR: u64 = 3;

/// Dampening factor to use for AR scale calculation.
pub const AR_SCALE_DAMP_FACTOR: u64 = 13;

/// Get the height where the difficulty patch will be added.
pub fn difficultyfix_height() -> u64 {
	let param_ref = global::CHAIN_TYPE.read();
	match *param_ref {
		global::ChainTypes::AutomatedTesting => TESTING_DIFFICULTY_ERA,
		global::ChainTypes::UserTesting => TESTING_DIFFICULTY_ERA,
		global::ChainTypes::Floonet => FLOONET_DIFFICULTY_ERA,
		_ => MAINNET_DIFFICULTY_ERA,
	}
}

/// Compute weight of a graph as number of siphash bits defining the graph
/// Must be made dependent on height to phase out smaller size over the years
/// This can wait until end of 2019 at latest
pub fn graph_weight(height: u64, edge_bits: u8) -> u64 {
	let mut xpr_edge_bits = edge_bits as u64;
	// Patch to use cuckatoo19, should be removed when switched just cuckatoo31+
	let mut min_edge_bits = global::min_edge_bits();
	if min_edge_bits == 19 {
		min_edge_bits += 12
	}
	let bits_over_min = edge_bits.saturating_sub(min_edge_bits);
	let expiry_height = (1 << bits_over_min) * YEAR_HEIGHT;
	if height >= expiry_height {
		xpr_edge_bits = xpr_edge_bits.saturating_sub(1 + (height - expiry_height) / WEEK_HEIGHT);
	}

	(2 << (if edge_bits > global::base_edge_bits() {
		edge_bits - global::base_edge_bits()
	} else {
		global::base_edge_bits() - edge_bits
	}) as u64)
		* xpr_edge_bits
}

/// Minimum difficulty, enforced in diff retargetting
/// avoids getting stuck when trying to increase difficulty subject to dampening
pub const MIN_DIFFICULTY: u64 = DIFFICULTY_DAMP_FACTOR;

/// RandomX Minimum difficulty (used for saturation)
pub const MIN_DIFFICULTY_RANDOMX: u64 = 4000;
/// RandomX Minimum difficulty until fork (used for saturation)
pub const OLD_MIN_DIFFICULTY_RANDOMX: u64 = 5000;

/// Progpow Minimum difficulty (used for saturation)
pub const MIN_DIFFICULTY_PROGPOW: u64 = 200000;
/// Progpow Minimum difficulty until fork (used for saturation)
pub const OLD_MIN_DIFFICULTY_PROGPOW: u64 = 100000;

/// RandomX Minimum difficulty (used for saturation)
pub const BLOCK_DIFF_FACTOR_RANDOMX: u64 = 64;

/// Progpow Minimum difficulty (used for saturation)
pub const BLOCK_DIFF_FACTOR_PROGPOW: u64 = 64;

/// Minimum scaling factor for AR pow, enforced in diff retargetting
/// avoids getting stuck when trying to increase ar_scale subject to dampening
pub const MIN_AR_SCALE: u64 = AR_SCALE_DAMP_FACTOR;

/// Clamp factor to use for difficulty adjustment
/// Limit value to within this factor of goal
pub const RX_CLAMP_FACTOR: u64 = 2;

/// Dampening factor to use for difficulty adjustment
pub const RX_DIFFICULTY_DAMP_FACTOR: u64 = 3;

/// Clamp factor to use for difficulty adjustment
/// Limit value to within this factor of goal
pub const PP_CLAMP_FACTOR: u64 = 2;

/// Dampening factor to use for difficulty adjustment
pub const PP_DIFFICULTY_DAMP_FACTOR: u64 = 3;

/// unit difficulty, equal to graph_weight(SECOND_POW_EDGE_BITS)
pub const UNIT_DIFFICULTY: u64 =
	((2 as u64) << (SECOND_POW_EDGE_BITS - BASE_EDGE_BITS)) * (SECOND_POW_EDGE_BITS as u64);

/// The initial difficulty at launch. This should be over-estimated
/// and difficulty should come down at launch rather than up
/// Currently grossly over-estimated at 10% of current
/// ethereum GPUs (assuming 1GPU can solve a block at diff 1 in one block interval)
pub const INITIAL_DIFFICULTY: u64 = 1_000_000 * UNIT_DIFFICULTY;

/// Minimal header information required for the Difficulty calculation to
/// take place
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeaderInfo {
	/// Timestamp of the header, 1 when not used (returned info)
	pub timestamp: u64,
	/// Network difficulty or next difficulty to use
	pub difficulty: Difficulty,
	/// Network secondary PoW factor or factor to use
	pub secondary_scaling: u32,
	/// Whether the header is a secondary proof of work
	pub is_secondary: bool,
	/// timespan of the previous block of the same algorithm type
	pub prev_timespan: u64,
}

impl HeaderInfo {
	/// Default constructor
	pub fn new(
		timestamp: u64,
		difficulty: Difficulty,
		secondary_scaling: u32,
		is_secondary: bool,
		prev_timespan: u64,
	) -> HeaderInfo {
		HeaderInfo {
			timestamp,
			difficulty,
			secondary_scaling,
			is_secondary,
			prev_timespan,
		}
	}

	/// Constructor from a timestamp and difficulty, setting a default secondary
	/// PoW factor
	pub fn from_ts_diff(timestamp: u64, difficulty: Difficulty) -> HeaderInfo {
		HeaderInfo {
			timestamp,
			difficulty,
			secondary_scaling: global::initial_graph_weight(),
			is_secondary: true,
			prev_timespan: 0,
		}
	}

	/// Constructor from a difficulty and secondary factor, setting a default
	/// timestamp
	pub fn from_diff_scaling(difficulty: Difficulty, secondary_scaling: u32) -> HeaderInfo {
		HeaderInfo {
			timestamp: 1,
			difficulty,
			secondary_scaling,
			is_secondary: true,
			prev_timespan: 0,
		}
	}
}

/// Move value linearly toward a goal
pub fn damp(actual: u64, goal: u64, damp_factor: u64) -> u64 {
	(actual + (damp_factor - 1) * goal) / damp_factor
}

/// limit value to be within some factor from a goal
pub fn clamp(actual: u64, goal: u64, clamp_factor: u64) -> u64 {
	max(goal / clamp_factor, min(actual, goal * clamp_factor))
}

pub fn next_policy<T>(policy: u8, cursor: T) -> (PoWType, Policy)
where
	T: IntoIterator<Item = Policy>,
{
	let prev_bottles: Vec<Policy> = cursor.into_iter().take(1).collect();

	let bottles = if let Some(p_bottles) = prev_bottles.first() {
		p_bottles.clone()
	} else {
		get_bottles_default()
	};

	let pow_type = Deterministic::choose_algo(&global::get_policies(policy).unwrap(), &bottles);
	let b = next_block_bottles(pow_type, &bottles);

	(pow_type, b)
}

macro_rules! error_invalid_pow {
	($pow:expr) => {
		panic!("The function next_hash_difficulty is only used by Progpow and RandomX, but it got a {:?}", $pow);
	}
}

/// Computes the proof-of-work difficulty that the next block should comply
/// with. Takes an iterator over past block headers information, from latest
/// (highest height) to oldest (lowest height).
///
/// The difficulty calculation is based on both Digishield and GravityWave
/// family of difficulty computation, coming to something very close to Zcash.
/// The reference difficulty is an average of the difficulty over a window of
/// DIFFICULTY_ADJUST_WINDOW blocks. The corresponding timespan is calculated
/// by using the difference between the median timestamps at the beginning
/// and the end of the window.
///
/// The secondary proof-of-work factor is calculated along the same lines, as
/// an adjustment on the deviation against the ideal value.
/// changes the header info with new difficulty for the block to mine
pub fn next_difficulty<T>(height: u64, prev_algo: PoWType, cursor: T) -> HeaderInfo
where
	T: IntoIterator<Item = HeaderInfo>,
{
	let diff_data = match prev_algo.clone() {
		PoWType::Cuckatoo => global::difficulty_data_to_vector(cursor, DIFFICULTY_ADJUST_WINDOW),
		PoWType::Cuckaroo => global::difficulty_data_to_vector(cursor, DIFFICULTY_ADJUST_WINDOW),
		PoWType::RandomX => global::difficulty_data_to_vector(cursor, 1),
		PoWType::ProgPow => global::difficulty_data_to_vector(cursor, 1),
	};

	// First, get the ratio of secondary PoW vs primary, skipping initial header
	let sec_pow_scaling = secondary_pow_scaling(height, &diff_data[1..]);
	let mut diff = diff_data.last().unwrap().difficulty.num.clone();

	match prev_algo {
		PoWType::Cuckatoo => {
			diff.insert(
				PoWType::Cuckatoo,
				next_cuckoo_difficulty(height, PoWType::Cuckatoo, &diff_data),
			);
		}
		PoWType::Cuckaroo => {
			diff.insert(
				PoWType::Cuckaroo,
				next_cuckoo_difficulty(height, PoWType::Cuckaroo, &diff_data),
			);
		}
		PoWType::RandomX => {
			diff.insert(
				PoWType::RandomX,
				next_hash_difficulty(PoWType::RandomX, &diff_data),
			);
		}
		PoWType::ProgPow => {
			diff.insert(
				PoWType::ProgPow,
				next_hash_difficulty(PoWType::ProgPow, &diff_data),
			);
		}
	};

	HeaderInfo::from_diff_scaling(Difficulty::from_dic_number(diff), sec_pow_scaling)
}

/// calculates the next difficulty level for cuckoo
fn next_cuckoo_difficulty(height: u64, pow: PoWType, diff_data: &Vec<HeaderInfo>) -> u64 {
	// Get the timestamp delta across the window

	let ts_delta: u64 =
		diff_data[DIFFICULTY_ADJUST_WINDOW as usize].timestamp - diff_data[0].timestamp;

	// Get the difficulty sum of the last DIFFICULTY_ADJUST_WINDOW elements
	let diff_sum: u64 = diff_data
		.iter()
		.skip(1)
		.map(|dd| dd.difficulty.to_num(pow))
		.sum();

	// adjust time delta toward goal subject to dampening and clamping
	let adj_ts = clamp(
		damp(ts_delta, BLOCK_TIME_WINDOW, DIFFICULTY_DAMP_FACTOR),
		BLOCK_TIME_WINDOW,
		CLAMP_FACTOR,
	);

	// minimum difficulty avoids getting stuck due to dampening
	max(MIN_DIFFICULTY, diff_sum * BLOCK_TIME_SEC / adj_ts)
}

/// returns the median timestamp from last 6 mined blocks
pub fn timestamp_median<T>(header_ts: u64, prev_algo: PoWType, cursor: T) -> u64
where
	T: IntoIterator<Item = HeaderInfo>,
{
	let diff_data = match prev_algo.clone() {
		PoWType::Cuckatoo => global::ts_data_to_vector(cursor, 6),
		PoWType::Cuckaroo => global::ts_data_to_vector(cursor, 6),
		PoWType::RandomX => global::ts_data_to_vector(cursor, 6),
		PoWType::ProgPow => global::ts_data_to_vector(cursor, 6),
	};

	let mut ts: Vec<u64> = vec![];

	for i in 0..diff_data.len() {
		ts.push(diff_data[i].timestamp);
	}
	ts.push(header_ts);
	ts.sort();

	let half = ts.len() / 2;
	let median_ts: u64 = if (ts.len() % 2) == 0 {
		ts[half]
	} else {
		(ts[half - 1] + ts[half]) / 2
	};

	median_ts
}

/// changes the header info with new difficulty era1 for the block to mine
pub fn next_difficulty_era1<T>(height: u64, prev_algo: PoWType, cursor: T) -> HeaderInfo
where
	T: IntoIterator<Item = HeaderInfo>,
{
	let diff_data = match prev_algo.clone() {
		PoWType::Cuckatoo => global::difficulty_data_to_vector(cursor, DIFFICULTY_ADJUST_WINDOW),
		PoWType::Cuckaroo => global::difficulty_data_to_vector(cursor, DIFFICULTY_ADJUST_WINDOW),
		PoWType::RandomX => global::difficulty_data_to_vector(cursor, DIFFICULTY_ADJUST_WINDOW),
		PoWType::ProgPow => global::difficulty_data_to_vector(cursor, DIFFICULTY_ADJUST_WINDOW),
	};

	// First, get the ratio of secondary PoW vs primary, skipping initial header
	let sec_pow_scaling = secondary_pow_scaling(height, &diff_data[1..]);
	let mut diff = diff_data.last().unwrap().difficulty.num.clone();

	match prev_algo {
		PoWType::Cuckatoo => {
			diff.insert(
				PoWType::Cuckatoo,
				next_cuckoo_difficulty_era1(PoWType::Cuckatoo, &diff_data),
			);
		}
		PoWType::Cuckaroo => {
			diff.insert(
				PoWType::Cuckaroo,
				next_cuckoo_difficulty_era1(PoWType::Cuckaroo, &diff_data),
			);
		}
		PoWType::RandomX => {
			diff.insert(
				PoWType::RandomX,
				next_randomx_difficulty_era1(PoWType::RandomX, &diff_data),
			);
		}
		PoWType::ProgPow => {
			diff.insert(
				PoWType::ProgPow,
				next_progpow_difficulty_era1(PoWType::ProgPow, &diff_data),
			);
		}
	};

	HeaderInfo::from_diff_scaling(Difficulty::from_dic_number(diff), sec_pow_scaling)
}

/// calculates the next difficulty level for progpow
fn next_progpow_difficulty_era1(pow: PoWType, diff_data: &Vec<HeaderInfo>) -> u64 {
	// Get the timestamp delta across the window
	let mut ts_delta: u64 = 0;
	for i in 1..diff_data.len() {
		ts_delta += diff_data[i - 1].timestamp
			- diff_data[i - 1]
				.timestamp
				.saturating_sub(diff_data[i - 1].prev_timespan);
	}

	// Get the difficulty sum of the last DIFFICULTY_ADJUST_WINDOW elements
	let diff_sum: u64 = diff_data
		.iter()
		.skip(1)
		.map(|dd| dd.difficulty.to_num(pow))
		.sum();

	// adjust time delta toward goal subject to dampening and clamping
	let adj_ts = clamp(
		damp(ts_delta, BLOCK_TIME_WINDOW, PP_DIFFICULTY_DAMP_FACTOR),
		BLOCK_TIME_WINDOW,
		PP_CLAMP_FACTOR,
	);

	// minimum difficulty avoids getting stuck due to dampening
	max(MIN_DIFFICULTY_PROGPOW, diff_sum * BLOCK_TIME_SEC / adj_ts)
}

/// calculates the next difficulty level for randomx
fn next_randomx_difficulty_era1(pow: PoWType, diff_data: &Vec<HeaderInfo>) -> u64 {
	// Get the timestamp delta across the window
	let mut ts_delta: u64 = 0;
	for i in 1..diff_data.len() {
		ts_delta += diff_data[i - 1].timestamp
			- diff_data[i - 1]
				.timestamp
				.saturating_sub(diff_data[i - 1].prev_timespan);
	}

	// Get the difficulty sum of the last DIFFICULTY_ADJUST_WINDOW elements
	let diff_sum: u64 = diff_data
		.iter()
		.skip(1)
		.map(|dd| dd.difficulty.to_num(pow))
		.sum();

	// adjust time delta toward goal subject to dampening and clamping
	let adj_ts = clamp(
		damp(ts_delta, BLOCK_TIME_WINDOW, RX_DIFFICULTY_DAMP_FACTOR),
		BLOCK_TIME_WINDOW,
		RX_CLAMP_FACTOR,
	);

	// minimum difficulty avoids getting stuck due to dampening
	max(MIN_DIFFICULTY_RANDOMX, diff_sum * BLOCK_TIME_SEC / adj_ts)
}

/// calculates the next difficulty era1 level for cuckoo
fn next_cuckoo_difficulty_era1(pow: PoWType, diff_data: &Vec<HeaderInfo>) -> u64 {
	// Get the timestamp delta across the window
	let mut ts_delta: u64 = 0;
	for i in 1..diff_data.len() {
		ts_delta += diff_data[i - 1].timestamp
			- diff_data[i - 1]
				.timestamp
				.saturating_sub(diff_data[i - 1].prev_timespan);
	}

	// Get the difficulty sum of the last DIFFICULTY_ADJUST_WINDOW elements
	let diff_sum: u64 = diff_data
		.iter()
		.skip(1)
		.map(|dd| dd.difficulty.to_num(pow))
		.sum();

	let adj_ts = clamp(
		damp(ts_delta, BLOCK_TIME_WINDOW, DIFFICULTY_DAMP_FACTOR),
		BLOCK_TIME_WINDOW,
		CLAMP_FACTOR,
	);

	// minimum difficulty avoids getting stuck due to dampening
	max(MIN_DIFFICULTY, diff_sum * BLOCK_TIME_SEC / adj_ts)
}

/// calculates the next difficulty level for progpow and randomx
pub fn next_hash_difficulty(pow: PoWType, diff_data: &Vec<HeaderInfo>) -> u64 {
	// Desired time per block
	let diff_adjustment_cutoff = 60;

	// Constant used to divide the previous difficulty.
	let block_diff_factor = match pow {
		PoWType::RandomX => BLOCK_DIFF_FACTOR_RANDOMX,
		PoWType::ProgPow => BLOCK_DIFF_FACTOR_PROGPOW,
		_ => panic!("The function next_hash_difficulty is only used by Progpow and RandomX, but it got a {:?}", pow),
	};

	let current_diff = diff_data[1].difficulty.to_num(pow);
	let current_timestamp = diff_data[1].timestamp;
	let prev_timestamp = diff_data[0].timestamp;

	let min_diff = match pow {
		PoWType::RandomX => OLD_MIN_DIFFICULTY_RANDOMX,
		PoWType::ProgPow => OLD_MIN_DIFFICULTY_PROGPOW,
		_ => panic!("The function next_hash_difficulty is only used by Progpow and RandomX, but it got a {:?}", pow),
	};

	// Get the timestamp delta across the window
	let ts_delta: u64 = current_timestamp - prev_timestamp;
	let offset: i64 = (current_diff / block_diff_factor) as i64;
	let sign: i64 = max(1 - 2 * (ts_delta as i64 / diff_adjustment_cutoff), -99);

	// Minimum difficulty saturation
	max(
		// Making sure that we not get a negative difficulty
		max(current_diff as i64 + offset * sign, 1) as u64,
		min(current_diff, min_diff),
	)
}

/// Count, in units of 1/100 (a percent), the number of "secondary" (AR) blocks in the provided window of blocks.
pub fn ar_count(_height: u64, diff_data: &[HeaderInfo]) -> u64 {
	100 * diff_data.iter().filter(|n| n.is_secondary).count() as u64
}

/// Factor by which the secondary proof of work difficulty will be adjusted
pub fn secondary_pow_scaling(height: u64, diff_data: &[HeaderInfo]) -> u32 {
	// Get the scaling factor sum of the last DIFFICULTY_ADJUST_WINDOW elements
	let scale_sum: u64 = diff_data.iter().map(|dd| dd.secondary_scaling as u64).sum();

	// compute ideal 2nd_pow_fraction in pct and across window
	let target_pct = secondary_pow_ratio(height);
	let target_count = DIFFICULTY_ADJUST_WINDOW * target_pct;

	// Get the secondary count across the window, adjusting count toward goal
	// subject to dampening and clamping.
	let adj_count = clamp(
		damp(
			ar_count(height, diff_data),
			target_count,
			ar_scale_damp_factor(height),
		),
		target_count,
		CLAMP_FACTOR,
	);
	let scale = scale_sum * target_pct / max(1, adj_count);

	// minimum AR scale avoids getting stuck due to dampening
	max(MIN_AR_SCALE, scale) as u32
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn test_graph_weight() {
		// initial weights
		assert_eq!(graph_weight(1, 31), 256 * 31);
		assert_eq!(graph_weight(1, 32), 512 * 32);
		assert_eq!(graph_weight(1, 33), 1024 * 33);

		// one year in, 31 starts going down, the rest stays the same
		assert_eq!(graph_weight(YEAR_HEIGHT, 31), 256 * 30);
		assert_eq!(graph_weight(YEAR_HEIGHT, 32), 512 * 32);
		assert_eq!(graph_weight(YEAR_HEIGHT, 33), 1024 * 33);

		// 31 loses one factor per week
		assert_eq!(graph_weight(YEAR_HEIGHT + WEEK_HEIGHT, 31), 256 * 29);
		assert_eq!(graph_weight(YEAR_HEIGHT + 2 * WEEK_HEIGHT, 31), 256 * 28);
		assert_eq!(graph_weight(YEAR_HEIGHT + 32 * WEEK_HEIGHT, 31), 0);

		// 2 years in, 31 still at 0, 32 starts decreasing
		assert_eq!(graph_weight(2 * YEAR_HEIGHT, 31), 0);
		assert_eq!(graph_weight(2 * YEAR_HEIGHT, 32), 512 * 31);
		assert_eq!(graph_weight(2 * YEAR_HEIGHT, 33), 1024 * 33);

		// 32 loses one factor per week
		assert_eq!(graph_weight(2 * YEAR_HEIGHT + WEEK_HEIGHT, 32), 512 * 30);
		assert_eq!(graph_weight(2 * YEAR_HEIGHT + WEEK_HEIGHT, 31), 0);
		assert_eq!(graph_weight(2 * YEAR_HEIGHT + 30 * WEEK_HEIGHT, 32), 512);
		assert_eq!(graph_weight(2 * YEAR_HEIGHT + 31 * WEEK_HEIGHT, 32), 0);

		// 3 years in, nothing changes
		assert_eq!(graph_weight(3 * YEAR_HEIGHT, 31), 0);
		assert_eq!(graph_weight(3 * YEAR_HEIGHT, 32), 0);
		assert_eq!(graph_weight(3 * YEAR_HEIGHT, 33), 1024 * 33);

		// 4 years in, 33 starts starts decreasing
		assert_eq!(graph_weight(4 * YEAR_HEIGHT, 31), 0);
		assert_eq!(graph_weight(4 * YEAR_HEIGHT, 32), 0);
		assert_eq!(graph_weight(4 * YEAR_HEIGHT, 33), 1024 * 32);
	}
}
