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

//! The proof of work needs to strike a balance between fast header
//! verification to avoid DoS attacks and difficulty for block verifiers to
//! build new blocks. In addition, mining new blocks should also be as
//! difficult on high end custom-made hardware (ASICs) as on commodity hardware
//! or smartphones. For this reason we use Cuckoo Cycle (see the cuckoo
//! module for more information).
//!
//! Note that this miner implementation is here mostly for tests and
//! reference. It's not optimized for speed.

#![deny(non_upper_case_globals)]
#![deny(non_camel_case_types)]
#![deny(non_snake_case)]
#![deny(unused_mut)]
//#![warn(missing_docs)]

use chrono;
use num;

#[macro_use]
mod common;
pub mod cuckaroo;
pub mod cuckatoo;
mod error;
#[allow(dead_code)]
pub mod lean;
pub mod md5;
pub mod progpow;
pub mod randomx;
mod siphash;
mod types;

use crate::core::{Block, BlockHeader};
use crate::genesis;
use crate::global;
use chrono::{NaiveDateTime, TimeZone, Utc};

pub use self::common::EdgeType;
pub use self::types::*;
pub use crate::pow::cuckaroo::{new_cuckaroo_ctx, CuckarooContext};
pub use crate::pow::cuckatoo::{new_cuckatoo_ctx, CuckatooContext};
pub use crate::pow::error::Error;
pub use crate::pow::md5::{new_md5_ctx, MD5Context};
pub use crate::pow::progpow::{new_progpow_ctx, ProgPowContext};
pub use crate::pow::randomx::{new_randomx_ctx, RXContext};

const MAX_SOLS: u32 = 10;

/// Validates the proof of work of a given header, and that the proof of work
/// satisfies the requirements of the header.
pub fn verify_size(bh: &BlockHeader) -> Result<(), Error> {
	let mut ctx = match bh.pow.proof {
		Proof::ProgPowProof { .. } => new_progpow_ctx(),
		Proof::RandomXProof { .. } => new_randomx_ctx(bh.pow.seed),
		Proof::MD5Proof { .. } => new_md5_ctx(bh.pow.edge_bits(), global::proofsize(), MAX_SOLS),
		Proof::CuckooProof { ref nonces, .. } => Ok(global::create_pow_context::<u64>(
			bh.height,
			bh.pow.edge_bits(),
			nonces.len(),
			MAX_SOLS,
		)?),
	}
	.unwrap();

	if let Proof::CuckooProof { .. } = bh.pow.proof {
		ctx.set_header_nonce(bh.pre_pow(), None, Some(bh.height), false)?;
	} else {
		ctx.set_header_nonce(bh.pre_pow(), Some(bh.pow.nonce), Some(bh.height), false)?;
	}

	ctx.verify(&bh.pow.proof)
}

/// Mines a genesis block using the internal miner
pub fn mine_genesis_block() -> Result<Block, Error> {
	let mut gen = genesis::genesis_dev();

	if global::is_user_testing_mode() {
		gen.header.timestamp = Utc::now();
	}

	// total_difficulty on the genesis header *is* the difficulty of that block
	let genesis_difficulty = gen.header.pow.total_difficulty.clone();

	let sz = global::min_edge_bits();
	let proof_size = global::proofsize();

	pow_size(&mut gen.header, genesis_difficulty, proof_size, sz)?;
	Ok(gen)
}

/// Runs a proof of work computation over the provided block using the provided
/// Mining Worker, until the required difficulty target is reached. May take a
/// while for a low target...
pub fn pow_size(
	bh: &mut BlockHeader,
	diff: Difficulty,
	proof_size: usize,
	sz: u8,
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
		let mut ctx = global::create_pow_context::<u32>(bh.height, sz, proof_size, MAX_SOLS)?;
		ctx.set_header_nonce(bh.pre_pow(), None, None, true)?;
		if let Ok(proofs) = ctx.pow_solve() {
			bh.pow.proof = proofs[0].clone();
			if bh
				.pow
				.to_difficulty(&bh.pre_pow(), bh.height, bh.pow.nonce)
				.to_num(PoWType::Cuckatoo)
				>= diff.to_num(PoWType::Cuckatoo)
			{
				return Ok(());
			}
		}

		// otherwise increment the nonce
		let (res, _) = bh.pow.nonce.overflowing_add(1);
		bh.pow.nonce = res;

		// and if we're back where we started, update the time (changes the hash as
		// well)
		if bh.pow.nonce == start_nonce {
			bh.timestamp = TimeZone::from_utc_datetime(
				&Utc,
				&NaiveDateTime::from_timestamp_opt(0, 0).unwrap(),
			);
		}
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::genesis;
	use crate::global;
	use crate::global::ChainTypes;

	/// We'll be generating genesis blocks differently
	#[test]
	fn genesis_pow() {
		global::set_mining_mode(ChainTypes::UserTesting);

		let mut b = genesis::genesis_dev();
		b.header.pow.nonce = 28106;
		if let Proof::CuckooProof {
			ref mut edge_bits, ..
		} = b.header.pow.proof
		{
			*edge_bits = global::min_edge_bits();
		}
		println!("proof {}", global::proofsize());
		pow_size(
			&mut b.header,
			Difficulty::min(),
			global::proofsize(),
			global::min_edge_bits(),
		)
		.unwrap();
		println!("nonce {}", b.header.pow.nonce);
		assert_ne!(b.header.pow.nonce, 310);
		assert!(
			b.header
				.pow
				.to_difficulty(&b.header.pre_pow(), b.header.height, b.header.pow.nonce)
				>= Difficulty::min()
		);
		assert!(verify_size(&b.header).is_ok());
	}
}
