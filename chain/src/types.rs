// Copyright 2019-2023, Epic Cash Developers
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

//! Base types that the block chain pipeline requires.

use chrono::prelude::{DateTime, Utc};
use std::sync::Arc;

use crate::core::core::hash::{Hash, Hashed, ZERO_HASH};
use crate::core::core::{Block, BlockHeader, HeaderVersion};
use crate::core::pow::Difficulty;
use crate::core::ser::{self, PMMRIndexHashable, Readable, Reader, Writeable, Writer};
use crate::error::{Error, ErrorKind};
use crate::util::RwLock;

bitflags! {
/// Options for block validation
	pub struct Options: u32 {
		/// No flags
		const NONE = 0b0000_0000;
		/// Runs without checking the Proof of Work, mostly to make testing easier.
		const SKIP_POW = 0b0000_0001;
		/// Adds block while in syncing mode.
		const SYNC = 0b0000_0010;
		/// Block validation on a block we mined ourselves
		const MINE = 0b0000_0100;
	}
}

/// Various status sync can be in, whether it's fast sync or archival.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize, Serialize)]
#[allow(missing_docs)]
pub enum SyncStatus {
	/// Initial State (we do not yet know if we are/should be syncing)
	Initial,
	/// Not syncing
	NoSync,
	/// Not enough peers to do anything yet, boolean indicates whether
	/// we should wait at all or ignore and start ASAP
	AwaitingPeers(bool),
	/// Downloading block headers
	HeaderSync {
		current_height: u64,
		highest_height: u64,
	},
	/// Downloading the various txhashsets
	TxHashsetDownload {
		start_time: DateTime<Utc>,
		prev_update_time: DateTime<Utc>,
		update_time: DateTime<Utc>,
		prev_downloaded_size: u64,
		downloaded_size: u64,
		total_size: u64,
	},
	/// Setting up before validation
	TxHashsetSetup,
	/// Validating the kernels
	TxHashsetKernelsValidation {
		kernels: u64,
		kernels_total: u64,
	},
	/// Validating the range proofs
	TxHashsetRangeProofsValidation {
		rproofs: u64,
		rproofs_total: u64,
	},
	/// Finalizing the new state
	TxHashsetSave,
	/// State sync finalized
	TxHashsetDone,
	/// Downloading blocks
	BodySync {
		current_height: u64,
		highest_height: u64,
	},
	Shutdown,
}

/// Current sync state. Encapsulates the current SyncStatus.
pub struct SyncState {
	current: RwLock<SyncStatus>,
	sync_error: Arc<RwLock<Option<Error>>>,
}

impl SyncState {
	/// Return a new SyncState initialize to NoSync
	pub fn new() -> SyncState {
		SyncState {
			current: RwLock::new(SyncStatus::Initial),
			sync_error: Arc::new(RwLock::new(None)),
		}
	}

	/// Whether the current state matches any active syncing operation.
	/// Note: This includes our "initial" state.
	pub fn is_syncing(&self) -> bool {
		*self.current.read() != SyncStatus::NoSync
	}

	/// Current syncing status
	pub fn status(&self) -> SyncStatus {
		*self.current.read()
	}

	/// Update the syncing status
	pub fn update(&self, new_status: SyncStatus) {
		if self.status() == new_status {
			return;
		}

		let mut status = self.current.write();

		debug!("sync_state: sync_status: {:?} -> {:?}", *status, new_status,);

		*status = new_status;
	}

	/// Update txhashset downloading progress
	pub fn update_txhashset_download(&self, new_status: SyncStatus) -> bool {
		if let SyncStatus::TxHashsetDownload { .. } = new_status {
			let mut status = self.current.write();
			*status = new_status;
			true
		} else {
			false
		}
	}

	/// Communicate sync error
	pub fn set_sync_error(&self, error: Error) {
		*self.sync_error.write() = Some(error);
	}

	/// Get sync error
	pub fn sync_error(&self) -> Arc<RwLock<Option<Error>>> {
		Arc::clone(&self.sync_error)
	}

	/// Clear sync error
	pub fn clear_sync_error(&self) {
		*self.sync_error.write() = None;
	}
}

impl TxHashsetWriteStatus for SyncState {
	fn on_setup(&self) {
		self.update(SyncStatus::TxHashsetSetup);
	}

	fn on_validation_kernels(&self, kernels: u64, kernels_total: u64) {
		self.update(SyncStatus::TxHashsetKernelsValidation {
			kernels,
			kernels_total,
		});
	}

	fn on_validation_rproofs(&self, rproofs: u64, rproofs_total: u64) {
		self.update(SyncStatus::TxHashsetRangeProofsValidation {
			rproofs,
			rproofs_total,
		});
	}

	fn on_save(&self) {
		self.update(SyncStatus::TxHashsetSave);
	}

	fn on_done(&self) {
		self.update(SyncStatus::TxHashsetDone);
	}
}

/// A helper for the various txhashset MMR roots.
#[derive(Debug)]
pub struct TxHashSetRoots {
	/// Output roots
	pub output_roots: OutputRoots,
	/// Range Proof root
	pub rproof_root: Hash,
	/// Kernel root
	pub kernel_root: Hash,
}

impl TxHashSetRoots {
	/// Accessor for the output PMMR root (rules here are block height dependent).
	/// We assume the header version is consistent with the block height, validated
	/// as part of pipe::validate_header().
	pub fn output_root(&self, header: &BlockHeader) -> Hash {
		self.output_roots.root(header)
	}

	/// Validate roots against the provided block header.
	pub fn validate(&self, header: &BlockHeader) -> Result<(), Error> {
		debug!(
			"validate roots: {} at {}, {} vs. {} (original: {}, merged: {})",
			header.hash(),
			header.height,
			header.output_root,
			self.output_root(header),
			self.output_roots.pmmr_root,
			self.output_roots.merged_root(header),
		);

		if header.output_root != self.output_root(header)
			|| header.range_proof_root != self.rproof_root
			|| header.kernel_root != self.kernel_root
		{
			Err(ErrorKind::InvalidRoot.into())
		} else {
			Ok(())
		}
	}
}

/// A helper for the various output roots.
#[derive(Debug)]
pub struct OutputRoots {
	/// The output PMMR root
	pub pmmr_root: Hash,
	/// The bitmap accumulator root
	pub bitmap_root: Hash,
}

impl OutputRoots {
	/// The root of our output PMMR. The rules here are block height specific.
	/// We use the merged root here for header version 3 and later.
	/// We assume the header version is consistent with the block height, validated
	/// as part of pipe::validate_header().
	pub fn root(&self, header: &BlockHeader) -> Hash {
		if header.version < HeaderVersion(7) {
			self.output_root()
		} else {
			self.merged_root(header)
		}
	}

	/// The root of the underlying output PMMR.
	fn output_root(&self) -> Hash {
		self.pmmr_root
	}

	/// Hash the root of the output PMMR and the root of the bitmap accumulator
	/// together with the size of the output PMMR (for consistency with existing PMMR impl).
	/// H(pmmr_size | pmmr_root | bitmap_root)
	fn merged_root(&self, header: &BlockHeader) -> Hash {
		(self.pmmr_root, self.bitmap_root).hash_with_index(header.output_mmr_size)
	}
}

/// Minimal struct representing a known MMR position and associated block height.
#[derive(Debug)]
pub struct CommitPos {
	/// MMR position
	pub pos: u64,
	/// Block height
	pub height: u64,
}

impl Readable for CommitPos {
	fn read(reader: &mut dyn Reader) -> Result<CommitPos, ser::Error> {
		let pos = reader.read_u64()?;
		let height = reader.read_u64()?;
		Ok(CommitPos { pos, height })
	}
}

impl Writeable for CommitPos {
	fn write<W: Writer>(&self, writer: &mut W) -> Result<(), ser::Error> {
		writer.write_u64(self.pos)?;
		writer.write_u64(self.height)?;
		Ok(())
	}
}

/// The tip of a fork. A handle to the fork ancestry from its leaf in the
/// blockchain tree. References the max height and the latest and previous
/// blocks
/// for convenience and the total difficulty.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Tip {
	/// Height of the tip (max height of the fork)
	pub height: u64,
	/// Last block pushed to the fork
	pub last_block_h: Hash,
	/// Previous block
	pub prev_block_h: Hash,
	/// Total difficulty accumulated on that fork
	pub total_difficulty: Difficulty,
}

impl Tip {
	/// Creates a new tip based on provided header.
	pub fn from_header(header: &BlockHeader) -> Tip {
		Tip {
			height: header.height,
			last_block_h: header.hash(),
			prev_block_h: header.prev_hash,
			total_difficulty: header.total_difficulty(),
		}
	}
}

impl Hashed for Tip {
	/// The hash of the underlying block.
	fn hash(&self) -> Hash {
		self.last_block_h
	}
}

impl Default for Tip {
	fn default() -> Self {
		Tip {
			height: 0,
			last_block_h: ZERO_HASH,
			prev_block_h: ZERO_HASH,
			total_difficulty: Difficulty::min(),
		}
	}
}

/// Serialization of a tip, required to save to datastore.
impl ser::Writeable for Tip {
	fn write<W: ser::Writer>(&self, writer: &mut W) -> Result<(), ser::Error> {
		writer.write_u64(self.height)?;
		writer.write_fixed_bytes(&self.last_block_h)?;
		writer.write_fixed_bytes(&self.prev_block_h)?;
		self.total_difficulty.write(writer)
	}
}

impl ser::Readable for Tip {
	fn read(reader: &mut dyn ser::Reader) -> Result<Tip, ser::Error> {
		let height = reader.read_u64()?;
		let last = Hash::read(reader)?;
		let prev = Hash::read(reader)?;
		let diff = Difficulty::read(reader)?;
		Ok(Tip {
			height: height,
			last_block_h: last,
			prev_block_h: prev,
			total_difficulty: diff,
		})
	}
}

/// Bridge between the chain pipeline and the rest of the system. Handles
/// downstream processing of valid blocks by the rest of the system, most
/// importantly the broadcasting of blocks to our peers.
pub trait ChainAdapter {
	/// The blockchain pipeline has accepted this block as valid and added
	/// it to our chain.
	fn block_accepted(&self, block: &Block, status: BlockStatus, opts: Options);
}

/// Inform the caller of the current status of a txhashset write operation,
/// as it can take quite a while to process. Each function is called in the
/// order defined below and can be used to provide some feedback to the
/// caller. Functions taking arguments can be called repeatedly to update
/// those values as the processing progresses.
pub trait TxHashsetWriteStatus {
	/// First setup of the txhashset
	fn on_setup(&self);
	/// Starting kernel validation
	fn on_validation_kernels(&self, kernels: u64, kernel_total: u64);
	/// Starting rproof validation
	fn on_validation_rproofs(&self, rproofs: u64, rproof_total: u64);
	/// Starting to save the txhashset and related data
	fn on_save(&self);
	/// Done writing a new txhashset
	fn on_done(&self);
}

/// Do-nothing implementation of TxHashsetWriteStatus
pub struct NoStatus;

impl TxHashsetWriteStatus for NoStatus {
	fn on_setup(&self) {}
	fn on_validation_kernels(&self, _ks: u64, _kts: u64) {}
	fn on_validation_rproofs(&self, _rs: u64, _rt: u64) {}
	fn on_save(&self) {}
	fn on_done(&self) {}
}

/// Dummy adapter used as a placeholder for real implementations
pub struct NoopAdapter {}

impl ChainAdapter for NoopAdapter {
	fn block_accepted(&self, _b: &Block, _status: BlockStatus, _opts: Options) {}
}

/// Status of an accepted block.
#[derive(Debug, Clone, PartialEq)]
pub enum BlockStatus {
	/// Block is the "next" block, updating the chain head.
	Next,
	/// Block does not update the chain head and is a fork.
	Fork,
	/// Block updates the chain head via a (potentially disruptive) "reorg".
	/// Previous block was not our previous chain head.
	Reorg(u64),
}

// Elements in checkpoint data vector
#[derive(Debug)]
pub struct Checkpoint {
	pub height: u64,
	pub block_hash: Hash,
}

#[derive(Debug)]
pub struct BlockchainCheckpoints {
	pub checkpoints: Vec<Checkpoint>,
}

impl BlockchainCheckpoints {
	pub fn new() -> BlockchainCheckpoints {
		let checkpoints = vec![
			Checkpoint {
				height: 100000,
				block_hash: Hash::from_hex(
					"e835eb9ebc9f2e13b11061691cb268f44b20001f081003169b634497eb730848",
				)
				.unwrap(),
			},
			Checkpoint {
				height: 200000,
				block_hash: Hash::from_hex(
					"b2365a8c9719a709f11d450bbddfd012011e21c862239bdc8590aba00815e84c",
				)
				.unwrap(),
			},
			Checkpoint {
				height: 400000,
				block_hash: Hash::from_hex(
					"6578f1cdf5504d29fc757424e75ac60494e0f6d24b7553d124c8bea6ef99b5d8",
				)
				.unwrap(),
			},
			Checkpoint {
				height: 600000,
				block_hash: Hash::from_hex(
					"de483eafb2141d66bf541a94d8e41858f01ffc517b9fa61d8781483c34c2a6f7",
				)
				.unwrap(),
			},
			Checkpoint {
				height: 800000,
				block_hash: Hash::from_hex(
					"1465e7c094376e781b1e80ebd6b7a0c6350ec4d6554f9acdd843802162831003",
				)
				.unwrap(),
			},
			Checkpoint {
				height: 1000000,
				block_hash: Hash::from_hex(
					"00e4a404130ac192face23fd25f2c46a99a38a31d8cf2d3cc79ea7a518830686",
				)
				.unwrap(),
			},
			Checkpoint {
				height: 1200000,
				block_hash: Hash::from_hex(
					"8d69282df5579d32346ad0f6d3f4e03a43b1e00e741b1f3ba71c2934d81e5e1a",
				)
				.unwrap(),
			},
			Checkpoint {
				height: 1400000,
				block_hash: Hash::from_hex(
					"e7e34e50e8a5c9bcf3fe7b7ad99e62a848cda37171ce8d37f21bc334035df4d2",
				)
				.unwrap(),
			},
			Checkpoint {
				height: 1600000,
				block_hash: Hash::from_hex(
					"ba44beaf37776c3e7da3f4a1b906ae238e1178794cbaa90685e3945d2662d7a2",
				)
				.unwrap(),
			},
			Checkpoint {
				height: 1800000,
				block_hash: Hash::from_hex(
					"4f23aaf2e83e4041cac670226d3024f4468e3b9bb6ffa2548ebc59489bd09b63",
				)
				.unwrap(),
			},
			Checkpoint {
				height: 2000000,
				block_hash: Hash::from_hex(
					"eaf5d7a4b6f07ccb8bdbe5db2f39e10eea3ee1c28f8333907d91c9ccc21ce99d",
				)
				.unwrap(),
			},
			Checkpoint {
				height: 2200000,
				block_hash: Hash::from_hex(
					"1243520890d08026daba8207ed3d67186da64d2b71b5c1e2dd26d34092dee6ba",
				)
				.unwrap(),
			},
		];
		return BlockchainCheckpoints { checkpoints };
	}
}
