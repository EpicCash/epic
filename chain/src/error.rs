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

//! Error types for chain
use crate::core::core::{block, committed, transaction};
use crate::core::ser;
use crate::keychain;
use crate::util::secp;
use crate::util::secp::pedersen::Commitment;

use std::io;
use thiserror::Error;

/// Chain error definitions
#[derive(Debug, Error)]
pub enum Error {
	#[error("IO error: {0}")]
	Io(#[from] io::Error),

	#[error("Store error: {0}")]
	StoreErr(#[from] epic_store::Error),

	#[error("Chain store error: {0}, {1}")]
	ChainStoreErr(epic_store::Error, String),

	#[error("Pipe store error: {0}, {1}")]
	PipeStoreErr(epic_store::Error, String),

	/// The block doesn't fit anywhere in our chain
	#[error("Block is unfit: {0}")]
	Unfit(String),
	/// Special case of orphan blocks
	#[error("Orphan")]
	Orphan,
	/// Difficulty is too low either compared to ours or the block PoW hash
	#[error("Difficulty is too low compared to ours or the block PoW hash")]
	DifficultyTooLow,

	#[error("Difficulty is too high compared to ours or the block PoW hash")]
	DifficultyTooHigh,

	/// Addition of difficulties on all previous block is wrong
	#[error("Addition of difficulties on all previous blocks is wrong")]
	WrongTotalDifficulty,
	/// Block header edge_bits is lower than our min
	#[error("Cuckoo Size too small")]
	LowEdgebits,
	/// Scaling factor between primary and secondary PoW is invalid
	#[error("Wrong scaling factor")]
	InvalidScaling,
	/// The proof of work is invalid
	#[error("Invalid PoW")]
	InvalidPow,
	/// Peer abusively sending us an old block we already have
	#[error("Old Block")]
	OldBlock,
	/// The block doesn't sum correctly or a tx signature is invalid
	#[error("Invalid Block Proof {0}")]
	InvalidBlockProof(#[from] block::Error),
	/// Block time is too old
	#[error("Invalid Block Time")]
	InvalidBlockTime,
	/// Block height is invalid (not previous + 1)
	#[error("Invalid Block Height")]
	InvalidBlockHeight,
	/// One of the root hashes in the block is invalid
	#[error("Invalid Root")]
	InvalidRoot,
	/// One of the MMR sizes in the block header is invalid
	#[error("Invalid MMR Size")]
	InvalidMMRSize,
	/// Error from underlying keychain impl
	#[error("Keychain Error {0}")]
	Keychain(#[from] keychain::Error),
	/// Error from underlying secp lib
	#[error("Secp Lib Error {0}")]
	Secp(#[from] secp::Error),
	/// One of the inputs in the block has already been spent
	#[error("Already Spent: {:?}", _0)]
	AlreadySpent(Commitment),
	/// An output with that commitment already exists (should be unique)
	#[error("Duplicate Commitment: {:?}", _0)]
	DuplicateCommitment(Commitment),
	/// Attempt to spend a coinbase output before it sufficiently matures.
	#[error("Attempt to spend immature coinbase")]
	ImmatureCoinbase,
	/// Error validating a Merkle proof (coinbase output)
	#[error("Error validating merkle proof")]
	MerkleProof,
	/// Output not found
	#[error("Output not found")]
	OutputNotFound,
	/// Rangeproof not found
	#[error("Rangeproof not found")]
	RangeproofNotFound,
	/// Tx kernel not found
	#[error("Tx kernel not found")]
	TxKernelNotFound,
	/// output spent
	#[error("Output is spent")]
	OutputSpent,
	/// Invalid block version, either a mistake or outdated software
	#[error("Invalid Block Version: {:?}", _0)]
	InvalidBlockVersion(block::HeaderVersion),
	/// We've been provided a bad txhashset
	#[error("Invalid TxHashSet: {0}")]
	InvalidTxHashSet(String),

	/// Internal issue when trying to save or load data from append only files
	#[error("File Read Error: {0}")]
	FileReadErr(String),
	/// Error serializing or deserializing a type
	#[error("Serialization Error")]
	SerErr(ser::Error),
	/// Error with the txhashset
	#[error("TxHashSetErr: {0}")]
	TxHashSetErr(String),
	/// Tx not valid based on lock_height.
	#[error("Transaction Lock Height")]
	TxLockHeight,
	/// No chain exists and genesis block is required
	#[error("Genesis Block Required")]
	GenesisBlockRequired,
	/// Error from underlying tx handling
	#[error("Transaction Validation Error: {0}")]
	Transaction(#[from] transaction::Error),
	/// Anything else
	#[error("Other Error: {0}")]
	Other(String),
	/// Error from summing and verifying kernel sums via committed trait.
	#[error("Committed Trait: Error summing and verifying kernel sums {0}")]
	Committed(#[from] committed::Error),
	/// We cannot process data once the Epic server has been stopped.
	#[error("Stopped (Epic Shutting Down)")]
	Stopped,
	/// Internal Roaring Bitmap error
	#[error("Roaring Bitmap error")]
	Bitmap,
	/// Error during chain sync
	#[error("Sync error {0}")]
	SyncError(String),
	/// comment here
	#[error("Wrong sort algorithm")]
	InvalidSortAlgo,
	/// comment here
	#[error("There's not policy")]
	ThereIsNotPolicy,

	#[error("Policy is not allowed")]
	PolicyIsNotAllowed,

	#[error("Invalid seed")]
	InvalidSeed,

	#[error("Checkpoint Integrity Failure: Mismatched hashes")]
	CheckpointFailure,
}
impl Error {
	pub fn is_bad_data(&self) -> bool {
		match self {
			Error::Unfit(_)
			| Error::Orphan
			| Error::StoreErr(_)
			| Error::SerErr(_)
			| Error::TxHashSetErr(_)
			| Error::GenesisBlockRequired
			| Error::Other(_) => false,
			_ => true,
		}
	}
}
