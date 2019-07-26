// Copyright 2018 The Epic Foundation
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

use crate::core::block::HeaderVersion;
use crate::global;
use crate::pow::{Difficulty, DifficultyNumber, PoWType};

/// A epic is divisible to 10^9, following the SI prefixes
pub const EPIC_BASE: u64 = 1_000_000_000;
/// Milliepic, a thousand of a epic
pub const MILLI_EPIC: u64 = EPIC_BASE / 1_000;
/// Microepic, a thousand of a milliepic
pub const MICRO_EPIC: u64 = MILLI_EPIC / 1_000;
/// Nanoepic, smallest unit, takes a billion to make a epic
pub const NANO_EPIC: u64 = 1;

/// Block interval, in seconds, the network will tune its next_target for. Note
/// that we may reduce this value in the future as we get more data on mining
/// with Cuckoo Cycle, networks improve and block propagation is optimized
/// (adjusting the reward accordingly).
pub const BLOCK_TIME_SEC: u64 = 60;

/// The block subsidy amount, one epic per second on average
pub const REWARD: u64 = 200 * EPIC_BASE;

/// The amount of grins per block sent to the Epic foundation wallet
pub const FOUNDATION_REWARD: u64 = 7 * EPIC_BASE;

/// Set the height (and its multiples) where the foundation coinbase will be added to the block.
/// This variable will sparse the blocks that receive the foundation coinbase
/// Therefore, the FOUNDATION_REWARD will be multiplied by this variable to adjust the amount of grins
/// sent to the Epic foundation.
pub const FOUNDATION_HEIGHT: u64 = 1;

/*
/// Actual block reward for a given total fee amount
pub fn reward(fee: u64) -> u64 {
	REWARD.saturating_add(fee)
}
*/

/// Check if the parameter height is multiple of FOUNDATION_HEIGHT
pub fn is_foundation_height(height: u64) -> bool {
	height % FOUNDATION_HEIGHT == 0
}

/// Get the current position of the foundation coinbase in the file `foundation.json` based on the block's height
pub fn foundation_index(height: u64) -> u64 {
	height / FOUNDATION_HEIGHT
}

///sundaram Working area
// Reward inbetween particular timestamp
pub const INITIAL_REWARD: u64 = 50 * EPIC_BASE;
pub const SECONDARY_REWARD: u64 = 30 * EPIC_BASE;
/// get te reward based on the timestamp (date)
pub fn reward(fee: u64, height: u64) -> u64 {
	let reward = reward_at_height(height);
	return reward.saturating_add(fee);
}
pub fn reward_at_height(height: u64) -> u64 {
	if height <= 1440 {
		return 200 * EPIC_BASE;
	} else if height <= 2880 {
		return 180 * EPIC_BASE;
	} else if height <= 4320 {
		return 160 * EPIC_BASE;
	} else if height <= 5760 {
		return 140 * EPIC_BASE;
	} else if height <= 7200 {
		return 120 * EPIC_BASE;
	} else if height <= 8640 {
		return 100 * EPIC_BASE;
	} else if height <= 10080 {
		return 80 * EPIC_BASE;
	} else if height <= 11520 {
		return 60 * EPIC_BASE;
	} else if height <= 12960 {
		return INITIAL_REWARD;
	} else {
		return REWARD;
	}
}

pub fn reward_foundation(fees: u64, height: u64) -> u64 {
	reward(fees, height) + if height > 0 { FOUNDATION_REWARD } else { 0 }
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
		sum += (reward + (FOUNDATION_REWARD as i64));
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

/// Fork every 6 months.
pub const HARD_FORK_INTERVAL: u64 = YEAR_HEIGHT / 2;

/// Check whether the block version is valid at a given height, implements
/// 6 months interval scheduled hard forks for the first 2 years.
pub fn valid_header_version(height: u64, version: HeaderVersion) -> bool {
	// uncomment below as we go from hard fork to hard fork
	if height < HARD_FORK_INTERVAL {
		version == HeaderVersion::default()
	/* } else if height < 2 * HARD_FORK_INTERVAL {
		version == 2
	} else if height < 3 * HARD_FORK_INTERVAL {
		version == 3
	} else if height < 4 * HARD_FORK_INTERVAL {
		version == 4
	} else if height >= 5 * HARD_FORK_INTERVAL {
		version > 4 */
	} else {
		false
	}
}

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

/// Compute weight of a graph as number of siphash bits defining the graph
/// Must be made dependent on height to phase out smaller size over the years
/// This can wait until end of 2019 at latest
pub fn graph_weight(height: u64, edge_bits: u8) -> u64 {
	let mut xpr_edge_bits = edge_bits as u64;

	let bits_over_min = edge_bits.saturating_sub(global::min_edge_bits());
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

/// Minimum scaling factor for AR pow, enforced in diff retargetting
/// avoids getting stuck when trying to increase ar_scale subject to dampening
pub const MIN_AR_SCALE: u64 = AR_SCALE_DAMP_FACTOR;

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
}

impl HeaderInfo {
	/// Default constructor
	pub fn new(
		timestamp: u64,
		difficulty: Difficulty,
		secondary_scaling: u32,
		is_secondary: bool,
	) -> HeaderInfo {
		HeaderInfo {
			timestamp,
			difficulty,
			secondary_scaling,
			is_secondary,
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
pub fn next_difficulty<T>(height: u64, prev_algo: PoWType, cursor: T) -> HeaderInfo
where
	T: IntoIterator<Item = HeaderInfo>,
{
	let diff_data = global::difficulty_data_to_vector(cursor);
	// First, get the ratio of secondary PoW vs primary, skipping initial header
	let sec_pow_scaling = secondary_pow_scaling(height, &diff_data[1..]);
	let prev_difficulty = diff_data[0].difficulty.to_num(prev_algo);

	let mut diff = diff_data[0].difficulty.num.clone();

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
				next_hash_difficulty(height, prev_difficulty, &diff_data),
			);
		}
		PoWType::ProgPow => {
			diff.insert(
				PoWType::ProgPow,
				next_hash_difficulty(height, prev_difficulty, &diff_data),
			);
		}
	};

	HeaderInfo::from_diff_scaling(Difficulty::from_dic_number(diff), sec_pow_scaling)
}

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

pub fn next_hash_difficulty(height: u64, prev_diff: u64, diff_data: &Vec<HeaderInfo>) -> u64 {
	let block_diff_factor = 4;
	let min_diff = 1000;
	let diff_adjustment_cutoff = 60;

	let prev_timestamp = diff_data[0].timestamp;

	// Get the timestamp delta across the window
	let ts_delta: u64 = diff_data[1].timestamp - prev_timestamp;
	let offset: i64 = (prev_diff / block_diff_factor) as i64;
	let sign: i64 = max(1 - 2 * (ts_delta as i64 / diff_adjustment_cutoff), -99);

	//
	max(
		prev_diff as i64 + offset * sign,
		min(prev_diff as i64, min_diff),
	) as u64
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
